use std::collections::{BTreeSet, HashMap, VecDeque};
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::game_observer::ProcessIdentity;
use crate::game_observer::adapter::{self, AdapterSupport, ScaleformLayout};
use crate::game_observer::memory::{CaptureBlock, ProcessMemory, Region};
use crate::game_observer::ui::{self, Movie, ScanMetrics, Snapshot, TextScan};
use retinue_native::recording::Directory;
use retinue_native::work::{Budget, Limits};

mod typed;

const BLOCK_SIZE: usize = 512;
const MAX_DEPTH: u8 = 6;
const MAX_BLOCKS: usize = 65_536;
const MAX_BLOCKS_PER_ROOT: usize = 1024;
const MAX_CHILDREN: usize = 48;
const FLASH_RECORD_OFFSET: u64 = 0xc0;
const MEMORY_FILE: &str = "scaleform-memory.bin";
const MAX_CAPTURE_BYTES: usize = MAX_BLOCKS * BLOCK_SIZE;

#[derive(Clone, Debug, Serialize)]
pub struct EvidenceSummary {
    pub report: PathBuf,
    pub memory: PathBuf,
    pub maps: PathBuf,
    pub movies: usize,
    pub blocks: usize,
    pub bytes: usize,
    pub truncated: bool,
    pub duration_ms: u128,
}

#[derive(Debug, Serialize)]
pub struct EvidenceReplay {
    pub report: PathBuf,
    pub schema: u8,
    pub captured_at_unix_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<ProcessIdentity>,
    pub adapter: AdapterSupport,
    pub snapshot: Snapshot,
    pub recorded_snapshot: Snapshot,
    pub memory: ReplayMemorySummary,
    pub selection: ReplayProbe<ui::RelicSelection>,
    pub rewards: ReplayProbe<ui::RelicRewardText>,
}

pub use retinue_native::inspect::memory::ReplayMemorySummary;

