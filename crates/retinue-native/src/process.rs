use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, Read};
use std::os::unix::fs::FileExt;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use crate::work::Budget;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod recording;
pub use recording::RecordedReads;
use recording::RecordingMemory;

#[derive(Clone, Debug)]
pub struct Region {
    pub start: u64,
    pub end: u64,
    pub permissions: String,
    pub path: String,
}

#[derive(Clone, Debug)]
pub struct ScanRange {
    pub start: u64,
    pub end: u64,
    pub permissions: String,
    pub path: String,
}

#[derive(Debug)]
pub struct ProcessMemory {
    pid: u32,
    backing: MemoryBacking,
    maps: String,
    regions: Vec<Region>,
    budget: Option<Budget>,
    image_path: Option<PathBuf>,
}

#[derive(Debug)]
enum MemoryBacking {
    Live(File),
    Capture(CaptureMemory),
    Recording(RecordingMemory),
}

#[derive(Clone, Debug)]
pub struct CaptureBlock {
    pub address: u64,
    pub length: usize,
    pub file_offset: usize,
}

#[derive(Debug)]
struct CaptureMemory {
    bytes: Vec<u8>,
    blocks: Vec<CaptureBlock>,
}

impl ProcessMemory {
    pub fn open(pid: u32) -> Result<Self, String> {
        let maps = fs::read_to_string(format!("/proc/{pid}/maps"))
            .map_err(|error| format!("could not read process {pid} maps: {error}"))?;
        let file = File::open(format!("/proc/{pid}/mem"))
            .map_err(|error| format!("could not open process {pid} memory: {error}"))?;
        Ok(Self {
            pid,
            backing: MemoryBacking::Live(file),
            maps: maps.clone(),
            regions: parse_regions(&maps),
            budget: None,
            image_path: fs::read_link(format!("/proc/{pid}/exe")).ok(),
        })
    }

