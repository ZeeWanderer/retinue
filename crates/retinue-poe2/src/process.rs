use std::fs;
use std::path::{Path, PathBuf};

use retinue_native::process::{ProcessMemory, parse_regions};
use serde::{Deserialize, Serialize};

pub const APP_ID: u32 = 2694490;
const EXECUTABLES: &[&str] = &["PathOfExileSteam.exe", "PathOfExile.exe"];

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Process {
    pub pid: u32,
    pub started: u64,
    pub executable: PathBuf,
    pub image_base: u64,
}

impl Process {
    pub fn discover(pid: Option<u32>) -> Result<Option<Self>, String> {
        if let Some(pid) = pid {
            return Self::identify(pid).map(Some);
        }
        let mut found = None;
        let mut failure = None;
        for entry in fs::read_dir("/proc").map_err(|e| e.to_string())?.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<u32>().ok())
            else {
                continue;
            };
            let Ok(cmdline) = fs::read(entry.path().join("cmdline")) else {
                continue;
            };
            if !is_game_command(&cmdline) {
                continue;
            }
            match Self::identify(pid) {
                Ok(process) => {
                    if found.is_some() {
                        return Err("multiple PoE2 processes; select --pid".into());
                    }
                    found = Some(process);
                }
                Err(error) => failure = Some(error),
            }
        }
        if found.is_none()
            && let Some(error) = failure
        {
            return Err(error);
        }
        Ok(found)
    }

    fn identify(pid: u32) -> Result<Self, String> {
        let command = fs::read(format!("/proc/{pid}/cmdline")).map_err(|e| e.to_string())?;
        if !is_game_command(&command) {
            return Err(format!("process {pid} is not PoE2"));
        }
        let started = start_time(pid)?;
        let maps = fs::read_to_string(format!("/proc/{pid}/maps")).map_err(|e| e.to_string())?;
        let regions = parse_regions(&maps);
        let executable = regions
            .iter()
            .find(|r| r.permissions.contains('r') && supported_path(Path::new(&r.path)))
            .ok_or("PoE2 executable mapping not found")?
            .path
            .clone();
        let base = regions
            .iter()
            .find(|r| r.path == executable)
            .ok_or("PoE2 image base not found")?
            .start;
        if start_time(pid)? != started {
            return Err("PoE2 process changed during discovery".into());
        }
        Ok(Self {
            pid,
            started,
            executable: executable.into(),
            image_base: base,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        if start_time(self.pid)? != self.started {
            return Err("PoE2 process was replaced".into());
        }
        Ok(())
    }

    pub fn open(&self) -> Result<ProcessMemory, String> {
        self.validate()?;
        let memory = ProcessMemory::open(self.pid)?.with_image_path(Some(self.executable.clone()));
        if memory.image_base() != Some(self.image_base) {
            return Err("PoE2 image mapping changed".into());
        }
        let mut header = [0; 2];
        memory
            .read_exact_at(&mut header, self.image_base)
            .map_err(|e| e.to_string())?;
        if header != *b"MZ" {
            return Err("PoE2 image header is invalid".into());
        }
        self.validate()?;
        Ok(memory)
    }
}

fn supported_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| EXECUTABLES.contains(&name))
        && path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == "Path of Exile 2")
}

fn is_game_command(bytes: &[u8]) -> bool {
    let mut args = bytes.split(|b| *b == 0);
    let name = |arg: &[u8]| {
        arg.rsplit(|b| matches!(*b, b'/' | b'\\'))
            .next()
            .unwrap_or_default()
            .to_vec()
    };
    let command = name(args.next().unwrap_or_default());
    let executable = if matches!(command.as_slice(), b"wine64-preloader" | b"wine-preloader") {
        name(args.next().unwrap_or_default())
    } else {
        command
    };
    EXECUTABLES
        .iter()
        .any(|candidate| executable == candidate.as_bytes())
}

fn start_time(pid: u32) -> Result<u64, String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).map_err(|e| e.to_string())?;
    stat.rsplit_once(')')
        .and_then(|(_, fields)| fields.split_whitespace().nth(19))
        .and_then(|started| started.parse().ok())
        .ok_or_else(|| "invalid PoE2 process identity".into())
}

pub fn installed_executable() -> Result<PathBuf, String> {
    let steam = steamlocate::SteamDir::locate().map_err(|e| e.to_string())?;
    let (app, library) = steam
        .find_app(APP_ID)
        .map_err(|e| e.to_string())?
        .ok_or("PoE2 Steam installation not found; pass --exe")?;
    let directory = library.resolve_app_dir(&app);
    EXECUTABLES
        .iter()
        .map(|name| directory.join(name))
        .find(|path| path.is_file())
        .ok_or_else(|| "PoE2 executable not found".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_arguments_require_an_exact_executable_name() {
        assert!(is_game_command(
            b"Z:\\SteamLibrary\\PathOfExileSteam.exe\0--wait\0"
        ));
        assert!(is_game_command(
            b"wine64-preloader\0C:\\games\\PathOfExile.exe\0"
        ));
        assert!(!is_game_command(b"bash\0-c\0PathOfExileSteam.exe\0"));
        assert!(!is_game_command(b"PathOfExileSteam.exe.log\0"));
        assert!(!is_game_command(b"PathOfExileSteam.exe_helper\0"));
        assert!(!is_game_command(
            b"steam.exe\0C:\\games\\PathOfExileSteam.exe\0"
        ));
    }

    #[test]
    fn poe1_mapping_is_not_accepted() {
        assert!(supported_path(Path::new(
            "/games/Path of Exile 2/PathOfExileSteam.exe"
        )));
        assert!(!supported_path(Path::new(
            "/games/Path of Exile/PathOfExileSteam.exe"
        )));
    }
}
