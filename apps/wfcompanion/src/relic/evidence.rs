use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use image::DynamicImage;
use retinue_native::work::Budget;
use retinue_warframe::game_observer;

use super::{CaptureArm, REWARD_CAPTURE_DELAY, unix_time_millis};
use crate::daemon::OutboundSender;
use crate::{capture, incident};

static NEXT_CAPTURE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub(super) struct ArmedCapture {
    job: String,
    pub(super) directory: PathBuf,
    pub(super) armed_at: Instant,
    pub(super) armed_at_unix_ms: u128,
    pub(super) expires_at: Instant,
    pub(super) game: Option<game_observer::ProcessIdentity>,
}

pub(super) struct PendingCapture {
    armed: ArmedCapture,
    session: crate::runtime::session::Session,
    observed_at: Instant,
    observed_at_unix_ms: u128,
    image_captured_at_unix_ms: Option<u128>,
    armed_to_capture_ms: Option<u128>,
    trigger_to_capture_ms: Option<u128>,
    jobs: crate::runtime::jobs::Spawner,
    daemon: OutboundSender,
    finished: bool,
    budget: Budget,
}

impl ArmedCapture {
    pub(super) fn new(request: &CaptureArm) -> Self {
        let now = Instant::now();
        Self {
            job: format!(
                "{}-{}-{}",
                std::process::id(),
                unix_time_millis(),
                NEXT_CAPTURE.fetch_add(1, Ordering::Relaxed)
            ),
            directory: request.directory.clone(),
            armed_at: now,
            armed_at_unix_ms: unix_time_millis(),
            expires_at: now + request.timeout,
            game: None,
        }
    }

    pub(super) fn expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }
}

pub(super) fn begin_armed_capture(
    armed: ArmedCapture,
    session: crate::runtime::session::Session,
    observed_at: Instant,
    observed_at_unix_ms: u128,
    jobs: crate::runtime::jobs::Spawner,
    daemon: OutboundSender,
) -> PendingCapture {
    PendingCapture {
        armed,
        session,
        observed_at,
        observed_at_unix_ms,
        image_captured_at_unix_ms: None,
        armed_to_capture_ms: None,
        trigger_to_capture_ms: None,
        jobs,
        daemon,
        finished: false,
        budget: Budget::new(retinue_warframe::observation::ui_capture::limits()),
    }
}

impl PendingCapture {
    fn image_captured(&mut self) {
        self.image_captured_at_unix_ms = Some(unix_time_millis());
        self.armed_to_capture_ms = Some(self.armed.armed_at.elapsed().as_millis());
        self.trigger_to_capture_ms = Some(self.observed_at.elapsed().as_millis());
    }

    fn complete(&mut self, state: &str, error: Option<&str>) {
        self.finished = true;
        publish_capture_result(&self.daemon, &self.armed, state, error, &self.budget);
    }

    pub(super) fn spawn(mut self, work: impl FnOnce(Self) + Send + 'static) {
        let jobs = self.jobs.clone();
        let daemon = self.daemon.clone();
        let armed = self.armed.clone();
        let budget = self.budget.clone();
        // Rejected submissions report failure here, not cancellation from Drop.
        self.finished = true;
        if let Err(error) = jobs.spawn(budget.clone(), move || {
            self.finished = false;
            work(self);
        }) {
            incident::error("relic.capture_rejected", &error);
            publish_capture_result(&daemon, &armed, "failed", Some(&error), &budget);
        }
    }
}

impl Drop for PendingCapture {
    fn drop(&mut self) {
        if !self.finished {
            let (state, reason) = if thread::panicking() {
                ("failed", "capture worker panicked")
            } else {
                ("cancelled", "capture work retired before completion")
            };
            self.complete(state, Some(reason));
        }
    }
}

pub(super) fn publish_capture(daemon: &OutboundSender, state: &str, armed: Option<&ArmedCapture>) {
    let _ = daemon.send(crate::daemon::Outbound::Publish {
        dataset: "player",
        source: "capture",
        data: serde_json::json!({
            "companion_pid": std::process::id(),
            "state": state,
            "job": armed.map(|capture| &capture.job),
            "directory": armed.map(|capture| &capture.directory),
            "expires_at": armed.map(|capture| capture.armed_at_unix_ms
                + capture.expires_at.saturating_duration_since(capture.armed_at).as_millis()),
            "updated_at": unix_time_millis(),
        }),
    });
}

