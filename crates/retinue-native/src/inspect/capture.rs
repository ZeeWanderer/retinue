use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::memory::SourceMetadata;
use super::query::{self, WalkOptions, WalkReport};
use crate::process::{CaptureBlock, ProcessMemory};

const MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Deserialize, Serialize)]
pub struct Capture {
    schema: u8,
    pub source: SourceMetadata,
    pub captured_at_unix_ms: u128,
    pub duration_ms: u128,
    pub bytes: usize,
    pub truncated: bool,
    pub roots: Vec<u64>,
    pub blocks: Vec<Block>,
    #[serde(skip_deserializing)]
    pub walks: Vec<WalkReport>,
}

#[derive(Deserialize, Serialize)]
pub struct Block {
    address: u64,
    length: usize,
    file_offset: usize,
}

pub fn write(
    memory: &ProcessMemory,
    source: SourceMetadata,
    directory: &Path,
    roots: &[u64],
    options: &WalkOptions,
) -> Result<Capture, String> {
    let captured_at_unix_ms = super::unix_time_ms();
    let started = std::time::Instant::now();
    if roots.is_empty() && options.selection.ranges.is_empty() {
        return Err("capture requires at least one --root or --range".into());
    }
    if roots.len() > 64 {
        return Err("capture accepts at most 64 roots".into());
    }
    let mut chunks = BTreeMap::<u64, Vec<u8>>::new();
    let mut walks = Vec::new();
    let mut truncated = source
        .capture
        .as_ref()
        .is_some_and(|capture| capture.memory.truncated);
    let mut total = 0;
    for root in roots {
        let walk = query::walk(memory, *root, None, options)?;
        truncated |= walk.report.truncated;
        walks.push(walk.report);
        for (address, bytes) in walk.blocks {
            total += bytes.len();
            if total > MAX_BYTES {
                truncated = true;
                break;
            }
            chunks.entry(address).or_insert(bytes);
        }
        if total > MAX_BYTES {
            break;
        }
    }
    if !options.selection.ranges.is_empty() && total <= MAX_BYTES {
        truncated |= options.selection.ranges.iter().any(|range| {
            usize::try_from(range.end - range.start)
                .ok()
                .is_none_or(|length| !memory.supports_read_range(range.start, length))
        });
        for range in options.selection.ranges(memory) {
            let length = ((range.end - range.start) as usize).min(MAX_BYTES - total);
            if length == 0 {
                truncated = true;
                break;
            }
            let mut bytes = vec![0; length];
            if memory.read_exact_at(&mut bytes, range.start).is_err() {
                truncated = true;
                continue;
            }
            truncated |= length as u64 != range.end - range.start;
            total += length;
            let chunk = chunks.entry(range.start).or_default();
            if chunk.len() < bytes.len() {
                *chunk = bytes;
            }
        }
    }
    let mut bytes = Vec::new();
    let mut blocks = Vec::new();
    let mut end = 0_u64;
    for (address, chunk) in chunks {
        let skip = end.saturating_sub(address).min(chunk.len() as u64) as usize;
        if skip == chunk.len() {
            continue;
        }
        blocks.push(Block {
            address: address + skip as u64,
            length: chunk.len() - skip,
            file_offset: bytes.len(),
        });
        bytes.extend_from_slice(&chunk[skip..]);
        end = address + chunk.len() as u64;
    }
    if bytes.is_empty() {
        return Err("selected memory was not available; capture not written".into());
    }
    let report = Capture {
        schema: 1,
        source,
        captured_at_unix_ms,
        duration_ms: started.elapsed().as_millis(),
        bytes: bytes.len(),
        truncated,
        roots: roots.to_vec(),
        blocks,
        walks,
    };
    let directory = crate::recording::Directory::create(directory)?;
    for (name, data) in [
        ("memory.bin", bytes),
        ("maps.txt", memory.maps().as_bytes().to_vec()),
        (
            "memory.json",
            serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
        ),
    ] {
        directory.write(name, &data)?;
    }
    Ok(report)
}

pub fn load(path: &Path) -> Result<(ProcessMemory, Capture), String> {
    let directory = path.parent().unwrap_or(Path::new("."));
    let report: Capture = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("invalid memory capture: {e}"))?;
    if report.schema != 1 || report.bytes > MAX_BYTES || report.blocks.len() > 65536 {
        return Err("unsupported or oversized memory capture".into());
    }
    let maps = fs::read_to_string(directory.join("maps.txt")).map_err(|e| e.to_string())?;
    let file = directory.join("memory.bin");
    if fs::metadata(&file).map_err(|e| e.to_string())?.len() != report.bytes as u64 {
        return Err("capture memory size differs from manifest".into());
    }
    let bytes = fs::read(file).map_err(|e| e.to_string())?;
    let blocks = report
        .blocks
        .iter()
        .map(|block| CaptureBlock {
            address: block.address,
            length: block.length,
            file_offset: block.file_offset,
        })
        .collect();
    let memory = ProcessMemory::from_capture(report.source.pid, maps, blocks, bytes)?;
    Ok((memory, report))
}
