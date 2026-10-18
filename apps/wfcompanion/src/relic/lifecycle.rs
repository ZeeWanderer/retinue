use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use super::*;

const MAX_WORKERS: usize = 2;

#[derive(Clone, Debug)]
pub(crate) struct Context {
    pub(crate) generation: u64,
    pub(crate) deadline: Option<Instant>,
    cancelled: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
    session: Option<crate::runtime::session::Session>,
}

impl Context {
    #[cfg(test)]
    pub(crate) fn for_test(deadline: Option<Instant>) -> Self {
        Lifecycle::default().start(deadline, &Arc::new(AtomicBool::new(false)), None)
    }

    pub(crate) fn is_current(&self) -> bool {
        !self.cancelled.load(Ordering::Acquire)
            && !self.stopping.load(Ordering::Relaxed)
            && self
                .session
                .as_ref()
                .is_none_or(|session| session.is_current())
            && self
                .deadline
                .is_none_or(|deadline| Instant::now() < deadline)
    }

    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub(super) fn check(&self) -> Result<(), String> {
        self.is_current()
            .then_some(())
            .ok_or_else(|| "relic context cancelled".to_owned())
    }
}

#[derive(Default)]
struct Lifecycle {
    generation: u64,
    current: Option<Context>,
    last_reward: Option<Instant>,
    last_suggestion: Option<Instant>,
    last_era: Option<String>,
    opened: Option<Instant>,
    after_reward: bool,
    dismissed: bool,
    suggesting: bool,
}

impl Lifecycle {
    fn start(
        &mut self,
        deadline: Option<Instant>,
        stopping: &Arc<AtomicBool>,
        session: Option<crate::runtime::session::Session>,
    ) -> Context {
        self.cancel();
        self.generation += 1;
        let context = Context {
            generation: self.generation,
            deadline,
            cancelled: Arc::new(AtomicBool::new(false)),
            stopping: stopping.clone(),
            session,
        };
        self.current = Some(context.clone());
        context
    }

    fn cancel(&mut self) {
        if let Some(context) = self.current.take() {
            context.cancel();
        }
    }

    fn close(&mut self, now: Instant) -> bool {
        if !self.suggesting {
            return false;
        }
        if self.opened.is_some_and(|opened| {
            ignore_suggestion_close(self.after_reward, now.saturating_duration_since(opened))
        }) {
            return false;
        }
        self.cancel();
        self.opened = None;
        self.after_reward = false;
        self.dismissed = false;
        self.last_suggestion = None;
        self.suggesting = false;
        true
    }

    fn completed(&mut self, generation: u64, era: Option<String>, failed: bool) {
        if !self
            .current
            .as_ref()
            .is_some_and(|context| context.generation == generation && context.is_current())
        {
            return;
        }
        if let Some(era) = era {
            self.last_era = Some(era);
        }
        if failed {
            self.last_reward = None;
        }
    }
}

struct Job {
    trigger: Trigger,
    context: Context,
    fallback_era: Option<String>,
    updates: super::Sender,
}

pub(crate) fn spawn(
    triggers: super::Receiver,
    sender: super::Sender,
    daemon: OutboundSender,
    ui: crate::runtime::presentation::Sender,
    stopping: Arc<AtomicBool>,
    reload: retinue_client::reload::Gate,
) -> thread::JoinHandle<()> {
    spawn_with_worker(triggers, sender, daemon, ui, stopping, reload, run_job)
}

