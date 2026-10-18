use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use poe2companion::runtime::{self, RELOAD};
use retinue_client::paths::{Base, Game, Paths};
use serde_json::json;

#[derive(Parser)]
#[command(
    name = "poe2companion",
    version,
    about = "Read-only Path of Exile 2 native observer"
)]
struct Args {
    /// Observe one PoE2 process; otherwise discover it automatically.
    #[arg(long, global = true)]
    pid: Option<u32>,
    /// Disable the Evaluate overlay and shortcut.
    #[arg(long, global = true)]
    no_overlay: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Observe character, zone and UI state once per second.
    Watch,
    /// Observe while the supplied command runs, preserving its exit status.
    Launch {
        #[arg(last = true, required = true, num_args = 1..)]
        command: Vec<OsString>,
    },
    /// Report used directories without creating them.
    Paths {
        #[arg(long)]
        json: bool,
    },
    /// Render an Evaluate fixture without connecting to the game or desktop.
    Preview {
        #[arg(long)]
        output: std::path::PathBuf,
        /// Normalized evaluation/results JSON; defaults to a deterministic sample.
        #[arg(long)]
        input: Option<std::path::PathBuf>,
        #[arg(long, default_value_t = 1.0)]
        scale: f32,
        #[arg(long, default_value_t = 1920)]
        width: u32,
        #[arg(long, default_value_t = 1080)]
        height: u32,
        /// Keep the full game-sized canvas instead of cropping to the panel.
        #[arg(long)]
        full_frame: bool,
        #[arg(long, value_enum, default_value_t = poe2companion::evaluate::PreviewState::Results)]
        state: poe2companion::evaluate::PreviewState,
    },
}

fn main() -> ExitCode {
    let _ = retinue_client::executable_path();
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "--reload-check")
    {
        return finish(RELOAD.check::<runtime::State>().map(|()| {
            print!("{}", RELOAD.ready);
            0
        }));
    }
    // Handoff descriptors and environment are consumed before any workers start.
    let resumed = match unsafe { RELOAD.resume::<runtime::State>() } {
        Ok(resumed) => resumed,
        Err(error) => return finish(Err(error)),
    };
    let args = Args::parse();
    if let Some(Command::Preview {
        output,
        input,
        scale,
        width,
        height,
        full_frame,
        state,
    }) = &args.command
    {
        return finish(
            poe2companion::evaluate::preview(
                output,
                input.as_deref(),
                *scale,
                (*width, *height),
                *full_frame,
                *state,
            )
            .map(|()| 0),
        );
    }
    if let Some(Command::Paths { json: structured }) = args.command {
        let paths = Paths::from_env();
        let runtime = paths
            .daemon_socket()
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let config = paths
            .game_directory(Base::Config, Game::Poe2)
            .join("companion");
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                    .join(".local/share")
            })
            .join("applications");
        if structured {
            println!(
                "{}",
                json!({"app":"poe2companion","paths":[{"kind":"runtime","path":runtime},
                    {"kind":"config","path":config},{"kind":"desktop entries","path":data}]})
            );
        } else {
            println!("runtime\t{}", runtime.display());
            println!("config\t{}", config.display());
            println!("desktop entries\t{}", data.display());
        }
        return ExitCode::SUCCESS;
    }
    let launch = match args.command {
        Some(Command::Launch { command }) => Some(command),
        _ => None,
    };
    finish(runtime::run(launch, args.pid, resumed, !args.no_overlay))
}

fn finish(result: Result<u8, String>) -> ExitCode {
    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("poe2companion: {error}");
            ExitCode::FAILURE
        }
    }
}
