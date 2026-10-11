use clap::{CommandFactory, Parser, Subcommand};
use std::process::ExitCode;

mod daemon;
#[path = "commands/memory.rs"]
mod memory;

#[path = "commands/args.rs"]
mod args;
#[path = "commands/files.rs"]
mod files;
#[path = "commands/game.rs"]
mod game;
#[path = "commands/scripts.rs"]
mod scripts;
#[path = "commands/support.rs"]
mod support;
use support::*;

/// Read-only process inspection, captures and game research.
#[derive(Parser)]
#[command(
    name = "retinue-inspect",
    version,
    propagate_version = true,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Inspect an explicit PID or capture without a game adapter.
    #[command(subcommand)]
    Memory(memory::Memory),
    /// Validate and inspect native analysis reports.
    #[command(subcommand)]
    Native(Native),
    /// Report Warframe adapters and external research tools (alias of warframe doctor).
    Doctor,
    /// Inspect Warframe memory, events and resource files.
    #[command(subcommand, name = "warframe", alias = "game")]
    Game(game::Game),
    /// Inspect the daemon handshake, datasets and subscriptions.
    Daemon {
        /// Game namespace for this daemon connection.
        #[arg(long, value_enum, default_value = "warframe", global = true)]
        game: args::DaemonGame,
        #[command(subcommand)]
        command: Daemon,
    },
    /// Generate shell completion; redirect into your shell's completion directory.
    Completion { shell: clap_complete::Shell },
}

#[derive(Subcommand)]
enum Native {
    /// Read exported Ghidra reports without a game adapter.
    #[command(subcommand)]
    Report(files::Report),
}

#[derive(Subcommand)]
enum Daemon {
    /// Connect and report negotiated interfaces.
    Handshake,
    /// Fetch a dataset; rejected requests exit nonzero and retain the server reply.
    Get { dataset: String },
    /// Stream dataset revision notifications as NDJSON.
    Subscribe {
        dataset: String,
        #[command(flatten)]
        watch: args::Watch,
    },
}

