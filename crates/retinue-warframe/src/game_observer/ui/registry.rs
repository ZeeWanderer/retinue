use std::time::Instant;

use serde::Serialize;

use super::{Movie, Snapshot, movie_path_at, parse_movie_record};
use crate::game_observer::adapter::ScaleformLayout;
use crate::game_observer::memory::ProcessMemory;

const REGISTRY_ENTRY_SIZE: usize = 16;
const MAX_REGISTRY_BYTES: usize = 32 * 1024 * 1024;
const FLASH_RECORD_OFFSET: u64 = 0xc0;
const MOVIE_PATH_OFFSET: usize = 0x60;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BoundedScanMetrics {
    pub registry_entries: usize,
    pub flash_candidates: usize,
    pub registry_bytes: usize,
    pub total_ms: u128,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BoundedScanResult {
    pub snapshot: Snapshot,
    pub metrics: BoundedScanMetrics,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BoundedProbe {
    Available { scan: BoundedScanResult },
    Unavailable { stage: &'static str, reason: String },
}

pub(super) fn probe(pid: u32, layout: ScaleformLayout) -> BoundedProbe {
    let memory = match crate::game_observer::memory::open(pid) {
        Ok(memory) => memory,
        Err(reason) => {
            return BoundedProbe::Unavailable {
                stage: "process_memory",
                reason,
            };
        }
    };
    match scan(&memory, layout) {
        Ok(scan) => BoundedProbe::Available { scan },
        Err((stage, reason)) => BoundedProbe::Unavailable { stage, reason },
    }
}

pub(super) fn scan(
    memory: &ProcessMemory,
    layout: ScaleformLayout,
) -> Result<BoundedScanResult, (&'static str, String)> {
    let started = Instant::now();
    let image = memory.image_base().ok_or_else(|| {
        (
            "image_base",
            "Warframe executable mapping not found".to_owned(),
        )
    })?;
    let registry = image + layout.registry_vector_rva;
    let mut descriptor = [0_u8; 16];
    memory
        .read_exact_at(&mut descriptor, registry)
        .map_err(|error| ("registry_descriptor", error.to_string()))?;
    let entries = u64::from_le_bytes(descriptor[..8].try_into().unwrap());
    let used = u32::from_le_bytes(descriptor[8..12].try_into().unwrap()) as usize;
    let capacity = u32::from_le_bytes(descriptor[12..16].try_into().unwrap()) as usize;
    if entries == 0
        || used == 0
        || !used.is_multiple_of(REGISTRY_ENTRY_SIZE)
        || capacity < used
        || capacity > MAX_REGISTRY_BYTES
    {
        return Err((
            "registry_descriptor",
            format!("invalid registry vector: base=0x{entries:x} used={used} capacity={capacity}"),
        ));
    }

    let mut bytes = vec![0_u8; used];
    memory
        .read_exact_at(&mut bytes, entries)
        .map_err(|error| ("registry_entries", error.to_string()))?;
    let expected_type = image + layout.flash_instance_type_rva;
    let expected_vtable = image + layout.flash_instance_vtable_rva;
    let mut flash_candidates = 0;
    let mut movies = Vec::new();

    for entry in bytes.chunks_exact(REGISTRY_ENTRY_SIZE) {
        let control = u64::from_le_bytes(entry[..8].try_into().unwrap());
        let object_type = u64::from_le_bytes(entry[8..16].try_into().unwrap());
        if object_type != expected_type || control == 0 {
            continue;
        }
        flash_candidates += 1;
        if let Some(movie) = read_flash_instance(memory, control, expected_type, expected_vtable) {
            movies.push(movie);
        }
    }
    movies.sort_by_key(|movie| movie.record_address);
    movies.dedup_by_key(|movie| movie.record_address);
    if movies.is_empty() {
        return Err((
            "flash_instances",
            format!("registry contained {flash_candidates} candidates but no valid movies"),
        ));
    }

    Ok(BoundedScanResult {
        snapshot: Snapshot {
            pid: memory.pid(),
            movies,
        },
        metrics: BoundedScanMetrics {
            registry_entries: used / REGISTRY_ENTRY_SIZE,
            flash_candidates,
            registry_bytes: used,
            total_ms: started.elapsed().as_millis(),
        },
    })
}

fn read_flash_instance(
    memory: &ProcessMemory,
    control: u64,
    expected_type: u64,
    expected_vtable: u64,
) -> Option<Movie> {
    let mut control_bytes = [0_u8; 16];
    memory.read_exact_at(&mut control_bytes, control).ok()?;
    let object = u64::from_le_bytes(control_bytes[..8].try_into().unwrap());
    if object == 0 {
        return None;
    }
    let mut header = [0_u8; 16];
    memory.read_exact_at(&mut header, object).ok()?;
    let vtable = u64::from_le_bytes(header[..8].try_into().unwrap());
    let object_type = u64::from_le_bytes(header[8..16].try_into().unwrap());
    if vtable != expected_vtable || object_type != expected_type {
        return None;
    }

    let record_address = object.checked_add(FLASH_RECORD_OFFSET)?;
    let mut record = [0_u8; 0x68];
    memory.read_exact_at(&mut record, record_address).ok()?;
    let path_address = u64::from_le_bytes(
        record[MOVIE_PATH_OFFSET..MOVIE_PATH_OFFSET + 8]
            .try_into()
            .unwrap(),
    );
    let mut path_bytes = [0_u8; 512];
    let read = memory.read_at(&mut path_bytes, path_address).ok()?;
    let path = movie_path_at(&path_bytes[..read], 0)?.to_owned();
    let (width, height, scale_x, scale_y) = parse_movie_record(&record, path_address)?;
    Some(Movie {
        path,
        record_address,
        path_address,
        width,
        height,
        scale_x,
        scale_y,
    })
}
