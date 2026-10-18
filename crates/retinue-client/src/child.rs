//! Monitor a launched game without taking ownership of its lifetime.

use std::ffi::OsStr;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rustix::process::{Pid, WaitOptions, waitpid};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChildIdentity {
    pid: u32,
    started: u64,
}

impl ChildIdentity {
    pub fn read(pid: u32, parent: u32) -> Result<Self, String> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map_err(|error| format!("launched game identity: {error}"))?;
        let fields: Vec<_> = stat
            .rsplit_once(')')
            .ok_or("invalid launched game stat")?
            .1
            .split_whitespace()
            .collect();
        if fields.get(1).and_then(|field| field.parse::<u32>().ok()) != Some(parent) {
            return Err("launched game is no longer our child".into());
        }
        let started = fields
            .get(19)
            .and_then(|field| field.parse().ok())
            .ok_or("launched game start time is missing")?;
        Ok(Self { pid, started })
    }

    pub fn validate(&self, parent: u32) -> Result<(), String> {
        let live = Self::read(self.pid, parent)?;
        if live.started != self.started {
            return Err("launched game PID has been reused".into());
        }
        Ok(())
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    fn try_wait(&self) -> Result<Option<ExitStatus>, String> {
        let pid = Pid::from_raw(self.pid as i32).ok_or("invalid launched game PID")?;
        loop {
            match waitpid(Some(pid), WaitOptions::NOHANG) {
                Ok(result) => {
                    return Ok(result.map(|(_, status)| ExitStatus::from_raw(status.as_raw())));
                }
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(format!("launched game wait: {error}")),
            }
        }
    }
}

#[derive(Debug)]
pub enum Outcome {
    Running(ChildIdentity),
    Exited(ExitStatus),
}

pub enum Event {
    WaitFailed(String),
    MonitorPanicked,
}

pub struct Monitor {
    stop: mpsc::Sender<()>,
    worker: Option<JoinHandle<Result<Outcome, String>>>,
    report: fn(Event),
}

impl Monitor {
    pub fn launch<S: AsRef<OsStr>>(
        command: &[S],
        shutdown: Arc<AtomicBool>,
        name: &str,
        report: fn(Event),
    ) -> Result<Self, String> {
        let (executable, arguments) = command
            .split_first()
            .ok_or_else(|| "game command is empty".to_owned())?;
        let child = Command::new(executable.as_ref())
            .args(arguments)
            .spawn()
            .map_err(|error| format!("failed to launch game: {error}"))?;
        let identity = ChildIdentity::read(child.id(), std::process::id())?;
        Self::resume(identity, shutdown, name, report)
    }