    pub fn from_capture(
        pid: u32,
        maps: String,
        blocks: Vec<CaptureBlock>,
        bytes: Vec<u8>,
    ) -> Result<Self, String> {
        let backing = CaptureMemory::new(blocks, bytes)?;
        Ok(Self {
            pid,
            regions: parse_regions(&maps),
            maps,
            backing: MemoryBacking::Capture(backing),
            budget: None,
            image_path: None,
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn with_image_path(mut self, path: Option<PathBuf>) -> Self {
        self.image_path = path;
        self
    }

    pub fn is_image(&self, region: &Region) -> bool {
        self.image_path.as_ref().is_some_and(|path| {
            std::path::Path::new(region.path.trim_end_matches(" (deleted)")) == path
        })
    }

    pub fn supports_research_scan(&self, region: &Region) -> bool {
        region.supports_heap() || (region.permissions.starts_with('r') && self.is_image(region))
    }

    pub fn with_budget(mut self, budget: Budget) -> Self {
        self.budget = Some(budget);
        self
    }

    pub fn record_reads(mut self, max_bytes: usize, max_blocks: usize) -> Self {
        self.backing =
            MemoryBacking::Recording(RecordingMemory::new(self.backing, max_bytes, max_blocks));
        self
    }

    pub fn into_recorded_reads(self) -> Result<RecordedReads, String> {
        match self.backing {
            MemoryBacking::Recording(recording) => recording.finish(),
            _ => Err("memory read recording is not enabled".into()),
        }
    }

    pub fn check_budget(&self) -> io::Result<()> {
        self.budget.as_ref().map_or(Ok(()), Budget::check)
    }

    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    pub fn maps(&self) -> &str {
        &self.maps
    }

    pub fn scan_ranges(&self) -> Vec<ScanRange> {
        self.selected_ranges(|region| self.supports_research_scan(region))
    }

    pub fn selected_ranges(&self, accepts: impl Fn(&Region) -> bool) -> Vec<ScanRange> {
        match self.backing.capture() {
            None => self
                .regions
                .iter()
                .filter(|region| accepts(region))
                .map(|region| ScanRange {
                    start: region.start,
                    end: region.end,
                    permissions: region.permissions.clone(),
                    path: region.path.clone(),
                })
                .collect(),
            Some(capture) => {
                let mut ranges = Vec::<ScanRange>::new();
                for block in &capture.blocks {
                    let block_end = block.address + block.length as u64;
                    let first = self
                        .regions
                        .partition_point(|region| region.end <= block.address);
                    for region in self.regions[first..]
                        .iter()
                        .take_while(|region| region.start < block_end)
                        .filter(|region| accepts(region))
                    {
                        let start = block.address.max(region.start);
                        let end = block_end.min(region.end);
                        if let Some(previous) = ranges.last_mut()
                            && previous.end == start
                            && previous.permissions == region.permissions
                            && previous.path == region.path
                        {
                            previous.end = end;
                        } else {
                            ranges.push(ScanRange {
                                start,
                                end,
                                permissions: region.permissions.clone(),
                                path: region.path.clone(),
                            });
                        }
                    }
                }
                ranges
            }
        }
    }

    pub fn image_base(&self) -> Option<u64> {
        self.regions
            .iter()
            .find(|region| self.is_image(region))
            .map(|region| region.start)
    }

    pub fn read_at(&self, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
        if let Some(budget) = &self.budget {
            budget.read(buffer.len())?;
        }
        self.backing.read_at(buffer, offset)
    }

    pub fn read_exact_at(&self, buffer: &mut [u8], offset: u64) -> io::Result<()> {
        if let Some(budget) = &self.budget {
            budget.read(buffer.len())?;
        }
        self.backing.read_exact_at(buffer, offset)
    }

    pub fn supports_private_range(&self, address: u64, length: usize) -> bool {
        let Some(end) = address.checked_add(length as u64) else {
            return false;
        };
        let mapped = self
            .regions
            .iter()
            .any(|region| region.supports_heap() && region.contains_range(address, end));
        mapped
            && self
                .backing
                .capture()
                .is_none_or(|capture| capture.contains_range(address, length))
    }

    pub fn supports_read_range(&self, address: u64, length: usize) -> bool {
        let Some(end) = address.checked_add(length as u64) else {
            return false;
        };
        let mapped = self.regions.iter().any(|region| {
            region.permissions.starts_with('r') && region.contains_range(address, end)
        });
        mapped
            && self
                .backing
                .capture()
                .is_none_or(|capture| capture.contains_range(address, length))
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn from_test_bytes(bytes: &[u8], regions: Vec<Region>) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT_FILE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "retinue-memory-{}-{}",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut output = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .expect("create test memory");
        std::io::Write::write_all(&mut output, bytes).expect("write test memory");
        drop(output);
        let file = File::open(&path).expect("open test memory");
        fs::remove_file(path).expect("unlink test memory");
        Self {
            pid: 0,
            backing: MemoryBacking::Live(file),
            maps: String::new(),
            regions,
            budget: None,
            image_path: None,
        }
    }
}

impl MemoryBacking {
    fn capture(&self) -> Option<&CaptureMemory> {
        match self {
            Self::Live(_) => None,
            Self::Capture(capture) => Some(capture),
            Self::Recording(recording) => recording.source.capture(),
        }
    }

    fn read_at(&self, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
        match self {
            Self::Live(file) => file.read_at(buffer, offset),
            Self::Capture(capture) => capture.read_at(buffer, offset),
            Self::Recording(recording) => recording.read_at(buffer, offset),
        }
    }

    fn read_exact_at(&self, buffer: &mut [u8], offset: u64) -> io::Result<()> {
        match self {
            Self::Live(file) => file.read_exact_at(buffer, offset),
            Self::Capture(capture) => capture.read_exact_at(buffer, offset),
            Self::Recording(recording) => {
                let mut read = 0;
                while read < buffer.len() {
                    let address = offset
                        .checked_add(read as u64)
                        .ok_or_else(|| io::Error::other("memory address overflow"))?;
                    match recording.read_at(&mut buffer[read..], address) {
                        Ok(0) => {
                            return Err(io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                "memory read incomplete",
                            ));
                        }
                        Ok(count) => read += count,
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        Err(error) => return Err(error),
                    }
                }
                Ok(())
            }
        }
    }
}

impl CaptureMemory {
    fn new(mut blocks: Vec<CaptureBlock>, bytes: Vec<u8>) -> Result<Self, String> {
        blocks.sort_by_key(|block| block.address);
        let mut previous_end = None;
        for block in &blocks {
            if block.length == 0 {
                return Err("capture contains an empty memory block".to_owned());
            }
            let virtual_end = block
                .address
                .checked_add(block.length as u64)
                .ok_or_else(|| "capture memory block overflows address space".to_owned())?;
            block
                .file_offset
                .checked_add(block.length)
                .filter(|end| *end <= bytes.len())
                .ok_or_else(|| "capture memory block exceeds data file".to_owned())?;
            if previous_end.is_some_and(|end| block.address < end) {
                return Err("capture contains overlapping memory blocks".to_owned());
            }
            previous_end = Some(virtual_end);
        }
        Ok(Self { bytes, blocks })
    }

    fn read_at(&self, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
        let mut address = offset;
        let mut written = 0;
        while written < buffer.len() {
            let index = self
                .blocks
                .partition_point(|block| block.address <= address);
            let Some(block) = index
                .checked_sub(1)
                .and_then(|index| self.blocks.get(index))
            else {
                break;
            };
            let block_end = block.address + block.length as u64;
            if address >= block_end {
                break;
            }
            let within = usize::try_from(address - block.address).unwrap();
            let length = (block.length - within).min(buffer.len() - written);
            let source = block.file_offset + within;
            buffer[written..written + length].copy_from_slice(&self.bytes[source..source + length]);
            written += length;
            address += length as u64;
        }
        Ok(written)
    }

    fn read_exact_at(&self, buffer: &mut [u8], offset: u64) -> io::Result<()> {
        let read = self.read_at(buffer, offset)?;
        if read == buffer.len() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "range is absent from capture",
            ))
        }
    }

    fn contains_range(&self, start: u64, length: usize) -> bool {
        if length == 0 {
            return true;
        }
        let Some(end) = start.checked_add(length as u64) else {
            return false;
        };
        let index = self.blocks.partition_point(|block| block.address <= start);
        let Some(mut index) = index.checked_sub(1) else {
            return false;
        };
        let mut covered = start;
        while covered < end {
            let Some(block) = self.blocks.get(index) else {
                return false;
            };
            if block.address > covered || block.address + block.length as u64 <= covered {
                return false;
            }
            covered = block.address + block.length as u64;
            index += 1;
        }
        true
    }
}

