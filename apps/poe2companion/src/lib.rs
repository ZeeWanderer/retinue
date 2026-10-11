use std::fs::{self, File};
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustix::fs::inotify;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

pub const MAX_EVENTS: usize = 128;
pub const READ_BUDGET: usize = 256 * 1024;
pub const MAX_LINE: usize = 16 * 1024;
const MAX_FRAME: usize = 128 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Event {
    /// Exclusive byte offset of the complete line, including its newline.
    pub offset: u64,
    pub timestamp_local: String,
    /// Windows client hint from the log, not the Linux game PID.
    pub client_pid: u32,
    pub tick_ms: u64,
    #[serde(flatten)]
    pub data: EventKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    AreaGenerated {
        area_id: String,
        area_level: u16,
        seed: u64,
    },
    AreaName {
        area_name: String,
    },
    CharacterLevel {
        character: String,
        character_class: String,
        character_level: u16,
    },
    Death {
        character: String,
    },
}

fn text(value: &str, limit: usize) -> bool {
    !value.is_empty()
        && value.len() <= limit
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn actor(value: &str) -> bool {
    text(value, 256) && !value.contains([':', '#', '@', '$', '&', '%', '<', '>', '|', '(', ')'])
}

/// Recognize only confirmed system records. Unknown lines never leave the reader.
pub fn parse_line(bytes: &[u8], offset: u64) -> Option<Event> {
    if offset == 0 {
        return None;
    }
    let line = std::str::from_utf8(bytes)
        .ok()?
        .trim_end_matches(['\r', '\n']);
    let time = line.get(..19)?;
    if !time.bytes().enumerate().all(|(i, b)| match i {
        4 | 7 => b == b'/',
        10 => b == b' ',
        13 | 16 => b == b':',
        _ => b.is_ascii_digit(),
    }) {
        return None;
    }
    let (tick, rest) = line.get(19..)?.strip_prefix(' ')?.split_once(' ')?;
    let tick_ms = tick.parse().ok()?;
    let (hash, rest) = rest.split_once(' ')?;
    if hash.is_empty() || hash.len() > 32 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let (prefix, body) = rest.strip_prefix('[')?.split_once("] ")?;
    let (severity, pid) = prefix.split_once(" Client ")?;
    let client_pid = pid.parse::<u32>().ok().filter(|pid| *pid > 0)?;
    let data = if severity == "DEBUG" {
        let (level, rest) = body
            .strip_prefix("Generating level ")?
            .split_once(" area \"")?;
        let (area_id, seed) = rest.split_once("\" with seed ")?;
        let area_level = level.parse::<u16>().ok().filter(|n| *n <= 1000)?;
        if !text(area_id, 256) {
            return None;
        }
        EventKind::AreaGenerated {
            area_id: area_id.into(),
            area_level,
            seed: seed.parse().ok()?,
        }
    } else if severity == "INFO" {
        if let Some(name) = body
            .strip_prefix("[SCENE] Set Source [")
            .and_then(|s| s.strip_suffix(']'))
        {
            let name = name.trim();
            if !text(name, 512) || matches!(name, "(null)" | "(unknown)") {
                return None;
            }
            EventKind::AreaName {
                area_name: name.into(),
            }
        } else {
            let message = body.strip_prefix(": ")?;
            if let Some(name) = message.strip_suffix(" has been slain.") {
                if !actor(name) {
                    return None;
                }
                EventKind::Death {
                    character: name.into(),
                }
            } else {
                let (identity, level) = message.rsplit_once(" is now level ")?;
                let (name, class) = identity.strip_suffix(')')?.split_once(" (")?;
                if !actor(name) || !text(class, 128) || class.contains(['(', ')']) {
                    return None;
                }
                let character_level = level
                    .parse::<u16>()
                    .ok()
                    .filter(|n| (1..=1000).contains(n))?;
                EventKind::CharacterLevel {
                    character: name.into(),
                    character_class: class.into(),
                    character_level,
                }
            }
        }
    } else {
        return None;
    };
    Some(Event {
        offset,
        timestamp_local: time.into(),
        client_pid,
        tick_ms,
        data,
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct Batch {
    pub source: String,
    pub generation: String,
    pub cursor: u64,
    pub events: Vec<Event>,
}

struct LogFile {
    file: File,
    identity: (u64, u64),
    generation: String,
    position: u64,
    cursor: u64,
    seen_len: u64,
    anchor: Vec<u8>,
    partial: Vec<u8>,
    skipping: bool,
    initial: bool,
}

impl LogFile {
    fn open(path: &Path, eof: bool, reset: u128) -> io::Result<Self> {
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
            .open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(io::Error::other("log must be a regular file"));
        }
        let position = if eof { metadata.len() } else { 0 };
        let mut result = Self {
            file,
            identity: (metadata.dev(), metadata.ino()),
            generation: format!("{}:{}:{reset}", metadata.dev(), metadata.ino()),
            position,
            cursor: position,
            seen_len: metadata.len(),
            anchor: Vec::new(),
            partial: Vec::new(),
            skipping: false,
            initial: true,
        };
        result.update_anchor()?;
        result.skipping = eof && position > 0 && result.anchor.last() != Some(&b'\n');
        if result.skipping {
            result.initial = false;
            result.cursor = 0;
        }
        Ok(result)
    }

    fn anchor_bytes(&self) -> io::Result<Vec<u8>> {
        let size = self.position.min(64) as usize;
        let mut bytes = vec![0; size];
        let count = self.file.read_at(&mut bytes, self.position - size as u64)?;
        bytes.truncate(count);
        Ok(bytes)
    }

    fn update_anchor(&mut self) -> io::Result<()> {
        self.anchor = self.anchor_bytes()?;
        Ok(())
    }

    fn refresh(&mut self, reset: u128) -> io::Result<()> {
        let length = self.file.metadata()?.len();
        if length < self.seen_len || self.anchor_bytes()? != self.anchor {
            if self.seen_len > self.position {
                return Err(io::Error::other(
                    "log was truncated with unread data; observation stopped to avoid silent loss",
                ));
            }
            self.position = 0;
            self.cursor = 0;
            self.partial.clear();
            self.anchor.clear();
            self.skipping = false;
            self.initial = true;
            self.generation = format!("{}:{}:{reset}", self.identity.0, self.identity.1);
        }
        self.seen_len = length;
        Ok(())
    }

    fn read_batch(&mut self, source: &str) -> io::Result<Option<Batch>> {
        // Publish the initial EOF watermark without replaying historical records.
        if self.initial {
            self.initial = false;
            return Ok(Some(Batch {
                source: source.into(),
                generation: self.generation.clone(),
                cursor: self.cursor,
                events: vec![],
            }));
        }
        let previous = self.cursor;
        let mut events = Vec::new();
        let mut budget = READ_BUDGET;
        let mut buffer = [0; 8192];
        'read: while budget > 0 && events.len() < MAX_EVENTS {
            let size = buffer.len().min(budget);
            let count = self.file.read_at(&mut buffer[..size], self.position)?;
            if count == 0 {
                break;
            }
            for &byte in &buffer[..count] {
                self.position += 1;
                budget -= 1;
                if byte == b'\n' {
                    self.cursor = self.position;
                    if !self.skipping
                        && let Some(event) = parse_line(&self.partial, self.cursor)
                    {
                        events.push(event);
                    }
                    self.partial.clear();
                    self.skipping = false;
                    if events.len() == MAX_EVENTS {
                        break 'read;
                    }
                } else if !self.skipping {
                    if self.partial.len() == MAX_LINE {
                        self.partial.clear();
                        self.skipping = true;
                    } else {
                        self.partial.push(byte);
                    }
                }
            }
        }
        self.update_anchor()?;
        Ok((self.cursor != previous).then(|| Batch {
            source: source.into(),
            generation: self.generation.clone(),
            cursor: self.cursor,
            events,
        }))
    }
}

/// A bounded reader retaining the old file handle until rotation has drained it.
pub struct Tailer {
    path: PathBuf,
    source: String,
    current: Option<LogFile>,
    replacement: Option<LogFile>,
    resets: u128,
}

pub fn canonical_log(path: &Path) -> io::Result<PathBuf> {
    match fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let name = path
                .file_name()
                .ok_or_else(|| io::Error::other("log needs a file name"))?;
            Ok(fs::canonicalize(parent)?.join(name))
        }
        Err(error) => Err(error),
    }
}

