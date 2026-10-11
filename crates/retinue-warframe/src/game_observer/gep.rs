use std::fs::File;
use std::io;
use std::os::unix::fs::FileExt;

use memchr::memchr;

use super::{ProcessIdentity, account, executable};
pub(crate) mod layout;
use layout::ResponsePath;

#[cfg(test)]
const MAX_PAYLOAD_SIZE: usize = 0x4e2000;
const RESPONSE_READ_SIZE: usize = 0x9e2000 - 1;
const RESPONSE_READ_CHUNK_SIZE: usize = 256 * 1024;

#[derive(Clone, Debug)]
pub struct Sources {
    manager_global: u64,
    account: Result<account::Reader, String>,
    response: ResponsePath,
}

pub struct PollState {
    direct: ChangeState,
    indirect: ChangeState,
    scratch: Vec<u8>,
}

impl Default for PollState {
    fn default() -> Self {
        Self {
            direct: ChangeState::default(),
            indirect: ChangeState::default(),
            scratch: vec![0; RESPONSE_READ_SIZE],
        }
    }
}

impl PollState {
    pub fn invalidate(&mut self) {
        self.direct.address = None;
        self.indirect.address = None;
    }
}

#[derive(Default)]
struct ChangeState {
    address: Option<u64>,
    payload: Vec<u8>,
}

impl Sources {
    pub fn discover(identity: &ProcessIdentity) -> Result<Self, String> {
        let (hash, bytes) = executable::read(&identity.executable.path)?;
        if hash != identity.executable.sha256 {
            return Err("Warframe executable changed during HTTP discovery".into());
        }
        let layout = layout::discover(&bytes)?;
        let base = crate::game_observer::memory::open(identity.pid)?
            .image_base()
            .ok_or("Warframe image mapping is unavailable")?;
        let manager_global = base
            .checked_add(layout.manager_rva)
            .ok_or("invalid Warframe HTTP manager address")?;
        Ok(Self {
            manager_global,
            account: account::Reader::discover(identity),
            response: layout.response,
        })
    }

    pub fn manager_global(&self) -> u64 {
        self.manager_global
    }

    pub fn account_bindings(&self) -> Result<serde_json::Value, String> {
        self.account
            .as_ref()
            .map(|reader| serde_json::json!(reader.bindings()))
            .map_err(Clone::clone)
    }

    pub fn account_seed(&self, mem: &File) -> io::Result<u32> {
        self.account
            .as_ref()
            .map_err(|reason| io::Error::other(reason.clone()))?
            .read(mem)
    }

    pub fn response_offsets(&self) -> (u64, u64, u64) {
        (
            self.response.queue_table,
            self.response.item_base,
            self.response.body,
        )
    }

    pub fn persistent_payloads(
        &self,
        mem: &File,
        state: &mut PollState,
    ) -> Vec<(&'static str, Vec<u8>)> {
        let Ok(manager) = read_u64(mem, self.manager_global).and_then(non_null) else {
            return Vec::new();
        };
        let mut payloads = Vec::with_capacity(2);
        if let Ok(body) = self.primary_body(mem, manager) {
            if let Some(payload) =
                changed_c_string(mem, body, &mut state.direct, &mut state.scratch)
            {
                payloads.push(("direct", payload));
            }
            if let Ok(indirect) = read_u64(mem, body).and_then(non_null)
                && let Some(payload) =
                    changed_c_string(mem, indirect, &mut state.indirect, &mut state.scratch)
            {
                payloads.push(("indirect", payload));
            }
        }
        payloads
    }

    fn primary_body(&self, mem: &File, manager: u64) -> io::Result<u64> {
        let table_slot = manager
            .checked_add(self.response.queue_table)
            .ok_or_else(|| invalid_pointer("Warframe HTTP table"))?;
        let table = non_null(read_u64(mem, table_slot)?)?;
        let bucket = non_null(read_u64(mem, table)?)?;
        let item = non_null(read_u64(mem, bucket)?)?;
        let body_slot = item
            .checked_add(self.response.item_base)
            .and_then(|address| address.checked_add(self.response.body))
            .ok_or_else(|| invalid_pointer("Warframe response"))?;
        non_null(read_u64(mem, body_slot)?)
    }
}

