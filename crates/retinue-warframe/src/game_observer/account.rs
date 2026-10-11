use std::fs::File;
use std::io;
use std::os::unix::fs::FileExt;

use iced_x86::{Decoder, DecoderOptions, Mnemonic, OpKind, Register};

use super::{ProcessIdentity, executable};
pub(crate) mod layout;
use layout::Layout;

#[derive(Clone, Debug)]
pub(crate) struct Reader {
    base: u64,
    layout: Layout,
}

impl Reader {
    pub(crate) fn discover(identity: &ProcessIdentity) -> Result<Self, String> {
        let (hash, bytes) = executable::read(&identity.executable.path)?;
        if hash != identity.executable.sha256 {
            return Err("Warframe executable changed during account discovery".into());
        }
        let base = crate::game_observer::memory::open(identity.pid)?
            .image_base()
            .ok_or("Warframe image mapping is unavailable")?;
        Ok(Self {
            base,
            layout: layout::discover(&bytes)?,
        })
    }

    pub(crate) fn bindings(&self) -> &Layout {
        &self.layout
    }

    pub(crate) fn read(&self, memory: &File) -> io::Result<u32> {
        let manager = self.manager(memory)?;
        let vector = Vector::read(
            memory,
            manager.saturating_add(self.layout.profiles_offset),
            8,
            32,
        )?;
        let mut selected = None;
        for holder in vector
            .bytes
            .chunks_exact(8)
            .map(u64_at)
            .filter(|&value| value != 0)
        {
            let profile = u64_at(&read::<8>(memory, holder)?);
            if profile == 0 {
                continue;
            }
            self.require_method(memory, profile, self.layout.seed_getter_rva)?;
            let table = u64_at(&read::<8>(memory, profile)?);
            let getter = u64_at(&read::<8>(
                memory,
                table.saturating_add(self.layout.identity_slot),
            )?);
            if !self.code_address(getter) {
                return Err(invalid("profile identity getter is outside game code"));
            }
            let offset = identity_offset(&read::<16>(memory, getter)?)?;
            if u64_at(&read::<8>(memory, profile.saturating_add(offset))?) == 0
                && selected.replace((holder, profile, offset)).is_some()
            {
                return Err(invalid("multiple primary player profiles"));
            }
        }
        let (holder, profile, identity_offset) =
            selected.ok_or_else(|| invalid("primary player profile is not initialized"))?;
        let platform = engine_string(
            memory,
            profile.saturating_add(self.layout.platform_id_offset),
        )?;
        let identifier = if platform.is_empty() {
            engine_string(
                memory,
                profile.saturating_add(self.layout.primary_id_offset),
            )?
        } else {
            platform
        };
        let value = seed(&identifier)
            .ok_or_else(|| invalid("primary profile account identifier is not initialized"))?;
        if u64_at(&read::<8>(memory, holder)?) != profile
            || u64_at(&read::<8>(memory, profile.saturating_add(identity_offset))?) != 0
        {
            return Err(invalid("primary player profile changed during read"));
        }
        vector.verify(memory)?;
        if self.manager(memory)? != manager {
            return Err(invalid("player profile manager changed during read"));
        }
        Ok(value)
    }

    fn manager(&self, memory: &File) -> io::Result<u64> {
        let registry = Vector::read(
            memory,
            self.base + self.layout.registry_rva + self.layout.registry_offset,
            16,
            4096,
        )?;
        let mut manager = None;
        for entry in registry.bytes.chunks_exact(16) {
            let holder = u64_at(&entry[8..]);
            if holder == 0 {
                continue;
            }
            let Ok(pointer) = read::<8>(memory, holder) else {
                continue;
            };
            let object = u64_at(&pointer);
            if object == 0 {
                continue;
            }
            if self
                .require_method(memory, object, self.layout.selector_rva)
                .is_err()
            {
                continue;
            }
            if manager.replace(object).is_some() {
                return Err(invalid("multiple player profile managers"));
            }
        }
        registry.verify(memory)?;
        manager.ok_or_else(|| invalid("player profile manager is not initialized"))
    }

    fn code_address(&self, address: u64) -> bool {
        (self.base + self.layout.code_start..self.base + self.layout.code_end).contains(&address)
    }

