use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode};

use clap::{Parser, Subcommand};
use serde_json::json;

const APP_ID: u32 = 1808500;

#[derive(Parser)]
#[command(
    version,
    about = "ARC Raiders launch scaffold; gameplay observation is not implemented"
)]
struct Args {
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Run the supplied Steam command unchanged.
    Launch {
        #[arg(last = true, required = true, num_args = 1..)]
        command: Vec<OsString>,
    },
    /// Locate the installed game and report implemented capabilities.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Report used Retinue directories (none until observation is implemented).
    Paths {
        #[arg(long)]
        json: bool,
    },
}

fn main() -> ExitCode {
    match Args::parse().command {
        Action::Launch { command } => {
            let error = Command::new(&command[0]).args(&command[1..]).exec();
            eprintln!("arccompanion: {error}");
            ExitCode::FAILURE
        }
        Action::Status { json: structured } => {
            let install = steamlocate::SteamDir::locate()
                .and_then(|steam| steam.find_app(APP_ID))
                .map(|app| app.map(|(app, library)| library.resolve_app_dir(&app)));
            let (directory, error) = match install {
                Ok(directory) => (directory, None),
                Err(error) => (None, Some(error.to_string())),
            };
            if structured {
                println!(
                    "{}",
                    json!({"game":"arc", "steam_app_id":APP_ID,
                    "observation":"not_implemented", "install_directory":directory,
                    "discovery_error":error})
                );
            } else {
                println!("ARC Raiders: launch only; gameplay observation is not implemented.");
                if let Some(directory) = directory {
                    println!("  installed: {}", directory.display());
                } else {
                    println!(
                        "  installation: {}",
                        error.as_deref().unwrap_or("not found")
                    );
                }
            }
            ExitCode::SUCCESS
        }
        Action::Paths { json: structured } => {
            if structured {
                println!("{}", json!({"app":"arccompanion", "paths":[]}));
            }
            ExitCode::SUCCESS
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_preserves_arguments() {
        let args = Args::try_parse_from(["arccompanion", "launch", "--", "game", "--flag", "a b"])
            .unwrap();
        let Action::Launch { command } = args.command else {
            panic!("launch expected")
        };
        assert_eq!(command, ["game", "--flag", "a b"].map(OsString::from));
    }

    #[test]
    fn launch_requires_command() {
        assert!(Args::try_parse_from(["arccompanion", "launch"]).is_err());
    }
}