    pub fn resume(
        child: ChildIdentity,
        shutdown: Arc<AtomicBool>,
        name: &str,
        report: fn(Event),
    ) -> Result<Self, String> {
        child.validate(std::process::id())?;
        let (stop, stopping) = mpsc::channel();
        let worker = thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                loop {
                    match child.try_wait() {
                        Ok(None) => {}
                        Ok(Some(status)) => {
                            shutdown.store(true, Ordering::Release);
                            return Ok(Outcome::Exited(status));
                        }
                        Err(error) => {
                            report(Event::WaitFailed(error.clone()));
                            shutdown.store(true, Ordering::Release);
                            return Err(error);
                        }
                    }
                    if stopping.recv_timeout(Duration::from_millis(200))
                        != Err(mpsc::RecvTimeoutError::Timeout)
                    {
                        break;
                    }
                }
                Ok(Outcome::Running(child))
            })
            .map_err(|error| format!("could not monitor launched game: {error}"))?;
        Ok(Self {
            stop,
            worker: Some(worker),
            report,
        })
    }

    /// Join the monitor, leaving a live game untouched or returning its exact exit status.
    /// Subsequent calls return `None`.
    pub fn stop_monitor(&mut self) -> Result<Option<Outcome>, String> {
        let _ = self.stop.send(());
        match self.worker.take() {
            Some(worker) => match worker.join() {
                Ok(result) => result.map(Some),
                Err(_) => {
                    (self.report)(Event::MonitorPanicked);
                    Err("game monitor panicked".into())
                }
            },
            None => Ok(None),
        }
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        // Stopping companion must not terminate the user's game.
        let _ = self.stop_monitor();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopping_monitor_preserves_live_child() {
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut game =
            Monitor::launch(&["sleep", "60"], shutdown.clone(), "test-game", |_| {}).unwrap();
        let Some(Outcome::Running(child)) = game.stop_monitor().unwrap() else {
            panic!("live child was lost");
        };
        let running = child.try_wait().unwrap().is_none();
        let pid = Pid::from_raw(child.pid as i32).unwrap();
        rustix::process::kill_process(pid, rustix::process::Signal::KILL).unwrap();
        waitpid(Some(pid), WaitOptions::empty()).unwrap();
        assert!(running);
        assert!(!shutdown.load(Ordering::Acquire));
    }

    #[test]
    fn child_exit_notifies_core_and_is_reaped() {
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut game = Monitor::launch(
            &["sh", "-c", "exit 7"],
            shutdown.clone(),
            "test-game",
            |_| {},
        )
        .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !shutdown.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(shutdown.load(Ordering::Acquire));
        let Some(Outcome::Exited(status)) = game.stop_monitor().unwrap() else {
            panic!("child exit status was lost");
        };
        assert_eq!(status.code(), Some(7));
        assert!(game.stop_monitor().unwrap().is_none());
    }

    #[test]
    fn dropping_monitor_joins_without_terminating_the_game() {
        let mut child = Command::new("sleep").arg("60").spawn().unwrap();
        let identity = ChildIdentity::read(child.id(), std::process::id()).unwrap();
        assert!(identity.validate(std::process::id() + 1).is_err());
        let shutdown = Arc::new(AtomicBool::new(false));
        let monitor = Monitor::resume(identity, shutdown.clone(), "test-game", |_| {}).unwrap();
        drop(monitor);
        let running = child.try_wait().unwrap().is_none();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(running);
        assert!(!shutdown.load(Ordering::Acquire));
        assert_eq!(Arc::strong_count(&shutdown), 1);
    }

    #[test]
    fn launch_failure_is_returned_to_caller() {
        assert!(
            Monitor::launch(
                &["/wfcompanion-test/no-such-command"],
                Arc::new(AtomicBool::new(false)),
                "test-game",
                |_| {}
            )
            .is_err()
        );
    }

    #[test]
    fn launch_preserves_non_utf8_arguments() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("argument");
        let bytes = vec![b'x', 0xff, b'y'];
        let command = vec![
            OsString::from("sh"),
            OsString::from("-c"),
            OsString::from("printf '%s' \"$1\" > \"$2\""),
            OsString::from("test"),
            OsString::from_vec(bytes.clone()),
            output.clone().into_os_string(),
        ];
        let stopping = Arc::new(AtomicBool::new(false));
        let mut monitor = Monitor::launch(&command, stopping.clone(), "test-game", |_| {}).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !stopping.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(stopping.load(Ordering::Acquire));
        let Some(Outcome::Exited(status)) = monitor.stop_monitor().unwrap() else {
            panic!("child exit status was lost");
        };
        assert!(status.success());
        assert_eq!(std::fs::read(output).unwrap(), bytes);
    }

    #[test]
    fn monitor_can_resume_and_reap_the_same_child() {
        let stopping = Arc::new(AtomicBool::new(false));
        let mut first =
            Monitor::launch(&["sleep", "60"], stopping.clone(), "test-game", |_| {}).unwrap();
        let Some(Outcome::Running(child)) = first.stop_monitor().unwrap() else {
            panic!("live child was lost");
        };
        let encoded = serde_json::to_vec(&child).unwrap();
        let mut wrong = child.clone();
        wrong.started += 1;
        assert!(Monitor::resume(wrong, stopping.clone(), "test-game", |_| {}).is_err());
        let mut second = Monitor::resume(
            serde_json::from_slice(&encoded).unwrap(),
            stopping.clone(),
            "test-game",
            |_| {},
        )
        .unwrap();
        let pid = Pid::from_raw(child.pid as i32).unwrap();
        rustix::process::kill_process(pid, rustix::process::Signal::KILL).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !stopping.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(stopping.load(Ordering::Acquire));
        let Some(Outcome::Exited(status)) = second.stop_monitor().unwrap() else {
            panic!("child signal status was lost");
        };
        assert_eq!(status.signal(), Some(libc::SIGKILL));
        assert_eq!(
            waitpid(Some(pid), WaitOptions::NOHANG).unwrap_err(),
            rustix::io::Errno::CHILD
        );
    }
}
