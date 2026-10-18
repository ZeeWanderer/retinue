use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

use crate::{daemon, incident, observer, overlay, relic, shortcut};
use retinue_client::child::{self, Monitor};
use retinue_client::reload::{Gate, Watch};
use retinue_client::signals::Signals;

pub(crate) mod diagnostics;
pub(crate) mod inbox;
pub(crate) mod jobs;
pub(crate) mod presentation;
pub(crate) mod reload;
pub(crate) mod session;

pub(crate) fn run(
    mut launch: Option<Vec<String>>,
    mut relic_screenshot: Option<PathBuf>,
    resumed: Option<reload::Handoff>,
    log: &mut Option<incident::Writer>,
) -> Result<(), String> {
    reload::CONFIG.enabled()?;
    let (installed, mut child, mut state) = match resumed {
        Some(handoff) => {
            launch = None;
            relic_screenshot = None;
            incident::info("reload.resumed", "reattaching observation services");
            (handoff.installed, handoff.child, handoff.state)
        }
        None => (
            std::env::current_exe().map_err(|error| error.to_string())?,
            None,
            reload::State::default(),
        ),
    };
    let mode = if launch.is_some() || child.is_some() {
        "launch"
    } else {
        "standalone"
    };
    incident::info(
        "process.start",
        format!("mode={mode} version={}", env!("WFCLI_VERSION")),
    );
    let mut rejected = None;
    loop {
        let (ui, events) = presentation::channel();
        let (relic, triggers) = relic::channel();
        let (diagnostics, requests) = diagnostics::channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let signals = Signals::start(stopping.clone(), "wfcompanion-signals", report_signal)?;
        let gate = Gate::default();
        let mut core = Core::new(stopping);
        core.daemon = Some(daemon::spawn(
            ui.clone(),
            relic.clone(),
            diagnostics,
            mode,
            std::mem::take(&mut state.publications),
        )?);
        let outbound = core.daemon.as_ref().unwrap().outbound();
        core.observer = Some(observer::spawn(
            outbound.clone(),
            relic.clone(),
            requests,
            core.stopping.clone(),
            state.inventory.take(),
        ));
        core.relic = Some(relic::spawn(
            triggers,
            relic.clone(),
            outbound.clone(),
            ui.clone(),
            core.stopping.clone(),
            gate.clone(),
        ));
        if let Some(path) = relic_screenshot.take() {
            let _ = relic.send(relic::Trigger::Screenshot(path));
        }
        if core.stopping.load(Ordering::Acquire) {
            return Ok(());
        }
        let mut game = if let Some(child) = child.take() {
            Some(Monitor::resume(
                child,
                core.stopping.clone(),
                "wfcompanion-game",
                report_child,
            )?)
        } else {
            launch
                .take()
                .map(|command| {
                    Monitor::launch(
                        &command,
                        core.stopping.clone(),
                        "wfcompanion-game",
                        report_child,
                    )
                })
                .transpose()?
        };
        let mut watch = Watch::start(
            &reload::CONFIG,
            installed.clone(),
            gate.clone(),
            core.stopping.clone(),
            rejected,
            reload::report,
        )?;
        core.shortcut = Some(shortcut::spawn(ui)?);
        if let Err(error) = overlay::run(
            &events,
            relic,
            outbound,
            core.shortcut.as_ref().unwrap().controller(),
            core.stopping.clone(),
            gate.clone(),
        ) {
            incident::error("overlay.unavailable", error.to_string());
            eprintln!("wfcompanion: overlay unavailable ({error}); game observation continues");
            core.shortcut
                .as_ref()
                .unwrap()
                .controller()
                .set_enabled(false);
            wait_without_overlay(&events, &core.stopping);
        }
        state = core.shutdown();
        child = game.as_mut().and_then(|game| match game.stop_monitor() {
            Ok(Some(child::Outcome::Running(child))) => Some(child),
            _ => None,
        });
        let candidate = watch.finish()?;
        if !gate.approved() || signals.requested() || (mode == "launch" && child.is_none()) {
            incident::info("process.stop", format!("mode={mode}"));
            return Ok(());
        }
        let candidate = candidate.ok_or("reload approved without a replacement")?;
        rejected = Some(candidate.stamp);
        let handoff = reload::Handoff::new(&reload::CONFIG, installed.clone(), child.take(), state);
        incident::info(
            "reload.exec",
            "observation paused; preserving child and inventory stream",
        );
        drop(log.take());
        let error = candidate
            .replace(&reload::CONFIG, &handoff, || !signals.requested())
            .unwrap_err();
        *log = Some(incident::Writer::start()?);
        if signals.requested() {
            return Ok(());
        }
        incident::warn("reload.failed", error);
        child = handoff.child;
        state = handoff.state;
    }
}