fn changed_c_string(
    mem: &File,
    address: u64,
    state: &mut ChangeState,
    scratch: &mut [u8],
) -> Option<Vec<u8>> {
    if state.address == Some(address)
        && !state.payload.is_empty()
        && !c_string_changed(mem, address, state, scratch)
    {
        return None;
    }
    let payload = read_c_string(mem, address, scratch).ok()?;
    if state.address == Some(address) && state.payload == payload {
        return None;
    }
    remember_payload(state, address, &payload);
    Some(payload)
}

fn remember_payload(state: &mut ChangeState, address: u64, payload: &[u8]) {
    state.address = Some(address);
    state.payload.clear();
    state.payload.extend_from_slice(payload);
}

fn c_string_changed(mem: &File, address: u64, state: &ChangeState, scratch: &mut [u8]) -> bool {
    let Some(length) = state.payload.len().checked_add(1) else {
        return true;
    };
    let Some(current) = scratch.get_mut(..length) else {
        return true;
    };
    if read_exact_at(mem, address, current).is_err() {
        return true;
    }
    current.last() != Some(&0) || current[..state.payload.len()] != state.payload
}

fn read_c_string(mem: &File, address: u64, scratch: &mut [u8]) -> io::Result<Vec<u8>> {
    let mut offset = 0;
    while offset < scratch.len() {
        let end = scratch.len().min(offset + RESPONSE_READ_CHUNK_SIZE);
        let chunk_address = address
            .checked_add(offset as u64)
            .ok_or_else(|| invalid_pointer("Warframe HTTP response"))?;
        let read = match mem.read_at(&mut scratch[offset..end], chunk_address) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Warframe HTTP response",
            ));
        }
        if let Some(relative_end) = memchr(0, &scratch[offset..offset + read]) {
            let payload_end = offset + relative_end;
            if payload_end == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "empty Warframe HTTP response",
                ));
            }
            return Ok(scratch[..payload_end].to_vec());
        }
        offset += read;
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "unterminated Warframe HTTP response",
    ))
}

fn non_null(address: u64) -> io::Result<u64> {
    if address == 0 {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "null Warframe pointer",
        ))
    } else {
        Ok(address)
    }
}

fn invalid_pointer(name: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("invalid {name} pointer"),
    )
}

fn read_exact_at(mem: &File, address: u64, buffer: &mut [u8]) -> io::Result<()> {
    let mut read = 0;
    while read < buffer.len() {
        let read_address = address
            .checked_add(read as u64)
            .ok_or_else(|| invalid_pointer("process-memory read"))?;
        let count = mem.read_at(&mut buffer[read..], read_address)?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "short process-memory read",
            ));
        }
        read += count;
    }
    Ok(())
}