fn spawn_with_worker(
    triggers: super::Receiver,
    sender: super::Sender,
    daemon: OutboundSender,
    ui: crate::runtime::presentation::Sender,
    stopping: Arc<AtomicBool>,
    reload: retinue_client::reload::Gate,
    work: impl Fn(Job, &OutboundSender, &crate::runtime::presentation::Sender) -> (Option<String>, bool)
    + Send
    + Sync
    + 'static,
) -> thread::JoinHandle<()> {
    let work = Arc::new(work);
    thread::spawn(move || {
        let mut captures = crate::runtime::jobs::Jobs::new("capture", 2);
        let mut lifecycle = Lifecycle::default();
        let mut armed: Option<ArmedCapture> = None;
        let mut workers: BTreeMap<u64, crate::runtime::Worker> = BTreeMap::new();
        let mut pending: Option<Job> = None;
        publish_capture(&daemon, "idle", None);
        while !stopping.load(Ordering::Relaxed) {
            let trigger = triggers.recv_timeout(Duration::from_millis(200));
            if let Some(expired) = armed.take_if(|capture| capture.expired()) {
                incident::info("relic.capture_arm_expired", "relic_reward");
                publish_capture(&daemon, "expired", Some(&expired));
            }
            match trigger {
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Ok(Trigger::ArmCapture(request)) => {
                    incident::info(
                        "relic.capture_armed",
                        format!(
                            "output={} timeout_ms={}",
                            request.directory.display(),
                            request.timeout.as_millis()
                        ),
                    );
                    armed = Some(ArmedCapture::new(&request));
                    publish_capture(&daemon, "armed", armed.as_ref());
                }
                Ok(Trigger::CancelCapture) => {
                    captures.cancel_all();
                    if let Some(cancelled) = armed.take() {
                        incident::info("relic.capture_arm_cancelled", "relic_reward");
                        publish_capture(&daemon, "cancelled", Some(&cancelled));
                    }
                }
                Ok(event @ (Trigger::GameStopped | Trigger::IntakeGap)) => {
                    if matches!(event, Trigger::GameStopped) {
                        captures.cancel_all();
                    }
                    lifecycle.cancel();
                    lifecycle = Lifecycle {
                        generation: lifecycle.generation,
                        ..Lifecycle::default()
                    };
                    pending = None;
                    if let Some(cancelled) = armed.take() {
                        publish_capture(&daemon, "cancelled", Some(&cancelled));
                    }
                    let _ = ui.send(UiEvent::RelicDismiss);
                }
                Ok(Trigger::CloseSuggestions) => {
                    if lifecycle.close(Instant::now()) {
                        pending = None;
                        let _ = ui.send(UiEvent::RelicDismiss);
                    } else {
                        incident::info("relic.suggestion_close_ignored", "debug_output");
                    }
                }
                Ok(Trigger::DismissSuggestions { generation }) => {
                    if !lifecycle.suggesting
                        || lifecycle
                            .current
                            .as_ref()
                            .is_none_or(|context| context.generation != generation)
                    {
                        continue;
                    }
                    lifecycle.cancel();
                    lifecycle.dismissed = true;
                    lifecycle.opened = None;
                    pending = None;
                    let _ = ui.send(UiEvent::RelicDismiss);
                }
                Ok(Trigger::WorkFinished {
                    generation,
                    era,
                    failed,
                }) => {
                    drop(workers.remove(&generation));
                    lifecycle.completed(generation, era, failed);
                }
                Ok(Trigger::SuggestionReady { generation, era }) => {
                    lifecycle.completed(generation, Some(era), false);
                }
                Ok(trigger) => {
                    if matches!(&trigger,
                        Trigger::Rewards { session, .. } | Trigger::Suggestions { session, .. }
                        if !session.is_current())
                    {
                        continue;
                    }
                    let now = Instant::now();
                    let mut fallback_era = None;
                    let deadline = match &trigger {
                        Trigger::Suggestions { observed_at, .. } => {
                            if lifecycle.dismissed
                                || reject_recent_trigger(
                                    &mut lifecycle.last_suggestion,
                                    SUGGESTION_TRIGGER_DEDUPLICATION,
                                )
                            {
                                continue;
                            }
                            lifecycle.after_reward = lifecycle.last_reward.is_some_and(|seen| {
                                now.saturating_duration_since(seen) < SUGGESTION_REWARD_FALLBACK
                            });
                            if lifecycle.after_reward {
                                fallback_era = lifecycle.last_era.clone();
                            }
                            lifecycle.opened = Some(*observed_at);
                            lifecycle.suggesting = true;
                            None
                        }
                        Trigger::Rewards {
                            session,
                            observed_at,
                            observed_at_unix_ms,
                        } => {
                            if reject_duplicate_reward_trigger(&mut lifecycle.last_reward) {
                                continue;
                            }
                            lifecycle.dismissed = false;
                            lifecycle.opened = None;
                            lifecycle.suggesting = false;
                            if let Some(armed) = armed.take() {
                                publish_capture(&daemon, "triggered", Some(&armed));
                                begin_armed_capture(
                                    armed,
                                    session.clone(),
                                    *observed_at,
                                    *observed_at_unix_ms,
                                    captures.spawner(),
                                    daemon.clone(),
                                )
                                .spawn(record_evidence);
                            }
                            Some(*observed_at + REWARD_SCENE_LIFETIME)
                        }
                        Trigger::Screenshot(_) => {
                            lifecycle.suggesting = false;
                            Some(now + REWARD_SCENE_LIFETIME)
                        }
                        _ => unreachable!(),
                    };
                    let session = match &trigger {
                        Trigger::Rewards { session, .. } | Trigger::Suggestions { session, .. } => {
                            Some(session.clone())
                        }
                        _ => None,
                    };
                    let context = lifecycle.start(deadline, &stopping, session);
                    let _ = ui.send(UiEvent::RelicStart(context.clone()));
                    pending = Some(Job {
                        trigger,
                        context,
                        fallback_era,
                        updates: sender.clone(),
                    });
                }
            }
            if workers.len() < MAX_WORKERS
                && let Some(job) = pending.take()
            {
                if !job.context.is_current() {
                    continue;
                }
                let generation = job.context.generation;
                let sender = sender.clone();
                let daemon = daemon.clone();
                let ui = ui.clone();
                let work = work.clone();
                workers.insert(
                    generation,
                    crate::runtime::Worker::new(
                        "relic",
                        thread::spawn(move || {
                            let result =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    work(job, &daemon, &ui)
                                }));
                            let (era, failed) = result.unwrap_or_else(|_| {
                                incident::error("relic.worker_failed", "worker panicked");
                                (None, true)
                            });
                            let _ = sender.send(Trigger::WorkFinished {
                                generation,
                                era,
                                failed,
                            });
                        }),
                    ),
                );
            }
            if reload.pending() {
                let idle = armed.is_none()
                    && pending.is_none()
                    && workers.is_empty()
                    && captures.is_idle()
                    && !lifecycle.suggesting
                    && lifecycle
                        .current
                        .as_ref()
                        .is_none_or(|context| !context.is_current())
                    && sender.stats().items == 0;
                if reload.quiesce_if_idle(idle, &stopping) {
                    break;
                }
            }
        }
        lifecycle.cancel();
        drop(pending);
        if let Some(cancelled) = armed.take() {
            publish_capture(&daemon, "cancelled", Some(&cancelled));
        }
        drop(workers);
        captures.shutdown();
    })
}

