use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Component, Path, PathBuf};

use rustix::fs::{Mode, OFlags, mkdirat, open, openat};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Base {
    Config,
    Data,
    State,
    Cache,
    Runtime,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Game {
    Warframe,
    Poe2,
    Arc,
}

impl Game {
    pub fn id(self) -> &'static str {
        match self {
            Self::Warframe => "warframe",
            Self::Poe2 => "poe2",
            Self::Arc => "arc",
        }
    }
}

/// Logical paths only: resolving or reporting paths never creates directories.
#[derive(Clone, Debug)]
pub struct Paths {
    config: PathBuf,
    data: PathBuf,
    state: PathBuf,
    cache: PathBuf,
    runtime: PathBuf,
    runtime_base: Option<PathBuf>,
    socket: PathBuf,
}

impl Paths {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var_os(key))
    }

    /// Resolve an environment snapshot without changing process-global variables.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<OsString>) -> Self {
        let home = lookup("HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(std::env::home_dir)
            .expect("HOME or an account home directory is required");
        let persistent = |key, default| {
            absolute(lookup(key))
                .unwrap_or_else(|| home.join(default))
                .join("retinue")
        };
        let runtime_base = absolute(lookup("XDG_RUNTIME_DIR"));
        let runtime = runtime_base
            .as_ref()
            .map(|base| base.join("retinue"))
            .unwrap_or_else(|| {
                PathBuf::from(format!(
                    "/tmp/retinue-{}",
                    rustix::process::getuid().as_raw()
                ))
            });
        let socket = lookup("RETINUE_DAEMON_SOCKET")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| runtime.join("daemon.sock"));
        Self {
            config: persistent("XDG_CONFIG_HOME", ".config"),
            data: persistent("XDG_DATA_HOME", ".local/share"),
            state: persistent("XDG_STATE_HOME", ".local/state"),
            cache: persistent("XDG_CACHE_HOME", ".cache"),
            runtime,
            runtime_base,
            socket,
        }
    }

    pub fn directory(&self, base: Base) -> &Path {
        match base {
            Base::Config => &self.config,
            Base::Data => &self.data,
            Base::State => &self.state,
            Base::Cache => &self.cache,
            Base::Runtime => &self.runtime,
        }
    }

    pub fn game_directory(&self, base: Base, game: Game) -> PathBuf {
        self.directory(base).join("games").join(game.id())
    }

    pub fn daemon_socket(&self) -> &Path {
        &self.socket
    }

    /// Create only Retinue-owned runtime directories; reject unsafe existing paths.
    pub fn ensure_runtime_directory(&self) -> io::Result<PathBuf> {
        self.runtime_handle()?;
        Ok(self.runtime.clone())
    }

    pub fn ensure_game_runtime_directory(&self, game: Game) -> io::Result<PathBuf> {
        let root = self.runtime_handle()?;
        let games = private_child(&root, OsStr::new("games"))?;
        private_child(&games, OsStr::new(game.id()))?;
        Ok(self.game_directory(Base::Runtime, game))
    }

    fn runtime_handle(&self) -> io::Result<File> {
        let parent = open_directory(self.runtime.parent().expect("absolute runtime path"))?;
        if self.runtime_base.is_some() {
            validate_private(&parent)?;
        }
        private_child(&parent, self.runtime.file_name().expect("runtime name"))
    }
}

/// Persistent directories may have user-managed symlink parents. Existing modes
/// are left alone; only newly created directories receive private permissions.
pub fn create_private_dir_all(path: &Path) -> io::Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
}

fn absolute(value: Option<OsString>) -> Option<PathBuf> {
    value.map(PathBuf::from).filter(|path| path.is_absolute())
}

const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

// Walk by directory handles so symlink parents cannot redirect runtime creation.
fn open_directory(path: &Path) -> io::Result<File> {
    let mut directory = File::from(open("/", DIRECTORY_FLAGS, Mode::empty())?);
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(name) => {
                directory = File::from(openat(&directory, name, DIRECTORY_FLAGS, Mode::empty())?);
            }
            Component::ParentDir => {
                directory = File::from(openat(&directory, "..", DIRECTORY_FLAGS, Mode::empty())?);
            }
            Component::Prefix(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid runtime path",
                ));
            }
        }
    }
    Ok(directory)
}

