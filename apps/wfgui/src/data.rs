use retinue_client::{
    local_protocol as protocol,
    paths::{Base, Game, Paths},
    transport::JsonChannel,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Usage {
    pub cache_root: PathBuf,
    pub bytes: u64,
    pub objects: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub source: Result<Usage, String>,
    pub rendered: Result<Usage, String>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            source: Err("Connecting...".into()),
            rendered: Err("Measuring...".into()),
        }
    }
}

impl Snapshot {
    pub fn fixture() -> Self {
        Self {
            source: Ok(Usage {
                cache_root: "/home/player/.cache/retinue/games/warframe/assets".into(),
                bytes: 173_015_040,
                objects: 2401,
            }),
            rendered: Ok(Usage {
                cache_root: "/home/player/.cache/retinue/games/warframe/gui/derivatives".into(),
                bytes: 37_748_736,
                objects: 920,
            }),
        }
    }
}

#[derive(Clone, Copy)]
pub enum Job {
    Refresh,
    ClearRendered,
    ClearSource,
}

pub struct Update {
    pub snapshot: Snapshot,
    pub error: Option<String>,
}

pub struct Worker {
    jobs: mpsc::Sender<Job>,
    stop: Option<oneshot::Sender<()>>,
    cancelled: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    pub fn new(send: impl Fn(Update) -> bool + Send + 'static) -> Result<Self, String> {
        let (jobs, mut receiver) = mpsc::channel(1);
        let (stop, mut stopped) = oneshot::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let thread =
            thread::Builder::new()
                .name("wfgui-data".into())
                .spawn(move || {
                    runtime.block_on(async {
                let paths = Paths::from_env();
                loop {
                    let job = tokio::select! {
                        _ = &mut stopped => break,
                        job = receiver.recv() => match job { Some(job) => job, None => break },
                    };
                    let update = tokio::select! {
                        _ = &mut stopped => break,
                        update = execute(&paths, job, &flag) => update,
                    };
                    if !send(update) { break; }
                }
            });
                })
                .map_err(|e| e.to_string())?;
        Ok(Self {
            jobs,
            stop: Some(stop),
            cancelled,
            thread: Some(thread),
        })
    }