pub(crate) struct LoadedEvidence {
    pub(crate) report: PathBuf,
    pub(crate) schema: u8,
    pub(crate) captured_at_unix_ms: u128,
    pub(crate) identity: Option<ProcessIdentity>,
    pub(crate) scaleform: Option<ScaleformLayout>,
    pub(crate) snapshot: Snapshot,
    pub(crate) memory_summary: ReplayMemorySummary,
    pub(crate) memory: ProcessMemory,
    pub(crate) typed_reads: bool,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ReplayProbe<T> {
    Available { value: T },
    Unavailable { reason: String },
}

#[derive(Debug, Serialize)]
struct Report {
    schema: u8,
    captured_at_unix_ms: u128,
    duration_ms: u128,
    identity: ProcessIdentity,
    #[serde(skip_serializing_if = "Option::is_none")]
    scaleform: Option<ScaleformLayout>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scaleform_error: Option<String>,
    snapshot: Snapshot,
    scan: CaptureScan,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<TextScan>,
    memory: MemoryManifest,
}

#[derive(Debug, Serialize)]
#[serde(tag = "method", rename_all = "snake_case")]
enum CaptureScan {
    TypedReads {
        registry: ui::BoundedScanMetrics,
        movies: Vec<typed::MovieCapture>,
    },
    HeapGraph {
        metrics: ScanMetrics,
        reason: String,
    },
}

#[derive(Debug, Deserialize, Serialize)]
struct MemoryManifest {
    file: String,
    #[serde(default)]
    typed_reads: bool,
    block_size: usize,
    max_depth: u8,
    max_blocks: usize,
    max_blocks_per_root: usize,
    max_children_per_block: usize,
    bytes: usize,
    truncated: bool,
    roots: Vec<MovieRoot>,
    blocks: Vec<MemoryBlock>,
}

#[derive(Debug, Deserialize, Serialize)]
struct MovieRoot {
    path: String,
    record_address: u64,
    path_address: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct MemoryBlock {
    address: u64,
    length: usize,
    file_offset: usize,
    depth: u8,
    permissions: String,
    region: String,
}

#[derive(Debug, Deserialize)]
struct StoredReport {
    schema: u8,
    captured_at_unix_ms: u128,
    #[serde(default)]
    identity: Option<ProcessIdentity>,
    #[serde(default)]
    scaleform: Option<ScaleformLayout>,
    snapshot: Snapshot,
    memory: MemoryManifest,
}

pub struct Frozen {
    report: Report,
    maps: String,
    bytes: Vec<u8>,
}

pub fn capture(pid: u32, directory: &Path, terms: &[String]) -> Result<EvidenceSummary, String> {
    let identity = crate::game_observer::identify_process(pid)?;
    let budget = Budget::new(limits());
    collect(&identity, terms, &budget)?.save(&Directory::create(directory)?.with_budget(budget))
}

pub fn limits() -> Limits {
    Limits {
        duration: Duration::from_secs(30),
        read_bytes: 64 * 1024 * 1024 * 1024,
        write_bytes: 256 * 1024 * 1024,
    }
}

pub fn collect(
    identity: &ProcessIdentity,
    terms: &[String],
    budget: &Budget,
) -> Result<Frozen, String> {
    budget.check().map_err(|error| error.to_string())?;
    let captured_at_unix_ms = unix_time_millis();
    let started = Instant::now();
    let memory = crate::game_observer::memory::open(identity.pid)?.with_budget(budget.clone());
    let maps = memory.maps().to_owned();
    let scaleform = adapter::require_scaleform(identity);
    budget.check().map_err(|error| error.to_string())?;
    if terms.is_empty()
        && let Ok(layout) = scaleform.as_ref()
    {
        match typed::collect(memory, *layout) {
            Ok(capture) => {
                return Ok(Frozen {
                    report: Report {
                        schema: 5,
                        captured_at_unix_ms,
                        duration_ms: started.elapsed().as_millis(),
                        identity: identity.clone(),
                        scaleform: Some(*layout),
                        scaleform_error: None,
                        snapshot: capture.snapshot,
                        scan: CaptureScan::TypedReads {
                            registry: capture.registry,
                            movies: capture.movies,
                        },
                        text: None,
                        memory: capture.manifest,
                    },
                    maps,
                    bytes: capture.bytes,
                });
            }
            Err(reason) => {
                return collect_graph(
                    identity,
                    terms,
                    budget,
                    scaleform,
                    captured_at_unix_ms,
                    started,
                    reason,
                );
            }
        }
    }
    let reason = scaleform
        .as_ref()
        .err()
        .cloned()
        .unwrap_or_else(|| "explicit text search".into());
    collect_graph(
        identity,
        terms,
        budget,
        scaleform,
        captured_at_unix_ms,
        started,
        reason,
    )
}

fn collect_graph(
    identity: &ProcessIdentity,
    terms: &[String],
    budget: &Budget,
    scaleform: Result<ScaleformLayout, String>,
    captured_at_unix_ms: u128,
    started: Instant,
    reason: String,
) -> Result<Frozen, String> {
    budget.check().map_err(|error| error.to_string())?;
    let memory = crate::game_observer::memory::open(identity.pid)?.with_budget(budget.clone());
    let text = (!terms.is_empty())
        .then(|| ui::scan_text_memory(&memory, terms))
        .transpose()?;
    let scan = ui::scan_memory(&memory)?;
    let (mut manifest, bytes) = capture_graph(&memory, &scan.snapshot.movies, text.as_ref())?;
    budget.check().map_err(|error| error.to_string())?;
    manifest.bytes = bytes.len();
    let duration_ms = started.elapsed().as_millis();
    let report = Report {
        schema: 5,
        captured_at_unix_ms,
        duration_ms,
        identity: identity.clone(),
        scaleform: scaleform.as_ref().ok().copied(),
        scaleform_error: scaleform.err(),
        snapshot: scan.snapshot,
        scan: CaptureScan::HeapGraph {
            metrics: scan.metrics,
            reason,
        },
        text,
        memory: manifest,
    };
    Ok(Frozen {
        report,
        maps: memory.maps().to_owned(),
        bytes,
    })
}

impl Frozen {
    pub fn save(self, directory: &Directory) -> Result<EvidenceSummary, String> {
        let maps = directory.write("maps.txt", self.maps.as_bytes())?;
        let memory = directory.write(MEMORY_FILE, &self.bytes)?;
        let mut output = BufWriter::new(directory.file("ui.json")?);
        serde_json::to_writer_pretty(&mut output, &self.report)
            .map_err(|error| format!("could not encode UI evidence: {error}"))?;
        output
            .flush()
            .map_err(|error| format!("could not flush UI evidence: {error}"))?;
        let report = directory.path().join("ui.json");
        Ok(EvidenceSummary {
            report,
            memory,
            maps,
            movies: self.report.snapshot.movies.len(),
            blocks: self.report.memory.blocks.len(),
            bytes: self.report.memory.bytes,
            truncated: self.report.memory.truncated,
            duration_ms: self.report.duration_ms,
        })
    }
}

pub(crate) fn load_evidence(path: &Path) -> Result<LoadedEvidence, String> {
    let report_path = if path.is_dir() {
        path.join("ui.json")
    } else {
        path.to_owned()
    };
    let directory = report_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let report_bytes = fs::read(&report_path)
        .map_err(|error| format!("could not read {}: {error}", report_path.display()))?;
    let report: StoredReport = serde_json::from_slice(&report_bytes)
        .map_err(|error| format!("could not parse {}: {error}", report_path.display()))?;
    if !(2..=5).contains(&report.schema) {
        return Err(format!("unsupported UI evidence schema {}", report.schema));
    }
    if report.memory.file != MEMORY_FILE {
        return Err(format!(
            "unsupported UI evidence memory file: {}",
            report.memory.file
        ));
    }
    if report.memory.blocks.len() > MAX_BLOCKS || report.memory.bytes > MAX_CAPTURE_BYTES {
        return Err("UI evidence exceeds capture limits".to_owned());
    }
    let maps_path = directory.join("maps.txt");
    let maps = fs::read_to_string(&maps_path)
        .map_err(|error| format!("could not read {}: {error}", maps_path.display()))?;
    let memory_path = directory.join(&report.memory.file);
    let bytes = fs::read(&memory_path)
        .map_err(|error| format!("could not read {}: {error}", memory_path.display()))?;
    if bytes.len() != report.memory.bytes {
        return Err(format!(
            "capture memory size mismatch: manifest={} file={}",
            report.memory.bytes,
            bytes.len()
        ));
    }
    let blocks = report
        .memory
        .blocks
        .iter()
        .map(|block| CaptureBlock {
            address: block.address,
            length: block.length,
            file_offset: block.file_offset,
        })
        .collect();
    let memory =
        crate::game_observer::memory::from_capture(report.snapshot.pid, maps, blocks, bytes)?;
    Ok(LoadedEvidence {
        report: report_path,
        schema: report.schema,
        captured_at_unix_ms: report.captured_at_unix_ms,
        identity: report.identity,
        scaleform: report.scaleform,
        snapshot: report.snapshot,
        memory_summary: ReplayMemorySummary {
            blocks: report.memory.blocks.len(),
            bytes: report.memory.bytes,
            truncated: report.memory.truncated,
        },
        memory,
        typed_reads: report.memory.typed_reads,
    })
}

pub fn replay_evidence(path: &Path) -> Result<EvidenceReplay, String> {
    let evidence = load_evidence(path)?;
    let layout = adapter::replay_scaleform(evidence.identity.as_ref(), evidence.scaleform);
    let snapshot = if evidence.typed_reads {
        ui::scan_registry_memory(&evidence.memory, *layout.as_ref().map_err(Clone::clone)?)
            .map_err(|(stage, reason)| format!("{stage}: {reason}"))?
            .snapshot
    } else {
        ui::scan_memory(&evidence.memory)?.snapshot
    };
    let adapter = adapter::layout_support(&layout);
    let (selection, rewards) = match layout {
        Ok(layout) => (
            replay_probe(ui::relic_selection_memory(
                &evidence.memory,
                layout,
                &snapshot,
            )),
            replay_probe(ui::relic_rewards_memory(
                &evidence.memory,
                layout,
                &snapshot,
            )),
        ),
        Err(reason) => (
            ReplayProbe::Unavailable {
                reason: reason.clone(),
            },
            ReplayProbe::Unavailable { reason },
        ),
    };
    Ok(EvidenceReplay {
        report: evidence.report,
        schema: evidence.schema,
        captured_at_unix_ms: evidence.captured_at_unix_ms,
        identity: evidence.identity,
        adapter,
        snapshot,
        recorded_snapshot: evidence.snapshot,
        memory: evidence.memory_summary,
        selection,
        rewards,
    })
}

fn replay_probe<T>(result: Result<T, String>) -> ReplayProbe<T> {
    match result {
        Ok(value) => ReplayProbe::Available { value },
        Err(reason) => ReplayProbe::Unavailable { reason },
    }
}

fn capture_graph(
    memory: &ProcessMemory,
    movies: &[Movie],
    text: Option<&TextScan>,
) -> Result<(MemoryManifest, Vec<u8>), String> {
    let roots = movies
        .iter()
        .map(|movie| MovieRoot {
            path: movie.path.clone(),
            record_address: movie.record_address,
            path_address: movie.path_address,
        })
        .collect::<Vec<_>>();
    let mut root_spans = movies
        .iter()
        .flat_map(|movie| {
            [
                (movie.record_address, 0x68),
                (movie.path_address, movie.path.len() + 1),
            ]
        })
        .collect::<Vec<_>>();
    root_spans.extend(
        movies
            .iter()
            .filter_map(|movie| movie.record_address.checked_sub(FLASH_RECORD_OFFSET))
            .map(|address| (address, FLASH_RECORD_OFFSET as usize)),
    );
    if let Some(text) = text {
        for found in text.terms.iter().flat_map(|term| &term.matches) {
            root_spans.push((found.address, BLOCK_SIZE));
            root_spans.extend(
                found
                    .references
                    .iter()
                    .filter_map(|reference| reference.display_object_candidate)
                    .map(|address| (address, BLOCK_SIZE)),
            );
        }
    }
    let mut graph = MemoryGraph {
        memory,
        bytes: Vec::new(),
        blocks: Vec::new(),
        captured: HashMap::new(),
        truncated: false,
    };
    let mut root_addresses = BTreeSet::new();
    // Retain complete movie headers and paths before spending depth budgets.
    for (address, length) in root_spans {
        let end = address
            .checked_add(length as u64)
            .ok_or("invalid UI capture root")?;
        for block in (block_address(address)..end).step_by(BLOCK_SIZE) {
            root_addresses.insert(block);
            graph.retain(block, 0)?;
        }
    }
    'roots: for root in root_addresses {
        let mut queue = VecDeque::from([(root, 0_u8)]);
        let mut visited = BTreeSet::new();
        while let Some((address, depth)) = queue.pop_front() {
            memory.check_budget().map_err(|error| error.to_string())?;
            if !visited.insert(address) {
                continue;
            }
            if visited.len() > MAX_BLOCKS_PER_ROOT {
                graph.truncated = true;
                continue 'roots;
            }
            let Some(block) = graph.retain(address, depth)? else {
                continue;
            };
            let pointers = pointer_targets(block, memory.regions());
            if depth >= MAX_DEPTH {
                graph.truncated |= pointers
                    .iter()
                    .any(|pointer| !graph.captured.contains_key(&block_address(*pointer)));
                continue;
            }
            if pointers.len() > MAX_CHILDREN {
                graph.truncated = true;
            }
            for pointer in pointers.into_iter().take(MAX_CHILDREN) {
                queue.push_back((block_address(pointer), depth + 1));
            }
        }
    }