fn private_child(parent: &File, name: &OsStr) -> io::Result<File> {
    match mkdirat(parent, name, Mode::from_raw_mode(0o700)) {
        Ok(()) | Err(rustix::io::Errno::EXIST) => {}
        Err(error) => return Err(error.into()),
    }
    let directory = File::from(openat(parent, name, DIRECTORY_FLAGS, Mode::empty())?);
    validate_private(&directory)?;
    Ok(directory)
}

fn validate_private(directory: &File) -> io::Result<()> {
    let metadata = directory.metadata()?;
    if !private_identity(metadata.uid(), metadata.mode()) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "runtime directory must be owned by the current user with mode 0700",
        ));
    }
    Ok(())
}

fn private_identity(uid: u32, mode: u32) -> bool {
    uid == rustix::process::getuid().as_raw() && mode & 0o7777 == 0o700
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static SEQUENCE: AtomicU64 = AtomicU64::new(0);
            let path = PathBuf::from(format!(
                "/tmp/retinue-paths-test-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
            Self(path)
        }

        fn paths(&self) -> Paths {
            Paths::from_lookup(|key| {
                (key == "XDG_RUNTIME_DIR").then(|| self.0.as_os_str().to_owned())
            })
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn absolute_xdg_roots_and_all_games_are_distinct() {
        let paths = Paths::from_lookup(|key| {
            match key {
                "HOME" => Some("/home/test"),
                "XDG_CONFIG_HOME" => Some("/config"),
                "XDG_DATA_HOME" => Some("/data"),
                "XDG_STATE_HOME" => Some("/state"),
                "XDG_CACHE_HOME" => Some("/cache"),
                "XDG_RUNTIME_DIR" => Some("/runtime"),
                _ => None,
            }
            .map(OsString::from)
        });
        for (base, prefix) in [
            (Base::Config, "/config"),
            (Base::Data, "/data"),
            (Base::State, "/state"),
            (Base::Cache, "/cache"),
            (Base::Runtime, "/runtime"),
        ] {
            assert_eq!(paths.directory(base), Path::new(prefix).join("retinue"));
            for game in [Game::Warframe, Game::Poe2, Game::Arc] {
                assert_eq!(
                    paths.game_directory(base, game),
                    Path::new(prefix).join("retinue/games").join(game.id())
                );
            }
        }
        assert_eq!(
            paths.daemon_socket(),
            Path::new("/runtime/retinue/daemon.sock")
        );
    }

    #[test]
    fn absent_empty_and_relative_xdg_use_home_and_uid_defaults() {
        for value in [None, Some(""), Some("relative"), Some("~/cache")] {
            let paths = Paths::from_lookup(|key| {
                if key == "HOME" {
                    Some("/home/test")
                } else if key.starts_with("XDG_") {
                    value
                } else {
                    None
                }
                .map(OsString::from)
            });
            for (base, suffix) in [
                (Base::Config, ".config"),
                (Base::Data, ".local/share"),
                (Base::State, ".local/state"),
                (Base::Cache, ".cache"),
            ] {
                assert_eq!(
                    paths.directory(base),
                    Path::new("/home/test").join(suffix).join("retinue")
                );
            }
            assert_eq!(
                paths.directory(Base::Runtime),
                PathBuf::from(format!(
                    "/tmp/retinue-{}",
                    rustix::process::getuid().as_raw()
                ))
            );
            assert_eq!(
                paths.daemon_socket(),
                paths.directory(Base::Runtime).join("daemon.sock")
            );
        }
    }

    #[test]
    fn socket_override_is_explicit_and_legacy_override_is_ignored() {
        for value in [None, Some(""), Some("/custom.sock"), Some("relative.sock")] {
            let paths = Paths::from_lookup(|key| match key {
                "XDG_RUNTIME_DIR" => Some(OsString::from("/runtime")),
                "RETINUE_DAEMON_SOCKET" => value.map(OsString::from),
                "WFCLI_DAEMON_SOCKET" => Some(OsString::from("/legacy.sock")),
                _ => None,
            });
            assert_eq!(
                paths.daemon_socket(),
                Path::new(
                    value
                        .filter(|value| !value.is_empty())
                        .unwrap_or("/runtime/retinue/daemon.sock")
                )
            );
        }
    }

    #[test]
    fn runtime_needs_no_home_and_ignores_cache_and_temp_overrides() {
        let paths = Paths::from_lookup(|key| match key {
            "XDG_CACHE_HOME" | "TMPDIR" => Some(OsString::from("/remote")),
            _ => None,
        });
        assert_eq!(
            paths.daemon_socket(),
            paths.directory(Base::Runtime).join("daemon.sock")
        );
        assert!(paths.directory(Base::Runtime).starts_with("/tmp"));
    }

    #[test]
    fn lookup_does_not_create_and_runtime_creation_is_private_and_idempotent() {
        let temp = TempDir::new();
        let paths = temp.paths();
        let game = paths.game_directory(Base::Runtime, Game::Warframe);
        assert!(!paths.directory(Base::Runtime).exists());
        for _ in 0..2 {
            assert_eq!(
                paths.ensure_game_runtime_directory(Game::Warframe).unwrap(),
                game
            );
        }
        for directory in [
            paths.directory(Base::Runtime),
            game.parent().unwrap(),
            &game,
        ] {
            let metadata = fs::symlink_metadata(directory).unwrap();
            assert_eq!(metadata.mode() & 0o7777, 0o700);
            assert_eq!(metadata.uid(), rustix::process::getuid().as_raw());
            assert!(metadata.is_dir());
        }
    }

    #[test]
    fn runtime_rejects_unsafe_modes_and_files_without_changing_them() {
        let temp = TempDir::new();
        let paths = temp.paths();
        let root = paths.directory(Base::Runtime);
        fs::write(root, "occupied").unwrap();
        assert!(paths.ensure_runtime_directory().is_err());
        assert_eq!(fs::read_to_string(root).unwrap(), "occupied");
        fs::remove_file(root).unwrap();
        fs::DirBuilder::new().mode(0o755).create(root).unwrap();
        fs::set_permissions(root, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            paths.ensure_runtime_directory().unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(fs::metadata(root).unwrap().mode() & 0o7777, 0o755);
        fs::remove_dir(root).unwrap();
        fs::set_permissions(&temp.0, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(paths.ensure_runtime_directory().is_err());
        assert!(!root.exists());
    }

    #[test]
    fn runtime_rejects_symlink_roots_parents_and_game_directories() {
        let temp = TempDir::new();
        let target = TempDir::new();
        let paths = temp.paths();
        symlink(&target.0, paths.directory(Base::Runtime)).unwrap();
        assert!(paths.ensure_runtime_directory().is_err());
        fs::remove_file(paths.directory(Base::Runtime)).unwrap();
        paths.ensure_runtime_directory().unwrap();
        symlink(&target.0, paths.directory(Base::Runtime).join("games")).unwrap();
        assert!(paths.ensure_game_runtime_directory(Game::Warframe).is_err());
        assert!(!target.0.join("warframe").exists());
        let linked_parent = temp.0.join("linked");
        symlink(&target.0, &linked_parent).unwrap();
        let linked_paths = Paths::from_lookup(|key| {
            (key == "XDG_RUNTIME_DIR").then(|| linked_parent.as_os_str().to_owned())
        });
        assert!(linked_paths.ensure_runtime_directory().is_err());
        assert!(!target.0.join("retinue").exists());
    }

    #[test]
    fn private_runtime_check_rejects_foreign_owner() {
        let uid = rustix::process::getuid().as_raw();
        assert!(private_identity(uid, 0o40700));
        assert!(!private_identity(uid.wrapping_add(1), 0o40700));
        assert!(!private_identity(uid, 0o40750));
        assert!(!private_identity(uid, 0o41700));
    }

    #[test]
    fn persistent_paths_preserve_symlink_roots_and_create_private_children() {
        let temp = TempDir::new();
        let target = TempDir::new();
        let root = temp.0.join("retinue");
        symlink(&target.0, &root).unwrap();
        let paths = Paths::from_lookup(|key| {
            (key == "XDG_CACHE_HOME").then(|| temp.0.as_os_str().to_owned())
        });
        let game = paths.game_directory(Base::Cache, Game::Poe2);
        assert_eq!(game, root.join("games/poe2"));
        create_private_dir_all(&game).unwrap();
        assert!(fs::symlink_metadata(root).unwrap().is_symlink());
        assert_eq!(fs::metadata(&game).unwrap().mode() & 0o7777, 0o700);
        assert!(target.0.join("games/poe2").is_dir());
    }
}