fn publish_capture_result(
    daemon: &OutboundSender,
    armed: &ArmedCapture,
    state: &str,
    error: Option<&str>,
    budget: &Budget,
) {
    let _ = daemon.send(crate::daemon::Outbound::Publish {
        dataset: "player",
        source: "capture_result",
        data: serde_json::json!({
            "companion_pid": std::process::id(),
            "state": state,
            "job": armed.job,
            "directory": armed.directory,
            "budget": budget.usage(),
            "error": error,
            "updated_at": unix_time_millis(),
        }),
    });
}

pub(super) fn record_evidence(mut pending: PendingCapture) {
    while Instant::now() < pending.observed_at + REWARD_CAPTURE_DELAY {
        if let Err(error) = pending.budget.check() {
            pending.complete(
                if pending.budget.cancelled() {
                    "cancelled"
                } else {
                    "failed"
                },
                Some(&error.to_string()),
            );
            return;
        }
        thread::sleep(Duration::from_millis(25).min(
            (pending.observed_at + REWARD_CAPTURE_DELAY).saturating_duration_since(Instant::now()),
        ));
    }
    if let Err(error) = pending.budget.check() {
        pending.complete(
            if pending.budget.cancelled() {
                "cancelled"
            } else {
                "failed"
            },
            Some(&error.to_string()),
        );
        return;
    }
    pending.armed.game = pending
        .session
        .identity()
        .ok()
        .map(|identity| (*identity).clone());
    publish_capture_result(
        &pending.daemon,
        &pending.armed,
        "capturing_image",
        None,
        &pending.budget,
    );
    let image = match pending
        .session
        .check()
        .and_then(|()| capture::relic_window_with_budget(&pending.budget))
    {
        Ok(image) => {
            pending.image_captured();
            Some(image)
        }
        Err(error) => {
            incident::warn("relic.capture_failed", error);
            None
        }
    };
    publish_capture_result(
        &pending.daemon,
        &pending.armed,
        "capturing_memory",
        None,
        &pending.budget,
    );
    let ui = pending
        .budget
        .check()
        .map_err(|error| error.to_string())
        .and_then(|()| {
            pending.session.read(|identity| {
                retinue_warframe::observation::ui_capture::collect(identity, &[], &pending.budget)
            })
        });
    let capture_error = ui.as_ref().err().cloned().or_else(|| {
        image
            .is_none()
            .then(|| "screenshot unavailable; see companion log".to_owned())
    });
    publish_capture_result(
        &pending.daemon,
        &pending.armed,
        "writing",
        None,
        &pending.budget,
    );
    match save_armed_capture(&pending, image.as_ref(), ui) {
        Ok(directory) => {
            incident::info(
                "relic.capture_saved",
                format!("target=relic_reward output={}", directory.display()),
            );
            pending.complete(
                if capture_error.is_some() {
                    "partial"
                } else {
                    "saved"
                },
                capture_error.as_deref(),
            );
        }
        Err(error) => {
            incident::error("relic.capture_save_failed", &error);
            pending.complete(
                if pending.budget.cancelled() {
                    "cancelled"
                } else {
                    "failed"
                },
                Some(&error),
            );
        }
    }
}

