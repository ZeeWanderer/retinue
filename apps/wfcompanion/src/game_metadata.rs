use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use retinue_warframe::game_observer::metadata;
use serde_json::Value;

use crate::daemon::{self, OutboundSender};
use crate::runtime::{inbox, session::Session};

const RETRY_INTERVAL: Duration = Duration::from_secs(30);
const STOP_CHECK_INTERVAL: Duration = Duration::from_millis(250);
const METADATA_SCHEMA: u64 = 2;

type Sender = inbox::Sender<Event>;

pub(crate) fn channel() -> (Sender, inbox::Receiver<Event>) {
    inbox::channel(
        "game_metadata",
        inbox::Limit {
            items: 1,
            bytes: 8 * 1024 * 1024,
        },
        inbox::Limit { items: 0, bytes: 0 },
    )
}

#[derive(Debug)]
pub(crate) enum Event {
    Captured {
        game_pid: u32,
        data: Value,
        cached: bool,
    },
    Unavailable {
        game_pid: u32,
        reason: String,
    },
}

impl inbox::Message for Event {
    fn bytes(&self) -> usize {
        size_of::<Self>()
            + match self {
                Self::Captured { data, .. } => inbox::value_bytes(data),
                Self::Unavailable { reason, .. } => reason.capacity(),
            }
    }

    fn replaces(&self, queued: &Self) -> bool {
        let pid = |event: &Self| match event {
            Self::Captured { game_pid, .. } | Self::Unavailable { game_pid, .. } => *game_pid,
        };
        pid(self) == pid(queued)
    }
}

pub(crate) struct Bridge {
    game_pid: u32,
    stopping: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Bridge {
    pub(crate) fn start(
        session: Session,
        outbound: OutboundSender,
        events: Sender,
    ) -> Result<Self, String> {
        let game_pid = session.pid();
        let stopping = Arc::new(AtomicBool::new(false));
        let worker_stopping = Arc::clone(&stopping);
        let worker = thread::Builder::new()
            .name("wfcompanion-game-metadata".to_owned())
            .spawn(move || scan(session, outbound, worker_stopping, events))
            .map_err(|error| format!("could not start game metadata collector: {error}"))?;
        Ok(Self {
            game_pid,
            stopping,
            worker: Some(worker),
        })
    }

    pub(crate) fn game_pid(&self) -> u32 {
        self.game_pid
    }

    pub(crate) fn is_running(&self) -> bool {
        self.worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
    }

    pub(crate) fn stop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            crate::runtime::join_worker("game-metadata", worker);
        }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop();
    }
}

fn scan(session: Session, outbound: OutboundSender, stopping: Arc<AtomicBool>, events: Sender) {
    let game_pid = session.pid();
    if let Ok(Some(data)) = cached_metadata(&session, &outbound) {
        let _ = events.send(Event::Captured {
            game_pid,
            data,
            cached: true,
        });
        wait(&stopping, Duration::MAX);
        return;
    }
    let mut previous_error = None;
    while !stopping.load(Ordering::Relaxed) && session.is_current() {
        let result = session
            .read(|identity| metadata::capture_for_identity(game_pid, identity.clone()))
            .and_then(|captured| {
                serde_json::to_value(captured)
                    .map_err(|error| format!("could not serialize game metadata: {error}"))
            });
        session.record("equipment_pools", &result);
        match result {
            Ok(data) => {
                let _ = events.send(Event::Captured {
                    game_pid,
                    data,
                    cached: false,
                });
                wait(&stopping, Duration::MAX);
                return;
            }
            Err(reason) => {
                if previous_error.as_deref() != Some(reason.as_str()) {
                    if events
                        .send(Event::Unavailable {
                            game_pid,
                            reason: reason.clone(),
                        })
                        .is_err()
                    {
                        return;
                    }
                    previous_error = Some(reason.clone());
                }
                if reason.starts_with("unsupported Warframe executable ") {
                    wait(&stopping, Duration::MAX);
                    return;
                }
                wait(&stopping, RETRY_INTERVAL);
            }
        }
    }
}

fn cached_metadata(session: &Session, outbound: &OutboundSender) -> Result<Option<Value>, String> {
    let identity = session.identity()?;
    let response = daemon::dataset_get(outbound, "game_metadata")?;
    session.check()?;
    Ok(cached_payload(&response, &identity.executable.sha256))
}

fn cached_payload(response: &Value, executable_sha256: &str) -> Option<Value> {
    let Some(data) = response.pointer("/data/data").cloned() else {
        return None;
    };
    let hash = data.pointer("/executable/sha256").and_then(Value::as_str);
    let schema = data.get("schema").and_then(Value::as_u64);
    let usable =
        data.get("archimedea").is_some_and(Value::is_object) && data.get("capture_error").is_none();
    (schema == Some(METADATA_SCHEMA) && hash == Some(executable_sha256) && usable).then_some(data)
}

fn wait(stopping: &AtomicBool, duration: Duration) {
    let deadline = Instant::now().checked_add(duration);
    while !stopping.load(Ordering::Relaxed)
        && deadline.is_none_or(|deadline| Instant::now() < deadline)
    {
        thread::park_timeout(STOP_CHECK_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_observer_receives_latest_metadata_state() {
        let (sender, receiver) = channel();
        for attempt in 0..1000 {
            sender
                .send(Event::Unavailable {
                    game_pid: 42,
                    reason: attempt.to_string(),
                })
                .unwrap();
        }
        sender
            .send(Event::Captured {
                game_pid: 42,
                data: serde_json::json!({"archimedea": {}}),
                cached: false,
            })
            .unwrap();
        assert_eq!(sender.stats().items, 1);
        assert_eq!(sender.stats().coalesced, 1000);
        assert!(matches!(
            receiver.recv().unwrap(),
            Event::Captured { game_pid: 42, .. }
        ));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn wait_returns_immediately_when_stopped() {
        let stopping = AtomicBool::new(true);
        let started = Instant::now();
        wait(&stopping, Duration::MAX);
        assert!(started.elapsed() < STOP_CHECK_INTERVAL);
    }

    #[test]
    fn cache_payload_requires_matching_executable_and_archimedea_data() {
        let response = serde_json::json!({
            "data": {
                "revision": 1,
                "data": {
                    "schema": 2,
                    "executable": {"sha256": "current"},
                    "archimedea": {}
                }
            }
        });
        assert!(cached_payload(&response, "current").is_some());
        assert!(cached_payload(&response, "stale").is_none());
        let obsolete = serde_json::json!({
            "data": {"data": {
                "schema": 1,
                "executable": {"sha256": "current"},
                "archimedea": {}
            }}
        });
        assert!(cached_payload(&obsolete, "current").is_none());
        let mut failed = response;
        failed["data"]["data"]["capture_error"] =
            serde_json::json!({"reason": "unsupported_executable"});
        assert!(cached_payload(&failed, "current").is_none());
    }
}