impl Region {
    pub fn contains(&self, address: u64) -> bool {
        (self.start..self.end).contains(&address)
    }

    pub fn supports_heap(&self) -> bool {
        self.permissions.starts_with("rw")
            && self.permissions.ends_with('p')
            && (self.path.is_empty() || self.path.starts_with('['))
    }

    fn contains_range(&self, start: u64, end: u64) -> bool {
        self.start <= start && start <= end && end <= self.end
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExecutableIdentity {
    pub path: PathBuf,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_unix_ms: Option<u128>,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub executable: ExecutableIdentity,
}

pub fn identify_process(pid: u32) -> Result<ProcessIdentity, String> {
    let path = fs::read_link(format!("/proc/{pid}/exe"))
        .map_err(|error| format!("could not resolve process {pid} executable: {error}"))?;
    identify_executable(pid, path)
}

pub fn identify_executable(pid: u32, path: PathBuf) -> Result<ProcessIdentity, String> {
    let metadata = fs::metadata(&path)
        .map_err(|error| format!("could not stat {}: {error}", path.display()))?;
    let modified_unix_ms = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis());
    let sha256 = hash_file(&path)?;
    Ok(ProcessIdentity {
        pid,
        executable: ExecutableIdentity {
            path,
            size: metadata.len(),
            modified_unix_ms,
            sha256,
        },
    })
}

fn hash_file(path: &PathBuf) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("could not open {} for hashing: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("could not hash {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let mut encoded = String::with_capacity(64);
    for byte in hasher.finalize() {
        let _ = write!(encoded, "{byte:02x}");
    }
    Ok(encoded)
}

pub fn parse_regions(maps: &str) -> Vec<Region> {
    maps.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let range = fields.next()?;
            let permissions = fields.next()?.to_owned();
            let _offset = fields.next()?;
            let _device = fields.next()?;
            let _inode = fields.next()?;
            let path = decode_maps_path(&fields.collect::<Vec<_>>().join(" "));
            let (start, end) = range.split_once('-')?;
            Some(Region {
                start: u64::from_str_radix(start, 16).ok()?,
                end: u64::from_str_radix(end, 16).ok()?,
                permissions,
                path,
            })
        })
        .collect()
}