    memory.check_budget().map_err(|error| error.to_string())?;
    Ok((
        MemoryManifest {
            file: MEMORY_FILE.to_owned(),
            typed_reads: false,
            block_size: BLOCK_SIZE,
            max_depth: MAX_DEPTH,
            max_blocks: MAX_BLOCKS,
            max_blocks_per_root: MAX_BLOCKS_PER_ROOT,
            max_children_per_block: MAX_CHILDREN,
            bytes: 0,
            truncated: graph.truncated,
            roots,
            blocks: graph.blocks,
        },
        graph.bytes,
    ))
}

struct MemoryGraph<'a> {
    memory: &'a ProcessMemory,
    bytes: Vec<u8>,
    blocks: Vec<MemoryBlock>,
    captured: HashMap<u64, usize>,
    truncated: bool,
}

impl MemoryGraph<'_> {
    fn retain(&mut self, address: u64, depth: u8) -> Result<Option<&[u8]>, String> {
        self.memory
            .check_budget()
            .map_err(|error| error.to_string())?;
        let index = if let Some(&index) = self.captured.get(&address) {
            index
        } else {
            if self.blocks.len() == MAX_BLOCKS {
                self.truncated = true;
                return Ok(None);
            }
            let Some(region) = graph_region(self.memory, address) else {
                return Ok(None);
            };
            let wanted = ((region.end - address).min(BLOCK_SIZE as u64)) as usize;
            let mut bytes = vec![0; wanted];
            let Ok(read) = self.memory.read_at(&mut bytes, address) else {
                return Ok(None);
            };
            if read == 0 {
                return Ok(None);
            }
            let index = self.blocks.len();
            self.blocks.push(MemoryBlock {
                address,
                length: read,
                file_offset: self.bytes.len(),
                depth,
                permissions: region.permissions.clone(),
                region: region.path.clone(),
            });
            self.bytes.extend_from_slice(&bytes[..read]);
            self.captured.insert(address, index);
            index
        };
        let block = &mut self.blocks[index];
        block.depth = block.depth.min(depth);
        Ok(Some(
            &self.bytes[block.file_offset..block.file_offset + block.length],
        ))
    }
}

