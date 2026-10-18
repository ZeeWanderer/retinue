use std::path::PathBuf;

use clap::Subcommand;
use retinue_poe2::{layout::Layout, process};

use crate::support::print_json;

#[derive(Subcommand)]
pub enum Command {
    /// Discover native bindings from an executable without launching the game.
    Adapter {
        #[arg(long)]
        exe: Option<PathBuf>,
    },
    /// Show the running PoE2 process and its loaded executable.
    Process {
        #[arg(long)]
        pid: Option<u32>,
    },
    /// Read character, zone and UI state; optionally retain only the bytes read.
    Snapshot {
        #[arg(long)]
        pid: Option<u32>,
        #[arg(long, value_name = "NEW_DIRECTORY")]
        output: Option<PathBuf>,
    },
    /// Decode a saved PoE2 snapshot without a running game.
    Replay { directory: PathBuf },
}

impl Command {
    pub fn run(self) -> Result<(), String> {
        match self {
            Self::Adapter { exe } => {
                let executable = exe.map_or_else(process::installed_executable, Ok)?;
                print_json(&Layout::read(&executable)?)
            }
            Self::Process { pid } => print_json(&process::Process::discover(pid)?),
            Self::Snapshot { pid, output } => {
                report(retinue_poe2::inspect::snapshot(pid, output.as_deref())?)
            }
            Self::Replay { directory } => report(retinue_poe2::inspect::replay(&directory)?),
        }
    }
}

fn report(report: retinue_poe2::inspect::Report) -> Result<(), String> {
    print_json(&report)?;
    report.snapshot.map(|_| ())
}