    fn require_method(&self, memory: &File, object: u64, method_rva: u64) -> io::Result<()> {
        let table = u64_at(&read::<8>(memory, object)?);
        let entries = read::<512>(memory, table)?;
        // Stop at the next RTTI/data entry instead of searching adjacent vtables.
        for entry in entries.chunks_exact(8) {
            let method = u64_at(entry);
            if !self.code_address(method) {
                break;
            }
            if method == self.base + method_rva {
                return Ok(());
            }
        }
        Err(invalid(
            "player profile vtable does not contain the validated getter",
        ))
    }
}

struct Vector {
    address: u64,
    header: [u8; 16],
    bytes: Vec<u8>,
}

impl Vector {
    fn read(memory: &File, address: u64, stride: usize, max_items: usize) -> io::Result<Self> {
        let header = read::<16>(memory, address)?;
        let pointer = u64_at(&header);
        let used = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
        let capacity = u32::from_le_bytes(header[12..].try_into().unwrap()) as usize;
        if !used.is_multiple_of(stride)
            || used > capacity
            || capacity > max_items * stride
            || (used != 0 && pointer == 0)
            || pointer.checked_add(used as u64).is_none()
        {
            return Err(invalid("invalid player profile registry vector"));
        }
        let mut bytes = vec![0; used];
        memory.read_exact_at(&mut bytes, pointer)?;
        Ok(Self {
            address,
            header,
            bytes,
        })
    }

    fn verify(&self, memory: &File) -> io::Result<()> {
        let mut current = vec![0; self.bytes.len()];
        memory.read_exact_at(&mut current, u64_at(&self.header))?;
        if read::<16>(memory, self.address)? != self.header || current != self.bytes {
            return Err(invalid("player profile registry changed during read"));
        }
        Ok(())
    }
}

fn identity_offset(code: &[u8]) -> io::Result<u64> {
    let mut decoder = Decoder::new(64, code, DecoderOptions::NONE);
    let load = decoder.decode();
    if load.mnemonic() != Mnemonic::Mov
        || load.op0_register() != Register::RAX
        || load.op1_kind() != OpKind::Memory
        || load.memory_base() != Register::RCX
        || load.memory_index() != Register::None
        || decoder.decode().mnemonic() != Mnemonic::Ret
    {
        return Err(invalid("unrecognized profile identity getter"));
    }
    let offset = load.memory_displacement64();
    if !(24..0x10000).contains(&offset) || !offset.is_multiple_of(8) {
        return Err(invalid("invalid profile identity field"));
    }
    Ok(offset)
}

fn engine_string(memory: &File, address: u64) -> io::Result<Vec<u8>> {
    let storage = read::<16>(memory, address)?;
    match storage[15] {
        0..=15 => Ok(storage[..usize::from(15 - storage[15])].to_vec()),
        0xff => {
            let pointer = u64_at(&storage);
            let length =
                (u32::from_le_bytes(storage[8..12].try_into().unwrap()) & 0x0fff_ffff) as usize;
            if length > 256
                || (length != 0 && pointer == 0)
                || pointer.checked_add(length as u64).is_none()
            {
                return Err(invalid("invalid player profile identifier length"));
            }
            let mut value = vec![0; length];
            memory.read_exact_at(&mut value, pointer)?;
            if read::<16>(memory, address)? != storage {
                return Err(invalid("player profile identifier changed during read"));
            }
            Ok(value)
        }
        _ => Err(invalid("invalid player profile identifier tag")),
    }
}

fn seed(identifier: &[u8]) -> Option<u32> {
    let token = std::str::from_utf8(identifier.get(2..8)?).ok()?;
    token
        .chars()
        .all(|value| value.is_ascii_hexdigit())
        .then(|| u32::from_str_radix(token, 16).ok())
        .flatten()
}

fn read<const N: usize>(memory: &File, address: u64) -> io::Result<[u8; N]> {
    if address.checked_add(N as u64).is_none() {
        return Err(invalid("invalid account memory address"));
    }
    let mut bytes = [0; N];
    memory.read_exact_at(&mut bytes, address)?;
    Ok(bytes)
}

fn u64_at(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes[..8].try_into().unwrap())
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests;