impl Tailer {
    pub fn open(path: &Path, eof: bool) -> io::Result<Self> {
        let path = canonical_log(path)?;
        let source = path
            .to_str()
            .filter(|s| s.len() <= 4096)
            .ok_or_else(|| io::Error::other("log path must be UTF-8 and at most 4096 bytes"))?
            .to_owned();
        let current = match LogFile::open(&path, eof, 0) {
            Ok(file) => Some(file),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        Ok(Self {
            path,
            source,
            current,
            replacement: None,
            resets: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn refresh(&mut self) -> io::Result<()> {
        self.resets += 1;
        if let Some(current) = self.current.as_mut() {
            current.refresh(self.resets)?;
        }
        let metadata = match fs::metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let identity = (metadata.dev(), metadata.ino());
        if self
            .current
            .as_ref()
            .is_some_and(|f| f.identity == identity)
        {
            return Ok(());
        }
        if let Some(replacement) = self.replacement.as_mut() {
            if replacement.identity != identity {
                return Err(io::Error::other(
                    "multiple rotations while backpressured; unread generation may be lost",
                ));
            }
            replacement.refresh(self.resets)?;
        } else {
            let file = LogFile::open(&self.path, false, self.resets)?;
            if self.current.is_none() {
                self.current = Some(file);
            } else {
                self.replacement = Some(file);
            }
        }
        Ok(())
    }

    pub fn next_batch(&mut self) -> io::Result<Option<Batch>> {
        self.refresh()?;
        if let Some(current) = self.current.as_mut() {
            if let Some(batch) = current.read_batch(&self.source)? {
                return Ok(Some(batch));
            }
            if current.position >= current.seen_len && self.replacement.is_some() {
                if !current.partial.is_empty() {
                    eprintln!("poe2companion: ignoring incomplete final line in rotated log");
                }
                self.current = self.replacement.take();
                return self.current.as_mut().unwrap().read_batch(&self.source);
            }
        }
        Ok(None)
    }

    pub fn has_more(&self) -> bool {
        self.replacement.is_some()
            || self
                .current
                .as_ref()
                .is_some_and(|f| f.initial || f.position < f.seen_len)
    }
}

pub struct Watcher {
    fd: tokio::io::unix::AsyncFd<OwnedFd>,
    name: Vec<u8>,
}

impl Watcher {
    pub fn new(path: &Path) -> io::Result<Self> {
        let fd = inotify::init(inotify::CreateFlags::CLOEXEC | inotify::CreateFlags::NONBLOCK)?;
        inotify::add_watch(
            &fd,
            path.parent()
                .ok_or_else(|| io::Error::other("log has no parent"))?,
            inotify::WatchFlags::MODIFY
                | inotify::WatchFlags::CLOSE_WRITE
                | inotify::WatchFlags::CREATE
                | inotify::WatchFlags::MOVED_TO
                | inotify::WatchFlags::MOVED_FROM
                | inotify::WatchFlags::DELETE
                | inotify::WatchFlags::ATTRIB
                | inotify::WatchFlags::DELETE_SELF
                | inotify::WatchFlags::MOVE_SELF,
        )?;
        Ok(Self {
            fd: tokio::io::unix::AsyncFd::new(fd)?,
            name: path
                .file_name()
                .ok_or_else(|| io::Error::other("log has no name"))?
                .as_bytes()
                .to_vec(),
        })
    }

    pub async fn changed(&self) -> io::Result<()> {
        loop {
            let mut ready = self.fd.readable().await?;
            let result = ready.try_io(|fd| {
                let mut buffer = [MaybeUninit::uninit(); 8192];
                let mut reader = inotify::Reader::new(fd.get_ref(), &mut buffer);
                let mut changed = false;
                loop {
                    let event = reader.next().map_err(io::Error::from)?;
                    if event.events().intersects(
                        inotify::ReadFlags::QUEUE_OVERFLOW
                            | inotify::ReadFlags::DELETE_SELF
                            | inotify::ReadFlags::MOVE_SELF
                            | inotify::ReadFlags::IGNORED,
                    ) {
                        return Err(io::Error::other(
                            "log watch lost events or directory; observation stopped",
                        ));
                    }
                    changed |= event
                        .file_name()
                        .is_some_and(|name| name.to_bytes() == self.name);
                    if reader.is_buffer_empty() {
                        return Ok(changed);
                    }
                }
            });
            match result {
                Ok(Ok(true)) => return Ok(()),
                Ok(Err(error)) => return Err(error),
                _ => {}
            }
        }
    }
}

pub fn locate_log() -> io::Result<PathBuf> {
    let steam = steamlocate::locate().map_err(io::Error::other)?;
    let (app, library) = steam
        .find_app(2694490)
        .map_err(io::Error::other)?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Path of Exile 2 installation not found; use --log PATH",
            )
        })?;
    Ok(library.resolve_app_dir(&app).join("logs/Client.txt"))
}

pub struct Connection {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
}

impl Connection {
    pub async fn connect(path: &Path, mode: &str) -> io::Result<Self> {
        let stream =
            tokio::time::timeout(Duration::from_secs(2), UnixStream::connect(path)).await??;
        let (reader, writer) = stream.into_split();
        let mut connection = Self {
            reader: BufReader::new(reader),
            writer,
        };
        let hello = json!({"op":"hello", "id":1, "game":"poe2", "envelope":2,
            "interfaces":{"session":1}, "features":[], "client":"poe2companion",
            "version":env!("CARGO_PKG_VERSION"), "pid":std::process::id(), "mode":mode});
        let reply =
            tokio::time::timeout(Duration::from_secs(3), connection.request(&hello)).await??;
        if reply["ok"] != true
            || reply["compatible"] != true
            || reply["envelope"] != 2
            || reply["interfaces"]["session"] != 1
            || reply["game"] != "poe2"
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "daemon contract mismatch: need envelope 2, poe2 session 1",
            ));
        }
        Ok(connection)
    }

    async fn frame(&mut self) -> io::Result<Value> {
        let mut frame = Vec::new();
        loop {
            let available = self.reader.fill_buf().await?;
            if available.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "daemon disconnected",
                ));
            }
            let newline = available.iter().position(|b| *b == b'\n');
            let count = newline.map_or(available.len(), |n| n + 1);
            if frame.len() + count > MAX_FRAME {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "daemon frame exceeds limit",
                ));
            }
            frame.extend_from_slice(&available[..count]);
            self.reader.consume(count);
            if newline.is_some() {
                return serde_json::from_slice(&frame).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid daemon JSON frame")
                });
            }
        }
    }

    async fn request(&mut self, request: &Value) -> io::Result<Value> {
        let mut bytes = serde_json::to_vec(request)?;
        bytes.push(b'\n');
        tokio::time::timeout(Duration::from_secs(5), self.writer.write_all(&bytes)).await??;
        let reply = self.frame().await?;
        if reply.get("id") != request.get("id") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "daemon reply ID does not match outstanding request",
            ));
        }
        Ok(reply)
    }

    pub async fn publish(&mut self, id: u64, batch: &Batch) -> io::Result<()> {
        let request = json!({"op":"publish", "id":id, "dataset":"session", "data":batch});
        let reply = tokio::time::timeout(REQUEST_TIMEOUT, self.request(&request)).await??;
        if reply["ok"] != true {
            if matches!(
                reply["error"].as_str(),
                Some("invalid_session_batch" | "unsupported_poe2_request" | "unauthorized")
            ) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "daemon rejected the session contract",
                ));
            }
            return Err(io::Error::other(
                "daemon rejected session batch; retaining pending batch",
            ));
        }
        if reply["dataset"] != "session" || reply["data"]["revision"].as_u64().is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid session acknowledgement",
            ));
        }
        Ok(())
    }
}

