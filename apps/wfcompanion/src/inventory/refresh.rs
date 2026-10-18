use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::runtime::session::Session;
use retinue_warframe::game_observer::inventory::{Reader, Snapshot};

pub(super) struct Refresh {
    pub receiver: mpsc::Receiver<(Instant, Instant, u128, Result<Snapshot, String>)>,
    stopping: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Refresh {
    pub fn start(session: Session, stopping: Arc<AtomicBool>) -> Self {
        let (sender, receiver) = mpsc::sync_channel(1);
        let stop = Arc::clone(&stopping);
        let worker = thread::spawn(move || {
            let mut reader = match session.read(Reader::open_for_identity) {
                Ok(reader) => reader,
                Err(error) => {
                    session.record::<()>("native_inventory", &Err(error.clone()));
                    crate::incident::warn("inventory.native_refresh_unavailable", error);
                    return;
                }
            };
            while !stop.load(Ordering::Relaxed) && session.is_current() {
                let started = Instant::now();
                let snapshot = session
                    .check()
                    .and_then(|()| reader.read())
                    .and_then(|snapshot| {
                        session.check()?;
                        Ok(snapshot)
                    });
                let finished = Instant::now();
                session.record("native_inventory", &snapshot);
                let captured_at = super::unix_time_millis();
                match sender.try_send((started, finished, captured_at, snapshot)) {
                    Ok(()) => {}
                    Err(mpsc::TrySendError::Full(_)) => {}
                    Err(mpsc::TrySendError::Disconnected(_)) => break,
                }
                thread::park_timeout(Duration::from_secs(1));
            }
        });
        Self {
            receiver,
            stopping,
            worker: Some(worker),
        }
    }
}

impl Drop for Refresh {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            crate::runtime::join_worker("inventory-refresh", worker);
        }
    }
}