fn save_armed_capture(
    pending: &PendingCapture,
    image: Option<&DynamicImage>,
    ui: Result<retinue_warframe::observation::ui_capture::Frozen, String>,
) -> Result<PathBuf, String> {
    let armed = &pending.armed;
    let directory = retinue_native::recording::Directory::create(&armed.directory)?
        .with_budget(pending.budget.clone());
    let mut failure = pending.budget.check().err().map(|error| error.to_string());
    let image_metadata = image
        .map(|image| {
            (|| -> Result<_, String> {
                let image_path = armed.directory.join("relic-reward.png");
                let mut output = std::io::BufWriter::new(directory.file("relic-reward.png")?);
                image
                    .write_to(&mut output, image::ImageFormat::Png)
                    .map_err(|error| format!("could not save {}: {error}", image_path.display()))?;
                std::io::Write::flush(&mut output).map_err(|error| {
                    format!("could not flush {}: {error}", image_path.display())
                })?;
                Ok(serde_json::json!({
                    "status": "captured",
                    "path": image_path,
                    "width": image.width(),
                    "height": image.height(),
                }))
            })()
        })
        .map(|result| match result {
            Ok(metadata) => metadata,
            Err(error) => {
                failure.get_or_insert_with(|| error.clone());
                serde_json::json!({"status": "failed", "error": error})
            }
        });
    let metadata_path = armed.directory.join("metadata.json");
    let ui_metadata = match ui {
        Ok(frozen) => match frozen.save(&directory) {
            Ok(summary) => serde_json::json!({"status": "captured", "summary": summary}),
            Err(error) => {
                failure.get_or_insert_with(|| error.clone());
                serde_json::json!({"status": "failed", "error": error})
            }
        },
        Err(error) => serde_json::json!({"status": "failed", "error": error}),
    };
    if let Err(error) = pending.budget.check() {
        failure.get_or_insert_with(|| error.to_string());
    }
    let metadata = serde_json::json!({
        "schema": 2,
        "kind": "relic_reward",
        "job": armed.job,
        "budget": pending.budget.usage(),
        "error": failure,
        "trigger": "debug_output",
        "game_pid": pending.session.pid(),
        "session_generation": pending.session.generation(),
        "armed_at_unix_ms": armed.armed_at_unix_ms,
        "observed_at_unix_ms": pending.observed_at_unix_ms,
        "image_captured_at_unix_ms": pending.image_captured_at_unix_ms,
        "saved_at_unix_ms": unix_time_millis(),
        "armed_to_capture_ms": pending.armed_to_capture_ms,
        "trigger_to_capture_ms": pending.trigger_to_capture_ms,
        "image": image_metadata,
        "game": armed.game,
        "ui_capture": ui_metadata,
    });
    let encoded = serde_json::to_vec_pretty(&metadata)
        .map_err(|error| format!("could not encode capture metadata: {error}"))?;
    directory
        .write_report("metadata.json", &encoded)
        .map_err(|error| format!("could not write {}: {error}", metadata_path.display()))?;
    failure.map_or_else(|| Ok(armed.directory.clone()), Err)
}

#[cfg(test)]
mod tests {
    use super::super::lifecycle::Context;
    use super::*;
    use std::{fs, sync::mpsc, time::Duration};

    #[test]
    fn cancelled_capture_writes_only_terminal_metadata() {
        let path =
            std::env::temp_dir().join(format!("wf-cancelled-evidence-{}", std::process::id()));
        let jobs = crate::runtime::jobs::Jobs::new("test", 1);
        let (daemon, _) = crate::daemon::outbound_channel();
        let pending = begin_armed_capture(
            ArmedCapture::new(&CaptureArm {
                directory: path.clone(),
                timeout: Duration::from_secs(1),
            }),
            crate::runtime::session::Session::for_test(42),
            Instant::now(),
            100,
            jobs.spawner(),
            daemon,
        );
        pending.budget.cancel();
        assert!(
            save_armed_capture(
                &pending,
                Some(&DynamicImage::new_rgba8(8, 4)),
                Err("cancelled".into())
            )
            .unwrap_err()
            .contains("cancelled")
        );
        let metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(path.join("metadata.json")).unwrap()).unwrap();
        assert_eq!(metadata["job"], pending.armed.job);
        assert_eq!(metadata["image"]["status"], "failed");
        assert_eq!(metadata["budget"]["write_bytes_reserved"], 0);
        assert!(!path.join("relic-reward.png").exists());
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn capture_results_do_not_replace_new_armed_requests() {
        let (daemon, mut receiver) = crate::daemon::outbound_channel();
        let now = Instant::now();
        let armed = ArmedCapture {
            job: "next".into(),
            directory: PathBuf::from("/capture/next"),
            armed_at: now,
            armed_at_unix_ms: 100,
            expires_at: now + Duration::from_secs(1),
            game: None,
        };
        for state in ["armed", "cancelled", "expired", "triggered"] {
            publish_capture(&daemon, state, Some(&armed));
            let crate::daemon::Outbound::Publish { source, data, .. } =
                receiver.try_recv().unwrap()
            else {
                panic!("expected capture report")
            };
            assert_eq!(source, "capture");
            assert_eq!(data["companion_pid"], std::process::id());
            assert_eq!(data["state"], state);
            assert_eq!(data["directory"], "/capture/next");
            assert_eq!(data["expires_at"], 1100);
        }
        let previous = ArmedCapture {
            job: "previous".into(),
            directory: "/capture/previous".into(),
            ..armed.clone()
        };
        publish_capture_result(
            &daemon,
            &previous,
            "partial",
            Some("UI unavailable"),
            &Budget::new(retinue_warframe::observation::ui_capture::limits()),
        );
        let crate::daemon::Outbound::Publish { source, data, .. } = receiver.try_recv().unwrap()
        else {
            panic!("expected capture result")
        };
        assert_eq!(source, "capture_result");
        assert_eq!(data["directory"], "/capture/previous");
        assert_eq!(data["state"], "partial");
        assert_eq!(data["error"], "UI unavailable");
    }

