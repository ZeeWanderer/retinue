use std::collections::{BTreeMap, HashMap};
use std::io;
use std::time::Instant;

use memchr::memmem;
use serde::{Deserialize, Serialize};

use super::adapter;
use super::memory::ProcessMemory;
use super::{ProcessIdentity, identify_process};

mod display;
pub(crate) mod ducats;
pub use ducats::{DucatHover, DucatReader, DucatState};
pub(crate) mod geometry;
pub use geometry::{DisplayNode, DisplayReader, DisplayTree};
pub(crate) mod layout;
pub(crate) use display::enumerate as enumerate_text_objects;
pub use display::{ObjectReport, TextObject};
mod refs;
mod registry;
mod relic;
mod text;

pub use refs::{PointerReference, PointerReferences};
pub use registry::{BoundedProbe, BoundedScanMetrics, BoundedScanResult};
pub use relic::{RelicEra, RelicRewardText, RelicSelection};

const CHUNK: usize = 4 * 1024 * 1024;
const MOVIE_PREFIX: &[u8] = b"/Lotus/Interface/";
const MAX_MOVIE_PATH: usize = 512;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Movie {
    pub path: String,
    pub record_address: u64,
    pub path_address: u64,
    pub width: u32,
    pub height: u32,
    pub scale_x: f32,
    pub scale_y: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Snapshot {
    pub pid: u32,
    pub movies: Vec<Movie>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ScanMetrics {
    pub regions: usize,
    pub mapped_bytes_per_pass: u64,
    pub path_instances: usize,
    pub path_scan_ms: u128,
    pub record_scan_ms: u128,
    pub total_ms: u128,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ScanResult {
    pub snapshot: Snapshot,
    pub metrics: ScanMetrics,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TextScan {
    pub pid: u32,
    pub terms: Vec<TextTerm>,
    pub metrics: TextScanMetrics,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TextScanMetrics {
    pub regions: usize,
    pub mapped_bytes_per_pass: u64,
    pub text_scan_ms: u128,
    pub reference_scan_ms: u128,
    pub total_ms: u128,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TextTerm {
    pub term: String,
    pub matches: Vec<TextMatch>,
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TextMatch {
    pub address: u64,
    pub encoding: &'static str,
    pub region: String,
    pub context: String,
    pub references: Vec<TextReference>,
    pub references_truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TextReference {
    pub pointer_address: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_object_candidate: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DisplayScanMetrics {
    pub registered_objects: usize,
    pub containers: usize,
    pub child_objects: usize,
    pub text_objects: usize,
    pub bytes_read: usize,
    pub total_ms: u128,
}

#[derive(Debug, Default)]
pub struct TransitionTracker {
    previous: Option<Snapshot>,
}

impl TransitionTracker {
    pub fn observe(&mut self, snapshot: Snapshot) -> Option<Snapshot> {
        if self.previous.as_ref() == Some(&snapshot) {
            return None;
        }
        self.previous = Some(snapshot.clone());
        Some(snapshot)
    }
}

pub fn bounded_probe(pid: u32) -> BoundedProbe {
    let identity = match identify_process(pid) {
        Ok(identity) => identity,
        Err(reason) => {
            return BoundedProbe::Unavailable {
                stage: "executable_identity",
                reason,
            };
        }
    };
    bounded_probe_for_identity(pid, &identity)
}

pub fn bounded_probe_for_identity(pid: u32, identity: &ProcessIdentity) -> BoundedProbe {
    if identity.pid != pid {
        return BoundedProbe::Unavailable {
            stage: "executable_identity",
            reason: "Warframe process identity PID mismatch".to_owned(),
        };
    }
    let layout = match adapter::require_scaleform(identity) {
        Ok(layout) => layout,
        Err(reason) => {
            return BoundedProbe::Unavailable {
                stage: "adapter",
                reason,
            };
        }
    };
    registry::probe(pid, layout)
}

#[derive(Debug)]
pub struct Reader {
    pid: u32,
    layout: adapter::ScaleformLayout,
}

impl Reader {
    pub fn open_for_identity(identity: &ProcessIdentity) -> Result<Self, String> {
        Ok(Self {
            pid: identity.pid,
            layout: adapter::require_scaleform(identity)?,
        })
    }

    pub fn relic_selection(&self) -> Result<RelicSelection, String> {
        relic::selection(self.pid, self.layout)
    }

    pub fn relic_rewards(&self) -> Result<RelicRewardText, String> {
        relic::rewards(self.pid, self.layout)
    }
}

pub fn probe_relic_selection(pid: u32) -> Result<RelicSelection, String> {
    let identity = identify_process(pid)?;
    probe_relic_selection_for_identity(pid, &identity)
}

pub fn probe_relic_selection_for_identity(
    pid: u32,
    identity: &ProcessIdentity,
) -> Result<RelicSelection, String> {
    if identity.pid != pid {
        return Err("Warframe process identity PID mismatch".to_owned());
    }
    Reader::open_for_identity(identity)?.relic_selection()
}

pub fn probe_relic_rewards(pid: u32) -> Result<RelicRewardText, String> {
    let identity = identify_process(pid)?;
    probe_relic_rewards_for_identity(pid, &identity)
}

pub fn probe_relic_rewards_for_identity(
    pid: u32,
    identity: &ProcessIdentity,
) -> Result<RelicRewardText, String> {
    if identity.pid != pid {
        return Err("Warframe process identity PID mismatch".to_owned());
    }
    Reader::open_for_identity(identity)?.relic_rewards()
}

pub fn explicit_scan(pid: u32) -> Result<ScanResult, String> {
    let memory = crate::game_observer::memory::open(pid)?;
    scan_snapshot(&memory)
}

pub fn explicit_text_scan(pid: u32, terms: &[String]) -> Result<TextScan, String> {
    let memory = crate::game_observer::memory::open(pid)?;
    text::scan(&memory, terms)
}

pub fn explicit_pointer_scan(
    pid: u32,
    target_start: u64,
    target_end: u64,
) -> Result<PointerReferences, String> {
    let memory = crate::game_observer::memory::open(pid)?;
    refs::scan(&memory, target_start, target_end)
}

pub(crate) fn scan_memory(memory: &ProcessMemory) -> Result<ScanResult, String> {
    scan_snapshot(memory)
}

pub(crate) fn scan_text_memory(
    memory: &ProcessMemory,
    terms: &[String],
) -> Result<TextScan, String> {
    text::scan(memory, terms)
}

pub(crate) fn scan_pointers_memory(
    memory: &ProcessMemory,
    target_start: u64,
    target_end: u64,
) -> Result<PointerReferences, String> {
    refs::scan(memory, target_start, target_end)
}

pub(crate) fn scan_registry_memory(
    memory: &ProcessMemory,
    layout: adapter::ScaleformLayout,
) -> Result<BoundedScanResult, (&'static str, String)> {
    registry::scan(memory, layout)
}

pub(crate) fn relic_selection_memory(
    memory: &ProcessMemory,
    layout: adapter::ScaleformLayout,
    snapshot: &Snapshot,
) -> Result<RelicSelection, String> {
    relic::selection_from_snapshot(memory, layout, snapshot)
}

pub(crate) fn relic_rewards_memory(
    memory: &ProcessMemory,
    layout: adapter::ScaleformLayout,
    snapshot: &Snapshot,
) -> Result<RelicRewardText, String> {
    relic::rewards_from_snapshot(memory, layout, snapshot)
}

pub(crate) fn scan_snapshot(memory: &ProcessMemory) -> Result<ScanResult, String> {
    let started = Instant::now();
    let regions = memory.scan_ranges();
    let mapped_bytes_per_pass = regions
        .iter()
        .map(|region| region.end.saturating_sub(region.start))
        .sum();
    let path_started = Instant::now();
    let paths = scan_movie_paths(memory)
        .map_err(|error| format!("could not scan Warframe movie paths: {error}"))?;
    let path_scan_ms = path_started.elapsed().as_millis();
    let record_started = Instant::now();
    let movies = scan_movie_records(memory, &paths)
        .map_err(|error| format!("could not scan Warframe movie records: {error}"))?;
    let record_scan_ms = record_started.elapsed().as_millis();
    Ok(ScanResult {
        snapshot: Snapshot {
            pid: memory.pid(),
            movies,
        },
        metrics: ScanMetrics {
            regions: regions.len(),
            mapped_bytes_per_pass,
            path_instances: paths.len(),
            path_scan_ms,
            record_scan_ms,
            total_ms: started.elapsed().as_millis(),
        },
    })
}

fn address_low_bits(addresses: impl Iterator<Item = u64>) -> Vec<u64> {
    let mut low_bits = vec![0_u64; 1024];
    for address in addresses {
        let low = address as u16 as usize;
        low_bits[low >> 6] |= 1_u64 << (low & 63);
    }
    low_bits
}

fn address_may_match(address: u64, min: u64, max: u64, low_bits: &[u64]) -> bool {
    if address < min || address > max {
        return false;
    }
    let low = address as u16 as usize;
    low_bits[low >> 6] & (1_u64 << (low & 63)) != 0
}

fn scan_movie_paths(memory: &ProcessMemory) -> io::Result<HashMap<u64, String>> {
    let mut paths = HashMap::new();
    let mut buffer = vec![0_u8; CHUNK];
    let mut tail = Vec::new();

    for region in memory.scan_ranges() {
        let mut offset = region.start;
        tail.clear();
        while offset < region.end {
            let wanted = usize::try_from((region.end - offset).min(CHUNK as u64)).unwrap();
            let read = match memory.read_at(&mut buffer[..wanted], offset) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if matches!(error.raw_os_error(), Some(5 | 14)) => break,
                Err(error) => return Err(error),
            };
            let base = offset.saturating_sub(tail.len() as u64);
            let mut searchable = Vec::with_capacity(tail.len() + read);
            searchable.extend_from_slice(&tail);
            searchable.extend_from_slice(&buffer[..read]);
            for index in memmem::find_iter(&searchable, MOVIE_PREFIX) {
                if let Some(path) = movie_path_at(&searchable, index) {
                    paths.insert(base + index as u64, path.to_owned());
                    if paths.len() > 65_536 {
                        return Err(io::Error::other("UI movie path limit exceeded"));
                    }
                }
            }
            tail.clear();
            tail.extend_from_slice(&searchable[searchable.len().saturating_sub(MAX_MOVIE_PATH)..]);
            offset += read as u64;
        }
    }
    Ok(paths)
}

fn scan_movie_records(
    memory: &ProcessMemory,
    paths: &HashMap<u64, String>,
) -> io::Result<Vec<Movie>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let min_path = paths.keys().copied().min().unwrap();
    let max_path = paths.keys().copied().max().unwrap();
    let low_bits = address_low_bits(paths.keys().copied());
    let mut movies = BTreeMap::new();
    let mut buffer = vec![0_u8; CHUNK];

    for region in memory.scan_ranges() {
        let mut offset = region.start;
        while offset < region.end {
            let wanted = usize::try_from((region.end - offset).min(CHUNK as u64)).unwrap();
            let read = match memory.read_at(&mut buffer[..wanted], offset) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if matches!(error.raw_os_error(), Some(5 | 14)) => break,
                Err(error) => return Err(error),
            };
            let alignment = usize::try_from((8 - (offset & 7)) & 7).unwrap();
            for index in (alignment..read.saturating_sub(7)).step_by(8) {
                let path_address = u64::from_le_bytes(buffer[index..index + 8].try_into().unwrap());
                if !address_may_match(path_address, min_path, max_path, &low_bits) {
                    continue;
                }
                let Some(path) = paths.get(&path_address) else {
                    continue;
                };
                let Some(record_address) = (offset + index as u64).checked_sub(0x60) else {
                    continue;
                };
                let Some((width, height, scale_x, scale_y)) =
                    read_movie_record(memory, record_address, path_address)
                else {
                    continue;
                };
                movies.insert(
                    record_address,
                    Movie {
                        path: path.clone(),
                        record_address,
                        path_address,
                        width,
                        height,
                        scale_x,
                        scale_y,
                    },
                );
                if movies.len() > 4096 {
                    return Err(io::Error::other("UI movie record limit exceeded"));
                }
            }
            offset += read as u64;
        }
    }
    Ok(movies.into_values().collect())
}

fn read_movie_record(
    memory: &ProcessMemory,
    record: u64,
    expected_path: u64,
) -> Option<(u32, u32, f32, f32)> {
    let mut bytes = [0_u8; 0x68];
    memory.read_exact_at(&mut bytes, record).ok()?;
    parse_movie_record(&bytes, expected_path)
}

fn parse_movie_record(bytes: &[u8], expected_path: u64) -> Option<(u32, u32, f32, f32)> {
    let u32_at = |offset| -> Option<u32> {
        Some(u32::from_le_bytes(
            bytes.get(offset..offset + 4)?.try_into().ok()?,
        ))
    };
    let u64_at = |offset| -> Option<u64> {
        Some(u64::from_le_bytes(
            bytes.get(offset..offset + 8)?.try_into().ok()?,
        ))
    };
    let width = u32_at(0x08)?;
    let height = u32_at(0x0c)?;
    let scale_x = f32::from_bits(u32_at(0x18)?);
    let scale_y = f32::from_bits(u32_at(0x1c)?);
    let dimensions_match = width == u32_at(0x20)? && height == u32_at(0x24)?;
    let dimensions_sane = (640..=16384).contains(&width) && (360..=16384).contains(&height);
    let scale_sane = scale_x.is_finite()
        && scale_y.is_finite()
        && (0.05..=8.0).contains(&scale_x)
        && (0.05..=8.0).contains(&scale_y);
    (dimensions_match && dimensions_sane && scale_sane && u64_at(0x60)? == expected_path)
        .then_some((width, height, scale_x, scale_y))
}

fn movie_path_at(bytes: &[u8], index: usize) -> Option<&str> {
    let suffix = bytes.get(index..index.saturating_add(MAX_MOVIE_PATH).min(bytes.len()))?;
    let length = suffix.iter().position(|value| *value == 0)?;
    let path = std::str::from_utf8(&suffix[..length]).ok()?;
    (path.starts_with("/Lotus/Interface/") && path.ends_with(".swf")).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_movie_record_shape() {
        let mut bytes = [0_u8; 0x68];
        bytes[0x08..0x0c].copy_from_slice(&2560_u32.to_le_bytes());
        bytes[0x0c..0x10].copy_from_slice(&1440_u32.to_le_bytes());
        bytes[0x18..0x1c].copy_from_slice(&1.0_f32.to_le_bytes());
        bytes[0x1c..0x20].copy_from_slice(&1.0_f32.to_le_bytes());
        bytes[0x20..0x24].copy_from_slice(&2560_u32.to_le_bytes());
        bytes[0x24..0x28].copy_from_slice(&1440_u32.to_le_bytes());
        bytes[0x60..0x68].copy_from_slice(&0x1234_u64.to_le_bytes());
        assert_eq!(
            parse_movie_record(&bytes, 0x1234),
            Some((2560, 1440, 1.0, 1.0))
        );
        assert_eq!(parse_movie_record(&bytes, 0x5678), None);
    }

    #[test]
    fn tracker_suppresses_identical_snapshots_without_scheduling() {
        let snapshot = Snapshot {
            pid: 10,
            movies: Vec::new(),
        };
        let mut tracker = TransitionTracker::default();
        assert_eq!(tracker.observe(snapshot.clone()), Some(snapshot.clone()));
        assert_eq!(tracker.observe(snapshot), None);
    }

    #[test]
    fn address_filter_has_no_false_negatives() {
        let addresses = [0x1234_u64, 0x8abc, 0x101_1234];
        let low_bits = address_low_bits(addresses.into_iter());
        for address in addresses {
            assert!(address_may_match(address, 0x1000, 0x102_0000, &low_bits));
        }
        assert!(!address_may_match(0x1235, 0x1000, 0x102_0000, &low_bits));
        assert!(!address_may_match(0x200, 0x1000, 0x102_0000, &low_bits));
    }
}
