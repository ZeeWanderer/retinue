use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use retinue_client::paths::{Base, Game, Paths, create_private_dir_all};

const MAX_LOG_BYTES: u64 = 1024 * 1024;
const DUPLICATE_WINDOW: Duration = Duration::from_secs(60);
const MAX_PENDING: usize = 128;
// Leave room for warnings/errors when informational traffic fills the queue.
const MAX_PENDING_INFO: usize = 96;
const MAX_MESSAGE_BYTES: usize = 8192;
const MAX_EVENT_BYTES: usize = 128;
const MAX_RECENT: usize = 256;

struct Sink {
    path: PathBuf,
    recent: BTreeMap<String, SystemTime>,
}

#[derive(Default)]
struct Counters {
    pending_info: AtomicUsize,
    dropped_info: AtomicU64,
    dropped_important: AtomicU64,
}

struct Record {
    time: SystemTime,
    level: &'static str,
    event: String,
    message: String,
}

#[derive(Clone)]
struct Producer {
    sender: mpsc::SyncSender<Record>,
    counters: Arc<Counters>,
}

static ACTIVE: Mutex<Option<Producer>> = Mutex::new(None);

pub(crate) struct Writer {
    worker: Option<JoinHandle<()>>,
}

impl Writer {
    pub(crate) fn start() -> Result<Self, String> {
        let mut active = ACTIVE.lock().unwrap();
        if active.is_some() {
            return Err("incident writer is already running".into());
        }
        let (producer, receiver) = channel();
        let counters = producer.counters.clone();
        let path = log_path();
        let worker = thread::Builder::new()
            .name("wfcompanion-log".into())
            .spawn(move || drain(path, receiver, counters))
            .map_err(|error| format!("could not start incident writer: {error}"))?;
        *active = Some(producer);
        Ok(Self {
            worker: Some(worker),
        })
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        ACTIVE.lock().unwrap().take();
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            eprintln!("wfcompanion: incident writer panicked");
        }
    }
}

fn channel() -> (Producer, mpsc::Receiver<Record>) {
    let (sender, receiver) = mpsc::sync_channel(MAX_PENDING);
    (
        Producer {
            sender,
            counters: Arc::new(Counters::default()),
        },
        receiver,
    )
}

