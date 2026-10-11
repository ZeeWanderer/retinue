use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::process::{ProcessMemory, ScanRange};
#[derive(Clone, Debug, Serialize)]
pub struct PointerHop {
    pub block_address: u64,
    pub pointer_address: u64,
    pub target_address: u64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    #[default]
    Research,
    Readable,
    Heap,
    Image,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Selection {
    pub scope: Scope,
    pub ranges: Vec<Range<u64>>,
}

impl Selection {
    pub fn ranges(&self, memory: &ProcessMemory) -> Vec<ScanRange> {
        let mut ranges = memory.selected_ranges(|region| {
            region.permissions.starts_with('r')
                && match self.scope {
                    Scope::Research => memory.supports_research_scan(region),
                    Scope::Readable => true,
                    Scope::Heap => region.supports_heap(),
                    Scope::Image => memory.is_image(region),
                }
        });
        if !self.ranges.is_empty() {
            let mut requested = self.ranges.clone();
            requested.sort_by_key(|range| range.start);
            let mut merged: Vec<Range<u64>> = Vec::new();
            for range in requested {
                if let Some(last) = merged.last_mut()
                    && range.start <= last.end
                {
                    last.end = last.end.max(range.end);
                } else {
                    merged.push(range);
                }
            }
            ranges = ranges
                .into_iter()
                .flat_map(|range| {
                    merged.iter().filter_map(move |selected| {
                        let start = range.start.max(selected.start);
                        let end = range.end.min(selected.end);
                        (start < end).then(|| ScanRange {
                            start,
                            end,
                            ..range.clone()
                        })
                    })
                })
                .collect();
        }
        ranges
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct WalkOptions {
    pub selection: Selection,
    pub block_size: usize,
    pub max_depth: u8,
    pub max_blocks: usize,
    pub stride: usize,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            selection: Selection {
                scope: Scope::Readable,
                ranges: Vec::new(),
            },
            block_size: 512,
            max_depth: 16,
            max_blocks: 8192,
            stride: 8,
        }
    }
}

#[derive(Default, Debug, Serialize)]
pub struct Coverage {
    pub examined_bytes: u64,
    pub unreadable_blocks: usize,
    pub depth_limited: bool,
    pub work_limited: bool,
    pub match_limited: bool,
}

impl Coverage {
    pub fn incomplete(&self) -> bool {
        self.unreadable_blocks != 0 || self.depth_limited || self.work_limited || self.match_limited
    }
}

#[derive(Debug, Serialize)]
pub struct WalkReport {
    pub root: u64,
    pub target: Option<Range<u64>>,
    pub hops: Vec<PointerHop>,
    pub blocks: usize,
    pub truncated: bool,
    pub coverage: Coverage,
}

pub struct Walk {
    pub report: WalkReport,
    pub blocks: BTreeMap<u64, Vec<u8>>,
}

pub fn walk(
    memory: &ProcessMemory,
    root: u64,
    target: Option<Range<u64>>,
    options: &WalkOptions,
) -> Result<Walk, String> {
    if !(8..=65536).contains(&options.block_size)
        || options.max_depth > 64
        || !(1..=65536).contains(&options.max_blocks)
        || !matches!(options.stride, 1 | 2 | 4 | 8)
        || options.block_size.saturating_mul(options.max_blocks) > 32 * 1024 * 1024
    {
        return Err(
            "walk limits: block 8..65536 bytes, depth 0..64, stride 1/2/4/8, total <=32 MiB".into(),
        );
    }
    if target
        .as_ref()
        .is_some_and(|range| range.start >= range.end)
    {
        return Err("target range must not be empty".into());
    }
    let ranges = options.selection.ranges(memory);
    let find = |address| {
        ranges
            .get(ranges.partition_point(|range| range.end <= address))
            .filter(|range| range.start <= address)
    };
    if find(root).is_none() {
        return Err(format!(
            "root 0x{root:x} is outside selected or captured readable ranges"
        ));
    }
    let mut queue = VecDeque::from([(root, 0)]);
    let mut seen = HashSet::from([root]);
    let mut parents = HashMap::<u64, PointerHop>::new();
    let mut blocks = BTreeMap::new();
    let mut coverage = Coverage::default();
    let mut hops = Vec::new();
    'walk: while let Some((address, depth)) = queue.pop_front() {
        if blocks.len() == options.max_blocks {
            coverage.work_limited = true;
            break;
        }
        let range = find(address).expect("only selected addresses enter queue");
        let length = options.block_size.min((range.end - address) as usize);
        let mut bytes = vec![0; length];
        if memory.read_exact_at(&mut bytes, address).is_err() {
            coverage.unreadable_blocks += 1;
            continue;
        }
        coverage.examined_bytes += length as u64;
        for offset in (0..length.saturating_sub(7)).step_by(options.stride) {
            let pointer = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
            let hop = PointerHop {
                block_address: address,
                pointer_address: address + offset as u64,
                target_address: pointer,
            };
            if target
                .as_ref()
                .is_some_and(|range| range.contains(&pointer))
            {
                hops.push(hop);
                let mut child = address;
                while child != root {
                    let parent = parents[&child].clone();
                    child = parent.block_address;
                    hops.push(parent);
                }
                hops.reverse();
                blocks.insert(address, bytes);
                break 'walk;
            }
            if pointer == 0 || seen.contains(&pointer) {
                continue;
            }
            if find(pointer).is_none() {
                // Mapped but missing capture data is distinct from a non-pointer value.
                if memory
                    .regions()
                    .iter()
                    .any(|r| r.permissions.starts_with('r') && r.contains(pointer))
                    && !memory.supports_read_range(pointer, 1)
                {
                    coverage.unreadable_blocks += 1;
                }
                continue;
            }
            if depth == options.max_depth {
                coverage.depth_limited = true;
            } else if seen.len() >= options.max_blocks {
                coverage.work_limited = true;
            } else {
                seen.insert(pointer);
                parents.insert(pointer, hop);
                queue.push_back((pointer, depth + 1));
            }
        }
        blocks.insert(address, bytes);
    }
    Ok(Walk {
        report: WalkReport {
            root,
            target,
            hops,
            blocks: blocks.len(),
            truncated: coverage.incomplete(),
            coverage,
        },
        blocks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{CaptureBlock, ProcessMemory};

    #[test]
    fn image_roots_depth_limits_and_unaligned_targets() {
        let mut bytes = vec![0; 4096];
        for index in 0..20 {
            bytes[index * 128..index * 128 + 8]
                .copy_from_slice(&(0x1000 + (index as u64 + 1) * 128).to_le_bytes());
        }
        let memory = ProcessMemory::from_capture(
            1,
            "1000-2000 r--p 0 0:0 0 /test/program\n".into(),
            vec![CaptureBlock {
                address: 0x1000,
                length: bytes.len(),
                file_offset: 0,
            }],
            bytes,
        )
        .unwrap();
        let mut options = WalkOptions {
            block_size: 8,
            ..WalkOptions::default()
        };
        let result = walk(&memory, 0x1000, Some(0x1980..0x1981), &options).unwrap();
        assert!(result.report.coverage.depth_limited);
        assert!(result.report.truncated);
        options.max_depth = 24;
        assert_eq!(
            walk(&memory, 0x1000, Some(0x1980..0x1981), &options)
                .unwrap()
                .report
                .hops
                .len(),
            19
        );
        assert!(walk(&memory, 0x3000, None, &options).is_err());
    }
}
