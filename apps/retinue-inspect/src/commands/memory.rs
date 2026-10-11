use super::{args, support::*};
use clap::Subcommand;
use retinue_native::inspect;
use std::path::PathBuf;

fn open_source(
    source: args::Source,
    warframe: bool,
) -> Result<inspect::memory::OpenedSource, String> {
    if warframe {
        return retinue_warframe::inspect::memory::open_memory(source.open()?);
    }
    let source = match (source.pid, source.capture) {
        (Some(pid), None) => inspect::memory::Source::Live(pid),
        (None, Some(path)) => inspect::memory::Source::Capture(path),
        _ => return Err("memory commands require --pid PID or --capture PATH".into()),
    };
    inspect::memory::OpenedSource::open(source)
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

impl Memory {
    pub fn run(self, warframe: bool) -> Result<(), String> {
        use inspect::memory;
        match self {
            Self::Watch {
                source,
                recording,
                ranges,
            } => {
                let summary = inspect::watch::memory(
                    open_source(source, warframe)?,
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
            Self::Maps { source } => print_json(&memory::maps(open_source(source, warframe)?)?),
            Self::Read {
                source,
                address,
                length,
            } => write_bytes(&memory::read(
                open_source(source, warframe)?,
                address,
                length,
            )?),
            Self::Scan {
                source,
                selection,
                pattern,
                limit,
                max_bytes,
            } => print_json(&memory::scan_with(
                open_source(source, warframe)?,
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
                open_source(source, warframe)?,
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
                open_source(source, warframe)?,
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
                open_source(source, warframe)?,
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
                open_source(source, warframe)?,
                &directory,
                &roots,
                &walk.query(),
            )?),
        }
    }
}
