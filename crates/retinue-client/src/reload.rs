//! Development-only, same-PID companion replacement using a sealed checkpoint.
//!
//! Call `Config::resume` at single-threaded startup and handle `--reload-check`
//! before starting services. Each application owns its checkpoint validation,
//! idle policy, service teardown and recovery loop. Join every application worker
//! (including its async runtime) before `Candidate::replace`.

use std::ffi::CString;
use std::fs::File;
use std::io::{Read, Seek};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rustix::fs::{MemfdFlags, SealFlags, fcntl_add_seals, fcntl_get_seals, memfd_create};
use rustix::io::{FdFlags, fcntl_setfd};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};

use crate::child::ChildIdentity;

/// Keep environment names and the readiness token unique to the application.
#[derive(Clone, Copy)]
pub struct Config {
    /// Memfd and watcher thread name.
    pub name: &'static str,
    pub enable_env: &'static str,
    pub handoff_env: &'static str,
    pub schema: u32,
    pub ready: &'static str,
}

const MAX_BYTES: u64 = 40 * 1024 * 1024;
const SEALS: SealFlags = SealFlags::SEAL
    .union(SealFlags::WRITE)
    .union(SealFlags::GROW)
    .union(SealFlags::SHRINK);

pub trait Checkpoint: Serialize + DeserializeOwned {
    fn validate(&self) -> Result<(), String>;
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handoff<S> {
    schema: u32,
    owner: u32,
    pub installed: PathBuf,
    pub child: Option<ChildIdentity>,
    pub state: S,
}

impl<S: Checkpoint> Handoff<S> {
    pub fn new(
        config: &Config,
        installed: PathBuf,
        child: Option<ChildIdentity>,
        state: S,
    ) -> Self {
        Self {
            schema: config.schema,
            owner: std::process::id(),
            installed,
            child,
            state,
        }
    }

    fn validate(&self, config: &Config, owner: u32) -> Result<(), String> {
        if self.schema != config.schema || self.owner != owner || !self.installed.is_absolute() {
            return Err("incompatible development reload handoff".into());
        }
        self.state.validate()?;
        if let Some(child) = &self.child {
            child.validate(owner)?;
        }
        Ok(())
    }

    fn file(&self, config: &Config) -> Result<File, String> {
        let fd = memfd_create(config.name, MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING)
            .map_err(|error| error.to_string())?;
        let mut file = File::from(fd);
        serde_json::to_writer(&mut file, self).map_err(|error| error.to_string())?;
        if file.metadata().map_err(|error| error.to_string())?.len() > MAX_BYTES {
            return Err("development reload handoff is too large".into());
        }
        file.rewind().map_err(|error| error.to_string())?;
        fcntl_add_seals(&file, SEALS).map_err(|error| error.to_string())?;
        Ok(file)
    }
}

fn decode<S: Checkpoint>(
    config: &Config,
    reader: impl Read,
    owner: u32,
) -> Result<Handoff<S>, String> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("development reload handoff is too large".into());
    }
    let handoff: Handoff<S> = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    handoff.validate(config, owner)?;
    Ok(handoff)
}

impl Config {
    pub fn enabled(&self) -> Result<bool, String> {
        self.enabled_for_build(cfg!(debug_assertions), std::env::var(self.enable_env))
    }

    fn enabled_for_build(
        &self,
        debug: bool,
        value: Result<String, std::env::VarError>,
    ) -> Result<bool, String> {
        match value.as_deref() {
            Err(std::env::VarError::NotPresent) => Ok(debug),
            Ok("0") => Ok(false),
            Ok("1") if debug => Ok(true),
            Ok("1") => Err(format!("{} requires a development build", self.enable_env)),
            _ => Err(format!("{} must be 0 or 1", self.enable_env)),
        }
    }