fn block_address(pointer: u64) -> u64 {
    pointer & !((BLOCK_SIZE as u64) - 1)
}

fn graph_region(memory: &ProcessMemory, address: u64) -> Option<&Region> {
    memory
        .regions()
        .iter()
        .find(|region| region.supports_heap() && region.contains(address))
}

fn pointer_targets(bytes: &[u8], regions: &[Region]) -> Vec<u64> {
    let mut pointers = BTreeSet::new();
    for chunk in bytes.chunks_exact(8) {
        let pointer = u64::from_le_bytes(chunk.try_into().unwrap());
        if pointer & 7 != 0 {
            continue;
        }
        if regions
            .iter()
            .any(|region| region.supports_heap() && region.contains(pointer))
        {
            pointers.insert(pointer);
        }
    }
    pointers.into_iter().collect()
}

fn unix_time_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn memory(bytes: Vec<u8>) -> ProcessMemory {
        crate::game_observer::memory::from_capture(
            42,
            format!(
                "200000000-{:x} rw-p 0 00:00 0 [heap]\n",
                0x200000000_u64 + bytes.len() as u64
            ),
            vec![CaptureBlock {
                address: 0x200000000,
                length: bytes.len(),
                file_offset: 0,
            }],
            bytes,
        )
        .unwrap()
    }

    #[test]
    fn breadth_first_capture_preserves_direct_siblings_under_budget_pressure() {
        let base = 0x200000000_u64;
        let mut bytes = vec![0; 5000 * BLOCK_SIZE];
        for index in 1..625 {
            for child in 0..8 {
                let target = index * 8 + child;
                if target >= 4999 {
                    continue;
                }
                let offset = index * BLOCK_SIZE + child * 8;
                bytes[offset..offset + 8]
                    .copy_from_slice(&(base + (target * BLOCK_SIZE) as u64).to_le_bytes());
            }
        }
        bytes[..8].copy_from_slice(&(base + BLOCK_SIZE as u64).to_le_bytes());
        let sibling = base + (4999 * BLOCK_SIZE) as u64;
        bytes[0x88..0x90].copy_from_slice(&sibling.to_le_bytes());
        let movie = Movie {
            path: "/Lotus/Interface/Test.swf".into(),
            record_address: base + 0xc0,
            path_address: base + 0x100,
            width: 1920,
            height: 1080,
            scale_x: 1.0,
            scale_y: 1.0,
        };
        let (manifest, captured) = capture_graph(&memory(bytes), &[movie], None).unwrap();
        assert!(manifest.truncated);
        assert!(
            manifest
                .blocks
                .iter()
                .any(|block| block.address == sibling && block.depth == 1)
        );
        assert!(captured.len() <= MAX_CAPTURE_BYTES);
    }

    #[test]
    fn movie_headers_and_paths_crossing_blocks_replay_from_retained_bytes() {
        let base = 0x200000000_u64;
        let mut bytes = vec![0; 8192];
        let record = BLOCK_SIZE - 32;
        let path = 4096 - 8;
        let name = "/Lotus/Interface/Test.swf";
        for (offset, value) in [
            (8, 1920_u32),
            (12, 1080),
            (0x20, 1920),
            (0x24, 1080),
            (0x18, 1.0_f32.to_bits()),
            (0x1c, 1.0_f32.to_bits()),
        ] {
            bytes[record + offset..record + offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[record + 0x60..record + 0x68].copy_from_slice(&(base + path as u64).to_le_bytes());
        bytes[path..path + name.len()].copy_from_slice(name.as_bytes());
        let movie = Movie {
            path: name.into(),
            record_address: base + record as u64,
            path_address: base + path as u64,
            width: 1920,
            height: 1080,
            scale_x: 1.0,
            scale_y: 1.0,
        };
        let source = memory(bytes);
        let (manifest, captured) =
            capture_graph(&source, std::slice::from_ref(&movie), None).unwrap();
        let blocks = manifest
            .blocks
            .iter()
            .map(|block| CaptureBlock {
                address: block.address,
                length: block.length,
                file_offset: block.file_offset,
            })
            .collect();
        let replay =
            crate::game_observer::memory::from_capture(42, source.maps().into(), blocks, captured)
                .unwrap();
        assert_eq!(
            ui::scan_memory(&replay).unwrap().snapshot.movies,
            vec![movie]
        );
    }

    #[test]
    fn cancelled_graph_is_not_reported_as_an_empty_success() {
        let budget = Budget::new(limits());
        let memory = crate::game_observer::memory::from_capture(42, String::new(), vec![], vec![])
            .unwrap()
            .with_budget(budget.clone());
        budget.cancel();
        assert!(
            capture_graph(&memory, &[], None)
                .unwrap_err()
                .contains("cancelled")
        );
    }

    #[test]
    fn heap_scans_cannot_exceed_the_shared_read_budget() {
        let budget = Budget::new(Limits {
            read_bytes: 4096,
            ..limits()
        });
        let memory = crate::game_observer::memory::from_capture(
            42,
            "200000000-200002000 rw-p 0 00:00 0 [heap]\n".into(),
            vec![CaptureBlock {
                address: 0x200000000,
                length: 8192,
                file_offset: 0,
            }],
            vec![0; 8192],
        )
        .unwrap()
        .with_budget(budget.clone());
        assert!(
            ui::scan_memory(&memory)
                .unwrap_err()
                .contains("read budget exceeded")
        );
        assert_eq!(budget.usage().read_bytes_reserved, 0);
    }

    #[test]
    fn saving_frozen_evidence_preserves_read_interval_and_replays() {
        let identity = ProcessIdentity {
            pid: 42,
            executable: crate::game_observer::ExecutableIdentity {
                path: "/game/Warframe.x64.exe".into(),
                size: 1,
                modified_unix_ms: None,
                sha256: "unknown".into(),
            },
        };
        let memory = crate::game_observer::memory::from_capture(
            42,
            "200000000-200001000 rw-p 0 00:00 0 [heap]\n".into(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        let scan = ui::scan_memory(&memory).unwrap();
        let (manifest, bytes) = capture_graph(&memory, &[], None).unwrap();
        let frozen = Frozen {
            report: Report {
                schema: 4,
                captured_at_unix_ms: 123,
                duration_ms: 7,
                identity,
                scaleform: Some(adapter::test_scaleform()),
                scaleform_error: None,
                snapshot: scan.snapshot,
                scan: CaptureScan::HeapGraph {
                    metrics: scan.metrics,
                    reason: "test".into(),
                },
                text: None,
                memory: manifest,
            },
            maps: memory.maps().to_owned(),
            bytes,
        };
        let path = std::env::temp_dir().join(format!("wf-frozen-ui-{}", std::process::id()));
        let output = Directory::create(&path).unwrap();
        let summary = frozen.save(&output).unwrap();
        assert_eq!(summary.duration_ms, 7);
        let replay = replay_evidence(&path).unwrap();
        assert_eq!(replay.captured_at_unix_ms, 123);
        assert_eq!(replay.identity.unwrap().pid, 42);
        assert_eq!(replay.memory.bytes, 0);
        assert!(matches!(replay.adapter, AdapterSupport::Supported { .. }));
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn follows_only_aligned_private_writable_pointers() {
        let regions = vec![Region {
            start: 0x1000,
            end: 0x2000,
            permissions: "rw-p".to_owned(),
            path: "[heap]".to_owned(),
        }];
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0x1800_u64.to_le_bytes());
        bytes.extend_from_slice(&0x1801_u64.to_le_bytes());
        bytes.extend_from_slice(&0x3000_u64.to_le_bytes());
        assert_eq!(pointer_targets(&bytes, &regions), vec![0x1800]);
    }

    #[test]
    fn loads_legacy_capture_for_offline_inspection() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "wfinspect-evidence-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("maps.txt"),
            "140000000-141000000 r-xp 0 00:00 1 /game/Warframe.x64.exe\n\
             200000000-200001000 rw-p 0 00:00 0 [heap]\n",
        )
        .unwrap();
        fs::write(directory.join("scaleform-memory.bin"), b"abcdefgh").unwrap();
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
                "bytes": 8,
                "truncated": false,
                "roots": [],
                "blocks": [{
                    "address": 0x200000000_u64,
                    "length": 8,
                    "file_offset": 0,
                    "depth": 0,
                    "permissions": "rw-p",
                    "region": "[heap]"
                }]
            }
        });
        fs::write(
            directory.join("ui.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();

        let replay = replay_evidence(&directory).unwrap();
        assert_eq!(replay.schema, 2);
        assert_eq!(replay.memory.blocks, 1);
        assert!(matches!(replay.selection, ReplayProbe::Unavailable { .. }));
        assert!(matches!(replay.rewards, ReplayProbe::Unavailable { .. }));

        let mut inventory_only = report.clone();
        inventory_only["identity"] = serde_json::json!({
            "pid": 42,
            "executable": {
                "path": "/game/Warframe.x64.exe", "size": 1,
                "sha256": "e546599b62d0574db955fc93a6434728625be72ca1a0731e475d1ffa350ccf05"
            }
        });
        fs::write(
            directory.join("ui.json"),
            serde_json::to_vec_pretty(&inventory_only).unwrap(),
        )
        .unwrap();
        let replay = replay_evidence(&directory).unwrap();
        assert!(
            matches!(replay.selection, ReplayProbe::Unavailable { reason }
            if reason.contains("could not read Warframe executable"))
        );
        assert!(matches!(replay.rewards, ReplayProbe::Unavailable { reason }
            if reason.contains("could not read Warframe executable")));

        let mut invalid = report;
        invalid["memory"]["file"] = serde_json::json!("../outside.bin");
        fs::write(
            directory.join("ui.json"),
            serde_json::to_vec_pretty(&invalid).unwrap(),
        )
        .unwrap();
        assert!(replay_evidence(&directory).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
