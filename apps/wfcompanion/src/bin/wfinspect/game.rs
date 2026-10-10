use super::{args, files, scripts, support::*};
use clap::{Args, Subcommand};
use serde_json::json;
use std::path::PathBuf;
use wfcompanion::{game_observer, inspect};

#[derive(Subcommand)]
pub enum Game {
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
    Memory(Memory),
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

#[derive(Subcommand)]
pub enum Memory {
    /// Record changes to explicit ranges as bounded, timestamped NDJSON.
    Watch {
        #[command(flatten)]
        source: args::Source,
        #[command(flatten)]
        recording: args::Recording,
        /// START:LENGTH in decimal or 0x hex; repeat for multiple ranges.
        #[arg(long = "range", value_parser = args::range, required = true)]
        ranges: Vec<std::ops::Range<u64>>,
    },
    /// Summarize a recording, or retrieve the retained bytes at a sample index.
    Replay {
        file: PathBuf,
        #[arg(long)]
        sample: Option<u64>,
    },
    /// List mappings and available captured ranges.
    Maps {
        #[command(flatten)]
        source: args::Source,
    },
    /// Write a bounded memory range as raw bytes on stdout.
    Read {
        #[command(flatten)]
        source: args::Source,
        #[arg(value_parser = args::number)]
        address: u64,
        length: usize,
    },
    /// Find exact hexadecimal bytes in selected mappings.
    Scan {
        #[command(flatten)]
        source: args::Source,
        #[command(flatten)]
        selection: args::Selection,
        #[arg(value_parser = args::hex)]
        pattern: args::Hex,
        #[arg(long, default_value_t = 4096)]
        limit: usize,
        #[arg(long, default_value_t = 1073741824)]
        max_bytes: u64,
    },
    /// Find literal UTF-8 text; use -- before terms beginning with a dash.
    Find {
        #[command(flatten)]
        source: args::Source,
        #[command(flatten)]
        selection: args::Selection,
        text: String,
        #[arg(long, default_value_t = 4096)]
        limit: usize,
        #[arg(long, default_value_t = 1073741824)]
        max_bytes: u64,
    },
    /// Find stored pointers into ADDRESS:LENGTH.
    Refs {
        #[command(flatten)]
        source: args::Source,
        #[command(flatten)]
        selection: args::Selection,
        #[arg(value_parser = args::number)]
        address: u64,
        #[arg(long, default_value_t = 1)]
        length: u64,
        #[arg(long, default_value_t = 4096)]
        limit: usize,
        #[arg(long, default_value_t = 1073741824)]
        max_bytes: u64,
    },
    /// Find a pointer path, with explicit traversal and coverage limits.
    Path {
        #[command(flatten)]
        source: args::Source,
        #[command(flatten)]
        walk: args::Walk,
        #[arg(value_parser = args::number)]
        root: u64,
        #[arg(value_parser = args::number)]
        target: u64,
        #[arg(long, default_value_t = 1)]
        length: u64,
    },
    /// Capture selected roots/ranges into a new private evidence directory.
    Capture {
        #[command(flatten)]
        source: args::Source,
        #[command(flatten)]
        walk: args::Walk,
        directory: PathBuf,
        #[arg(long = "root", value_parser = args::number)]
        roots: Vec<u64>,
    },
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
            Self::Memory(command) => command.run(),
            Self::Combat(command) => command.run(),
            Self::Ui(command) => command.run(),
            Self::Cache(command) => command.run(),
            Self::Script(command) => command.run(),
            Self::Report(command) => command.run(),
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

impl Memory {
    fn run(self) -> Result<(), String> {
        use inspect::memory;
        match self {
            Self::Watch {
                source,
                recording,
                ranges,
            } => {
                let summary = inspect::watch::memory(
                    source.open()?,
                    &ranges,
                    &recording.output,
                    recording.options(),
                )?;
                print_json(&summary)?;
                if let Some(error) = summary.error {
                    return Err(error);
                }
                Ok(())
            }
            Self::Replay { file, sample } => {
                print_json(&inspect::watch::replay_memory(&file, sample)?)
            }
            Self::Maps { source } => print_json(&memory::maps(source.open()?)?),
            Self::Read {
                source,
                address,
                length,
            } => write_bytes(&memory::read(source.open()?, address, length)?),
            Self::Scan {
                source,
                selection,
                pattern,
                limit,
                max_bytes,
            } => print_json(&memory::scan_with(
                source.open()?,
                &pattern.0,
                &selection.query(),
                limit,
                max_bytes,
            )?),
            Self::Find {
                source,
                selection,
                text,
                limit,
                max_bytes,
            } => print_json(&memory::scan_with(
                source.open()?,
                text.as_bytes(),
                &selection.query(),
                limit,
                max_bytes,
            )?),
            Self::Refs {
                source,
                selection,
                address,
                length,
                limit,
                max_bytes,
            } => print_json(&memory::references(
                source.open()?,
                args::target(address, length)?,
                &selection.query(),
                limit,
                max_bytes,
            )?),
            Self::Path {
                source,
                walk,
                root,
                target,
                length,
            } => print_json(&memory::path(
                source.open()?,
                root,
                Some(args::target(target, length)?),
                &walk.query(),
            )?),
            Self::Capture {
                source,
                walk,
                directory,
                roots,
            } => print_json(&memory::capture(
                source.open()?,
                &directory,
                &roots,
                &walk.query(),
            )?),
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
            Self::Relic { source } => print_json(&memory::ui_relic(source.open()?)?),
            Self::Capture { directory, terms } => {
                let evidence =
                    wfcompanion::observation::ui_capture::capture(game_pid()?, &directory, &terms)?;
                print_json(&json!({"evidence": evidence}))
            }
            Self::Replay { directory } => print_json(
                &wfcompanion::observation::ui_capture::replay_evidence(&directory)?,
            ),
        }
    }
}
