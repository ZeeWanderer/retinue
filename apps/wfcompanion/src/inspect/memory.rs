use std::path::PathBuf;
use std::time::Instant;

use super::query::{self, Coverage, Selection, WalkOptions, WalkReport};
use memchr::memmem;
use serde::{Deserialize, Serialize};

use crate::game_observer::adapter::{self, AdapterSupport};
use crate::game_observer::memory::ProcessMemory;
use crate::game_observer::ui::{
    self, BoundedScanMetrics, PointerReferences, RelicRewardText, RelicSelection, ScanMetrics,
    Snapshot, TextScan,
};
use crate::game_observer::{ProcessIdentity, identify_process};
use crate::observation::ui_capture::{self, ReplayMemorySummary};

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

#[derive(Debug, Serialize)]
pub struct UiStateReport {
    pub source: SourceMetadata,
    pub adapter: AdapterSupport,
    pub state: Probe<UiState>,
}

#[derive(Debug, Serialize)]
pub struct UiState {
    pub snapshot: Snapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<BoundedScanMetrics>,
}

#[derive(Debug, Serialize)]
pub struct UiMoviesReport {
    pub source: SourceMetadata,
    pub snapshot: Snapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<ScanMetrics>,
}

#[derive(Debug, Serialize)]
pub struct UiTextReport {
    pub source: SourceMetadata,
    pub text: TextScan,
}

#[derive(Debug, Serialize)]
pub struct UiPointerReport {
    pub source: SourceMetadata,
    pub pointers: PointerReferences,
}

#[derive(Debug, Serialize)]
pub struct UiRelicReport {
    pub source: SourceMetadata,
    pub adapter: AdapterSupport,
    pub selection: Probe<RelicSelection>,
    pub rewards: Probe<RelicRewardText>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Probe<T> {
    Available { value: T },
    Unavailable { reason: String },
}

pub(super) struct OpenedSource {
    pub memory: ProcessMemory,
    pub metadata: SourceMetadata,
    scaleform: Option<adapter::ScaleformLayout>,
    typed_capture: bool,
}

impl OpenedSource {
    pub(super) fn open(source: Source) -> Result<Self, String> {
        match source {
            Source::Live(pid) => {
                let memory = ProcessMemory::open(pid)?;
                let identity = identify_process(pid)?;
                Ok(Self {
                    memory,
                    scaleform: None,
                    typed_capture: false,
                    metadata: SourceMetadata {
                        kind: "live".into(),
                        pid,
                        identity: Some(identity),
                        capture: None,
                    },
                })
            }
            Source::Capture(path) => {
                let generic = if path.is_dir() {
                    path.join("memory.json")
                } else {
                    path.clone()
                };
                if generic
                    .file_name()
                    .is_some_and(|name| name == "memory.json")
                    && generic.is_file()
                {
                    return load_capture(&generic);
                }
                let evidence = ui_capture::load_evidence(&path)?;
                let metadata = SourceMetadata {
                    kind: "capture".into(),
                    pid: evidence.snapshot.pid,
                    identity: evidence.identity,
                    capture: Some(CaptureMetadata {
                        report: evidence.report,
                        schema: evidence.schema,
                        captured_at_unix_ms: evidence.captured_at_unix_ms,
                        memory: evidence.memory_summary,
                    }),
                };
                Ok(Self {
                    memory: evidence.memory,
                    metadata,
                    scaleform: evidence.scaleform,
                    typed_capture: evidence.typed_reads,
                })
            }
        }
    }

    fn layout(&self) -> Result<adapter::ScaleformLayout, String> {
        if self.metadata.kind == "capture" {
            return adapter::replay_scaleform(self.metadata.identity.as_ref(), self.scaleform);
        }
        let identity = self
            .metadata
            .identity
            .as_ref()
            .ok_or("capture has no executable identity")?;
        adapter::require_scaleform(identity)
    }

