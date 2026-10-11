use std::time::Instant;

use serde::Serialize;

use super::CHUNK;
use crate::game_observer::memory::ProcessMemory;

const MAX_REFERENCES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PointerReference {
    pub address: u64,
    pub target: u64,
    pub region: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PointerReferences {
    pub pid: u32,
    pub target_start: u64,
    pub target_end: u64,
    pub references: Vec<PointerReference>,
    pub truncated: bool,
    pub mapped_bytes: u64,
    pub total_ms: u128,
}

pub(super) fn scan(
    memory: &ProcessMemory,
    target_start: u64,
    target_end: u64,
) -> Result<PointerReferences, String> {
    if target_start >= target_end {
        return Err("pointer target range must not be empty".to_owned());
    }

    let started = Instant::now();
    let mut buffer = vec![0_u8; CHUNK];
    let mut references = Vec::new();
    let mut truncated = false;
    let mut mapped_bytes = 0_u64;

    'regions: for region in memory.scan_ranges() {
        mapped_bytes = mapped_bytes.saturating_add(region.end.saturating_sub(region.start));
        let mut offset = region.start;
        while offset < region.end {
            let wanted = usize::try_from((region.end - offset).min(CHUNK as u64)).unwrap();
            let read = match memory.read_at(&mut buffer[..wanted], offset) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if matches!(error.raw_os_error(), Some(5 | 14)) => break,
                Err(error) => return Err(format!("could not scan Warframe memory: {error}")),
            };
            let alignment = usize::try_from((8 - (offset & 7)) & 7).unwrap();
            for index in (alignment..read.saturating_sub(7)).step_by(8) {
                let target = u64::from_le_bytes(buffer[index..index + 8].try_into().unwrap());
                if !contains_target(target_start, target_end, target) {
                    continue;
                }
                references.push(PointerReference {
                    address: offset + index as u64,
                    target,
                    region: region.path.clone(),
                });
                if references.len() >= MAX_REFERENCES {
                    truncated = true;
                    break 'regions;
                }
            }
            offset += read as u64;
        }
    }

    Ok(PointerReferences {
        pid: memory.pid(),
        target_start,
        target_end,
        references,
        truncated,
        mapped_bytes,
        total_ms: started.elapsed().as_millis(),
    })
}

fn contains_target(start: u64, end: u64, target: u64) -> bool {
    (start..end).contains(&target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_range_is_end_exclusive() {
        assert!(contains_target(0x1000, 0x1200, 0x1000));
        assert!(contains_target(0x1000, 0x1200, 0x11f8));
        assert!(!contains_target(0x1000, 0x1200, 0x1200));
    }
}
