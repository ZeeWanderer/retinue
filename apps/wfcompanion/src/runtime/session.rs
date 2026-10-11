use std::fs;
use std::os::unix::fs::MetadataExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use retinue_warframe::game_observer::{ProcessIdentity, identify_process, ui};

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ProcessKey {
    pid: u32,
    started: u64,
    executable_device: u64,
    executable_inode: u64,
}

impl ProcessKey {
    fn read(pid: u32) -> Result<Self, String> {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat"))
            .map_err(|error| format!("could not read game process identity: {error}"))?;
        let started =
            process_start(&stat).ok_or_else(|| "game process start time is missing".to_owned())?;
        let executable = fs::metadata(format!("/proc/{pid}/exe"))
            .map_err(|error| format!("could not stat game process executable: {error}"))?;
        Ok(Self {
            pid,
            started,
            executable_device: executable.dev(),
            executable_inode: executable.ino(),
        })
    }
}

fn process_start(stat: &str) -> Option<u64> {
    // The parenthesized command can contain spaces and closing parentheses.
    stat.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()
}

#[derive(Clone, Debug)]
pub(crate) struct Session(Arc<State>);

#[derive(Debug)]
struct State {
    key: ProcessKey,
    generation: u64,
    retired: AtomicBool,
    identity: Mutex<Option<Arc<ProcessIdentity>>>,
    ui: OnceLock<Result<ui::Reader, String>>,
}

impl Session {
    pub(crate) fn key(&self) -> ProcessKey {
        self.0.key
    }

    pub(crate) fn pid(&self) -> u32 {
        self.0.key.pid
    }

    pub(crate) fn generation(&self) -> u64 {
        self.0.generation
    }

    pub(crate) fn process_started(&self) -> u64 {
        self.0.key.started
    }

    pub(crate) fn is_current(&self) -> bool {
        !self.0.retired.load(Ordering::Acquire)
    }

    pub(crate) fn check(&self) -> Result<(), String> {
        let result = ProcessKey::read(self.pid()).and_then(|key| {
            if self.is_current() && key == self.0.key {
                Ok(())
            } else {
                Err("game session has ended or been replaced".to_owned())
            }
        });
        if result.is_err() {
            self.0.retired.store(true, Ordering::Release);
        }
        result
    }

    pub(crate) fn identity(&self) -> Result<Arc<ProcessIdentity>, String> {
        self.check()?;
        let mut identity = self.0.identity.lock().unwrap();
        if identity.is_none() {
            let discovered = identify_process(self.pid())?;
            self.check()?;
            *identity = Some(Arc::new(discovered));
        }
        self.check()?;
        Ok(identity.as_ref().unwrap().clone())
    }

    pub(crate) fn read<T>(
        &self,
        read: impl FnOnce(&ProcessIdentity) -> Result<T, String>,
    ) -> Result<T, String> {
        let identity = self.identity()?;
        let result = read(&identity);
        self.check()?;
        result
    }

    pub(crate) fn read_ui<T>(
        &self,
        read: impl FnOnce(&ui::Reader) -> Result<T, String>,
    ) -> Result<T, String> {
        self.read(|identity| {
            let reader = self
                .0
                .ui
                .get_or_init(|| ui::Reader::open_for_identity(identity))
                .as_ref()
                .map_err(Clone::clone)?;
            self.check()?;
            read(reader)
        })
    }

    pub(crate) fn prepare_ui(&self) -> Result<super::Worker, String> {
        let session = self.clone();
        let worker = std::thread::Builder::new()
            .name("wfcompanion-ui-discovery".into())
            .spawn(move || {
                let started = Instant::now();
                match session.read_ui(|_| Ok(())) {
                    Ok(()) => crate::incident::info(
                        "observer.ui_bindings_ready",
                        format!(
                            "game_pid={} elapsed_ms={}",
                            session.pid(),
                            started.elapsed().as_millis()
                        ),
                    ),
                    Err(error) => crate::incident::warn(
                        "observer.ui_bindings_unavailable",
                        format!("game_pid={} {error}", session.pid()),
                    ),
                }
            })
            .map_err(|error| format!("could not start UI discovery: {error}"))?;
        Ok(super::Worker::new("ui-discovery", worker))
    }

    #[cfg(test)]
    pub(crate) fn for_test(pid: u32) -> Self {
        Self(Arc::new(State {
            key: ProcessKey {
                pid,
                started: 0,
                executable_device: 0,
                executable_inode: 0,
            },
            generation: 1,
            retired: AtomicBool::new(false),
            identity: Mutex::new(None),
            ui: OnceLock::new(),
        }))
    }
}

#[derive(Default)]
pub(crate) struct Sessions {
    current: Option<Session>,
    generation: u64,
}

impl Sessions {
    pub(crate) fn current(&self) -> Option<&Session> {
        self.current.as_ref()
    }

    pub(crate) fn update(&mut self, pid: Option<u32>) -> Result<bool, String> {
        match pid.map(ProcessKey::read).transpose() {
            Ok(key) => Ok(self.replace(key)),
            Err(error) => {
                self.replace(None);
                Err(error)
            }
        }
    }

    fn replace(&mut self, key: Option<ProcessKey>) -> bool {
        if self.current.as_ref().map(|session| session.0.key) == key
            && self.current.as_ref().is_none_or(Session::is_current)
        {
            return false;
        }
        if let Some(previous) = self.current.take() {
            previous.0.retired.store(true, Ordering::Release);
        }
        self.current = key.map(|key| {
            self.generation += 1;
            Session(Arc::new(State {
                key,
                generation: self.generation,
                retired: AtomicBool::new(false),
                identity: Mutex::new(None),
                ui: OnceLock::new(),
            }))
        });
        true
    }
}

impl Drop for Sessions {
    fn drop(&mut self) {
        self.replace(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_time_ignores_parentheses_and_spaces_in_process_name() {
        let fields = (4..=22)
            .map(|field| field.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            process_start(&format!("42 (odd ) game) R {fields}")),
            Some(22)
        );
        assert_eq!(process_start("42 (incomplete) R 1"), None);
        assert!(ProcessKey::read(std::process::id()).is_ok());
    }

    #[test]
    fn pid_reuse_and_exec_retire_old_work() {
        let mut sessions = Sessions::default();
        let mut key = ProcessKey {
            pid: 42,
            started: 1,
            executable_device: 2,
            executable_inode: 3,
        };
        assert!(sessions.replace(Some(key)));
        let first = sessions.current().unwrap().clone();
        first.0.ui.set(Err("unknown layout".into())).unwrap();
        assert!(!sessions.replace(Some(key)));
        assert!(first.is_current());
        key.started += 1;
        assert!(sessions.replace(Some(key)));
        assert!(!first.is_current());
        let second = sessions.current().unwrap().clone();
        assert!(second.0.ui.get().is_none());
        assert_eq!(second.generation(), first.generation() + 1);
        key.executable_inode += 1;
        assert!(sessions.replace(Some(key)));
        assert!(!second.is_current());
        let last = sessions.current().unwrap().clone();
        drop(sessions);
        assert!(!last.is_current());
    }

    #[test]
    fn failed_identity_reads_do_not_become_a_permanent_cached_failure() {
        let session = Session::for_test(u32::MAX);
        assert!(session.identity().is_err());
        assert!(session.0.identity.lock().unwrap().is_none());
        assert!(session.read_ui(|_| Ok(())).is_err());
        assert!(session.0.ui.get().is_none());
        drop(session.prepare_ui().unwrap());
        assert!(session.0.ui.get().is_none());
    }
}