    pub fn submit(&self, job: Job) -> Result<(), String> {
        self.jobs
            .try_send(job)
            .map_err(|_| "Cache operation already in progress".into())
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub fn native_cache_root() -> PathBuf {
    Paths::from_env()
        .game_directory(Base::Cache, Game::Warframe)
        .join("gui/derivatives")
        .join(retinue_ui::thumbnails::disk::DIRECTORY)
}

pub fn path_report(paths: &Paths) -> serde_json::Value {
    let cache = paths
        .game_directory(Base::Cache, Game::Warframe)
        .join("gui");
    json!({"app": "wfgui", "paths": [
        {"kind": "config", "path": paths.game_directory(Base::Config, Game::Warframe)},
        {"kind": "preferences", "path": crate::preferences::path(paths)},
        {"kind": "cache", "path": cache},
        {"kind": "derivatives", "path": cache.join("derivatives").join(retinue_ui::thumbnails::disk::DIRECTORY)},
        {"kind": "runtime", "path": paths.daemon_socket().parent()},
    ]})
}

pub async fn execute(paths: &Paths, job: Job, cancelled: &AtomicBool) -> Update {
    let root = paths
        .game_directory(Base::Cache, Game::Warframe)
        .join("gui/derivatives");
    let mut error = None;
    if matches!(job, Job::ClearRendered) {
        if let Err(reason) = rendered_cache(&root, true, cancelled) {
            error = Some(reason);
        }
    }
    let rendered = rendered_cache(&root, false, cancelled);
    if matches!(job, Job::ClearSource) {
        if let Err(reason) = source_cache(paths.daemon_socket(), true).await {
            error = Some(reason);
        }
    }
    let source = source_cache(paths.daemon_socket(), false).await;
    Update {
        snapshot: Snapshot { source, rendered },
        error,
    }
}

async fn source_cache(socket: &Path, clear: bool) -> Result<Usage, String> {
    tokio::time::timeout(Duration::from_secs(8), async {
        let stream = tokio::net::UnixStream::connect(socket).await.map_err(|e| format!("Daemon unavailable: {e}"))?;
        let mut channel = JsonChannel::new(stream, 128 * 1024);
        let hello = channel.request(&json!({
            "op":"hello", "id":1, "game":"warframe", "envelope":protocol::ENVELOPE_VERSION,
            "interfaces":{"assets":protocol::INTERFACE_ASSETS}, "features":[],
            "client":"wfgui", "version":env!("CARGO_PKG_VERSION"), "pid":std::process::id(), "mode":"desktop"
        })).await.map_err(|e| e.to_string())?;
        if hello["ok"] != true || hello["compatible"] != true || hello["game"] != "warframe"
            || hello["envelope"] != protocol::ENVELOPE_VERSION || hello["interfaces"]["assets"] != protocol::INTERFACE_ASSETS {
            return Err("Daemon contract mismatch; rebuild Retinue together".into());
        }
        let reply = channel.request(&json!({"op": if clear {"asset_cache_clear"} else {"asset_cache_status"}, "id":2}))
            .await.map_err(|e| e.to_string())?;
        if reply["ok"] != true {
            return Err(reply["error"].as_str().unwrap_or("Cache request failed").replace('_', " "));
        }
        serde_json::from_value(reply["data"].clone()).map_err(|e| format!("Invalid cache response: {e}"))
    }).await.map_err(|_| "Daemon request timed out".to_owned())?
}

fn rendered_cache(root: &Path, clear: bool, cancelled: &AtomicBool) -> Result<Usage, String> {
    let native = root.join(retinue_ui::thumbnails::disk::DIRECTORY);
    if clear && !cancelled.load(Ordering::Relaxed) {
        retinue_ui::thumbnails::disk::DiskCache::clear(&native).map_err(|e| e.to_string())?;
    }
    let mut usage = Usage {
        cache_root: root.to_owned(),
        bytes: 0,
        objects: 0,
    };
    let mut directories = vec![root.to_owned()];
    while let Some(directory) = directories.pop() {
        if cancelled.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if directory == root && error.kind() == std::io::ErrorKind::NotFound => {
                continue;
            }
            Err(error) => return Err(format!("{}: {error}", directory.display())),
        };
        for entry in entries {
            if cancelled.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                if !clear || entry.path() != native {
                    directories.push(entry.path());
                }
            } else if kind.is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "png")
            {
                if clear {
                    fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
                } else {
                    usage.objects += 1;
                    usage.bytes += entry.metadata().map_err(|e| e.to_string())?.len();
                }
            }
        }
    }
    Ok(usage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_maintenance_keeps_other_files_and_does_not_follow_nested_links() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("identity")).unwrap();
        fs::write(root.path().join("identity/24x24.png"), b"image").unwrap();
        fs::write(root.path().join("state.json"), b"state").unwrap();
        fs::write(outside.path().join("other.png"), b"other").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("linked")).unwrap();
        let stop = AtomicBool::new(false);
        assert_eq!(rendered_cache(root.path(), false, &stop).unwrap().bytes, 5);
        rendered_cache(root.path(), true, &stop).unwrap();
        assert_eq!(
            rendered_cache(root.path(), false, &stop).unwrap().objects,
            0
        );
        assert!(root.path().join("state.json").exists());
        assert!(outside.path().join("other.png").exists());
    }

    #[tokio::test]
    async fn cache_contract_is_negotiated_before_any_mutation() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("daemon.sock");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream = BufReader::new(stream);
            let mut line = String::new();
            stream.read_line(&mut line).await.unwrap();
            assert!(line.contains("\"op\":\"hello\""));
            stream
                .get_mut()
                .write_all(b"{\"id\":1,\"ok\":false}\n")
                .await
                .unwrap();
            line.clear();
            assert_eq!(stream.read_line(&mut line).await.unwrap(), 0);
        });
        assert!(
            source_cache(&path, true)
                .await
                .unwrap_err()
                .contains("contract mismatch")
        );
        server.await.unwrap();
    }
}