fn report_child(event: child::Event) {
    match event {
        child::Event::WaitFailed(error) => incident::warn("game.wait_failed", error),
        child::Event::MonitorPanicked => {
            incident::error("game.wait_failed", "game monitor panicked")
        }
    }
}

fn report_signal(name: &str) {
    if name == "signal worker panicked" {
        incident::error("runtime.worker_panicked", "worker=signals");
    } else {
        incident::info("process.stop_requested", name);
    }
}

fn wait_without_overlay(events: &presentation::Receiver, stopping: &AtomicBool) {
    while !stopping.load(Ordering::Acquire) {
        match events.recv_timeout(std::time::Duration::from_millis(250)) {
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                stopping.store(true, Ordering::Release);
            }
            _ => {}
        }
    }
}

struct Core {
    stopping: Arc<AtomicBool>,
    observer: Option<JoinHandle<Option<crate::inventory::Checkpoint>>>,
    relic: Option<JoinHandle<()>>,
    daemon: Option<daemon::Connection>,
    shortcut: Option<shortcut::Service>,
}

impl Core {
    fn new(stopping: Arc<AtomicBool>) -> Self {
        Self {
            stopping,
            observer: None,
            relic: None,
            daemon: None,
            shortcut: None,
        }
    }

    fn shutdown(&mut self) -> reload::State {
        let mut state = reload::State::default();
        self.stopping.store(true, Ordering::Release);
        if let Some(daemon) = &self.daemon {
            daemon.quiesce();
        }
        drop(self.shortcut.take());
        if let Some(worker) = self.observer.take() {
            match worker.join() {
                Ok(inventory) => state.inventory = inventory,
                Err(_) => incident::error("runtime.worker_panicked", "worker=observer"),
            }
        }
        if let Some(worker) = self.relic.take() {
            join_worker("relic", worker);
        }
        // Capture outcomes can still be published while their workers drain.
        if let Some(mut daemon) = self.daemon.take() {
            state.publications = daemon.finish();
        }
        state
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub(crate) fn join_worker(name: &str, worker: JoinHandle<()>) {
    if worker.join().is_err() {
        incident::error("runtime.worker_panicked", format!("worker={name}"));
    }
}

pub(crate) struct Worker {
    name: &'static str,
    handle: Option<JoinHandle<()>>,
}

impl Worker {
    pub(crate) fn new(name: &'static str, handle: JoinHandle<()>) -> Self {
        Self {
            name,
            handle: Some(handle),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(worker) = self.handle.take() {
            join_worker(self.name, worker);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UiEvent;

    #[test]
    fn presentation_failure_keeps_core_alive_until_explicit_shutdown() {
        let (sender, receiver) = presentation::channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let worker_stop = stopping.clone();
        let worker = std::thread::spawn(move || wait_without_overlay(&receiver, &worker_stop));
        sender.send(UiEvent::InteractionToggle).unwrap();
        sender.send(UiEvent::RelicDismiss).unwrap();
        assert!(!stopping.load(Ordering::Acquire));
        stopping.store(true, Ordering::Release);
        worker.join().unwrap();
        assert!(stopping.load(Ordering::Acquire));
    }

    #[test]
    fn partial_startup_and_repeated_shutdown_join_workers() {
        let mut core = Core::new(Arc::new(AtomicBool::new(false)));
        let stopping = core.stopping.clone();
        let (done, result) = mpsc::channel();
        core.observer = Some(std::thread::spawn(move || {
            while !stopping.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
            done.send(()).unwrap();
            None
        }));
        core.shutdown();
        core.shutdown();
        assert!(result.try_recv().is_ok());
        assert!(core.observer.is_none());
    }

    #[test]
    fn unwind_stops_and_joins_started_workers() {
        let (done, result) = mpsc::channel();
        let panic = std::panic::catch_unwind(|| {
            let mut core = Core::new(Arc::new(AtomicBool::new(false)));
            let stopping = core.stopping.clone();
            core.relic = Some(std::thread::spawn(move || {
                while !stopping.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
                done.send(()).unwrap();
            }));
            panic!("startup failed");
        });
        assert!(panic.is_err());
        assert!(result.try_recv().is_ok());
    }
}
