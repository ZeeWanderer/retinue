use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use super::mailbox::Mailbox;

mod subscribers;
use subscribers::{Acquisition, Hub};

const RECORD_HEADER_SIZE: usize = 8;
const MAX_MESSAGE_SIZE: usize = 4092;
const RUNTIME_ENVIRONMENT: &[&str] = &[
    "LD_LIBRARY_PATH",
    "PATH",
    "WINEDLLPATH",
    "WINEESYNC",
    "WINEFSYNC",
    "WINELOADER",
    "WINENTSYNC",
    "WINESERVER",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Runtime {
    game_pid: u32,
    prefix: PathBuf,
    wine: PathBuf,
    environment: BTreeMap<String, String>,
}

impl Runtime {
    pub fn discover(
        game_pid: u32,
        process_dir: &Path,
        environment: &BTreeMap<String, String>,
        compat_data: &Path,
    ) -> Option<Self> {
        let prefix = if compat_data.file_name().is_some_and(|name| name == "pfx") {
            compat_data.to_owned()
        } else {
            compat_data.join("pfx")
        };
        let wine = wine_candidates(process_dir, environment)
            .into_iter()
            .find(|path| path.is_file())?;
        Some(Self {
            game_pid,
            prefix,
            wine,
            environment: environment.clone(),
        })
    }

    pub fn game_pid(&self) -> u32 {
        self.game_pid
    }

    pub fn prefix(&self) -> &Path {
        &self.prefix
    }

    pub fn helper_command(&self, helper: &Path) -> Command {
        let mut command = Command::new(&self.wine);
        command
            .arg(helper)
            .env("WINEPREFIX", &self.prefix)
            .env("WINEDEBUG", "-all")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for key in RUNTIME_ENVIRONMENT {
            if let Some(value) = self.environment.get(*key) {
                command.env(key, value);
            }
        }
        command
    }
}

#[derive(Debug)]
pub enum Event {
    Record {
        game_pid: u32,
        sender_pid: u32,
        message: String,
        observed_at: Instant,
        observed_at_unix_ms: u128,
    },
    Stopped {
        game_pid: u32,
        reason: String,
    },
}

pub fn inbox() -> Arc<Mailbox<Event>> {
    Arc::new(Mailbox::new(256, 1024 * 1024))
}

impl Event {
    fn bytes(&self) -> usize {
        size_of::<Self>()
            + match self {
                Self::Record { message, .. } => message.capacity(),
                Self::Stopped { reason, .. } => reason.capacity(),
            }
    }
}

pub struct Bridge {
    game_pid: u32,
    child: Option<Child>,
    subscription: Option<UnixStream>,
    running: Arc<AtomicBool>,
    reader: Option<thread::JoinHandle<()>>,
}

impl Bridge {
    pub fn start(runtime: &Runtime, events: Arc<Mailbox<Event>>) -> Result<Self, String> {
        let acquisition = Hub::acquire(runtime.prefix())?;
        if let Acquisition::Subscriber(stream) = acquisition {
            let input = stream.try_clone().map_err(|e| e.to_string())?;
            let running = Arc::new(AtomicBool::new(true));
            let reader = forward(input, runtime.game_pid, events, None, running.clone());
            return Ok(Self {
                game_pid: runtime.game_pid,
                child: None,
                subscription: Some(stream),
                running,
                reader: Some(reader),
            });
        }
        let Acquisition::Owner(hub) = acquisition else {
            unreachable!()
        };
        let helper = helper_path().ok_or_else(|| {
            "wfcompanion DBWIN helper not found; rebuild companion with make companion".to_owned()
        })?;
        let mut command = runtime.helper_command(&helper);
        let mut child = command
            .spawn()
            .map_err(|error| format!("could not start Proton DBWIN helper: {error}"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "DBWIN helper stdout was not piped".to_owned())?;
        let running = Arc::new(AtomicBool::new(true));
        let reader = forward(stdout, runtime.game_pid, events, Some(hub), running.clone());
        Ok(Self {
            game_pid: runtime.game_pid,
            child: Some(child),
            subscription: None,
            running,
            reader: Some(reader),
        })
    }

    pub fn game_pid(&self) -> u32 {
        self.game_pid
    }

    pub fn is_running(&mut self) -> bool {
        self.running.load(Ordering::Relaxed)
            && self
                .child
                .as_mut()
                .is_none_or(|child| matches!(child.try_wait(), Ok(None)))
    }
}

fn forward(
    input: impl Read + Send + 'static,
    game_pid: u32,
    events: Arc<Mailbox<Event>>,
    mut hub: Option<Hub>,
    running: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut input = io::BufReader::new(input);
        loop {
            match read_record(&mut input) {
                Ok(Some((sender_pid, message))) => {
                    let observed_at = Instant::now();
                    let observed_at_unix_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis();
                    if let Some(hub) = &mut hub {
                        hub.publish(sender_pid, &message);
                    }
                    let event = Event::Record {
                        game_pid,
                        sender_pid,
                        observed_at,
                        observed_at_unix_ms,
                        message: String::from_utf8_lossy(&message).into_owned(),
                    };
                    let bytes = event.bytes();
                    if events.send(event, bytes).is_err() {
                        break;
                    }
                }
                Ok(None) => {
                    let event = Event::Stopped {
                        game_pid,
                        reason: "DBWIN helper closed its output".to_owned(),
                    };
                    let bytes = event.bytes();
                    let _ = events.send(event, bytes);
                    break;
                }
                Err(error) => {
                    let event = Event::Stopped {
                        game_pid,
                        reason: format!("DBWIN helper protocol failed: {error}"),
                    };
                    let bytes = event.bytes();
                    let _ = events.send(event, bytes);
                    break;
                }
            }
        }
        running.store(false, Ordering::Relaxed);
    })
}