    #[test]
    fn abandoned_and_rejected_capture_jobs_report_terminal_status() {
        let mut jobs = crate::runtime::jobs::Jobs::new("test", 1);
        let (daemon, mut outcomes) = crate::daemon::outbound_channel();
        let capture = || {
            begin_armed_capture(
                ArmedCapture::new(&CaptureArm {
                    directory: PathBuf::from("unused-test-capture"),
                    timeout: Duration::from_secs(1),
                }),
                crate::runtime::session::Session::for_test(1),
                Instant::now(),
                100,
                jobs.spawner(),
                daemon.clone(),
            )
        };
        drop(capture());
        let rejected = capture();
        jobs.shutdown();
        rejected.spawn(|_| panic!("closed jobs must not execute"));
        let states: Vec<_> = std::iter::from_fn(|| outcomes.try_recv().ok())
            .map(|message| {
                let crate::daemon::Outbound::Publish { source, data, .. } = message else {
                    panic!()
                };
                assert_eq!(source, "capture_result");
                data["state"].as_str().unwrap().to_owned()
            })
            .collect();
        assert_eq!(states, ["cancelled", "failed"]);
    }

    #[test]
    fn capture_completion_survives_scene_cancellation_and_is_joined() {
        let mut jobs = crate::runtime::jobs::Jobs::new("test", 1);
        let (daemon, mut outcomes) = crate::daemon::outbound_channel();
        let pending = begin_armed_capture(
            ArmedCapture::new(&CaptureArm {
                directory: PathBuf::from("unused-test-capture"),
                timeout: Duration::from_secs(1),
            }),
            crate::runtime::session::Session::for_test(1),
            Instant::now(),
            100,
            jobs.spawner(),
            daemon,
        );
        let scene = Context::for_test(None);
        let (release, wait) = mpsc::channel();
        pending.spawn(move |mut pending| {
            wait.recv().unwrap();
            pending.complete("saved", None);
        });
        scene.cancel();
        release.send(()).unwrap();
        jobs.shutdown();
        let crate::daemon::Outbound::Publish { data, .. } = outcomes.try_recv().unwrap() else {
            panic!()
        };
        assert_eq!(data["state"], "saved");
        assert!(outcomes.try_recv().is_err());
    }

    #[test]
    fn armed_capture_saves_visual_and_memory_metadata() {
        let directory = std::env::temp_dir().join(format!(
            "wfcompanion-armed-capture-{}-{}",
            std::process::id(),
            unix_time_millis()
        ));
        let now = Instant::now();
        let armed = ArmedCapture {
            job: "fixture".into(),
            directory: directory.clone(),
            armed_at: now,
            armed_at_unix_ms: 100,
            expires_at: now + Duration::from_secs(1),
            game: None,
        };
        let image = DynamicImage::new_rgba8(8, 4);
        let jobs = crate::runtime::jobs::Jobs::new("test", 1);
        let (daemon, _) = crate::daemon::outbound_channel();
        let pending = PendingCapture {
            armed,
            session: crate::runtime::session::Session::for_test(10),
            observed_at: now,
            observed_at_unix_ms: 200,
            image_captured_at_unix_ms: Some(300),
            armed_to_capture_ms: Some(200),
            trigger_to_capture_ms: Some(100),
            jobs: jobs.spawner(),
            daemon,
            finished: false,
            budget: Budget::new(retinue_warframe::observation::ui_capture::limits()),
        };

        save_armed_capture(&pending, Some(&image), Err("unavailable".to_owned())).unwrap();
        let metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.join("metadata.json")).unwrap()).unwrap();
        let image_exists = directory.join("relic-reward.png").is_file();
        let _ = fs::remove_dir_all(&directory);

        assert!(image_exists);
        assert_eq!(metadata["schema"], 2);
        assert_eq!(metadata["image"]["width"], 8);
        assert_eq!(metadata["image"]["height"], 4);
        assert_eq!(metadata["image_captured_at_unix_ms"], 300);
        assert_eq!(metadata["trigger_to_capture_ms"], 100);
        assert_eq!(metadata["ui_capture"]["status"], "failed");
        assert_eq!(metadata["game_pid"], 10);
    }
}
