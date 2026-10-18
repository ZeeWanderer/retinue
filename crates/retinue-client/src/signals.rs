//! Keep signal handling alive while application services drain and reload probes run.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::thread::{self, JoinHandle};

use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::oneshot;

pub struct Signals {
    cancel: Option<oneshot::Sender<()>>,
    worker: Option<JoinHandle<()>>,
    received: Arc<AtomicU8>,
    report: fn(&str),
}

impl Signals {
    /// Report `SIGINT`, `SIGTERM`, or `signal worker panicked` to the application.
    pub fn start(stopping: Arc<AtomicBool>, name: &str, report: fn(&str)) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("signal runtime: {error}"))?;
        let (mut terminate, mut interrupt) = {
            let _entered = runtime.enter();
            (
                signal(SignalKind::terminate()).map_err(|error| format!("SIGTERM: {error}"))?,
                signal(SignalKind::interrupt()).map_err(|error| format!("SIGINT: {error}"))?,
            )
        };
        let (cancel, cancelled) = oneshot::channel();
        let received = Arc::new(AtomicU8::new(0));
        let status = received.clone();
        let worker = thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                runtime.block_on(async {
                    let (name, code) = tokio::select! {
                        _ = cancelled => return,
                        _ = terminate.recv() => ("SIGTERM", 143),
                        _ = interrupt.recv() => ("SIGINT", 130),
                    };
                    status.store(code, Ordering::Release);
                    stopping.store(true, Ordering::Release);
                    report(name);
                });
            })
            .map_err(|error| format!("could not start signal worker: {error}"))?;
        Ok(Self {
            cancel: Some(cancel),
            worker: Some(worker),
            received,
            report,
        })
    }

    pub fn requested(&self) -> bool {
        self.exit_code().is_some()
    }

    pub fn exit_code(&self) -> Option<u8> {
        match self.received.load(Ordering::Acquire) {
            0 => None,
            code => Some(code),
        }
    }
}

impl Drop for Signals {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            (self.report)("signal worker panicked");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn unix_signals_request_shutdown() {
        const CHILD: &str = "RETINUE_CLIENT_TEST_SIGNAL";
        const DRAIN: &str = "RETINUE_CLIENT_TEST_SIGNAL_DRAIN";
        if let Ok(name) = std::env::var(CHILD) {
            let stopping = Arc::new(AtomicBool::new(std::env::var_os(DRAIN).is_some()));
            let signals = Signals::start(stopping.clone(), "test-signals", |_| {}).unwrap();
            assert!(!signals.requested());
            assert_eq!(signals.exit_code(), None);
            assert!(
                Command::new("kill")
                    .args([&format!("-{name}"), &std::process::id().to_string()])
                    .status()
                    .unwrap()
                    .success()
            );
            let deadline = Instant::now() + Duration::from_secs(5);
            while !signals.requested() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            assert!(stopping.load(Ordering::Acquire));
            assert!(signals.requested());
            assert_eq!(
                signals.exit_code(),
                Some(if name == "INT" { 130 } else { 143 })
            );
            drop(signals);
            return;
        }
        for (name, draining) in [
            ("TERM", false),
            ("INT", false),
            ("TERM", true),
            ("INT", true),
        ] {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "signals::tests::unix_signals_request_shutdown",
                    "--nocapture",
                ])
                .env(CHILD, name);
            if draining {
                command.env(DRAIN, "1");
            } else {
                command.env_remove(DRAIN);
            }
            let result = crate::process::output(command, Duration::from_secs(10), || true).unwrap();
            assert!(
                result.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }

    #[test]
    fn drop_cancels_and_joins_without_requesting_shutdown() {
        let stopping = Arc::new(AtomicBool::new(false));
        let signals = Signals::start(stopping.clone(), "test-signals", |_| {}).unwrap();
        let status = signals.received.clone();
        drop(signals);
        assert!(!stopping.load(Ordering::Acquire));
        assert_eq!(Arc::strong_count(&stopping), 1);
        assert_eq!(Arc::strong_count(&status), 1);
    }
}