    /// Consume the inherited handoff descriptor and remove its environment entry.
    ///
    /// # Safety
    /// Call at single-threaded startup, before any environment readers or writers
    /// run concurrently. The handoff descriptor must not be owned by another object.
    pub unsafe fn resume<S: Checkpoint>(&self) -> Result<Option<Handoff<S>>, String> {
        let Some(raw) = std::env::var_os(self.handoff_env) else {
            return Ok(None);
        };
        if !self.enabled()? {
            return Err("development reload handoff without reload enabled".into());
        }
        let fd: i32 = raw
            .to_str()
            .and_then(|value| value.parse().ok())
            .filter(|fd| *fd > 2)
            .ok_or("invalid reload descriptor")?;
        let file = File::open(format!("/proc/self/fd/{fd}")).map_err(|error| error.to_string())?;
        if !fcntl_get_seals(&file)
            .map_err(|error| error.to_string())?
            .contains(SEALS)
        {
            return Err("reload descriptor is not a sealed handoff".into());
        }
        // Startup is single-threaded; opening the proc descriptor validated ownership.
        drop(unsafe { File::from_raw_fd(fd) });
        // Do not propagate a consumed descriptor to helpers or a newly started daemon.
        unsafe {
            std::env::remove_var(self.handoff_env);
        }
        decode(self, file, std::process::id()).map(Some)
    }

    /// Validate a candidate's stdin checkpoint without starting its services.
    pub fn check<S: Checkpoint>(&self) -> Result<(), String> {
        if !cfg!(debug_assertions) {
            return Err("replacement is not a development build".into());
        }
        let owner = rustix::process::getppid()
            .ok_or("reload parent disappeared")?
            .as_raw_nonzero()
            .get() as u32;
        decode::<S>(self, std::io::stdin().lock(), owner).map(|_| ())
    }
}

#[derive(Clone, Default)]
pub struct Gate(Arc<GateState>);

#[derive(Default)]
struct GateState {
    requested: AtomicBool,
    activity: AtomicU8,
}

const INTERACTIVE: u8 = 1;
const APPROVED: u8 = 2;

impl Gate {
    pub fn request(&self) {
        self.0.requested.store(true, Ordering::Release);
    }

    pub fn pending(&self) -> bool {
        cfg!(debug_assertions) && self.0.requested.load(Ordering::Acquire)
    }

    pub fn interaction(&self, active: bool) {
        if active {
            self.try_interaction();
        } else {
            self.0.activity.fetch_and(!INTERACTIVE, Ordering::AcqRel);
        }
    }

    pub fn try_interaction(&self) -> bool {
        self.0
            .activity
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                (value & APPROVED == 0).then_some(value | INTERACTIVE)
            })
            .is_ok()
    }

    pub fn quiesce_if_idle(&self, idle: bool, stopping: &AtomicBool) -> bool {
        if idle
            && self.pending()
            && !stopping.load(Ordering::Acquire)
            && self
                .0
                .activity
                .compare_exchange(0, APPROVED, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            stopping.store(true, Ordering::Release);
            return true;
        }
        false
    }

    pub fn approved(&self) -> bool {
        self.0.activity.load(Ordering::Acquire) & APPROVED != 0
    }
}