fn decode_maps_path(path: &str) -> String {
    path.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_budget_is_checked_before_touching_memory() {
        let budget = Budget::new(crate::work::Limits {
            duration: std::time::Duration::from_secs(30),
            read_bytes: 4,
            write_bytes: 0,
        });
        let memory =
            ProcessMemory::from_test_bytes(b"abcdefgh", Vec::new()).with_budget(budget.clone());
        let mut bytes = [0; 4];
        memory.read_exact_at(&mut bytes, 0).unwrap();
        assert_eq!(&bytes, b"abcd");
        assert!(memory.read_at(&mut bytes, 4).is_err());
        assert_eq!(&bytes, b"abcd");
        assert_eq!(budget.usage().read_bytes_reserved, 4);
        assert!(memory.check_budget().is_err());
    }

    #[test]
    fn decodes_mapped_paths() {
        let maps = "140000000-141000000 r-xp 0 00:00 1 /test/Some\\040Program\n";
        assert_eq!(parse_regions(maps)[0].path, "/test/Some Program");
    }

    #[test]
    fn research_scans_use_explicit_image_and_private_memory() {
        let maps = concat!(
            "1000-2000 rw-p 0 00:00 0 \n",
            "2000-3000 r--p 0 00:00 1 /test/program\n",
            "3000-4000 r--p 0 00:00 2 /usr/lib/libc.so\n",
        );
        let memory = ProcessMemory::from_test_bytes(&[], parse_regions(maps))
            .with_image_path(Some("/test/program".into()));
        assert_eq!(memory.scan_ranges().len(), 2);
    }

    #[test]
    fn validates_complete_memory_ranges() {
        let region = Region {
            start: 0x1000,
            end: 0x2000,
            permissions: "rw-p".to_owned(),
            path: String::new(),
        };
        assert!(region.contains_range(0x1000, 0x2000));
        assert!(region.contains_range(0x1800, 0x1800));
        assert!(!region.contains_range(0x0fff, 0x1800));
        assert!(!region.contains_range(0x1800, 0x2001));
    }

    #[test]
    fn captured_memory_reads_across_adjacent_blocks() {
        let capture = CaptureMemory::new(
            vec![
                CaptureBlock {
                    address: 0x1000,
                    length: 4,
                    file_offset: 0,
                },
                CaptureBlock {
                    address: 0x1004,
                    length: 4,
                    file_offset: 4,
                },
            ],
            b"abcdefgh".to_vec(),
        )
        .unwrap();
        let mut bytes = [0_u8; 6];
        capture.read_exact_at(&mut bytes, 0x1001).unwrap();
        assert_eq!(&bytes, b"bcdefg");
        assert!(capture.contains_range(0x1001, 6));
        assert!(!capture.contains_range(0x0fff, 2));
    }
}
