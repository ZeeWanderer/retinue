mod account;
pub mod adapter;
pub(crate) mod equipment;
mod executable;
pub mod gep;
pub mod inventory;
pub(crate) mod memory;
pub mod metadata;
pub mod ui;

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;

pub use memory::{ExecutableIdentity, ProcessIdentity, identify_process};

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GamePhase {
    #[default]
    Stopped,
    Launcher,
    Game,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttachContext {
    pid: u32,
    process_dir: PathBuf,
    environment: BTreeMap<String, String>,
    compat_data: PathBuf,
}

impl AttachContext {
    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn process_dir(&self) -> &Path {
        &self.process_dir
    }

    pub fn environment(&self) -> &BTreeMap<String, String> {
        &self.environment
    }

    pub fn compat_data(&self) -> &Path {
        &self.compat_data
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct GameState {
    running: bool,
    launcher_running: bool,
    phase: GamePhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    compat_data: Option<PathBuf>,
    #[serde(skip)]
    attach: Option<AttachContext>,
}

impl GameState {
    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn pid(&self) -> Option<u32> {
        if self.running { self.pid } else { None }
    }

    pub fn attach(&self) -> Option<&AttachContext> {
        self.attach.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DebugOutputEvent {
    RelicRewards,
    RelicSuggestions,
    CloseRelicSuggestions,
    UiConsoleOpen,
}

pub fn classify_debug_output(line: &str) -> Option<DebugOutputEvent> {
    if line.contains("Got rewards") {
        Some(DebugOutputEvent::RelicRewards)
    } else if line.contains("ThemedProjectionManager.lua: LoadingCompleteEnd") {
        Some(DebugOutputEvent::RelicSuggestions)
    } else if line.contains("InitMapping for all devices with bindings") {
        Some(DebugOutputEvent::CloseRelicSuggestions)
    } else if line.contains("UIConsoleTrigger::Open()") {
        Some(DebugOutputEvent::UiConsoleOpen)
    } else {
        None
    }
}

pub fn find_warframe() -> GameState {
    let Ok(entries) = fs::read_dir("/proc") else {
        return GameState::default();
    };
    let mut game = None;
    let mut launcher = None;

    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let process_dir = entry.path();
        let Ok(cmdline) = fs::read(process_dir.join("cmdline")) else {
            continue;
        };
        if is_warframe_cmdline(&cmdline) {
            game.get_or_insert_with(|| process_state(pid, &process_dir, GamePhase::Game));
        } else if is_launcher_cmdline(&cmdline) {
            launcher.get_or_insert_with(|| process_state(pid, &process_dir, GamePhase::Launcher));
        }
    }

    match (game, launcher) {
        (Some(mut game), Some(launcher)) => {
            game.launcher_running = true;
            if game.compat_data.is_none() {
                game.compat_data.clone_from(&launcher.compat_data);
                if let (Some(attach), Some(compat_data)) =
                    (game.attach.as_mut(), launcher.compat_data)
                {
                    attach.compat_data = compat_data;
                }
            }
            game
        }
        (Some(game), None) => game,
        (None, Some(launcher)) => launcher,
        (None, None) => GameState::default(),
    }
}

pub fn current_process_identity() -> Result<Option<ProcessIdentity>, String> {
    let state = find_warframe();
    state.pid().map(identify_process).transpose()
}

fn process_state(pid: u32, process_dir: &Path, phase: GamePhase) -> GameState {
    let environment = read_environment(&process_dir.join("environ"));
    let compat_data = environment
        .get("STEAM_COMPAT_DATA_PATH")
        .or_else(|| environment.get("WINEPREFIX"))
        .map(PathBuf::from)
        .map(normalize_compat_data);
    let attach = if matches!(phase, GamePhase::Game) {
        compat_data.clone().map(|compat_data| AttachContext {
            pid,
            process_dir: process_dir.to_owned(),
            environment,
            compat_data,
        })
    } else {
        None
    };
    GameState {
        running: matches!(phase, GamePhase::Game),
        launcher_running: matches!(phase, GamePhase::Launcher),
        phase,
        pid: Some(pid),
        compat_data,
        attach,
    }
}

fn is_warframe_cmdline(cmdline: &[u8]) -> bool {
    matches!(
        executable_name(cmdline).as_deref(),
        Some("warframe.x64.exe" | "warframe.x64")
    )
}

fn is_launcher_cmdline(cmdline: &[u8]) -> bool {
    executable_path(cmdline).is_some_and(|path| path.contains("warframe"))
        && executable_name(cmdline).as_deref() == Some("launcher.exe")
        && !String::from_utf8_lossy(cmdline)
            .to_ascii_lowercase()
            .contains("--type=")
}

fn executable_name(cmdline: &[u8]) -> Option<String> {
    executable_path(cmdline)?
        .rsplit(['/', '\\'])
        .next()
        .map(str::to_owned)
}

fn executable_path(cmdline: &[u8]) -> Option<String> {
    let executable = cmdline.split(|byte| *byte == 0).next()?;
    Some(String::from_utf8_lossy(executable).to_ascii_lowercase())
}

fn read_environment(path: &Path) -> BTreeMap<String, String> {
    let mut bytes = Vec::new();
    if File::open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .is_err()
    {
        return BTreeMap::new();
    }
    parse_environment(&bytes)
}

fn parse_environment(bytes: &[u8]) -> BTreeMap<String, String> {
    bytes
        .split(|byte| *byte == 0)
        .filter_map(|entry| {
            let entry = String::from_utf8_lossy(entry);
            entry
                .split_once('=')
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
        })
        .collect()
}

fn normalize_compat_data(path: PathBuf) -> PathBuf {
    if path.file_name().is_some_and(|name| name == "pfx") {
        path.parent().unwrap_or(&path).to_owned()
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_proton_warframe_command() {
        assert!(is_warframe_cmdline(
            b"Z:\\games\\Warframe\\Warframe.x64.exe\0-x64\0"
        ));
        assert!(!is_warframe_cmdline(b"wine64-preloader\0explorer.exe\0"));
    }

    #[test]
    fn recognizes_warframe_launcher_only() {
        assert!(is_launcher_cmdline(
            b"S:\\common\\Warframe\\Tools\\Launcher.exe\0-cluster:public\0"
        ));
        assert!(!is_launcher_cmdline(b"other-game\\Launcher.exe\0"));
        assert!(!is_launcher_cmdline(
            b"/steam/reaper\0--\0S:\\common\\Warframe\\Tools\\Launcher.exe\0"
        ));
        assert!(!is_launcher_cmdline(
            b"S:\\common\\Warframe\\Tools\\Launcher.exe\0--type=renderer\0"
        ));
    }

    #[test]
    fn parses_nul_separated_environment() {
        let env = parse_environment(b"A=one\0STEAM_COMPAT_DATA_PATH=/tmp/compat\0");
        assert_eq!(env.get("A").map(String::as_str), Some("one"));
        assert_eq!(
            env.get("STEAM_COMPAT_DATA_PATH").map(String::as_str),
            Some("/tmp/compat")
        );
    }

    #[test]
    fn classifies_relic_debug_output() {
        assert_eq!(
            classify_debug_output("123.4 Script [Info]: Got rewards, waiting for choice"),
            Some(DebugOutputEvent::RelicRewards)
        );
        assert_eq!(
            classify_debug_output("ThemedProjectionManager.lua: LoadingCompleteEnd"),
            Some(DebugOutputEvent::RelicSuggestions)
        );
        assert_eq!(
            classify_debug_output("InitMapping for all devices with bindings"),
            Some(DebugOutputEvent::CloseRelicSuggestions)
        );
        assert_eq!(classify_debug_output("Mission rewards"), None);
    }
}