fn main() -> ExitCode {
    let _ = retinue_client::executable_path();
    let mut arguments = std::env::args_os().collect::<Vec<_>>();
    if arguments.last().is_some_and(|arg| arg == "help") && !arguments.iter().any(|arg| arg == "--")
    {
        *arguments.last_mut().unwrap() = "--help".into();
    }
    let cli = Cli::parse_from(arguments);
    match run(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error == BROKEN_PIPE => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("retinue-inspect: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Memory(command) => command.run(false),
        Command::Native(Native::Report(command)) => command.run(false),
        Command::Doctor => doctor(),
        Command::Game(command) => command.run(),
        Command::Completion { shell } => {
            let mut script = Vec::new();
            let invoked = std::env::args_os().next().map(std::path::PathBuf::from);
            let name = invoked
                .as_deref()
                .and_then(std::path::Path::file_name)
                .and_then(|name| name.to_str())
                .unwrap_or("retinue-inspect");
            clap_complete::generate(shell, &mut Cli::command(), name, &mut script);
            write_bytes(&script)
        }
        Command::Daemon { game, command } => run_daemon(game.into(), command),
    }
}

fn run_daemon(game: daemon::Game, command: Daemon) -> Result<(), String> {
    match command {
        Daemon::Handshake => print_json(&daemon::handshake(game)?),
        Daemon::Get { dataset } => {
            let report = daemon::dataset(game, &dataset)?;
            print_json(&report)?;
            if report
                .response
                .get("ok")
                .and_then(serde_json::Value::as_bool)
                != Some(true)
            {
                return Err("daemon rejected dataset request (see response)".into());
            }
            Ok(())
        }
        Daemon::Subscribe { dataset, watch } => {
            let summary =
                daemon::subscribe_stream(game, &dataset, watch.seconds, watch.limit, |record| {
                    print_ndjson("daemon.subscription", "event", record)
                })?;
            print_ndjson("daemon.subscription", "summary", &summary)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_game_selection_accepts_only_implemented_namespaces() {
        for args in [
            vec!["daemon", "--game", "poe2", "handshake"],
            vec!["daemon", "get", "session", "--game", "poe2"],
            vec!["daemon", "subscribe", "session", "--game", "poe2"],
        ] {
            let cli = Cli::try_parse_from(std::iter::once("retinue-inspect").chain(args)).unwrap();
            assert!(matches!(
                cli.command,
                Command::Daemon {
                    game: args::DaemonGame::Poe2,
                    ..
                }
            ));
        }
        for args in [
            vec!["daemon", "handshake"],
            vec!["daemon", "--game", "warframe", "handshake"],
        ] {
            let cli = Cli::try_parse_from(std::iter::once("retinue-inspect").chain(args)).unwrap();
            assert!(matches!(
                cli.command,
                Command::Daemon {
                    game: args::DaemonGame::Warframe,
                    ..
                }
            ));
        }
        for args in [
            vec!["daemon", "--game", "arc", "handshake"],
            vec!["daemon", "--game", "unknown", "get", "session"],
            vec!["--game", "poe2", "daemon", "handshake"],
        ] {
            assert!(Cli::try_parse_from(std::iter::once("retinue-inspect").chain(args)).is_err());
        }
    }

    #[test]
    fn warframe_namespace_keeps_game_alias() {
        for name in ["warframe", "game"] {
            let cli = Cli::try_parse_from(["retinue-inspect", name, "adapter", "--list"]).unwrap();
            assert!(matches!(
                cli.command,
                Command::Game(game::Game::Adapter { list: true, .. })
            ));
        }
    }

    #[test]
    fn command_tree_and_leaf_help_are_consistent() {
        Cli::command().debug_assert();
        for command in [
            vec!["daemon", "handshake", "--help"],
            vec!["daemon", "get", "--help"],
            vec!["daemon", "subscribe", "--help"],
            vec!["warframe", "inventory", "--help"],
            vec![
                "warframe",
                "cache",
                "paths",
                "/nonexistent",
                "Font",
                "--help",
            ],
            vec!["warframe", "ui", "objects", "--help"],
            vec!["warframe", "memory", "path", "--help"],
            vec!["warframe", "memory", "watch", "--help"],
            vec!["warframe", "memory", "replay", "--help"],
            vec!["warframe", "combat", "watch", "--help"],
            vec!["warframe", "combat", "probe", "--help"],
            vec!["warframe", "combat", "replay", "--help"],
            vec!["warframe", "script", "snapshot", "--help"],
            vec!["warframe", "script", "compare", "--help"],
        ] {
            let error = Cli::try_parse_from(std::iter::once("retinue-inspect").chain(command))
                .err()
                .unwrap();
            assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
        }
    }

    #[test]
    fn recording_cli_requires_bounds_and_preserves_discovery_controls() {
        for args in [
            vec![
                "combat",
                "watch",
                "new.jsonl",
                "--seconds",
                "0.2",
                "--owner",
                "0x10000",
                "--pid",
                "123",
            ],
            vec![
                "combat",
                "probe",
                "--capture",
                "evidence",
                "--range",
                "0x10000:0x2000",
            ],
            vec!["combat", "replay", "recording.jsonl", "--events"],
            vec![
                "memory",
                "watch",
                "new.jsonl",
                "--range",
                "0x10000:8",
                "--range",
                "0x20000:8",
            ],
            vec!["memory", "replay", "recording.jsonl", "--sample", "10"],
        ] {
            assert!(
                Cli::try_parse_from(["retinue-inspect", "warframe"].into_iter().chain(args))
                    .is_ok()
            );
        }
        for args in [
            vec!["combat", "watch", "new.jsonl", "--seconds", "NaN"],
            vec!["combat", "watch", "new.jsonl", "--interval-ms", "1"],
            vec!["memory", "watch", "new.jsonl"],
        ] {
            assert!(
                Cli::try_parse_from(["retinue-inspect", "warframe"].into_iter().chain(args))
                    .is_err()
            );
        }
    }

    #[test]
    fn script_snapshot_inputs_are_explicit_and_composable() {
        for args in [
            vec!["snapshot", "work", "build", "--executable", "Warframe.exe"],
            vec![
                "compare",
                "work",
                "old",
                "new",
                "--profile",
                "reviewed.json",
                "--check",
            ],
            vec![
                "infer",
                "work",
                "old",
                "new",
                "--output-profile",
                "candidate.json",
            ],
            vec!["info", "--snapshot", "work", "build", "/test.lua"],
            vec![
                "decompile",
                "--snapshot",
                "work",
                "build",
                "/test.lua",
                "--profile",
                "reviewed.json",
            ],
        ] {
            assert!(
                Cli::try_parse_from(
                    ["retinue-inspect", "warframe", "script"]
                        .into_iter()
                        .chain(args)
                )
                .is_ok()
            );
        }
        for args in [
            vec!["info", "raw.bc", "--snapshot", "work", "build", "/test.lua"],
            vec![
                "info",
                "--cache",
                "dir",
                "Font",
                "/test.lua",
                "--snapshot",
                "work",
                "build",
                "/test.lua",
            ],
            vec![
                "info",
                "raw.bc",
                "--adapter",
                "build",
                "--profile",
                "map.json",
            ],
        ] {
            assert!(
                Cli::try_parse_from(
                    ["retinue-inspect", "warframe", "script"]
                        .into_iter()
                        .chain(args)
                )
                .is_err()
            );
        }
    }

    #[test]
    fn literals_and_mistyped_flags_remain_distinct() {
        assert!(
            Cli::try_parse_from([
                "retinue-inspect",
                "warframe",
                "ui",
                "find",
                "--mistyped-option"
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "retinue-inspect",
                "warframe",
                "ui",
                "find",
                "--capture",
                "capture",
                "--",
                "help"
            ])
            .is_ok()
        );
        assert!(
            Cli::try_parse_from(["retinue-inspect", "warframe", "memory", "scan", "a\u{20ac}"])
                .is_err()
        );
        assert!(Cli::try_parse_from(["retinue-inspect", "warframe", "adapter", "--list"]).is_ok());
        assert!(
            Cli::try_parse_from([
                "retinue-inspect",
                "warframe",
                "script",
                "info",
                "-",
                "--adapter",
                "build"
            ])
            .is_ok()
        );
    }
}
