use std::ffi::OsString;
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use retinue_client::child::{self, Monitor, Outcome};
use retinue_client::reload::{self, Checkpoint, Config, Gate, Watch};
use retinue_client::signals::Signals;
use serde::{Deserialize, Serialize};

pub(crate) const RELOAD: Config = Config {
    name: "arccompanion-reload",
    enable_env: "ARCCOMPANION_DEV_RELOAD",
    handoff_env: "ARCCOMPANION_HANDOFF",
    schema: 1,
    ready: "arccompanion-reload/1\n",
};

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct State {}

impl Checkpoint for State {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

pub(crate) fn launch(
    command: Vec<OsString>,
    resumed: Option<reload::Handoff<State>>,
) -> Result<u8, String> {
    RELOAD.enabled()?;
    let mut command = Some(command);
    let (installed, mut child) = match resumed {
        Some(handoff) => {
            command = None;
            let child = handoff.child.ok_or("launch reload handoff has no child")?;
            eprintln!("arccompanion: reload.resumed");
            (handoff.installed, Some(child))
        }
        None => (
            std::env::current_exe().map_err(|error| error.to_string())?,
            None,
        ),
    };
    let mut rejected = None;
    loop {
        let stopping = Arc::new(AtomicBool::new(false));
        let signals = Signals::start(stopping.clone(), "arccompanion-signals", report_signal)?;
        if let Some(code) = signals.exit_code() {
            return Ok(code);
        }
        let mut game = match child.take() {
            Some(child) => {
                Monitor::resume(child, stopping.clone(), "arccompanion-game", report_child)?
            }
            None => Monitor::launch(
                &command.take().ok_or("launch command already consumed")?,
                stopping.clone(),
                "arccompanion-game",
                report_child,
            )?,
        };
        let gate = Gate::default();
        let mut watch = Watch::start(
            &RELOAD,
            installed.clone(),
            gate.clone(),
            stopping.clone(),
            rejected,
            report_reload,
        )?;
        while !stopping.load(Ordering::Acquire) {
            // ARC currently has no observation or interaction work to drain.
            if gate.quiesce_if_idle(true, &stopping) {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let outcome = game.stop_monitor()?;
        let candidate = watch.finish()?;
        if let Some(code) = signals.exit_code() {
            return Ok(code);
        }
        let identity = match outcome {
            Some(Outcome::Exited(status)) => return Ok(exit_code(status)),
            Some(Outcome::Running(child)) => child,
            None => return Err("game monitor stopped without a child outcome".into()),
        };
        if !gate.approved() {
            return Err("launch stopped without an exit or approved reload".into());
        }
        let candidate = candidate.ok_or("reload approved without a replacement")?;
        rejected = Some(candidate.stamp);
        let handoff =
            reload::Handoff::new(&RELOAD, installed.clone(), Some(identity), State::default());
        eprintln!("arccompanion: reload.exec");
        // Keep signal handling alive after other workers stop and throughout validation.
        let error = candidate
            .replace(&RELOAD, &handoff, || !signals.requested())
            .unwrap_err();
        if let Some(code) = signals.exit_code() {
            return Ok(code);
        }
        eprintln!("arccompanion: reload.failed {error}");
        child = handoff.child;
    }
}

fn exit_code(status: ExitStatus) -> u8 {
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(1)) as u8
}

fn report_signal(name: &str) {
    eprintln!("arccompanion: process.stop_requested {name}");
}

fn report_child(event: child::Event) {
    match event {
        child::Event::WaitFailed(error) => eprintln!("arccompanion: game.wait_failed {error}"),
        child::Event::MonitorPanicked => {
            eprintln!("arccompanion: game.wait_failed game monitor panicked")
        }
    }
}

fn report_reload(event: reload::Event) {
    match event {
        reload::Event::Watching(path) => {
            eprintln!("arccompanion: reload.watching {}", path.display())
        }
        reload::Event::Pending(reason) => eprintln!("arccompanion: reload.pending {reason}"),
        reload::Event::WatchFailed(error) => eprintln!("arccompanion: reload.watch_failed {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_child_codes_and_signals_to_shell_exit_codes() {
        assert_eq!(exit_code(ExitStatus::from_raw(7 << 8)), 7);
        assert_eq!(exit_code(ExitStatus::from_raw(15)), 143);
    }
}
