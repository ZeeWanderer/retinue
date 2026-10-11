use super::query::{self, Coverage, Selection, WalkOptions, WalkReport};
use crate::process::{ProcessIdentity, ProcessMemory};
use memchr::memmem;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;

const CHUNK: usize = 4 * 1024 * 1024;
const MAX_READ_BYTES: usize = 1024 * 1024;
const MAX_PATTERN_BYTES: usize = 4096;
const MAX_MATCHES: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Source {
    Live(u32),
    Capture(PathBuf),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SourceMetadata {
    pub kind: String,
    pub pid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<ProcessIdentity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture: Option<CaptureMetadata>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CaptureMetadata {
    pub report: PathBuf,
    pub schema: u8,
    pub captured_at_unix_ms: u128,
    pub memory: ReplayMemorySummary,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReplayMemorySummary {
    pub blocks: usize,
    pub bytes: usize,
    pub truncated: bool,
}

#[derive(Debug)]
pub struct OpenedSource {
    pub memory: ProcessMemory,
    pub metadata: SourceMetadata,
}

impl OpenedSource {
    pub fn open(source: Source) -> Result<Self, String> {
        match source {
            Source::Live(pid) => Ok(Self {
                memory: ProcessMemory::open(pid)?,
                metadata: SourceMetadata {
                    kind: "live".into(),
                    pid,
                    identity: crate::process::identify_process(pid).ok(),
                    capture: None,
                },
            }),
            Source::Capture(path) => {
                let path = if path.is_dir() {
                    path.join("memory.json")
                } else {
                    path
                };
                let (memory, capture) = super::capture::load(&path)?;
                let mut metadata = capture.source;
                metadata.kind = "capture".into();
                metadata.capture = Some(CaptureMetadata {
                    report: path,
                    schema: 1,
                    captured_at_unix_ms: capture.captured_at_unix_ms,
                    memory: ReplayMemorySummary {
                        blocks: capture.blocks.len(),
                        bytes: capture.bytes,
                        truncated: capture.truncated,
                    },
                });
                let image = metadata
                    .identity
                    .as_ref()
                    .map(|id| id.executable.path.clone());
                Ok(Self {
                    memory: memory.with_image_path(image),
                    metadata,
                })
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MemoryMapReport {
    pub source: SourceMetadata,
    pub regions: Vec<MemoryRegion>,
    pub searchable_ranges: Vec<MemoryRange>,
}

#[derive(Debug, Serialize)]
pub struct MemoryRegion {
    pub start: u64,
    pub end: u64,
    pub permissions: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct MemoryRange {
    pub start: u64,
    pub end: u64,
    pub permissions: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct ByteScanReport {
    pub source: SourceMetadata,
    pub pattern: String,
    pub matches: Vec<ByteMatch>,
    pub truncated: bool,
    pub searched_bytes: u64,
    pub candidate_bytes: u64,
    pub selection: Selection,
    pub coverage: Coverage,
    pub total_ms: u128,
}

#[derive(Debug, Serialize)]
pub struct ByteMatch {
    pub address: u64,
    pub region: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pointer: Option<u64>,
}

#[derive(Serialize)]
pub struct MemoryWalkReport {
    pub source: SourceMetadata,
    pub options: WalkOptions,
    pub path: WalkReport,
}

pub fn path(
    source: OpenedSource,
    root: u64,
    target: Option<std::ops::Range<u64>>,
    options: &WalkOptions,
) -> Result<MemoryWalkReport, String> {
    let walk = query::walk(&source.memory, root, target, options)?;
    Ok(MemoryWalkReport {
        source: source.metadata,
        options: options.clone(),
        path: walk.report,
    })
}

pub fn capture(
    source: OpenedSource,
    directory: &std::path::Path,
    roots: &[u64],
    options: &WalkOptions,
) -> Result<super::capture::Capture, String> {
    super::capture::write(&source.memory, source.metadata, directory, roots, options)
}

pub fn maps(source: OpenedSource) -> Result<MemoryMapReport, String> {
    let regions = source
        .memory
        .regions()
        .iter()
        .map(|region| MemoryRegion {
            start: region.start,
            end: region.end,
            permissions: region.permissions.clone(),
            path: region.path.clone(),
        })
        .collect();
    let searchable_ranges = source
        .memory
        .scan_ranges()
        .into_iter()
        .map(|range| MemoryRange {
            start: range.start,
            end: range.end,
            permissions: range.permissions,
            path: range.path,
        })
        .collect();
    Ok(MemoryMapReport {
        source: source.metadata,
        regions,
        searchable_ranges,
    })
}

pub fn read(source: OpenedSource, address: u64, length: usize) -> Result<Vec<u8>, String> {
    if length == 0 || length > MAX_READ_BYTES {
        return Err(format!(
            "memory read length must be between 1 and {MAX_READ_BYTES} bytes"
        ));
    }
    if !source.memory.supports_read_range(address, length) {
        return Err("memory range is unreadable or absent from capture".to_owned());
    }
    let mut bytes = vec![0_u8; length];
    source
        .memory
        .read_exact_at(&mut bytes, address)
        .map_err(|error| format!("could not read memory at 0x{address:x}: {error}"))?;
    Ok(bytes)
}

pub fn scan(source: OpenedSource, pattern: &[u8]) -> Result<ByteScanReport, String> {
    scan_with(
        source,
        pattern,
        &Selection::default(),
        MAX_MATCHES,
        1024 * 1024 * 1024,
    )
}

pub fn scan_with(
    source: OpenedSource,
    pattern: &[u8],
    selection: &Selection,
    limit: usize,
    max_bytes: u64,
) -> Result<ByteScanReport, String> {
    if pattern.is_empty() || pattern.len() > MAX_PATTERN_BYTES {
        return Err(format!(
            "memory scan pattern must be between 1 and {MAX_PATTERN_BYTES} bytes"
        ));
    }
    scan_query(source, Needle::Bytes(pattern), selection, limit, max_bytes)
}

pub fn references(
    source: OpenedSource,
    target: std::ops::Range<u64>,
    selection: &Selection,
    limit: usize,
    max_bytes: u64,
) -> Result<ByteScanReport, String> {
    if target.is_empty() {
        return Err("pointer target range is empty".into());
    }
    scan_query(source, Needle::Pointer(target), selection, limit, max_bytes)
}

enum Needle<'a> {
    Bytes(&'a [u8]),
    Pointer(std::ops::Range<u64>),
}

fn scan_query(
    source: OpenedSource,
    needle: Needle<'_>,
    selection: &Selection,
    limit: usize,
    max_bytes: u64,
) -> Result<ByteScanReport, String> {
    scan_opened(&source, needle, selection, limit, max_bytes)
}

pub fn find_bytes(
    memory: &ProcessMemory,
    metadata: &SourceMetadata,
    pattern: &[u8],
    selection: &Selection,
    limit: usize,
    max_bytes: u64,
) -> Result<ByteScanReport, String> {
    if pattern.is_empty() || pattern.len() > MAX_PATTERN_BYTES {
        return Err("invalid scan pattern size".into());
    }
    scan_memory(
        memory,
        metadata,
        Needle::Bytes(pattern),
        selection,
        limit,
        max_bytes,
    )
}

fn scan_opened(
    source: &OpenedSource,
    needle: Needle<'_>,
    selection: &Selection,
    limit: usize,
    max_bytes: u64,
) -> Result<ByteScanReport, String> {
    scan_memory(
        &source.memory,
        &source.metadata,
        needle,
        selection,
        limit,
        max_bytes,
    )
}

fn scan_memory(
    memory: &ProcessMemory,
    metadata: &SourceMetadata,
    needle: Needle<'_>,
    selection: &Selection,
    limit: usize,
    max_bytes: u64,
) -> Result<ByteScanReport, String> {
    if limit == 0 || limit > 1_000_000 || max_bytes == 0 {
        return Err("scan requires 1..1000000 matches and a nonzero byte budget".into());
    }
    let started = Instant::now();
    let ranges = selection.ranges(memory);
    let candidate_bytes = ranges
        .iter()
        .map(|range| range.end.saturating_sub(range.start))
        .sum();
    let mut buffer = vec![0_u8; CHUNK];
    let width = match &needle {
        Needle::Bytes(pattern) => pattern.len(),
        Needle::Pointer(_) => 8,
    };
    let overlap = width - 1;
    let mut tail = Vec::new();
    let mut matches = Vec::new();
    let mut coverage = Coverage::default();

    'ranges: for range in ranges {
        let mut offset = range.start;
        tail.clear();
        while offset < range.end {
            if coverage.examined_bytes == max_bytes {
                coverage.work_limited = true;
                break 'ranges;
            }
            let wanted = (range.end - offset)
                .min(CHUNK as u64)
                .min(max_bytes - coverage.examined_bytes) as usize;
            let read = match memory.read_at(&mut buffer[..wanted], offset) {
                Ok(0) => {
                    coverage.unreadable_blocks += 1;
                    break;
                }
                Ok(read) => read,
                Err(_) => {
                    coverage.unreadable_blocks += 1;
                    break;
                }
            };
            let base = offset.saturating_sub(tail.len() as u64);
            let mut searchable = Vec::with_capacity(tail.len() + read);
            searchable.extend_from_slice(&tail);
            searchable.extend_from_slice(&buffer[..read]);
            let hits: Box<dyn Iterator<Item = (usize, Option<u64>)> + '_> = match &needle {
                Needle::Bytes(pattern) => {
                    Box::new(memmem::find_iter(&searchable, pattern).map(|index| (index, None)))
                }
                Needle::Pointer(target) => {
                    Box::new((0..searchable.len().saturating_sub(7)).filter_map(|index| {
                        if !(base + index as u64).is_multiple_of(8) {
                            return None;
                        }
                        let pointer =
                            u64::from_le_bytes(searchable[index..index + 8].try_into().unwrap());
                        target.contains(&pointer).then_some((index, Some(pointer)))
                    }))
                }
            };
            for (index, pointer) in hits {
                matches.push(ByteMatch {
                    address: base + index as u64,
                    region: range.path.clone(),
                    pointer,
                });
                if matches.len() >= limit {
                    coverage.examined_bytes += (index + width).saturating_sub(tail.len()) as u64;
                    coverage.match_limited = true;
                    break 'ranges;
                }
            }
            coverage.examined_bytes += read as u64;
            tail.clear();
            tail.extend_from_slice(&searchable[searchable.len().saturating_sub(overlap)..]);
            offset += read as u64;
        }
    }

    Ok(ByteScanReport {
        source: metadata.clone(),
        pattern: match needle {
            Needle::Bytes(pattern) => pattern.iter().map(|byte| format!("{byte:02x}")).collect(),
            Needle::Pointer(target) => format!("pointer:0x{:x}..0x{:x}", target.start, target.end),
        },
        matches,
        truncated: coverage.incomplete(),
        searched_bytes: coverage.examined_bytes,
        candidate_bytes,
        selection: selection.clone(),
        coverage,
        total_ms: started.elapsed().as_millis(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live() -> OpenedSource {
        OpenedSource::open(Source::Live(std::process::id())).unwrap()
    }

    #[test]
    fn reads_and_scans_an_explicit_non_game_process() {
        let bytes = Box::new(*b"retinue generic process fixture");
        let address = bytes.as_ptr() as u64;
        assert_eq!(read(live(), address, bytes.len()).unwrap(), *bytes);
        let selection = Selection {
            scope: query::Scope::Readable,
            ranges: vec![address..address + bytes.len() as u64],
        };
        let result = scan_with(live(), &bytes[..], &selection, 1, 4096).unwrap();
        assert_eq!(result.matches[0].address, address);
        assert_eq!(result.source.pid, std::process::id());
    }

    #[test]
    fn validates_bounded_queries() {
        assert!(read(live(), 0, 0).is_err());
        assert!(read(live(), 0, MAX_READ_BYTES + 1).is_err());
        assert!(scan(live(), &[]).is_err());
        assert!(scan(live(), &vec![0; MAX_PATTERN_BYTES + 1]).is_err());
        assert!(references(live(), 1..1, &Selection::default(), 1, 1).is_err());
    }
}
