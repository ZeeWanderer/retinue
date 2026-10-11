use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use retinue_client::paths::{Game, Paths};
use sha2::{Digest, Sha256};

pub(super) enum Acquisition {
    Owner(Hub),
    Subscriber(UnixStream),
}

pub(super) struct Hub {
    _lock: File,
    path: PathBuf,
    listener: UnixListener,
    clients: Vec<UnixStream>,
}

impl Hub {
    pub(super) fn acquire(prefix: &Path) -> Result<Acquisition, String> {
        let root = Paths::from_env()
            .ensure_game_runtime_directory(Game::Warframe)
            .map_err(|error| format!("unsafe DBWIN runtime directory: {error}"))?;
        let prefix = fs::canonicalize(prefix).map_err(|e| e.to_string())?;
        let key = format!(
            "{:x}",
            Sha256::digest(prefix.as_os_str().as_encoded_bytes())
        );
        Self::at(&root.join(format!("dbwin-{}.sock", &key[..16])))
    }

    fn at(path: &Path) -> Result<Acquisition, String> {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path.with_extension("lock"))
            .map_err(|e| e.to_string())?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => {
                // Owner binds before starting its producer; tolerate that short startup race.
                for _ in 0..20 {
                    match UnixStream::connect(path) {
                        Ok(stream) => return Ok(Acquisition::Subscriber(stream)),
                        Err(error)
                            if matches!(
                                error.kind(),
                                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                            ) =>
                        {
                            std::thread::sleep(std::time::Duration::from_millis(25))
                        }
                        Err(error) => {
                            return Err(format!("could not subscribe to DBWIN owner: {error}"));
                        }
                    }
                }
                return Err("DBWIN owner is starting or not accepting subscribers; retry".into());
            }
            Err(error) => return Err(format!("could not lock DBWIN owner: {error}")),
        }
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        let listener = UnixListener::bind(path).map_err(|e| e.to_string())?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        Ok(Acquisition::Owner(Self {
            _lock: lock,
            path: path.to_owned(),
            listener,
            clients: Vec::new(),
        }))
    }

    pub(super) fn publish(&mut self, pid: u32, message: &[u8]) {
        for _ in 0..16 {
            let Ok((client, _)) = self.listener.accept() else {
                break;
            };
            if self.clients.len() < 16 && client.set_nonblocking(true).is_ok() {
                self.clients.push(client);
            }
        }
        let mut frame = Vec::with_capacity(8 + message.len());
        frame.extend_from_slice(&pid.to_le_bytes());
        frame.extend_from_slice(&(message.len() as u32).to_le_bytes());
        frame.extend_from_slice(message);
        // Slow observers disconnect; they must never stall or consume the owner's feed.
        self.clients
            .retain_mut(|client| client.write_all(&frame).is_ok());
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        // A concurrent fork can still hold this file description until exec.
        let _ = self._lock.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn single_owner_fans_out_and_releases_lock() {
        let path = std::env::temp_dir().join(format!("wf-dbwin-hub-{}.sock", std::process::id()));
        let Acquisition::Owner(mut hub) = Hub::at(&path).unwrap() else {
            panic!("owner")
        };
        let Acquisition::Subscriber(mut first) = Hub::at(&path).unwrap() else {
            panic!("subscriber")
        };
        let Acquisition::Subscriber(mut second) = Hub::at(&path).unwrap() else {
            panic!("subscriber")
        };
        hub.publish(42, b"test");
        for stream in [&mut first, &mut second] {
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(1)))
                .unwrap();
            let mut bytes = [0; 12];
            stream.read_exact(&mut bytes).unwrap();
            assert_eq!(&bytes[8..], b"test");
        }
        let inherited_lock = hub._lock.try_clone().unwrap();
        drop(hub);
        assert!(matches!(Hub::at(&path).unwrap(), Acquisition::Owner(_)));
        drop(inherited_lock);
        let _ = fs::remove_file(path.with_extension("lock"));
    }
}
