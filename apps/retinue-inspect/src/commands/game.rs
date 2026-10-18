use super::{args, files, memory, scripts, support::*};
use clap::{Args, Subcommand};
use retinue_warframe::{game_observer, inspect};
use serde_json::json;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum Game {
    /// Report Warframe adapters and external research tool availability.
    Doctor,
    /// Show the running process and executable identity.
    Process,
    /// Probe live adapters, inspect an executable offline, or list fixed layouts.
    Adapter {
        #[arg(long)]
        list: bool,
        #[arg(long, conflicts_with = "list", value_name = "PATH")]
        exe: Option<PathBuf>,
    },
    /// Capture game item metadata.
    Metadata {
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Read current resource stacks, blueprints and foundry jobs from memory.
    Inventory,
    /// Read attached native upgrades from an explicit weapon instance.
    Equipment(args::Equipment),
    /// Read, search, follow pointers and capture selected memory.
    #[command(subcommand)]
    Memory(memory::Memory),
    /// Observe sequenced damage displays without hooks or game input.
    #[command(subcommand)]
    Combat(Combat),
    /// Enumerate movies and text objects; run typed UI queries.
    #[command(subcommand)]
    Ui(Ui),
    /// Inspect retained API payloads and their changes.
    #[command(subcommand)]
    Gep(Gep),
    /// Subscribe to game debug events without competing with companion.
    #[command(subcommand)]
    Events(Events),
    /// Discover, extract and search game cache resources.
    #[command(subcommand)]
    Cache(files::Cache),
    /// Decode, retain and compare Warframe scripts across game updates.
    #[command(subcommand)]
    Script(scripts::Script),
    /// Read build-keyed Ghidra reports.
    #[command(subcommand)]
    Report(files::Report),
}

#[derive(Args)]
pub struct CombatProbe {
    #[command(flatten)]
    source: args::Source,
    /// Validated HUD owner address, avoiding automatic discovery.
    #[arg(long, value_parser = args::number)]
    owner: Option<u64>,
    #[command(flatten)]
    selection: args::Selection,
    /// One-time discovery byte budget; never scanned during sampling.
    #[arg(long, default_value_t = 4294967296)]
    max_bytes: u64,
}

impl CombatProbe {
    fn options(&self) -> inspect::combat::ProbeOptions {
        inspect::combat::ProbeOptions {
            owner: self.owner,
            selection: self.selection.query(),
            max_bytes: self.max_bytes,
        }
    }
}

#[derive(Subcommand)]
pub enum Combat {
    /// Locate and validate the native damage-display pool; do not record.
    Probe {
        #[command(flatten)]
        probe: CombatProbe,
    },
    /// Record HUD display observations. This is not a lossless hit-packet stream.
    Watch {
        #[command(flatten)]
        probe: CombatProbe,
        #[command(flatten)]
        recording: args::Recording,
        /// Also stream decoded observations to stdout.
        #[arg(long)]
        events: bool,
    },
    /// Decode a recording offline; default output is a compact summary.
    Replay {
        file: PathBuf,
        #[arg(long)]
        events: bool,
    },
}

impl Combat {
    fn run(self) -> Result<(), String> {
        match self {
            Self::Probe { probe } => {
                let options = probe.options();
                print_json(&inspect::combat::probe(probe.source.open()?, options)?)
            }
            Self::Watch {
                probe,
                recording,
                events,
            } => {
                let options = probe.options();
                let report = inspect::combat::record(
                    probe.source.open()?,
                    options,
                    &recording.output,
                    recording.options(),
                    |event| {
                        if events {
                            print_ndjson("game.combat", "event", event)
                        } else {
                            Ok(())
                        }
                    },
                )?;
                if events {
                    print_ndjson("game.combat", "summary", &report)?;
                } else {
                    print_json(&report)?;
                }
                if let Some(error) = report["polling"]["error"].as_str() {
                    return Err(error.into());
                }
                Ok(())
            }
            Self::Replay { file, events } => {
                let report = inspect::combat::replay(&file, |event| {
                    if events {
                        print_ndjson("game.combat", "event", event)
                    } else {
                        Ok(())
                    }
                })?;
                if events {
                    print_ndjson("game.combat", "summary", &report)
                } else {
                    print_json(&report)
                }
            }
        }
    }
}

#[derive(Subcommand)]
pub enum Ui {
    /// Query the current movie registry using the build adapter.
    State {
        #[command(flatten)]
        source: args::Source,
    },
    /// Discover movies from available bytes, including saved captures.
    Movies {
        #[command(flatten)]
        source: args::Source,
    },
    /// Enumerate known text objects, instance names and current values.
    Objects {
        #[command(flatten)]
        source: args::Source,
        /// Movie path substring; empty selects every discovered movie.
        #[arg(default_value = "")]
        movie: String,
        #[arg(long, default_value_t = 1024)]
        text_bytes: usize,
        #[arg(long, default_value_t = 2048)]
        limit: usize,
    },
    /// Inspect display hierarchy, effective visibility and screen-space leaf bounds.
    Tree {
        #[command(flatten)]
        source: args::Source,
        /// Movie path substring. Required to keep inspection bounded.
        movie: String,
        #[arg(long, default_value_t = 2048)]
        limit: usize,
        /// Retain exactly the memory read for offline replay with --capture.
        #[arg(long)]
        save: Option<PathBuf>,
    },
    /// Read the visible ducat kiosk tooltip.
    Ducats {
        #[command(flatten)]
        source: args::Source,
        /// Retain the exact reads for offline replay with --capture.
        #[arg(long)]
        save: Option<PathBuf>,
    },
    /// Find text and its display-object references in available memory.
    Find {
        #[command(flatten)]
        source: args::Source,
        #[arg(required = true)]
        terms: Vec<String>,
    },
    /// Resolve relic selection/reward data from known UI objects.
    Relic {
        #[command(flatten)]
        source: args::Source,
    },
    /// Capture movies and their reachable UI objects from the running game.
    Capture {
        directory: PathBuf,
        terms: Vec<String>,
    },
    /// Show recorded acquisition metadata and replay typed relic probes.
    Replay { directory: PathBuf },
}

#[derive(Subcommand)]
pub enum Gep {
    /// Report retained responses; optionally save exact payload bytes.
    State {
        #[arg(long)]
        payload_dir: Option<PathBuf>,
    },
    /// Stream response changes, with optional private payload files.
    Watch {
        #[command(flatten)]
        watch: args::Watch,
        #[arg(long)]
        payload_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum Events {
    /// Stream debug records from the single per-Proton-prefix owner.
    Watch {
        #[command(flatten)]
        watch: args::Watch,
    },
}

impl Game {
    pub fn run(self) -> Result<(), String> {
        match self {
            Self::Doctor => doctor(),
            Self::Process => process(),
            Self::Adapter { list: true, .. } => print_json(&game_observer::adapter::list()),
            Self::Adapter {
                exe: Some(path), ..
            } => print_json(&game_observer::adapter::inspect_executable(&path)?),
            Self::Adapter { .. } => adapter(),
            Self::Metadata { output } => metadata(output),
            Self::Inventory => {
                print_json(&game_observer::inventory::Reader::open(game_pid()?)?.read()?)
            }
            Self::Equipment(options) => print_json(&inspect::equipment::read(
                options.source.open()?,
                options.weapon,
            )?),
            Self::Memory(command) => command.run(true),
            Self::Combat(command) => command.run(),
            Self::Ui(command) => command.run(),
            Self::Cache(command) => command.run(),
            Self::Script(command) => command.run(),
            Self::Report(command) => command.run(true),
            Self::Gep(Gep::State { payload_dir }) => print_json(
                &inspect::gep::state_with_payloads(game_pid()?, payload_dir.as_deref())?,
            ),
            Self::Gep(Gep::Watch { watch, payload_dir }) => {
                let pid = game_pid()?;
                print_ndjson("game.gep", "start", &game_observer::identify_process(pid)?)?;
                let summary = inspect::gep::watch_payloads(
                    pid,
                    watch.seconds,
                    watch.limit,
                    payload_dir.as_deref(),
                    |event| print_ndjson("game.gep", "payload", event),
                )?;
                print_ndjson("game.gep", "summary", &summary)
            }
            Self::Events(Events::Watch { watch }) => {
                print_ndjson(
                    "game.events",
                    "start",
                    &game_observer::identify_process(game_pid()?)?,
                )?;
                let summary = inspect::events::watch_stream(watch.seconds, watch.limit, |event| {
                    print_ndjson("game.events", "event", event)
                })?;
                print_ndjson("game.events", "summary", &summary)?;
                if let Some(reason) = summary.stopped {
                    return Err(reason);
                }
                Ok(())
            }
        }
    }
}

impl Ui {
    fn run(self) -> Result<(), String> {
        use inspect::memory;
        match self {
            Self::State { source } => print_json(&memory::ui_state(source.open()?)?),
            Self::Movies { source } => print_json(&memory::ui_movies(source.open()?)?),
            Self::Objects {
                source,
                movie,
                text_bytes,
                limit,
            } => print_json(&memory::ui_objects(
                source.open()?,
                &movie,
                text_bytes,
                limit,
            )?),
            Self::Find { source, terms } => print_json(&memory::ui_find(source.open()?, &terms)?),
            Self::Tree {
                source,
                movie,
                limit,
                save,
            } => print_json(&memory::ui_tree(
                source.open()?,
                &movie,
                limit,
                save.as_deref(),
            )?),
            Self::Relic { source } => print_json(&memory::ui_relic(source.open()?)?),
            Self::Ducats { source, save } => {
                print_json(&memory::ui_ducats(source.open()?, save.as_deref())?)
            }
            Self::Capture { directory, terms } => {
                let evidence = retinue_warframe::observation::ui_capture::capture(
                    game_pid()?,
                    &directory,
                    &terms,
                )?;
                print_json(&json!({"evidence": evidence}))
            }
            Self::Replay { directory } => print_json(
                &retinue_warframe::observation::ui_capture::replay_evidence(&directory)?,
            ),
        }
    }
}