    fn typed_snapshot(
        &self,
        layout: adapter::ScaleformLayout,
    ) -> Result<(Snapshot, Option<BoundedScanMetrics>), String> {
        if self.metadata.kind == "capture" && !self.typed_capture {
            return ui::scan_memory(&self.memory).map(|scan| (scan.snapshot, None));
        }
        ui::scan_registry_memory(&self.memory, layout)
            .map(|scan| (scan.snapshot, Some(scan.metrics)))
            .map_err(|(stage, reason)| format!("{stage}: {reason}"))
    }
}

impl<T> Probe<T> {
    fn from_result(result: Result<T, String>) -> Self {
        match result {
            Ok(value) => Self::Available { value },
            Err(reason) => Self::Unavailable { reason },
        }
    }
}

fn load_capture(path: &std::path::Path) -> Result<OpenedSource, String> {
    let (memory, capture) = super::capture::load(path)?;
    let mut metadata = capture.source;
    metadata.kind = "capture".into();
    metadata.capture = Some(CaptureMetadata {
        report: path.to_owned(),
        schema: 1,
        captured_at_unix_ms: capture.captured_at_unix_ms,
        memory: ReplayMemorySummary {
            blocks: capture.blocks.len(),
            bytes: capture.bytes,
            truncated: capture.truncated,
        },
    });
    Ok(OpenedSource {
        memory,
        metadata,
        scaleform: None,
        typed_capture: false,
    })
}

#[derive(Serialize)]
pub struct MemoryWalkReport {
    pub source: SourceMetadata,
    pub options: WalkOptions,
    pub path: WalkReport,
}

pub fn path(
    source: Source,
    root: u64,
    target: Option<std::ops::Range<u64>>,
    options: &WalkOptions,
) -> Result<MemoryWalkReport, String> {
    let source = OpenedSource::open(source)?;
    let walk = query::walk(&source.memory, root, target, options)?;
    Ok(MemoryWalkReport {
        source: source.metadata,
        options: options.clone(),
        path: walk.report,
    })
}

pub fn capture(
    source: Source,
    directory: &std::path::Path,
    roots: &[u64],
    options: &WalkOptions,
) -> Result<super::capture::Capture, String> {
    let source = OpenedSource::open(source)?;
    super::capture::write(&source.memory, source.metadata, directory, roots, options)
}

pub fn maps(source: Source) -> Result<MemoryMapReport, String> {
    let source = OpenedSource::open(source)?;
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

pub fn read(source: Source, address: u64, length: usize) -> Result<Vec<u8>, String> {
    if length == 0 || length > MAX_READ_BYTES {
        return Err(format!(
            "memory read length must be between 1 and {MAX_READ_BYTES} bytes"
        ));
    }
    let source = OpenedSource::open(source)?;
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

pub fn scan(source: Source, pattern: &[u8]) -> Result<ByteScanReport, String> {
    scan_with(
        source,
        pattern,
        &Selection::default(),
        MAX_MATCHES,
        1024 * 1024 * 1024,
    )
}

pub fn scan_with(
    source: Source,
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
    source: Source,
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
    source: Source,
    needle: Needle<'_>,
    selection: &Selection,
    limit: usize,
    max_bytes: u64,
) -> Result<ByteScanReport, String> {
    scan_opened(
        &OpenedSource::open(source)?,
        needle,
        selection,
        limit,
        max_bytes,
    )
}

pub(super) fn find_bytes(
    source: &OpenedSource,
    pattern: &[u8],
    selection: &Selection,
    limit: usize,
    max_bytes: u64,
) -> Result<ByteScanReport, String> {
    if pattern.is_empty() || pattern.len() > MAX_PATTERN_BYTES {
        return Err("invalid scan pattern size".into());
    }
    scan_opened(source, Needle::Bytes(pattern), selection, limit, max_bytes)
}

fn scan_opened(
    source: &OpenedSource,
    needle: Needle<'_>,
    selection: &Selection,
    limit: usize,
    max_bytes: u64,
) -> Result<ByteScanReport, String> {
    if limit == 0 || limit > 1_000_000 || max_bytes == 0 {
        return Err("scan requires 1..1000000 matches and a nonzero byte budget".into());
    }
    let started = Instant::now();
    let ranges = selection.ranges(&source.memory);
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
            let read = match source.memory.read_at(&mut buffer[..wanted], offset) {
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
        source: source.metadata.clone(),
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

pub fn ui_state(source: Source) -> Result<UiStateReport, String> {
    let source = OpenedSource::open(source)?;
    let layout = source.layout();
    let adapter = adapter::layout_support(&layout);
    let state = layout
        .and_then(|layout| source.typed_snapshot(layout))
        .map(|(snapshot, metrics)| UiState { snapshot, metrics });
    Ok(UiStateReport {
        source: source.metadata,
        adapter,
        state: Probe::from_result(state),
    })
}

pub fn ui_movies(source: Source) -> Result<UiMoviesReport, String> {
    let source = OpenedSource::open(source)?;
    let scan = ui::scan_memory(&source.memory)?;
    let (snapshot, metrics) = (scan.snapshot, Some(scan.metrics));
    Ok(UiMoviesReport {
        source: source.metadata,
        snapshot,
        metrics,
    })
}

pub fn ui_find(source: Source, terms: &[String]) -> Result<UiTextReport, String> {
    let source = OpenedSource::open(source)?;
    let text = ui::scan_text_memory(&source.memory, terms)?;
    Ok(UiTextReport {
        source: source.metadata,
        text,
    })
}

#[derive(Serialize)]
pub struct UiObjectsReport {
    pub source: SourceMetadata,
    pub movies: Vec<MovieObjects>,
}

#[derive(Serialize)]
pub struct MovieObjects {
    pub movie: ui::Movie,
    pub objects: Probe<ui::ObjectReport>,
}

pub fn ui_objects(
    source: Source,
    movie: &str,
    max_text_bytes: usize,
    limit: usize,
) -> Result<UiObjectsReport, String> {
    let source = OpenedSource::open(source)?;
    let layout = source.layout()?;
    let (snapshot, _) = source.typed_snapshot(layout)?;
    let mut movies = Vec::new();
    for found in snapshot
        .movies
        .into_iter()
        .filter(|found| found.path.contains(movie))
    {
        let objects = found
            .record_address
            .checked_sub(0xc0)
            .ok_or("invalid movie address")
            .map_err(str::to_owned)
            .and_then(|flash| {
                ui::enumerate_text_objects(&source.memory, layout, flash, max_text_bytes, limit)
            });
        movies.push(MovieObjects {
            movie: found,
            objects: Probe::from_result(objects),
        });
    }
    if movies.is_empty() {
        return Err(format!("no movie matched {movie:?} in available memory"));
    }
    Ok(UiObjectsReport {
        source: source.metadata,
        movies,
    })
}

pub fn ui_refs(
    source: Source,
    target_start: u64,
    target_end: u64,
) -> Result<UiPointerReport, String> {
    let source = OpenedSource::open(source)?;
    let pointers = ui::scan_pointers_memory(&source.memory, target_start, target_end)?;
    Ok(UiPointerReport {
        source: source.metadata,
        pointers,
    })
}

pub fn ui_relic(source: Source) -> Result<UiRelicReport, String> {
    let source = OpenedSource::open(source)?;
    let layout = source.layout();
    let adapter = adapter::layout_support(&layout);
    let snapshot = layout.and_then(|layout| {
        source
            .typed_snapshot(layout)
            .map(|(snapshot, _metrics)| (layout, snapshot))
    });
    let (selection, rewards) = match snapshot {
        Ok((layout, snapshot)) => (
            Probe::from_result(ui::relic_selection_memory(
                &source.memory,
                layout,
                &snapshot,
            )),
            Probe::from_result(ui::relic_rewards_memory(&source.memory, layout, &snapshot)),
        ),
        Err(reason) => (
            Probe::Unavailable {
                reason: reason.clone(),
            },
            Probe::Unavailable { reason },
        ),
    };
    Ok(UiRelicReport {
        source: source.metadata,
        adapter,
        selection,
        rewards,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn capture() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "wfinspect-memory-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let base = 0x200000000_u64;
        let mut bytes = vec![0_u8; 256];
        bytes[0x40..0x47].copy_from_slice(b"NEO ERA");
        bytes[0x10..0x18].copy_from_slice(&(base + 0x40).to_le_bytes());
        std::fs::write(directory.join("scaleform-memory.bin"), &bytes).unwrap();
        std::fs::write(
            directory.join("maps.txt"),
            "140000000-141000000 r-xp 0 00:00 1 /game/Warframe.x64.exe\n\
             200000000-200001000 rw-p 0 00:00 0 [heap]\n",
        )
        .unwrap();
        let report = serde_json::json!({
            "schema": 2,
            "captured_at_unix_ms": 123,
            "snapshot": {"pid": 42, "movies": []},
            "memory": {
                "file": "scaleform-memory.bin",
                "block_size": 512,
                "max_depth": 4,
                "max_blocks": 16,
                "max_blocks_per_root": 4,
                "max_children_per_block": 4,
                "bytes": bytes.len(),
                "truncated": false,
                "roots": [],
                "blocks": [{
                    "address": base,
                    "length": bytes.len(),
                    "file_offset": 0,
                    "depth": 0,
                    "permissions": "rw-p",
                    "region": "[heap]"
                }]
            }
        });
        std::fs::write(
            directory.join("ui.json"),
            serde_json::to_vec(&report).unwrap(),
        )
        .unwrap();
        directory
    }

    #[test]
    fn validates_read_and_scan_bounds_without_opening_source() {
        assert!(read(Source::Live(0), 0, 0).is_err());
        assert!(read(Source::Live(0), 0, MAX_READ_BYTES + 1).is_err());
        assert!(scan(Source::Live(0), &[]).is_err());
        assert!(scan(Source::Live(0), &vec![0; MAX_PATTERN_BYTES + 1]).is_err());
    }

    #[test]
    fn capture_supports_same_research_queries_as_live_memory() {
        let directory = capture();
        let source = || Source::Capture(directory.clone());
        let base = 0x200000000_u64;

        assert_eq!(read(source(), base + 0x40, 7).unwrap(), b"NEO ERA");
        assert_eq!(scan(source(), b"NEO ERA").unwrap().matches.len(), 1);
        let text = ui_find(source(), &["NEO ERA".to_owned()]).unwrap();
        assert_eq!(text.text.terms[0].matches[0].address, base + 0x40);
        let refs = ui_refs(source(), base + 0x40, base + 0x47).unwrap();
        assert_eq!(refs.pointers.references[0].address, base + 0x10);
        let path = path(
            source(),
            base,
            Some(base + 0x40..base + 0x47),
            &WalkOptions::default(),
        )
        .unwrap();
        assert_eq!(path.path.hops.len(), 1);
        assert_eq!(ui_movies(source()).unwrap().snapshot.movies.len(), 0);

        std::fs::remove_dir_all(directory).unwrap();
    }
}
