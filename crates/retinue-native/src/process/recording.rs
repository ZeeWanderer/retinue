use std::collections::BTreeMap;
use std::io;
use std::sync::Mutex;

use super::{CaptureBlock, MemoryBacking};

#[derive(Debug)]
pub struct RecordingMemory {
    pub source: Box<MemoryBacking>,
    data: Mutex<Recording>,
    max_bytes: usize,
    max_blocks: usize,
}

#[derive(Debug, Default)]
struct Recording {
    ranges: BTreeMap<u64, Vec<u8>>,
    bytes: usize,
    truncated: bool,
}

pub struct RecordedReads {
    pub blocks: Vec<CaptureBlock>,
    pub bytes: Vec<u8>,
    pub truncated: bool,
}

impl RecordingMemory {
    pub fn new(source: MemoryBacking, max_bytes: usize, max_blocks: usize) -> Self {
        Self {
            source: Box::new(source),
            data: Mutex::new(Recording::default()),
            max_bytes,
            max_blocks,
        }
    }

    pub fn read_at(&self, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
        offset
            .checked_add(buffer.len() as u64)
            .ok_or_else(|| io::Error::other("memory address overflow"))?;
        let mut data = self
            .data
            .lock()
            .map_err(|_| io::Error::other("memory recording poisoned"))?;
        let mut read = 0;
        while read < buffer.len() {
            let address = offset + read as u64;
            // Reuse first-seen bytes, so overlapping reads cannot rewrite retained pointers.
            if let Some((&start, bytes)) = data.ranges.range(..=address).next_back()
                && address - start < bytes.len() as u64
            {
                let within = (address - start) as usize;
                let count = (bytes.len() - within).min(buffer.len() - read);
                buffer[read..read + count].copy_from_slice(&bytes[within..within + count]);
                read += count;
                continue;
            }
            let gap = data
                .ranges
                .range(address..)
                .next()
                .map_or(buffer.len() - read, |(&next, _)| {
                    (next - address).min((buffer.len() - read) as u64) as usize
                });
            if data.ranges.len() >= self.max_blocks || gap > self.max_bytes - data.bytes {
                data.truncated = true;
                return if read == 0 {
                    Err(io::Error::other("memory recording limit reached"))
                } else {
                    Ok(read)
                };
            }
            match self.source.read_at(&mut buffer[read..read + gap], address) {
                Ok(0) => break,
                Ok(count) => {
                    data.ranges
                        .insert(address, buffer[read..read + count].to_vec());
                    data.bytes += count;
                    read += count;
                }
                Err(error) if read == 0 => return Err(error),
                Err(_) => break,
            }
        }
        Ok(read)
    }

    pub fn finish(self) -> Result<RecordedReads, String> {
        let data = self
            .data
            .into_inner()
            .map_err(|_| "memory recording poisoned")?;
        let mut bytes = Vec::with_capacity(data.bytes);
        let mut blocks = Vec::with_capacity(data.ranges.len());
        for (address, range) in data.ranges {
            blocks.push(CaptureBlock {
                address,
                length: range.len(),
                file_offset: bytes.len(),
            });
            bytes.extend_from_slice(&range);
        }
        Ok(RecordedReads {
            blocks,
            bytes,
            truncated: data.truncated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessMemory;

    fn source() -> ProcessMemory {
        ProcessMemory::from_capture(
            1,
            String::new(),
            vec![
                CaptureBlock {
                    address: 100,
                    length: 20,
                    file_offset: 0,
                },
                CaptureBlock {
                    address: 130,
                    length: 10,
                    file_offset: 20,
                },
            ],
            (0..30).collect(),
        )
        .unwrap()
    }

    #[test]
    fn overlapping_reads_replay_without_overlapping_blocks() {
        let memory = source().record_reads(30, 10);
        memory.read_exact_at(&mut [0; 4], 105).unwrap();
        let mut bytes = [0; 20];
        memory.read_exact_at(&mut bytes, 100).unwrap();
        assert_eq!(bytes, std::array::from_fn::<_, 20, _>(|i| i as u8));
        let recording = memory.into_recorded_reads().unwrap();
        assert_eq!(recording.bytes.len(), 20);
        assert!(!recording.truncated);
        let replay =
            ProcessMemory::from_capture(1, String::new(), recording.blocks, recording.bytes)
                .unwrap();
        let mut replayed = [0; 20];
        replay.read_exact_at(&mut replayed, 100).unwrap();
        assert_eq!(replayed, bytes);
        assert!(replay.read_exact_at(&mut [0; 1], 120).is_err());
    }

    #[test]
    fn missing_source_bytes_are_not_fabricated() {
        let memory = source().record_reads(30, 10);
        let mut bytes = [255; 20];
        assert_eq!(memory.read_at(&mut bytes, 115).unwrap(), 5);
        assert_eq!(&bytes[..5], &[15, 16, 17, 18, 19]);
        assert_eq!(&bytes[5..], &[255; 15]);
        assert!(memory.read_exact_at(&mut [0; 1], 120).is_err());
        let recording = memory.into_recorded_reads().unwrap();
        assert_eq!(recording.bytes.len(), 5);
        assert!(!recording.truncated);
    }

    #[test]
    fn limits_fail_reads_and_preserve_previous_data() {
        for (byte_limit, block_limit) in [(4, 10), (30, 1)] {
            let memory = source().record_reads(byte_limit, block_limit);
            memory.read_exact_at(&mut [0; 4], 100).unwrap();
            assert!(memory.read_exact_at(&mut [0; 1], 110).is_err());
            memory.read_exact_at(&mut [0; 4], 100).unwrap();
            let recording = memory.into_recorded_reads().unwrap();
            assert_eq!(recording.bytes, [0, 1, 2, 3]);
            assert!(recording.truncated);
        }
    }

    #[test]
    fn overflow_is_rejected() {
        let memory = source().record_reads(30, 10);
        assert!(memory.read_at(&mut [0; 4], u64::MAX - 1).is_err());
    }

    #[test]
    fn repeated_reads_keep_first_seen_bytes_when_source_changes() {
        let mut recording = RecordingMemory::new(source().backing, 30, 10);
        recording.read_at(&mut [0; 4], 105).unwrap();
        let MemoryBacking::Capture(source) = &mut *recording.source else {
            unreachable!()
        };
        source.bytes.fill(99);
        let mut bytes = [0; 8];
        assert_eq!(recording.read_at(&mut bytes, 103).unwrap(), 8);
        assert_eq!(bytes, [99, 99, 5, 6, 7, 8, 99, 99]);
    }
}
