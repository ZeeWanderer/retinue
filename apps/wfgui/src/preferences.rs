#[cfg(test)]
mod tests;

use retinue_client::paths::{Base, Game, Paths, create_private_dir_all};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fs::{File, TryLockError},
    io::{self, Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Values {
    pub ui_scale: u16,
    pub favorites: BTreeSet<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

impl Default for Values {
    fn default() -> Self {
        Self {
            ui_scale: 100,
            favorites: BTreeSet::new(),
            extra: BTreeMap::new(),
        }
    }
}

impl Values {
    pub fn load(path: &Path) -> io::Result<Self> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error),
        };
        const LIMIT: u64 = 1024 * 1024;
        let mut bytes = Vec::new();
        file.take(LIMIT + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > LIMIT {
            return Err(io::Error::other("preferences exceed 1 MiB"));
        }
        let values: Self = serde_json::from_slice(&bytes)?;
        if !(75..=200).contains(&values.ui_scale) {
            return Err(io::Error::other("ui_scale must be between 75 and 200"));
        }
        Ok(values)
    }
}

#[derive(Default)]
struct State {
    values: Values,
    pending: Changes,
}

#[derive(Clone, Default)]
pub struct Preferences(Rc<RefCell<State>>);

impl Preferences {
    pub fn new(values: Values) -> Self {
        Self(Rc::new(RefCell::new(State {
            values,
            pending: Changes::default(),
        })))
    }

    pub fn scale(&self) -> f32 {
        self.0.borrow().values.ui_scale as f32 / 100.0
    }

    pub fn set_scale(&self, percent: u16) {
        let percent = percent.clamp(75, 200);
        let mut state = self.0.borrow_mut();
        if state.values.ui_scale != percent {
            state.values.ui_scale = percent;
            state.pending.ui_scale = Some(percent);
        }
    }

    pub fn is_favorite(&self, id: &str) -> bool {
        self.0.borrow().values.favorites.contains(id)
    }

    pub fn toggle_favorite(&self, id: &str) {
        let mut state = self.0.borrow_mut();
        let favorite = !state.values.favorites.remove(id);
        if favorite {
            state.values.favorites.insert(id.to_owned());
        }
        state.pending.favorites.insert(id.to_owned(), favorite);
    }

    pub fn take_changes(&self) -> Changes {
        std::mem::take(&mut self.0.borrow_mut().pending)
    }
}

#[derive(Default)]
pub struct Changes {
    ui_scale: Option<u16>,
    favorites: BTreeMap<String, bool>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.ui_scale.is_none() && self.favorites.is_empty()
    }

    fn merge(&mut self, newer: Self) {
        self.ui_scale = newer.ui_scale.or(self.ui_scale);
        self.favorites.extend(newer.favorites);
    }

    fn apply(&self, values: &mut Values) {
        if let Some(scale) = self.ui_scale {
            values.ui_scale = scale;
        }
        for (id, favorite) in &self.favorites {
            if *favorite {
                values.favorites.insert(id.clone());
            } else {
                values.favorites.remove(id);
            }
        }
    }
}

pub fn path(paths: &Paths) -> PathBuf {
    paths
        .game_directory(Base::Config, Game::Warframe)
        .join("gui.json")
}

fn save(path: &Path, changes: &Changes) -> io::Result<()> {
    if changes.is_empty() {
        return Ok(());
    }
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("no config directory"))?;
    create_private_dir_all(directory)?;
    let lock = File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path.with_extension("lock"))?;
    let deadline = Instant::now() + Duration::from_millis(250);
    loop {
        match lock.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error.into()),
        }
    }
    // Merge edits, not a window's stale snapshot, while serializing all writers.
    let mut latest = Values::load(path)?;
    let before = latest.clone();
    changes.apply(&mut latest);
    if latest == before {
        return Ok(());
    }
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    serde_json::to_writer_pretty(&mut file, &latest)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    File::open(directory)?.sync_all()
}

pub struct Writer {
    sender: Option<mpsc::Sender<Changes>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Writer {
    pub fn new(
        path: PathBuf,
        notify: impl Fn(Result<(), String>) + Send + 'static,
    ) -> io::Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("wfgui-preferences".into())
            .spawn(move || {
                let mut pending = Changes::default();
                while let Ok(change) = receiver.recv() {
                    pending.merge(change);
                    let deadline = Instant::now() + Duration::from_millis(250);
                    let mut closing = false;
                    loop {
                        if Instant::now() >= deadline {
                            break;
                        }
                        match receiver
                            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                        {
                            Ok(change) => pending.merge(change),
                            Err(mpsc::RecvTimeoutError::Timeout) => break,
                            Err(mpsc::RecvTimeoutError::Disconnected) => {
                                closing = true;
                                break;
                            }
                        }
                    }
                    let result = save(&path, &pending)
                        .map_err(|e| format!("Preferences not saved to {}: {e}", path.display()));
                    if result.is_ok() {
                        pending = Changes::default();
                    } else if let Err(error) = &result {
                        eprintln!("wfgui: {error}");
                    }
                    notify(result);
                    if closing {
                        break;
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            thread: Some(thread),
        })
    }

    pub fn submit(&self, changes: Changes) -> Result<(), String> {
        if changes.is_empty() {
            return Ok(());
        }
        self.sender
            .as_ref()
            .expect("writer running")
            .send(changes)
            .map_err(|_| "Preferences writer stopped; changes are not saved".into())
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
