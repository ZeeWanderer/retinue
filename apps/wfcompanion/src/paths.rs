use retinue_client::paths::{Base, Game, Paths};
use serde::Serialize;
use std::io::{self, Write};
use std::path::Path;

#[derive(Debug, PartialEq, Eq, Serialize)]
struct PathReport {
    app: &'static str,
    paths: Vec<PathEntry>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct PathEntry {
    kind: &'static str,
    path: String,
}

pub(crate) fn print() -> Result<(), String> {
    let report = report()?;
    let stdout = io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer(&mut output, &report)
        .map_err(|error| format!("could not encode path report: {error}"))?;
    writeln!(output).map_err(|error| format!("could not write path report: {error}"))
}

fn report() -> Result<PathReport, String> {
    let capture = crate::capture::capture_dir();
    let incident = crate::incident::log_path();
    let socket = crate::daemon::daemon_socket_path();
    let identity = crate::desktop::identity_path()?;
    Ok(PathReport {
        app: "wfcompanion",
        paths: vec![
            path_entry("cache", &capture),
            path_entry("state", parent(&incident)?),
            path_entry("runtime", parent(&socket)?),
            path_entry(
                "runtime",
                &Paths::from_env().game_directory(Base::Runtime, Game::Warframe),
            ),
            path_entry("data", parent(&identity)?),
        ],
    })
}

fn parent(path: &Path) -> Result<&Path, String> {
    path.parent()
        .ok_or_else(|| format!("path has no parent: {}", path.display()))
}

fn path_entry(kind: &'static str, path: &Path) -> PathEntry {
    PathEntry {
        kind,
        path: path.to_string_lossy().into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_entry_is_structured_data() {
        assert_eq!(
            path_entry("cache", Path::new("/cache/retinue/games/warframe/captures")),
            PathEntry {
                kind: "cache",
                path: "/cache/retinue/games/warframe/captures".to_owned(),
            }
        );
    }

    #[test]
    fn report_includes_dbwin_runtime_directory() {
        let expected = Paths::from_env().game_directory(Base::Runtime, Game::Warframe);
        assert!(
            report()
                .unwrap()
                .paths
                .contains(&path_entry("runtime", &expected))
        );
    }
}