fn run_job(
    job: Job,
    daemon: &OutboundSender,
    ui: &crate::runtime::presentation::Sender,
) -> (Option<String>, bool) {
    if let Trigger::Suggestions {
        session,
        observed_at,
    } = job.trigger
    {
        match show_suggestions(
            daemon,
            ui,
            &job.context,
            session,
            observed_at,
            job.fallback_era.as_deref(),
        ) {
            Ok(era) => {
                let _ = job.updates.send(Trigger::SuggestionReady {
                    generation: job.context.generation,
                    era: era.clone(),
                });
                if job.context.is_current() {
                    show_suggestion_prices(daemon, ui, &job.context, &era);
                }
                (Some(era), false)
            }
            Err(error) => {
                if job.context.is_current() {
                    incident::warn("relic.suggestion_failed", error);
                }
                (None, false)
            }
        }
    } else {
        (None, !read_rewards(job.trigger, daemon, ui, &job.context))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn retiring_game_session_invalidates_its_scene() {
        let mut sessions = crate::runtime::session::Sessions::default();
        sessions.update(Some(std::process::id())).unwrap();
        let stopping = Arc::new(AtomicBool::new(false));
        let context = Lifecycle::default().start(None, &stopping, sessions.current().cloned());
        assert!(context.is_current());
        sessions.update(None).unwrap();
        assert!(!context.is_current());
    }

    #[test]
    fn stale_session_trigger_cannot_replace_current_scene() {
        let (triggers, receiver) = super::super::channel();
        let (ui, events) = crate::runtime::presentation::channel();
        let (daemon, _) = crate::daemon::outbound_channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let worker = spawn_with_worker(
            receiver,
            triggers.clone(),
            daemon,
            ui,
            stopping.clone(),
            Default::default(),
            |_, _, _| (None, false),
        );
        let mut sessions = crate::runtime::session::Sessions::default();
        sessions.update(Some(std::process::id())).unwrap();
        let session = sessions.current().unwrap().clone();
        sessions.update(None).unwrap();
        triggers
            .send(Trigger::Suggestions {
                session,
                observed_at: Instant::now(),
            })
            .unwrap();
        triggers
            .send(Trigger::Screenshot(PathBuf::from("test")))
            .unwrap();
        let first = events.recv_timeout(Duration::from_secs(5)).unwrap();
        stopping.store(true, Ordering::Relaxed);
        worker.join().unwrap();
        assert!(matches!(
            first,
            UiEvent::RelicStart(Context {
                generation: 1,
                session: None,
                ..
            })
        ));
    }

    #[test]
    fn reload_waits_for_armed_capture_to_be_cancelled() {
        let (triggers, receiver) = super::super::channel();
        let (ui, _) = crate::runtime::presentation::channel();
        let (daemon, mut publications) = crate::daemon::outbound_channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let gate = retinue_client::reload::Gate::default();
        let worker = spawn_with_worker(
            receiver,
            triggers.clone(),
            daemon,
            ui,
            stopping.clone(),
            gate.clone(),
            |_, _, _| (None, false),
        );
        publications.blocking_recv().unwrap();
        triggers
            .send(Trigger::ArmCapture(CaptureArm {
                directory: PathBuf::from("unused"),
                timeout: Duration::from_secs(60),
            }))
            .unwrap();
        publications.blocking_recv().unwrap();
        gate.request();
        thread::sleep(Duration::from_millis(450));
        assert!(!stopping.load(Ordering::Acquire));
        triggers.send(Trigger::CancelCapture).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !stopping.load(Ordering::Acquire) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let approved = stopping.swap(true, Ordering::AcqRel);
        worker.join().unwrap();
        assert!(
            approved,
            "idle actor must accept pending reload after capture cancellation"
        );
    }

    #[test]
    fn intake_gap_retires_scene_and_cancels_pending_arm() {
        let (triggers, receiver) = super::super::channel();
        let (ui, events) = crate::runtime::presentation::channel();
        let (daemon, mut publications) = crate::daemon::outbound_channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let worker = spawn_with_worker(
            receiver,
            triggers.clone(),
            daemon,
            ui,
            stopping.clone(),
            Default::default(),
            |_, _, _| (None, false),
        );
        triggers
            .send(Trigger::ArmCapture(CaptureArm {
                directory: PathBuf::from("unused"),
                timeout: Duration::from_secs(60),
            }))
            .unwrap();
        triggers
            .send(Trigger::Screenshot(PathBuf::from("unused")))
            .unwrap();
        let UiEvent::RelicStart(context) = events.recv_timeout(Duration::from_secs(5)).unwrap()
        else {
            panic!("expected scene start");
        };
        triggers.send(Trigger::IntakeGap).unwrap();
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(5)).unwrap(),
            UiEvent::RelicDismiss
        ));
        assert!(!context.is_current());
        stopping.store(true, Ordering::Release);
        worker.join().unwrap();
        assert!(
            std::iter::from_fn(|| publications.try_recv().ok()).any(|message| matches!(
                message, crate::daemon::Outbound::Publish { source: "capture", data, .. }
                if data["state"] == "cancelled"
            ))
        );
    }

    #[test]
    fn game_stop_cancels_triggered_capture_before_more_reads() {
        let directory = std::env::temp_dir().join(format!(
            "wfcompanion-retired-scene-{}-{}",
            std::process::id(),
            unix_time_millis()
        ));
        let (triggers, receiver) = super::super::channel();
        let (ui, events) = crate::runtime::presentation::channel();
        let (daemon, mut publications) = crate::daemon::outbound_channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let worker = spawn_with_worker(
            receiver,
            triggers.clone(),
            daemon,
            ui,
            stopping.clone(),
            Default::default(),
            |_, _, _| (None, false),
        );
        triggers
            .send(Trigger::ArmCapture(CaptureArm {
                directory: directory.clone(),
                timeout: Duration::from_secs(5),
            }))
            .unwrap();
        triggers
            .send(Trigger::Rewards {
                session: crate::runtime::session::Session::for_test(u32::MAX),
                observed_at: Instant::now() + Duration::from_secs(10),
                observed_at_unix_ms: unix_time_millis(),
            })
            .unwrap();
        triggers.send(Trigger::GameStopped).unwrap();
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(5)).unwrap(),
            UiEvent::RelicStart(_)
        ));
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(5)).unwrap(),
            UiEvent::RelicDismiss
        ));
        stopping.store(true, Ordering::Relaxed);
        worker.join().unwrap();
        let results: Vec<_> = std::iter::from_fn(|| publications.try_recv().ok())
            .filter_map(|message| match message {
                crate::daemon::Outbound::Publish {
                    source: "capture_result",
                    data,
                    ..
                } => Some(data),
                _ => None,
            })
            .collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["state"], "cancelled");
        assert!(results[0]["job"].is_string());
        assert_eq!(results[0]["budget"]["read_bytes_reserved"], 0);
        assert!(!directory.exists());
    }

    #[test]
    fn slow_workers_do_not_block_close_and_only_latest_work_is_queued() {
        let (triggers, receiver) = super::super::channel();
        let (ui, events) = crate::runtime::presentation::channel();
        let (daemon, _) = crate::daemon::outbound_channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let (started, jobs) = mpsc::channel();
        let worker = spawn_with_worker(
            receiver,
            triggers.clone(),
            daemon,
            ui,
            stopping.clone(),
            Default::default(),
            move |job, _, _| {
                let (release, wait) = mpsc::channel();
                started.send((job.context.generation, release)).unwrap();
                wait.recv_timeout(Duration::from_secs(5)).unwrap();
                (None, false)
            },
        );
        let open = || {
            triggers
                .send(Trigger::Suggestions {
                    session: crate::runtime::session::Session::for_test(1),
                    observed_at: Instant::now() - Duration::from_secs(5),
                })
                .unwrap();
            let UiEvent::RelicStart(context) = events.recv_timeout(Duration::from_secs(2)).unwrap()
            else {
                panic!()
            };
            context
        };
        let close = || {
            triggers.send(Trigger::CloseSuggestions).unwrap();
            assert!(matches!(
                events.recv_timeout(Duration::from_secs(2)).unwrap(),
                UiEvent::RelicDismiss
            ));
        };
        let first = open();
        let (_, release_first) = jobs.recv_timeout(Duration::from_secs(2)).unwrap();
        close();
        assert!(!first.is_current());
        let second = open();
        let (_, release_second) = jobs.recv_timeout(Duration::from_secs(2)).unwrap();
        close();
        let third = open();
        close();
        let fourth = open();
        assert!(!second.is_current());
        assert!(!third.is_current());
        assert!(jobs.try_recv().is_err());
        release_first.send(()).unwrap();
        let (generation, release_fourth) = jobs.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(generation, fourth.generation);
        triggers.send(Trigger::GameStopped).unwrap();
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(2)).unwrap(),
            UiEvent::RelicDismiss
        ));
        assert!(!fourth.is_current());
        stopping.store(true, Ordering::Relaxed);
        release_second.send(()).unwrap();
        release_fourth.send(()).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn close_reopen_rejects_delayed_scenes_and_price_results() {
        let stopping = Arc::new(AtomicBool::new(false));
        let mut lifecycle = Lifecycle::default();
        let old = lifecycle.start(None, &stopping, None);
        lifecycle.opened = Some(Instant::now() - Duration::from_secs(4));
        lifecycle.suggesting = true;
        assert!(lifecycle.close(Instant::now()));
        let new = lifecycle.start(None, &stopping, None);
        lifecycle.completed(old.generation, Some("Axi".to_owned()), false);
        assert_eq!(lifecycle.last_era, None);
        assert!(!old.is_current());
        assert!(new.is_current());
        lifecycle.completed(new.generation, Some("Lith".to_owned()), false);
        assert_eq!(lifecycle.last_era.as_deref(), Some("Lith"));
    }

    #[test]
    fn cancellation_also_invalidates_already_queued_ui_results() {
        let stopping = Arc::new(AtomicBool::new(false));
        let mut lifecycle = Lifecycle::default();
        let context = lifecycle.start(None, &stopping, None);
        let (sender, receiver) = crate::runtime::presentation::channel();
        send_scene(&sender, &context, Scene::Reading, None);
        lifecycle.cancel();
        let UiEvent::RelicScene { context, .. } = receiver.recv().unwrap() else {
            panic!()
        };
        assert!(!context.is_current());
        let context = lifecycle.start(None, &stopping, None);
        stopping.store(true, Ordering::Relaxed);
        assert!(!context.is_current());
    }

    #[test]
    fn expired_rewards_cannot_publish() {
        let stopping = Arc::new(AtomicBool::new(false));
        let context = Lifecycle::default().start(Some(Instant::now()), &stopping, None);
        let (sender, receiver) = crate::runtime::presentation::channel();
        send_scene(&sender, &context, Scene::Reading, context.deadline);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn suggestion_close_does_not_cancel_rewards() {
        let stopping = Arc::new(AtomicBool::new(false));
        let mut lifecycle = Lifecycle::default();
        let context = lifecycle.start(
            Some(Instant::now() + REWARD_SCENE_LIFETIME),
            &stopping,
            None,
        );
        assert!(!lifecycle.close(Instant::now()));
        assert!(context.is_current());
    }
}
