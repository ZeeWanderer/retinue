use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const APP_ID: &str = if cfg!(debug_assertions) {
    "io.github.zeewanderer.wfcompanion.dev"
} else {
    "io.github.zeewanderer.wfcompanion"
};

const SCREENSHOT_INTERFACE: &str = "org.kde.KWin.ScreenShot2";

fn desktop_entry(executable: &Path) -> Result<String, String> {
    let executable = executable
        .to_str()
        .ok_or_else(|| "wfcompanion executable path is not UTF-8".to_owned())?;
    if executable.contains(['"', '\n', '\r']) {
        return Err(
            "wfcompanion executable path cannot be represented in desktop entry".to_owned(),
        );
    }
    Ok(format!(
        "\
[Desktop Entry]
Type=Application
Name=wfcompanion
Comment=Warframe overlay companion
Exec=\"{executable}\"
Terminal=false
NoDisplay=true
Categories=Game;Utility;
X-KDE-DBUS-Restricted-Interfaces={SCREENSHOT_INTERFACE}
"
    ))
}

pub(crate) fn ensure_identity() -> Result<PathBuf, String> {
    let path = identity_path()?;
    let executable = retinue_client::executable_path()
        .ok_or_else(|| "could not resolve wfcompanion executable".to_owned())?;
    let entry = desktop_entry(executable)?;
    if fs::read_to_string(&path).is_ok_and(|current| current == entry) {
        return Ok(path);
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("invalid desktop entry path: {}", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    let temporary = path.with_extension(format!("desktop.{}.tmp", std::process::id()));
    fs::write(&temporary, entry)
        .map_err(|error| format!("write {}: {error}", temporary.display()))?;
    fs::rename(&temporary, &path)
        .map_err(|error| format!("install {}: {error}", path.display()))?;
    Ok(path)
}

pub(crate) fn identity_path() -> Result<PathBuf, String> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .ok_or_else(|| "HOME and XDG_DATA_HOME are unset".to_owned())?;
    Ok(data_home
        .join("applications")
        .join(format!("{APP_ID}.desktop")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_identity_matches_portal_app_id() {
        let entry = desktop_entry(Path::new("/opt/wfcli/bin/wfcompanion")).unwrap();
        assert!(entry.contains("NoDisplay=true"));
        assert!(entry.contains("Exec=\"/opt/wfcli/bin/wfcompanion\""));
        assert!(entry.contains(&format!(
            "X-KDE-DBUS-Restricted-Interfaces={SCREENSHOT_INTERFACE}"
        )));
        assert!(APP_ID.contains('.'));
        assert!(format!("{APP_ID}.desktop").starts_with("io.github.zeewanderer.wfcompanion"));
    }
}