impl Producer {
    fn send(&self, level: &'static str, event: &str, message: &str) {
        let info = level == "info";
        if info
            && self
                .counters
                .pending_info
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |pending| {
                    (pending < MAX_PENDING_INFO).then_some(pending + 1)
                })
                .is_err()
        {
            self.counters.dropped_info.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let record = Record {
            time: SystemTime::now(),
            level,
            event: bounded(event, MAX_EVENT_BYTES),
            message: bounded(message, MAX_MESSAGE_BYTES),
        };
        if self.sender.try_send(record).is_err() {
            let counter = if info {
                self.counters.pending_info.fetch_sub(1, Ordering::Relaxed);
                &self.counters.dropped_info
            } else {
                &self.counters.dropped_important
            };
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn bounded(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit - 3;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

fn drain(path: PathBuf, receiver: mpsc::Receiver<Record>, counters: Arc<Counters>) {
    let mut sink = Sink {
        path,
        recent: BTreeMap::new(),
    };
    let mut next_report = Instant::now();
    let mut next_error = Instant::now();
    loop {
        let received = receiver.recv_timeout(Duration::from_secs(1));
        if let Ok(record) = &received {
            if record.level == "info" {
                counters.pending_info.fetch_sub(1, Ordering::Relaxed);
            }
            if let Err(error) = sink.write(record)
                && Instant::now() >= next_error
            {
                eprintln!(
                    "wfcompanion: could not write {}: {error}",
                    sink.path.display()
                );
                next_error = Instant::now() + Duration::from_secs(60);
            }
        }
        let done = matches!(received, Err(mpsc::RecvTimeoutError::Disconnected));
        if done || Instant::now() >= next_report {
            let info = counters.dropped_info.swap(0, Ordering::Relaxed);
            let important = counters.dropped_important.swap(0, Ordering::Relaxed);
            if info != 0 || important != 0 {
                let record = Record {
                    time: SystemTime::now(),
                    level: "warn",
                    event: "log.queue_overflow".into(),
                    message: format!("dropped_info={info} dropped_important={important}"),
                };
                if let Err(error) = sink.append(&record) {
                    eprintln!(
                        "wfcompanion: {} (log write failed: {error})",
                        record.message
                    );
                }
            }
            next_report = Instant::now() + Duration::from_secs(1);
        }
        if done {
            break;
        }
    }
}

pub(crate) fn info(event: &str, message: impl AsRef<str>) {
    write("info", event, message.as_ref());
}

pub(crate) fn warn(event: &str, message: impl AsRef<str>) {
    write("warn", event, message.as_ref());
}

pub(crate) fn error(event: &str, message: impl AsRef<str>) {
    write("error", event, message.as_ref());
}

pub(crate) fn log_path() -> PathBuf {
    log_path_from(
        std::env::var_os("WFCOMPANION_LOG").as_deref(),
        &Paths::from_env(),
    )
}

fn log_path_from(override_path: Option<&OsStr>, paths: &Paths) -> PathBuf {
    if let Some(path) = override_path {
        return PathBuf::from(path);
    }
    paths
        .game_directory(Base::State, Game::Warframe)
        .join("companion.log")
}

pub(crate) fn print_recent(limit: usize) -> Result<(), String> {
    let path = log_path();
    println!("{}", path.display());
    if !path.exists() {
        return Ok(());
    }
    let mut contents = String::new();
    fs::File::open(&path)
        .and_then(|mut file| file.read_to_string(&mut contents))
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    for line in tail_lines(&contents, limit) {
        println!("{line}");
    }
    Ok(())
}

fn write(level: &'static str, event: &str, message: &str) {
    let producer = ACTIVE.lock().unwrap().clone();
    if let Some(producer) = producer {
        producer.send(level, event, message);
    } else if level != "info" {
        eprintln!("wfcompanion: {event}: {message}");
    }
}

impl Sink {
    fn write(&mut self, record: &Record) -> io::Result<()> {
        self.recent.retain(|_, time| {
            record.time.duration_since(*time).unwrap_or_default() < DUPLICATE_WINDOW
        });
        let key = format!("{}\0{}\0{}", record.level, record.event, record.message);
        if self.recent.contains_key(&key) {
            return Ok(());
        }
        self.append(record)?;
        if self.recent.len() >= MAX_RECENT {
            let oldest = self
                .recent
                .iter()
                .min_by_key(|(_, time)| **time)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                self.recent.remove(&oldest);
            }
        }
        self.recent.insert(key, record.time);
        Ok(())
    }

    fn append(&self, record: &Record) -> io::Result<()> {
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            create_private_dir_all(parent)?;
        }
        rotate_if_needed(&self.path)?;
        let line = serde_json::json!({
            "timestamp_ms": record.time.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            "pid": std::process::id(),
            "level": record.level,
            "event": record.event,
            "message": record.message,
        });
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&self.path)?;
        writeln!(file, "{line}")
    }
}

fn rotate_if_needed(path: &Path) -> io::Result<()> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.len() >= MAX_LOG_BYTES => {
            fs::rename(path, path.with_extension("log.1"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn tail_lines(contents: &str, limit: usize) -> Vec<&str> {
    let lines: Vec<_> = contents.lines().collect();
    let start = lines.len().saturating_sub(limit);
    lines[start..].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_path_uses_game_state_and_preserves_override() {
        for (xdg, expected) in [
            (
                Some("/state"),
                "/state/retinue/games/warframe/companion.log",
            ),
            (
                None,
                "/home/test/.local/state/retinue/games/warframe/companion.log",
            ),
            (
                Some(""),
                "/home/test/.local/state/retinue/games/warframe/companion.log",
            ),
            (
                Some("relative"),
                "/home/test/.local/state/retinue/games/warframe/companion.log",
            ),
        ] {
            let paths = Paths::from_lookup(|key| match key {
                "XDG_STATE_HOME" => xdg.map(Into::into),
                "HOME" => Some("/home/test".into()),
                _ => None,
            });
            assert_eq!(log_path_from(None, &paths), PathBuf::from(expected));
            for path in ["/custom.log", "relative.log", ""] {
                assert_eq!(
                    log_path_from(Some(OsStr::new(path)), &paths),
                    PathBuf::from(path)
                );
            }
        }
    }

    fn test_directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "wfcompanion-log-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn stalled_writer_reserves_capacity_and_accounts_for_overflow() {
        let (producer, receiver) = channel();
        for _ in 0..MAX_PENDING {
            producer.send("info", "test", "info");
        }
        for _ in 0..(MAX_PENDING - MAX_PENDING_INFO) {
            producer.send("warn", "test", "warn");
        }
        producer.send("error", "test", "error");
        let records: Vec<_> = receiver.try_iter().collect();
        assert_eq!(records.len(), MAX_PENDING);
        assert_eq!(
            records
                .iter()
                .filter(|record| record.level == "info")
                .count(),
            MAX_PENDING_INFO
        );
        assert_eq!(producer.counters.dropped_info.load(Ordering::Relaxed), 32);
        assert_eq!(
            producer.counters.dropped_important.load(Ordering::Relaxed),
            1
        );
    }

    #[test]
    fn record_bounds_preserve_utf8() {
        let (producer, receiver) = channel();
        let text = "\u{20ac}".repeat(MAX_MESSAGE_BYTES);
        producer.send("warn", &text, &text);
        let record = receiver.recv().unwrap();
        assert!(record.message.len() <= MAX_MESSAGE_BYTES);
        assert!(record.event.len() <= MAX_EVENT_BYTES);
        assert!(record.message.ends_with("..."));
        assert_eq!(bounded("short", 10), "short");
    }

    #[test]
    fn shutdown_drains_accepted_records_and_reports_drops() {
        let directory = test_directory();
        let path = directory.join("wfcompanion.log");
        let (producer, receiver) = channel();
        for _ in 0..(MAX_PENDING_INFO + 1) {
            producer.send("info", "test", "repeated");
        }
        producer.send("error", "terminal", "failed");
        let counters = producer.counters.clone();
        let output = path.clone();
        let worker = thread::spawn(move || drain(output, receiver, counters));
        drop(producer);
        worker.join().unwrap();
        let records: Vec<serde_json::Value> = fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(
            records
                .iter()
                .filter(|record| record["event"] == "test")
                .count(),
            1
        );
        assert!(records.iter().any(|record| record["event"] == "terminal"));
        assert!(
            records
                .iter()
                .any(|record| record["event"] == "log.queue_overflow"
                    && record["message"] == "dropped_info=1 dropped_important=0")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_write_is_not_deduplicated_on_retry_and_rotation_replaces_previous() {
        let directory = test_directory();
        let parent = directory.join("parent");
        fs::write(&parent, "not a directory").unwrap();
        let mut sink = Sink {
            path: parent.join("wfcompanion.log"),
            recent: BTreeMap::new(),
        };
        let record = Record {
            time: SystemTime::now(),
            level: "warn",
            event: "test".into(),
            message: "failed".into(),
        };
        assert!(sink.write(&record).is_err());
        assert!(sink.recent.is_empty());
        fs::remove_file(&parent).unwrap();
        sink.write(&record).unwrap();
        fs::File::options()
            .write(true)
            .open(&sink.path)
            .unwrap()
            .set_len(MAX_LOG_BYTES)
            .unwrap();
        fs::write(sink.path.with_extension("log.1"), "old rotation").unwrap();
        sink.append(&record).unwrap();
        assert_eq!(
            fs::metadata(sink.path.with_extension("log.1"))
                .unwrap()
                .len(),
            MAX_LOG_BYTES
        );
        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&sink.path).unwrap()).unwrap();
        assert_eq!(value["event"], "test");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn tail_is_bounded() {
        assert_eq!(tail_lines("one\ntwo\nthree\n", 2), vec!["two", "three"]);
        assert_eq!(tail_lines("one\n", 2), vec!["one"]);
    }

    #[test]
    fn rotation_path_keeps_log_suffix() {
        assert_eq!(
            Path::new("/tmp/wfcompanion.log").with_extension("log.1"),
            Path::new("/tmp/wfcompanion.log.1")
        );
    }
}