fn read_u64(mem: &File, address: u64) -> io::Result<u64> {
    let mut bytes = [0_u8; 8];
    read_exact_at(mem, address, &mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, OpenOptions};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn reads_primary_persistent_response() {
        let mut bytes = vec![0_u8; 0x1000];
        put_u64(&mut bytes, 0x20, 0x100);
        put_u64(&mut bytes, 0x198, 0x300);
        put_u64(&mut bytes, 0x300, 0x400);
        put_u64(&mut bytes, 0x400, 0x500);
        put_u64(&mut bytes, 0x550, 0x700);
        let payload = b"{\"LastInventorySync\":\"live\"}\0";
        bytes[0x700..0x700 + payload.len()].copy_from_slice(payload);
        let path = temp_file(&bytes);
        let mem = File::open(&path).unwrap();
        let sources = Sources {
            manager_global: 0x20,
            account: Err("not requested".into()),
            response: ResponsePath {
                queue_table: 0x98,
                item_base: 0x18,
                body: 0x38,
            },
        };
        let mut state = PollState {
            scratch: vec![0; payload.len()],
            ..PollState::default()
        };
        let payloads = sources.persistent_payloads(&mem, &mut state);
        assert_eq!(payloads[0].0, "direct");
        assert_eq!(payloads[0].1, b"{\"LastInventorySync\":\"live\"}");
        assert!(sources.persistent_payloads(&mem, &mut state).is_empty());
        state.invalidate();
        assert_eq!(sources.persistent_payloads(&mem, &mut state), payloads);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn indirect_changes_do_not_depend_on_direct_changes() {
        let mut bytes = vec![0_u8; 0x1000];
        for (slot, pointer) in [
            (0x20, 0x100),
            (0x198, 0x300),
            (0x300, 0x400),
            (0x400, 0x500),
            (0x550, 0x700),
            (0x700, 0x801),
        ] {
            put_u64(&mut bytes, slot, pointer);
        }
        let payload = b"{\"LastInventorySync\":\"live\",\"XP\":1}\0";
        bytes[0x801..0x801 + payload.len()].copy_from_slice(payload);
        let path = temp_file(&bytes);
        let mem = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        fs::remove_file(path).unwrap();
        let sources = Sources {
            manager_global: 0x20,
            account: Err("not requested".into()),
            response: ResponsePath {
                queue_table: 0x98,
                item_base: 0x18,
                body: 0x38,
            },
        };
        let mut state = PollState {
            scratch: vec![0; 128],
            ..Default::default()
        };
        let initial = sources.persistent_payloads(&mem, &mut state);
        assert!(
            initial
                .iter()
                .any(|(name, data)| *name == "indirect" && data == &payload[..payload.len() - 1])
        );
        assert!(sources.persistent_payloads(&mem, &mut state).is_empty());
        mem.write_all_at(b"2", 0x801 + payload.len() as u64 - 3)
            .unwrap();
        assert_eq!(
            sources.persistent_payloads(&mem, &mut state),
            vec![(
                "indirect",
                b"{\"LastInventorySync\":\"live\",\"XP\":2}".to_vec()
            )]
        );
        state.invalidate();
        assert!(
            sources
                .persistent_payloads(&mem, &mut state)
                .iter()
                .any(|(name, _)| *name == "indirect")
        );
    }

    #[test]
    fn rejects_overflowing_response_pointer() {
        let path = temp_file(&[]);
        let mem = File::open(&path).unwrap();
        fs::remove_file(path).unwrap();
        let sources = Sources {
            manager_global: 0,
            account: Err("not requested".into()),
            response: ResponsePath {
                queue_table: 0x98,
                item_base: 0x18,
                body: 0x38,
            },
        };
        assert_eq!(
            sources.primary_body(&mem, u64::MAX - 8).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn shared_sampler_keeps_polling_when_consumer_is_stalled() {
        use crate::observation::gep::{Content, Options, Sampler};
        use std::time::{Duration, Instant};

        let mut bytes = vec![0_u8; 0x1000];
        put_u64(&mut bytes, 0x20, 0x100);
        put_u64(&mut bytes, 0x198, 0x300);
        put_u64(&mut bytes, 0x300, 0x400);
        put_u64(&mut bytes, 0x400, 0x500);
        put_u64(&mut bytes, 0x550, 0x700);
        let payload = b"{\"LastInventorySync\":1,\"XP\":000}\0";
        bytes[0x700..0x700 + payload.len()].copy_from_slice(payload);
        let path = temp_file(&bytes);
        let mem = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        fs::remove_file(path).unwrap();
        let sources = Sources {
            manager_global: 0x20,
            account: Err("not requested".into()),
            response: ResponsePath {
                queue_table: 0x98,
                item_base: 0x18,
                body: 0x38,
            },
        };
        let started = Instant::now();
        let mut sampler = Sampler::start(
            mem.try_clone().unwrap(),
            sources,
            Options::default(),
            || true,
        )
        .unwrap();
        let timeout = Instant::now() + Duration::from_secs(5);
        for change in 0..24 {
            let previous = sampler.report().poll.count;
            mem.write_all_at(
                format!("{change:03}").as_bytes(),
                0x700 + payload.len() as u64 - 5,
            )
            .unwrap();
            while sampler.report().poll.count <= previous + 1 {
                assert!(
                    Instant::now() < timeout,
                    "sampler stalled with a full queue"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        sampler.stop().unwrap();
        let report = sampler.report();
        assert_eq!(report.queue.items, 16);
        assert!(report.queue.dropped_items > 0);
        let mut previous = 0;
        while let Ok(sample) = sampler.queue.recv_timeout(Duration::ZERO) {
            assert!(sample.sequence > previous);
            assert!(sample.captured >= started);
            let Content::Payload { bytes, .. } = sample.content else {
                panic!()
            };
            assert!(bytes.starts_with(b"{\"LastInventorySync\""));
            previous = sample.sequence;
        }
        assert!(!sampler.is_running());
    }

    #[test]
    fn reads_terminated_response_at_readable_boundary() {
        let path = temp_file(b"{\"XP\":1}\0");
        let mem = File::open(&path).unwrap();
        fs::remove_file(path).unwrap();
        let mut scratch = vec![0; RESPONSE_READ_SIZE];
        assert_eq!(read_c_string(&mem, 0, &mut scratch).unwrap(), b"{\"XP\":1}");
        assert!(read_c_string(&mem, 0, &mut scratch[..5]).is_err());
    }

    #[test]
    #[ignore = "manual GEP read-volume benchmark"]
    fn benchmark_unchanged_response() {
        for size in [64 * 1024, 1024 * 1024, MAX_PAYLOAD_SIZE] {
            let mut payload = vec![b'a'; size];
            payload.push(0);
            let path = temp_file(&payload);
            let mem = File::open(&path).unwrap();
            fs::remove_file(path).unwrap();
            let mut state = ChangeState::default();
            let mut scratch = vec![0; RESPONSE_READ_SIZE];
            assert!(changed_c_string(&mem, 0, &mut state, &mut scratch).is_some());
            let start = std::time::Instant::now();
            for _ in 0..1000 {
                assert!(changed_c_string(&mem, 0, &mut state, &mut scratch).is_none());
            }
            eprintln!(
                "unchanged_response bytes_per_poll={} mean_us={:.1}",
                size + 1,
                start.elapsed().as_secs_f64() * 1000.0
            );
        }
    }

    #[test]
    fn detects_reused_inventory_buffer_changes() {
        let mut payload = br#"{"padding":""#.to_vec();
        payload.extend(std::iter::repeat_n(b'a', 256));
        payload.extend_from_slice(br#"","LastInventorySync":"one"}"#);
        payload.push(0);
        let path = temp_file(&payload);
        let mem = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let mut state = ChangeState::default();
        let mut scratch = vec![0; payload.len()];

        assert_eq!(
            changed_c_string(&mem, 0, &mut state, &mut scratch),
            Some(payload[..payload.len() - 1].to_vec())
        );
        assert_eq!(changed_c_string(&mem, 0, &mut state, &mut scratch), None);

        mem.write_all_at(b"b", 128).unwrap();
        let changed = changed_c_string(&mem, 0, &mut state, &mut scratch).unwrap();
        assert_eq!(changed[128], b'b');
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn missing_account_bindings_do_not_disable_http_sources() {
        let sources = Sources {
            manager_global: 0,
            account: Err("account seed getter signature not found".into()),
            response: ResponsePath {
                queue_table: 0,
                item_base: 0,
                body: 0,
            },
        };
        assert_eq!(
            sources.account_bindings().unwrap_err(),
            "account seed getter signature not found"
        );
        assert_eq!(sources.response_offsets(), (0, 0, 0));
    }

    #[test]
    fn detects_same_length_change_across_large_reused_buffer() {
        let mut payload = vec![b'a'; 1024 * 1024];
        payload.extend_from_slice(b"LastInventorySync:one");
        payload.push(0);
        let path = temp_file(&payload);
        let mem = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let mut state = ChangeState::default();
        let mut scratch = vec![0; payload.len()];

        assert!(changed_c_string(&mem, 0, &mut state, &mut scratch).is_some());
        assert_eq!(changed_c_string(&mem, 0, &mut state, &mut scratch), None);
        let changed_offset = 768 * 1024 + 16;
        mem.write_all_at(b"b", changed_offset as u64).unwrap();
        let changed = changed_c_string(&mem, 0, &mut state, &mut scratch).unwrap();
        assert_eq!(changed[changed_offset], b'b');
        fs::remove_file(path).unwrap();
    }

    fn temp_file(bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "wfcompanion-gep-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, bytes).unwrap();
        path
    }

    fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
}