impl Drop for Bridge {
    fn drop(&mut self) {
        if let Some(stream) = &self.subscription {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn read_record(input: &mut impl Read) -> io::Result<Option<(u32, Vec<u8>)>> {
    let mut header = [0_u8; RECORD_HEADER_SIZE];
    match input.read_exact(&mut header) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let sender_pid = u32::from_le_bytes(header[0..4].try_into().unwrap());
    let length = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
    if length > MAX_MESSAGE_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message length {length} exceeds {MAX_MESSAGE_SIZE}"),
        ));
    }
    let mut message = vec![0_u8; length];
    input.read_exact(&mut message)?;
    Ok(Some((sender_pid, message)))
}

fn helper_path() -> Option<PathBuf> {
    helper_candidates(
        std::env::var_os("WFCOMPANION_DEBUG_BRIDGE"),
        retinue_client::executable_path(),
        option_env!("WFCOMPANION_BUILD_DEBUG_BRIDGE"),
    )
    .into_iter()
    .find(|path| path.is_file())
}

fn helper_candidates(
    configured: Option<OsString>,
    current_exe: Option<&Path>,
    build_path: Option<&str>,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(configured) = configured.filter(|path| !path.is_empty()) {
        candidates.push(PathBuf::from(configured));
    }
    if let Some(bin_dir) = current_exe.and_then(Path::parent) {
        candidates.push(bin_dir.join("../libexec/wfcompanion-debug-output.exe"));
    }
    if let Some(build_path) = build_path.filter(|path| !path.is_empty()) {
        candidates.push(PathBuf::from(build_path));
    }
    candidates
}