pub enum Event {
    Watching(PathBuf),
    Pending(&'static str),
    WatchFailed(String),
}

pub struct Watch {
    worker: Option<JoinHandle<Result<Option<Candidate>, String>>>,
    cancel: Arc<AtomicBool>,
    report: fn(Event),
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Stamp {
    device: u64,
    inode: u64,
    length: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

impl Watch {
    pub fn start(
        config: &Config,
        installed: PathBuf,
        gate: Gate,
        stopping: Arc<AtomicBool>,
        rejected: Option<Stamp>,
        report: fn(Event),
    ) -> Result<Self, String> {
        let cancel = Arc::new(AtomicBool::new(false));
        if !config.enabled()? {
            return Ok(Self {
                worker: None,
                cancel,
                report,
            });
        }
        let cancelled = cancel.clone();
        let worker = thread::Builder::new()
            .name(config.name.into())
            .spawn(move || {
                let mut running =
                    File::open("/proc/self/exe").map_err(|error| error.to_string())?;
                let own_hash = digest(&mut running)?;
                let running_stamp = stamp(&running)?;
                let mut previous = running_stamp;
                report(Event::Watching(installed.clone()));
                while !stopping.load(Ordering::Acquire) && !cancelled.load(Ordering::Acquire) {
                    if let Ok(mut file) = File::open(&installed) {
                        let current = stamp(&file)?;
                        if current != previous {
                            let hash = digest(&mut file)?;
                            previous = current;
                            let candidate = Candidate {
                                file,
                                hash,
                                stamp: current,
                            };
                            if let Some(reason) = candidate.change_reason(running_stamp, own_hash)
                                && Some(current) != rejected
                            {
                                report(Event::Pending(reason));
                                gate.request();
                                return Ok(Some(candidate));
                            }
                        }
                    }
                    for _ in 0..10 {
                        if stopping.load(Ordering::Acquire) || cancelled.load(Ordering::Acquire) {
                            break;
                        }
                        thread::sleep(Duration::from_millis(100));
                    }
                }
                Ok(None)
            })
            .map_err(|error| format!("reload watcher: {error}"))?;
        Ok(Self {
            worker: Some(worker),
            cancel,
            report,
        })
    }

    pub fn finish(&mut self) -> Result<Option<Candidate>, String> {
        self.cancel.store(true, Ordering::Release);
        match self.worker.take() {
            Some(worker) => worker.join().map_err(|_| "reload watcher panicked")?,
            None => Ok(None),
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            (self.report)(Event::WatchFailed(error));
        }
    }
}

pub struct Candidate {
    file: File,
    hash: [u8; 32],
    pub stamp: Stamp,
}

impl Candidate {
    fn change_reason(&self, running: Stamp, own_hash: [u8; 32]) -> Option<&'static str> {
        if self.hash != own_hash {
            Some("code changed")
        } else if (self.stamp.device, self.stamp.inode) != (running.device, running.inode) {
            Some("installed identity changed")
        } else {
            None
        }
    }

    pub fn replace<S: Checkpoint>(
        &self,
        config: &Config,
        handoff: &Handoff<S>,
        active: impl Fn() -> bool,
    ) -> Result<(), String> {
        let mut state = handoff.file(config)?;
        let path = format!("/proc/{}/fd/{}", std::process::id(), self.file.as_raw_fd());
        let mut probe = Command::new(&path);
        probe.arg("--reload-check").env_remove(config.handoff_env);
        let result = crate::process::output_with_stdin(
            probe,
            Stdio::from(state.try_clone().map_err(|error| error.to_string())?),
            Duration::from_secs(5),
            &active,
        )?;
        if !result.status.success() || result.stdout != config.ready.as_bytes() || !active() {
            return Err(
                "replacement rejected the reload handoff or validation was cancelled".into(),
            );
        }
        let mut installed = File::open(&handoff.installed).map_err(|error| error.to_string())?;
        if stamp(&installed)? != self.stamp || digest(&mut installed)? != self.hash {
            return Err("replacement changed during reload validation".into());
        }
        state.rewind().map_err(|error| error.to_string())?;
        if !active() {
            return Err("replacement validation was cancelled".into());
        }
        self.exec(config, &state)
    }

    fn exec(&self, config: &Config, state: &File) -> Result<(), String> {
        let args: Vec<_> = std::env::args_os()
            .map(|arg| CString::new(arg.as_bytes()))
            .collect::<Result<_, _>>()
            .map_err(|error| error.to_string())?;
        let mut environment: Vec<_> = std::env::vars_os()
            .filter(|(key, _)| key != config.handoff_env)
            .map(|(key, value)| {
                let mut entry = key.as_bytes().to_vec();
                entry.push(b'=');
                entry.extend_from_slice(value.as_bytes());
                CString::new(entry)
            })
            .collect::<Result<_, _>>()
            .map_err(|error| error.to_string())?;
        environment.push(
            CString::new(format!("{}={}", config.handoff_env, state.as_raw_fd()))
                .map_err(|error| error.to_string())?,
        );
        let pointers = |strings: &[CString]| {
            strings
                .iter()
                .map(|value| value.as_ptr())
                .chain(std::iter::once(std::ptr::null()))
                .collect::<Vec<_>>()
        };
        let args_ptrs = pointers(&args);
        let env_ptrs = pointers(&environment);
        fcntl_setfd(state, FdFlags::empty()).map_err(|error| error.to_string())?;
        // Keep the validated inode pinned and leave the old process unchanged on failure.
        // Both null-terminated pointer arrays borrow live CStrings throughout fexecve.
        unsafe {
            libc::fexecve(self.file.as_raw_fd(), args_ptrs.as_ptr(), env_ptrs.as_ptr());
        }
        let error = std::io::Error::last_os_error();
        let _ = fcntl_setfd(state, FdFlags::CLOEXEC);
        Err(format!("replacement exec: {error}"))
    }
}

fn stamp(file: &File) -> Result<Stamp, String> {
    let stat = file.metadata().map_err(|error| error.to_string())?;
    Ok(Stamp {
        device: stat.dev(),
        inode: stat.ino(),
        length: stat.len(),
        modified: (stat.mtime(), stat.mtime_nsec()),
        changed: (stat.ctime(), stat.ctime_nsec()),
    })
}

fn digest(file: &mut File) -> Result<[u8; 32], String> {
    file.rewind().map_err(|error| error.to_string())?;
    let mut hash = Sha256::new();
    let mut bytes = [0; 64 * 1024];
    loop {
        let count = file.read(&mut bytes).map_err(|error| error.to_string())?;
        if count == 0 {
            return Ok(hash.finalize().into());
        }
        hash.update(&bytes[..count]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(debug_assertions)]
    use crate::child::{Monitor, Outcome};
    use std::io::Write;
    use std::os::fd::IntoRawFd;
    use std::os::unix::fs::PermissionsExt;
    #[cfg(debug_assertions)]
    use std::time::Instant;

    const CONFIG: Config = Config {
        name: "testcompanion-reload",
        enable_env: "RETINUE_CLIENT_TEST_DEV_RELOAD",
        handoff_env: "RETINUE_CLIENT_TEST_RELOAD_FD",
        schema: 1,
        ready: "testcompanion-reload/1\n",
    };

    #[derive(Default, Serialize, Deserialize)]
    struct State {
        invalid: bool,
    }

    impl Checkpoint for State {
        fn validate(&self) -> Result<(), String> {
            if self.invalid {
                Err("invalid checkpoint".into())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn reload_defaults_to_development_builds_and_allows_opt_out() {
        for debug in [false, true] {
            assert_eq!(
                CONFIG
                    .enabled_for_build(debug, Err(std::env::VarError::NotPresent))
                    .unwrap(),
                debug
            );
            assert!(!CONFIG.enabled_for_build(debug, Ok("0".into())).unwrap());
            for invalid in ["", "true", "2"] {
                assert!(CONFIG.enabled_for_build(debug, Ok(invalid.into())).is_err());
            }
        }
        assert!(CONFIG.enabled_for_build(true, Ok("1".into())).unwrap());
        assert!(CONFIG.enabled_for_build(false, Ok("1".into())).is_err());
    }

    #[test]
    fn gate_requires_idle_noninteractive_live_runtime() {
        let gate = Gate::default();
        let stopping = AtomicBool::new(false);
        assert!(!gate.quiesce_if_idle(true, &stopping));
        gate.request();
        assert!(!gate.quiesce_if_idle(false, &stopping));
        gate.interaction(true);
        assert!(!gate.quiesce_if_idle(true, &stopping));
        gate.interaction(false);
        if !cfg!(debug_assertions) {
            assert!(!gate.quiesce_if_idle(true, &stopping));
            assert!(!gate.pending());
            assert!(!gate.approved());
            return;
        }
        assert!(gate.quiesce_if_idle(true, &stopping));
        assert!(stopping.load(Ordering::Acquire));
        assert!(gate.approved());
        assert!(!gate.try_interaction());
        gate.interaction(false);
        assert!(gate.approved());
        let exiting = Gate::default();
        exiting.request();
        assert!(!exiting.quiesce_if_idle(true, &stopping));
        assert!(!exiting.approved());
    }

    #[test]
    fn sealed_handoff_is_private_bounded_and_schema_checked() {
        let handoff = Handoff::new(
            &CONFIG,
            std::env::current_exe().unwrap(),
            None,
            State::default(),
        );
        let mut file = handoff.file(&CONFIG).unwrap();
        assert_eq!(fcntl_get_seals(&file).unwrap(), SEALS);
        assert!(file.write_all(b"corrupt").is_err());
        let restored = decode::<State>(&CONFIG, file, std::process::id()).unwrap();
        assert_eq!(restored.owner, handoff.owner);
        let mut data = serde_json::to_value(&handoff).unwrap();
        data["schema"] = 0.into();
        assert!(
            decode::<State>(
                &CONFIG,
                serde_json::to_vec(&data).unwrap().as_slice(),
                handoff.owner
            )
            .is_err()
        );
        assert!(
            decode::<State>(&CONFIG, handoff.file(&CONFIG).unwrap(), handoff.owner + 1).is_err()
        );
    }

    #[test]
    fn handoff_rejects_invalid_checkpoint_relative_path_unknown_fields_and_oversize_input() {
        let mut handoff = Handoff::new(
            &CONFIG,
            std::env::current_exe().unwrap(),
            None,
            State { invalid: true },
        );
        assert!(
            decode::<State>(&CONFIG, handoff.file(&CONFIG).unwrap(), handoff.owner)
                .err()
                .unwrap()
                .contains("invalid checkpoint")
        );
        handoff.state.invalid = false;
        handoff.installed = "relative".into();
        assert!(decode::<State>(&CONFIG, handoff.file(&CONFIG).unwrap(), handoff.owner).is_err());
        handoff.installed = std::env::current_exe().unwrap();
        let mut value = serde_json::to_value(&handoff).unwrap();
        value["extra"] = true.into();
        assert!(
            decode::<State>(
                &CONFIG,
                serde_json::to_vec(&value).unwrap().as_slice(),
                handoff.owner
            )
            .is_err()
        );
        assert!(
            decode::<State>(&CONFIG, std::io::repeat(b' '), handoff.owner)
                .err()
                .unwrap()
                .contains("too large")
        );
    }

    #[test]
    fn resume_checks_seals_owner_schema_policy_and_consumes_descriptor() {
        const CASE: &str = "RETINUE_CLIENT_TEST_RESUME_CASE";
        if let Ok(case) = std::env::var(CASE) {
            let mut handoff = Handoff::new(
                &CONFIG,
                std::env::current_exe().unwrap(),
                None,
                State::default(),
            );
            if case == "owner" {
                handoff.owner += 1;
            }
            if case == "schema" {
                handoff.schema += 1;
            }
            let mut file = File::from(
                memfd_create(CONFIG.name, MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING).unwrap(),
            );
            serde_json::to_writer(&mut file, &handoff).unwrap();
            file.rewind().unwrap();
            let seals = match case.as_str() {
                "unsealed" => SealFlags::empty(),
                "partial" => SealFlags::WRITE,
                _ => SEALS,
            };
            fcntl_add_seals(&file, seals).unwrap();
            let fd = file.into_raw_fd();
            // This subprocess runs only this test; no concurrent environment access.
            let result = unsafe {
                std::env::set_var(CONFIG.handoff_env, fd.to_string());
                if case == "disabled" {
                    std::env::set_var(CONFIG.enable_env, "0");
                }
                CONFIG.resume::<State>()
            };
            let expected = if !cfg!(debug_assertions) || case == "disabled" {
                Some("without reload enabled")
            } else {
                match case.as_str() {
                    "unsealed" | "partial" => Some("not a sealed handoff"),
                    "owner" | "schema" => Some("incompatible"),
                    "valid" => None,
                    _ => panic!("unknown test case"),
                }
            };
            if let Some(expected) = expected {
                assert!(result.err().unwrap().contains(expected));
            } else {
                assert!(result.unwrap().is_some());
            }
            let consumed = cfg!(debug_assertions)
                && !matches!(case.as_str(), "disabled" | "unsealed" | "partial");
            assert_eq!(std::env::var_os(CONFIG.handoff_env).is_none(), consumed);
            if consumed {
                assert!(!std::path::Path::new(&format!("/proc/self/fd/{fd}")).exists());
            } else {
                // Failed preflight leaves the descriptor owned by this caller.
                drop(unsafe { File::from_raw_fd(fd) });
            }
            return;
        }
        for case in [
            "valid", "unsealed", "partial", "owner", "schema", "disabled",
        ] {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command.args(["--exact", "reload::tests::resume_checks_seals_owner_schema_policy_and_consumes_descriptor"])
                .env(CASE, case).env_remove(CONFIG.enable_env).env_remove(CONFIG.handoff_env);
            let result = crate::process::output(command, Duration::from_secs(5), || true).unwrap();
            assert!(
                result.status.success(),
                "{case}: {} {}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }

    fn candidate(path: &std::path::Path) -> Candidate {
        let mut file = File::open(path).unwrap();
        Candidate {
            stamp: stamp(&file).unwrap(),
            hash: digest(&mut file).unwrap(),
            file,
        }
    }

    fn ready_script(path: &std::path::Path) {
        std::fs::write(path, "#!/bin/sh\nprintf 'testcompanion-reload/1\\n'\n").unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn pinned_probe_rejects_replaced_inode_and_changed_candidate_contents() {
        for replace_inode in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let installed = dir.path().join("companion");
            ready_script(&installed);
            let candidate = candidate(&installed);
            let handoff = Handoff::new(&CONFIG, installed.clone(), None, State::default());
            if replace_inode {
                let other = dir.path().join("other");
                std::fs::copy("/usr/bin/true", &other).unwrap();
                std::fs::rename(other, &installed).unwrap();
            } else {
                std::fs::OpenOptions::new()
                    .append(true)
                    .open(&installed)
                    .unwrap()
                    .write_all(b"# changed\n")
                    .unwrap();
            }
            // The pinned probe still emits readiness even after its path names /usr/bin/true.
            assert!(
                candidate
                    .replace(&CONFIG, &handoff, || true)
                    .unwrap_err()
                    .contains("changed during reload validation")
            );
            assert!(
                decode::<State>(&CONFIG, handoff.file(&CONFIG).unwrap(), handoff.owner).is_ok()
            );
        }
    }

    #[test]
    fn different_application_readiness_and_cancelled_validation_are_rejected() {
        let installed = PathBuf::from("/usr/bin/echo");
        let candidate = candidate(&installed);
        let handoff = Handoff::new(&CONFIG, installed, None, State::default());
        let other = Config {
            ready: "othercompanion-reload/1\n",
            ..CONFIG
        };
        let error = candidate.replace(&other, &handoff, || true).unwrap_err();
        assert!(error.contains("rejected"), "{error}");
        assert!(
            candidate
                .replace(&CONFIG, &handoff, || false)
                .unwrap_err()
                .contains("cancelled")
        );
    }

    #[test]
    #[cfg(debug_assertions)]
    fn watcher_returns_candidate_suppresses_rejected_stamp_and_joins_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let installed = dir.path().join("companion");
        std::fs::copy("/usr/bin/true", &installed).unwrap();
        let gate = Gate::default();
        let stopping = Arc::new(AtomicBool::new(false));
        let mut watch = Watch::start(
            &CONFIG,
            installed.clone(),
            gate.clone(),
            stopping.clone(),
            None,
            |_| {},
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !gate.pending() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(gate.pending());
        let candidate = watch.finish().unwrap().unwrap();
        assert!(watch.finish().unwrap().is_none());

        static WATCHING: AtomicBool = AtomicBool::new(false);
        let gate = Gate::default();
        let mut watch = Watch::start(
            &CONFIG,
            installed,
            gate.clone(),
            stopping,
            Some(candidate.stamp),
            |event| {
                if matches!(event, Event::Watching(_)) {
                    WATCHING.store(true, Ordering::Release);
                }
            },
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !WATCHING.load(Ordering::Acquire) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(WATCHING.load(Ordering::Acquire));
        thread::sleep(Duration::from_millis(150));
        assert!(watch.finish().unwrap().is_none());
        assert!(!gate.pending());

        let watch = Watch::start(
            &CONFIG,
            std::env::current_exe().unwrap(),
            Gate::default(),
            Arc::new(AtomicBool::new(false)),
            None,
            |_| {},
        )
        .unwrap();
        let cancel = watch.cancel.clone();
        drop(watch);
        assert!(cancel.load(Ordering::Acquire));
        assert_eq!(Arc::strong_count(&cancel), 1);
    }

    #[test]
    fn metadata_touch_is_ignored_but_replaced_install_identity_is_not() {
        let loaded = Stamp {
            device: 1,
            inode: 2,
            ..Stamp::default()
        };
        let mut candidate = Candidate {
            file: File::open("/dev/null").unwrap(),
            hash: [0; 32],
            stamp: loaded,
        };
        candidate.stamp.modified = (100, 0);
        assert!(candidate.change_reason(loaded, [0; 32]).is_none());
        candidate.stamp.inode = 3;
        assert_eq!(
            candidate.change_reason(loaded, [0; 32]),
            Some("installed identity changed")
        );
        candidate.stamp = loaded;
        candidate.hash = [1; 32];
        assert_eq!(
            candidate.change_reason(loaded, [0; 32]),
            Some("code changed")
        );
    }

    #[test]
    fn failed_exec_keeps_handoff_readable_and_does_not_leak_it_to_children() {
        let handoff = Handoff::new(
            &CONFIG,
            std::env::current_exe().unwrap(),
            None,
            State::default(),
        );
        let state = handoff.file(&CONFIG).unwrap();
        let candidate = Candidate {
            file: File::open("/dev/null").unwrap(),
            hash: [0; 32],
            stamp: Stamp::default(),
        };
        assert!(
            candidate
                .exec(&CONFIG, &state)
                .unwrap_err()
                .contains("replacement exec")
        );
        assert!(
            rustix::io::fcntl_getfd(&state)
                .unwrap()
                .contains(FdFlags::CLOEXEC)
        );
        assert!(decode::<State>(&CONFIG, state, handoff.owner).is_ok());
    }

    #[test]
    fn zero_exit_without_companion_readiness_is_not_a_valid_replacement() {
        let handoff = Handoff::new(
            &CONFIG,
            std::env::current_exe().unwrap(),
            None,
            State::default(),
        );
        let mut file = File::open("/usr/bin/true").unwrap();
        let candidate = Candidate {
            stamp: stamp(&file).unwrap(),
            hash: digest(&mut file).unwrap(),
            file,
        };
        assert!(
            candidate
                .replace(&CONFIG, &handoff, || true)
                .unwrap_err()
                .contains("rejected")
        );
    }

    #[test]
    #[cfg(debug_assertions)]
    fn exec_preserves_pid_and_child_parentage_and_resumes_reaping() {
        const CHILD: &str = "RETINUE_CLIENT_TEST_REEXEC";
        if std::env::var_os(CHILD).is_some() {
            let stopping = Arc::new(AtomicBool::new(false));
            // Only this test runs in the subprocess; no other environment users exist.
            if let Some(saved) = unsafe { CONFIG.resume::<State>() }.unwrap() {
                assert_eq!(saved.owner, std::process::id());
                assert!(std::env::var_os(CONFIG.handoff_env).is_none());
                let mut game =
                    Monitor::resume(saved.child.unwrap(), stopping.clone(), "test-game", |_| {})
                        .unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                while !stopping.load(Ordering::Acquire) && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(10));
                }
                assert!(stopping.load(Ordering::Acquire));
                let Some(Outcome::Exited(status)) = game.stop_monitor().unwrap() else {
                    panic!("child exit status was lost across exec");
                };
                assert_eq!(status.code(), Some(7));
                return;
            }
            let mut game = Monitor::launch(
                &["sh", "-c", "sleep 1; exit 7"],
                stopping,
                "test-game",
                |_| {},
            )
            .unwrap();
            let Some(Outcome::Running(child)) = game.stop_monitor().unwrap() else {
                panic!("live child was lost");
            };
            let handoff = Handoff::new(
                &CONFIG,
                std::env::current_exe().unwrap(),
                Some(child),
                State::default(),
            );
            assert!(handoff.child.is_some());
            let candidate = Candidate {
                file: File::open("/proc/self/exe").unwrap(),
                hash: [0; 32],
                stamp: Stamp::default(),
            };
            panic!(
                "exec failed: {}",
                candidate
                    .exec(&CONFIG, &handoff.file(&CONFIG).unwrap())
                    .unwrap_err()
            );
        }
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "reload::tests::exec_preserves_pid_and_child_parentage_and_resumes_reaping",
            ])
            .env(CHILD, "1")
            .env_remove(CONFIG.enable_env)
            .env_remove(CONFIG.handoff_env);
        let result = crate::process::output(command, Duration::from_secs(10), || true).unwrap();
        assert!(
            result.status.success(),
            "{} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
