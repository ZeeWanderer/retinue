use clap::{Args, ValueEnum};
use std::ops::Range;
use std::path::PathBuf;
use std::time::Duration;
use wfcompanion::inspect::{memory, query};

#[derive(Args)]
pub struct Source {
    /// Read saved evidence instead of a live process.
    #[arg(long, conflicts_with = "pid")]
    capture: Option<PathBuf>,
    /// Select a Warframe PID; otherwise discover the running game.
    #[arg(long)]
    pid: Option<u32>,
}

impl Source {
    pub fn open(self) -> Result<memory::Source, String> {
        Ok(match self.capture {
            Some(path) => memory::Source::Capture(path),
            None => memory::Source::Live(match self.pid {
                Some(pid) => pid,
                None => super::support::game_pid()?,
            }),
        })
    }
}

#[derive(Args)]
pub struct Equipment {
    #[command(flatten)]
    pub source: Source,
    /// Explicit native weapon instance address from this process or capture.
    #[arg(long, value_parser = number, value_name = "ADDRESS")]
    pub weapon: u64,
}

#[derive(Args)]
#[group(id = "watch_options")]
pub struct Watch {
    /// Maximum duration in seconds (fractional values allowed, <=300).
    #[arg(long, default_value = "30", value_parser = duration)]
    pub seconds: Duration,
    /// Maximum records before stopping.
    #[arg(long, default_value_t = 1000)]
    pub limit: usize,
}

#[derive(Args)]
pub struct Recording {
    /// New private recording file; existing files are never overwritten.
    pub output: PathBuf,
    /// Duration in seconds (fractional values allowed, <=1800).
    #[arg(long, default_value = "30", value_parser = recording_duration)]
    seconds: Duration,
    /// Sampling interval; no busy-loop catchup after slow reads.
    #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u64).range(10..=5000))]
    interval_ms: u64,
    /// Stop before the recording exceeds this many bytes.
    #[arg(long, default_value_t = 67108864)]
    record_bytes: u64,
}

impl Recording {
    pub fn options(&self) -> wfcompanion::inspect::watch::Options {
        wfcompanion::inspect::watch::Options {
            duration: self.seconds,
            interval: Duration::from_millis(self.interval_ms),
            max_bytes: self.record_bytes,
        }
    }
}

fn recording_duration(value: &str) -> Result<Duration, String> {
    let seconds: f64 = value.parse().map_err(|_| "invalid seconds")?;
    if !seconds.is_finite() || seconds <= 0.0 || seconds > 1800.0 {
        return Err("seconds must be >0 and <=1800".into());
    }
    Ok(Duration::from_secs_f64(seconds))
}

#[derive(Clone, Copy, ValueEnum)]
enum Scope {
    Research,
    Readable,
    Heap,
    Image,
}

#[derive(Args)]
pub struct Selection {
    /// Mapping scope: research includes private writable memory and the game image.
    #[arg(long, value_enum, default_value = "research")]
    scope: Scope,
    /// Restrict to START:LENGTH (decimal or 0x hex); repeat for multiple ranges.
    #[arg(long = "range", value_parser = range)]
    ranges: Vec<Range<u64>>,
}

impl Selection {
    pub fn query(&self) -> query::Selection {
        query::Selection {
            scope: match self.scope {
                Scope::Research => query::Scope::Research,
                Scope::Readable => query::Scope::Readable,
                Scope::Heap => query::Scope::Heap,
                Scope::Image => query::Scope::Image,
            },
            ranges: self.ranges.clone(),
        }
    }
}

#[derive(Args)]
pub struct Walk {
    #[command(flatten)]
    selection: Selection,
    /// Bytes inspected from each pointer target.
    #[arg(long, default_value_t = 512)]
    block_size: usize,
    #[arg(long, default_value_t = 16)]
    depth: u8,
    #[arg(long, default_value_t = 8192)]
    blocks: usize,
    /// Pointer-field stride; use 1 for packed structures.
    #[arg(long, default_value_t = 8)]
    stride: usize,
}

impl Walk {
    pub fn query(&self) -> query::WalkOptions {
        query::WalkOptions {
            selection: self.selection.query(),
            block_size: self.block_size,
            max_depth: self.depth,
            max_blocks: self.blocks,
            stride: self.stride,
        }
    }
}

pub fn number(value: &str) -> Result<u64, String> {
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).map_err(|_| format!("invalid number: {value}"))
    } else {
        value
            .parse()
            .map_err(|_| format!("invalid number: {value}"))
    }
}

pub fn range(value: &str) -> Result<Range<u64>, String> {
    let (start, length) = value.split_once(':').ok_or("range requires START:LENGTH")?;
    target(number(start)?, number(length)?)
}

pub fn target(start: u64, length: u64) -> Result<Range<u64>, String> {
    let end = start
        .checked_add(length)
        .filter(|end| *end > start)
        .ok_or("empty or overflowing range")?;
    Ok(start..end)
}

#[derive(Clone)]
pub struct Hex(pub Vec<u8>);

pub fn hex(value: &str) -> Result<Hex, String> {
    let compact: String = value
        .chars()
        .filter(|c| !matches!(c, ' ' | ':' | '_' | '-'))
        .collect();
    let compact = compact
        .strip_prefix("0x")
        .or_else(|| compact.strip_prefix("0X"))
        .unwrap_or(&compact);
    if compact.is_empty()
        || !compact.len().is_multiple_of(2)
        || !compact.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(format!("invalid hexadecimal byte sequence: {value}"));
    }
    compact
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Hex)
}

pub fn split(value: &str) -> Result<char, String> {
    match value.to_ascii_uppercase().as_str() {
        "H" => Ok('H'),
        "B" => Ok('B'),
        "F" => Ok('F'),
        _ => Err("split must be H, B or F".into()),
    }
}

fn duration(value: &str) -> Result<Duration, String> {
    let seconds = value.parse::<f64>().map_err(|e| e.to_string())?;
    if !seconds.is_finite() || seconds <= 0.0 || seconds > 300.0 {
        return Err("seconds must be >0 and <=300".into());
    }
    Ok(Duration::from_secs_f64(seconds))
}

#[cfg(test)]
mod tests {
    use super::super::{Cli, Command, game::Game};
    use clap::Parser;

    #[test]
    fn equipment_requires_explicit_weapon_and_exclusive_source() {
        for source in [vec![], vec!["--pid", "42"], vec!["--capture", "evidence"]] {
            let mut args = vec!["wfinspect", "game", "equipment", "--weapon", "0x25ea8ae0"];
            args.extend(source);
            let cli = Cli::try_parse_from(args).unwrap();
            assert!(
                matches!(cli.command, Command::Game(Game::Equipment(options)) if options.weapon == 0x25ea8ae0)
            );
        }
        for args in [
            vec!["wfinspect", "game", "equipment"],
            vec!["wfinspect", "game", "equipment", "--weapon", "unknown"],
            vec![
                "wfinspect",
                "game",
                "equipment",
                "--weapon",
                "42",
                "--pid",
                "42",
                "--capture",
                "evidence",
            ],
            vec![
                "wfinspect",
                "game",
                "equipment",
                "--weapon",
                "42",
                "--rank",
                "1",
            ],
        ] {
            assert!(Cli::try_parse_from(args).is_err());
        }
    }
}