fn wine_candidates(process_dir: &Path, environment: &BTreeMap<String, String>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(loader) = environment.get("WINELOADER") {
        push_unique(&mut candidates, PathBuf::from(loader));
    }
    if let Some(tool_paths) = environment.get("STEAM_COMPAT_TOOL_PATHS") {
        for tool in tool_paths.split(':').filter(|path| !path.is_empty()) {
            push_unique(
                &mut candidates,
                PathBuf::from(tool).join("files/bin/wine64"),
            );
        }
    }
    if let Ok(executable) = fs::read_link(process_dir.join("exe")) {
        if executable
            .file_name()
            .is_some_and(|name| name == "wine64-preloader")
        {
            push_unique(&mut candidates, executable.with_file_name("wine64"));
        }
        push_unique(&mut candidates, executable);
    }
    if let Ok(maps) = fs::read_to_string(process_dir.join("maps")) {
        for path in maps
            .lines()
            .filter_map(|line| line.split_whitespace().last())
        {
            if path.ends_with("/files/bin/wine64") {
                push_unique(&mut candidates, PathBuf::from(path));
            }
        }
    }
    candidates
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !path.as_os_str().is_empty() && !paths.contains(&path) {
        paths.push(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn slow_consumer_is_bounded_and_keeps_acquisition_time() {
        let mut bytes = Vec::new();
        for pid in 0_u32..1000 {
            bytes.extend_from_slice(&pid.to_le_bytes());
            bytes.extend_from_slice(&4_u32.to_le_bytes());
            bytes.extend_from_slice(b"test");
        }
        let events = inbox();
        let running = Arc::new(AtomicBool::new(true));
        forward(
            Cursor::new(bytes),
            42,
            events.clone(),
            None,
            running.clone(),
        )
        .join()
        .unwrap();
        let finished_reading = Instant::now();
        assert!(!running.load(Ordering::Acquire));
        assert_eq!(events.stats().items, 256);
        assert_eq!(events.stats().dropped_items, 745);
        for expected_pid in 745..1000 {
            let Event::Record {
                sender_pid,
                observed_at,
                observed_at_unix_ms,
                ..
            } = events.recv_timeout(std::time::Duration::ZERO).unwrap()
            else {
                panic!("record");
            };
            assert_eq!(sender_pid, expected_pid);
            assert!(observed_at <= finished_reading);
            assert!(observed_at_unix_ms > 0);
        }
        assert!(matches!(
            events.recv_timeout(std::time::Duration::ZERO).unwrap(),
            Event::Stopped { .. }
        ));
    }

    #[test]
    fn drop_reaps_idle_helper_and_joins_reader() {
        let mut child = Command::new("sleep")
            .arg("60")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let events = inbox();
        let running = Arc::new(AtomicBool::new(true));
        let reader = forward(
            child.stdout.take().unwrap(),
            0,
            events,
            None,
            running.clone(),
        );
        drop(Bridge {
            game_pid: 0,
            child: Some(child),
            subscription: None,
            running: running.clone(),
            reader: Some(reader),
        });
        assert!(!running.load(Ordering::Relaxed));
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }

    #[test]
    fn drop_disconnects_idle_subscriber_and_joins_reader() {
        let (input, _writer) = UnixStream::pair().unwrap();
        let events = inbox();
        let running = Arc::new(AtomicBool::new(true));
        let reader = forward(input.try_clone().unwrap(), 0, events, None, running.clone());
        drop(Bridge {
            game_pid: 0,
            child: None,
            subscription: Some(input),
            running: running.clone(),
            reader: Some(reader),
        });
        assert!(!running.load(Ordering::Relaxed));
    }

    #[test]
    fn decodes_bridge_record() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&42_u32.to_le_bytes());
        bytes.extend_from_slice(&11_u32.to_le_bytes());
        bytes.extend_from_slice(b"Got rewards");
        assert_eq!(
            read_record(&mut Cursor::new(bytes)).unwrap(),
            Some((42, b"Got rewards".to_vec()))
        );
    }

    #[test]
    fn rejects_oversized_bridge_record() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&42_u32.to_le_bytes());
        bytes.extend_from_slice(&5000_u32.to_le_bytes());
        assert_eq!(
            read_record(&mut Cursor::new(bytes)).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn finds_proton_wine_from_tool_path() {
        let environment = BTreeMap::from([(
            "STEAM_COMPAT_TOOL_PATHS".to_owned(),
            "/steam/Proton 10.0:/steam/runtime".to_owned(),
        )]);
        assert_eq!(
            wine_candidates(Path::new("/missing"), &environment)[0],
            PathBuf::from("/steam/Proton 10.0/files/bin/wine64")
        );
    }

    #[test]
    fn packaged_helper_precedes_build_fallback() {
        assert_eq!(
            helper_candidates(
                None,
                Some(Path::new("/package/bin/wfcompanion")),
                Some("/repo/prod/libexec/helper.exe"),
            ),
            vec![
                PathBuf::from("/package/bin/../libexec/wfcompanion-debug-output.exe"),
                PathBuf::from("/repo/prod/libexec/helper.exe"),
            ]
        );
    }
}