pub struct Publisher {
    socket: PathBuf,
    mode: String,
    connection: Option<Connection>,
    ensured: bool,
}

impl Publisher {
    pub fn new(socket: PathBuf, mode: &str) -> Self {
        Self {
            socket,
            mode: mode.into(),
            connection: None,
            ensured: false,
        }
    }

    /// Retain the exact ID and payload until a correlated acknowledgement arrives.
    pub async fn publish(&mut self, id: u64, batch: &Batch) -> io::Result<()> {
        let mut delay = Duration::from_millis(500);
        loop {
            let result = async {
                if self.connection.is_none() {
                    self.connection = Some(Connection::connect(&self.socket, &self.mode).await?);
                }
                self.connection.as_mut().unwrap().publish(id, batch).await
            }
            .await;
            match result {
                Ok(()) => {
                    self.ensured = false;
                    return Ok(());
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::Unsupported
                            | io::ErrorKind::InvalidData
                            | io::ErrorKind::InvalidInput
                    ) =>
                {
                    return Err(error);
                }
                Err(error) => {
                    self.connection = None;
                    eprintln!(
                        "poe2companion: {error}; retrying batch {id} in {} ms",
                        delay.as_millis()
                    );
                    if !self.ensured {
                        match ensure_daemon().await {
                            Ok(()) => self.ensured = true,
                            Err(error) => {
                                eprintln!("poe2companion: daemon ensure failed: {error}");
                            }
                        }
                    }
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(Duration::from_secs(10));
                }
            }
        }
    }
}

async fn ensure_daemon() -> io::Result<()> {
    let executable = std::env::current_exe()?;
    let command = std::env::var_os("RETINUE_COMMAND")
        .map(PathBuf::from)
        .or_else(|| {
            executable
                .ancestors()
                .map(|p| p.join("retinue"))
                .find(|p| p.is_file())
        })
        .unwrap_or_else(|| PathBuf::from("retinue"));
    let mut process = tokio::process::Command::new(command);
    process
        .args(["daemon", "ensure"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    for key in [
        "LD_PRELOAD",
        "LD_LIBRARY_PATH",
        "STEAM_RUNTIME",
        "STEAM_RUNTIME_LIBRARY_PATH",
    ] {
        process.env_remove(key);
    }
    let status = tokio::time::timeout(Duration::from_secs(30), process.status()).await??;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "retinue daemon ensure exited with {status}"
        )))
    }
}
