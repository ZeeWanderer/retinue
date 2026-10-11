use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;

use serde::Serialize;
use serde_json::{Map, Value, json};

use super::executable;
use super::memory::{ProcessIdentity, ProcessMemory, identify_process};
use super::metadata;
use layout::InventoryLayout;

pub(crate) mod layout;

const MAX_STACKS: usize = 32_768;
const MAX_JOBS: usize = 512;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Snapshot {
    pub sync: String,
    pub fields: Map<String, Value>,
}

pub struct Reader {
    memory: ProcessMemory,
    base: u64,
    layout: InventoryLayout,
    names: HashMap<u64, String>,
}

impl Reader {
    pub fn open(pid: u32) -> Result<Self, String> {
        let identity = identify_process(pid)?;
        Self::open_for_identity(&identity)
    }

    pub fn open_for_identity(identity: &ProcessIdentity) -> Result<Self, String> {
        let (hash, bytes) = executable::read(&identity.executable.path)?;
        if hash != identity.executable.sha256 {
            return Err("Warframe executable changed during inventory discovery".into());
        }
        let layout = layout::discover(&bytes)
            .map_err(|reason| format!("native inventory discovery: {reason}"))?;
        let memory = crate::game_observer::memory::open(identity.pid)?;
        let base = memory
            .image_base()
            .ok_or("Warframe executable mapping not found")?;
        Ok(Self {
            memory,
            base,
            layout,
            names: HashMap::new(),
        })
    }

    pub fn read(&mut self) -> Result<Snapshot, String> {
        if self.names.len() > MAX_STACKS * 2 + MAX_JOBS {
            self.names.clear();
        }
        let profile = self.profile()?;
        let layout = self.layout;
        let sync = read::<12>(&self.memory, profile + layout.sync_offset)?;
        if sync == [0; 12] {
            return Err("native inventory is not synchronized".into());
        }
        let misc = Vector::read(&self.memory, profile + layout.misc_offset, 16, MAX_STACKS)?;
        let recipes = Vector::read(
            &self.memory,
            profile + layout.recipes_offset,
            16,
            MAX_STACKS,
        )?;
        let pending = Vector::read(
            &self.memory,
            profile + layout.pending_offset,
            0x58,
            MAX_JOBS,
        )?;
        let mut fields = Map::new();
        let mut used = HashSet::new();
        fields.insert("MiscItems".into(), self.stacks(&misc, &mut used)?);
        fields.insert("Recipes".into(), self.stacks(&recipes, &mut used)?);
        fields.insert("PendingRecipes".into(), self.jobs(&pending, &mut used)?);
        // Recheck bytes, not just vector pointers: stack counts change in place.
        for vector in [&misc, &recipes, &pending] {
            vector.verify(&self.memory)?;
        }
        if self.profile()? != profile
            || read::<12>(&self.memory, profile + layout.sync_offset)? != sync
        {
            return Err("native inventory changed during read".into());
        }
        self.names.retain(|address, _| used.contains(address));
        Ok(Snapshot {
            sync: object_id(&sync),
            fields,
        })
    }

    fn profile(&self) -> Result<u64, String> {
        let registry = Vector::read(
            &self.memory,
            self.base + self.layout.global_registry_rva + self.layout.global_registry_offset,
            16,
            4096,
        )?;
        let descriptor = self.base + self.layout.profile_descriptor_rva;
        let mut profile = None;
        for row in registry.bytes.chunks_exact(16) {
            let holder = u64_at(row, 8);
            let Ok(pointer) = read::<8>(&self.memory, holder) else {
                continue;
            };
            let address = u64::from_le_bytes(pointer);
            if address == 0 {
                continue;
            }
            let Ok(actual) = read::<8>(
                &self.memory,
                address.saturating_add(self.layout.resources.object_type_offset),
            ) else {
                continue;
            };
            if u64::from_le_bytes(actual) == descriptor {
                if u64::from_le_bytes(read::<8>(
                    &self.memory,
                    address.saturating_add(self.layout.resources.object_holder_offset),
                )?) != holder
                {
                    return Err("invalid native player profile backlink".into());
                }
                if profile.replace(address).is_some() {
                    return Err("ambiguous native player profile".into());
                }
            }
        }
        registry.verify(&self.memory)?;
        profile.ok_or_else(|| "native player profile is unavailable".into())
    }

    fn name(&mut self, descriptor: u64, used: &mut HashSet<u64>) -> Result<String, String> {
        if descriptor == 0 {
            return Err("null inventory item type".into());
        }
        used.insert(descriptor);
        if let Some(name) = self.names.get(&descriptor) {
            return Ok(name.clone());
        }
        let name =
            metadata::resource_name(&self.memory, self.base, self.layout.resources, descriptor)?;
        if !name.starts_with("/Lotus/") || name.len() > 2048 {
            return Err("invalid inventory item type".into());
        }
        self.names.insert(descriptor, name.clone());
        Ok(name)
    }

    fn stacks(&mut self, vector: &Vector, used: &mut HashSet<u64>) -> Result<Value, String> {
        let mut stacks = BTreeMap::new();
        for (index, row) in vector.bytes.chunks_exact(16).enumerate() {
            let count = quantity(row, vector.data + index as u64 * 16, self.layout)?;
            if count == 0 {
                continue;
            }
            let name = self.name(u64_at(row, 0), used)?;
            if stacks.insert(name, count).is_some() {
                return Err("duplicate native inventory stack".into());
            }
        }
        Ok(stacks
            .into_iter()
            .map(|(name, count)| json!({"ItemType":name,"ItemCount":count}))
            .collect())
    }

    fn jobs(&mut self, vector: &Vector, used: &mut HashSet<u64>) -> Result<Value, String> {
        let mut jobs = BTreeMap::new();
        for row in vector.bytes.chunks_exact(0x58) {
            if row[..12] == [0; 12] {
                return Err("invalid native foundry job identity".into());
            }
            let id = object_id(&row[..12]);
            let name = self.name(u64_at(row, 0x10), used)?;
            let seconds = engine_string(&self.memory, &row[0x18..0x28])?;
            let date = seconds
                .parse::<u64>()
                .ok()
                .and_then(|n| n.checked_mul(1000))
                .ok_or("invalid native foundry completion time")?;
            let mut job = json!({"ItemId":{"$oid":id},"ItemType":name,
                "CompletionDate":{"$date":{"$numberLong":date.to_string()}}});
            for (key, offset) in [
                ("TargetItemId", 0x28),
                ("TargetFingerprint", 0x38),
                ("IngredientId", 0x48),
            ] {
                let text = engine_string(&self.memory, &row[offset..offset + 16])?;
                if !text.is_empty() {
                    job[key] = text.into();
                }
            }
            if jobs.insert(id, job).is_some() {
                return Err("duplicate native foundry job".into());
            }
        }
        Ok(jobs.into_values().collect())
    }
}

struct Vector {
    address: u64,
    header: [u8; 16],
    data: u64,
    bytes: Vec<u8>,
}

impl Vector {
    fn read(
        memory: &ProcessMemory,
        address: u64,
        stride: usize,
        limit: usize,
    ) -> Result<Self, String> {
        let header = read(memory, address)?;
        let data = u64_at(&header, 0);
        let length = u32_at(&header, 8) as usize;
        let capacity = u32_at(&header, 12) as usize;
        if !length.is_multiple_of(stride)
            || length > limit * stride
            || length > capacity
            || (length != 0 && data == 0)
            || data.checked_add(length as u64).is_none()
        {
            return Err("invalid native inventory vector".into());
        }
        let mut bytes = vec![0; length];
        memory
            .read_exact_at(&mut bytes, data)
            .map_err(|e| format!("native inventory vector: {e}"))?;
        Ok(Self {
            address,
            header,
            data,
            bytes,
        })
    }

    fn verify(&self, memory: &ProcessMemory) -> Result<(), String> {
        let mut bytes = vec![0; self.bytes.len()];
        memory
            .read_exact_at(&mut bytes, self.data)
            .map_err(|e| format!("native inventory reread: {e}"))?;
        if bytes != self.bytes || read::<16>(memory, self.address)? != self.header {
            return Err("native inventory changed during read".into());
        }
        Ok(())
    }
}

fn quantity(row: &[u8], address: u64, layout: InventoryLayout) -> Result<u32, String> {
    let encoded = u32_at(row, 12);
    if u32_at(row, 8) != encoded ^ layout.check_mask {
        return Err("native inventory quantity check failed".into());
    }
    let count = encoded.rotate_left(layout.count_rotation)
        ^ ((address + 12) >> layout.address_shift) as u32
        ^ layout.count_mask;
    if count > i32::MAX as u32 {
        return Err("negative native inventory quantity".into());
    }
    Ok(count)
}

fn engine_string(memory: &ProcessMemory, storage: &[u8]) -> Result<String, String> {
    let bytes = match storage[15] {
        tag @ 0..=15 => storage[..15 - tag as usize].to_vec(),
        255 => {
            let length = (u32_at(storage, 8) & 0x0fff_ffff) as usize;
            if length > 4096 {
                return Err("native inventory string exceeds limit".into());
            }
            let mut bytes = vec![0; length];
            memory
                .read_exact_at(&mut bytes, u64_at(storage, 0))
                .map_err(|e| format!("native inventory string: {e}"))?;
            bytes
        }
        _ => return Err("invalid native inventory string tag".into()),
    };
    String::from_utf8(bytes).map_err(|e| format!("native inventory string: {e}"))
}

fn object_id(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(24);
    for byte in bytes {
        let _ = write!(result, "{byte:02x}");
    }
    result
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn read<const N: usize>(memory: &ProcessMemory, address: u64) -> Result<[u8; N], String> {
    let mut bytes = [0; N];
    memory
        .read_exact_at(&mut bytes, address)
        .map_err(|e| format!("native inventory read: {e}"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::super::executable::fixture::resources;
    use super::super::memory::Region;
    use super::*;

    fn encode_count(row: &mut [u8], address: u64, count: u32, layout: InventoryLayout) {
        let encoded = (count ^ ((address + 12) >> layout.address_shift) as u32 ^ layout.count_mask)
            .rotate_right(layout.count_rotation);
        row[8..12].copy_from_slice(&(encoded ^ layout.check_mask).to_le_bytes());
        row[12..16].copy_from_slice(&encoded.to_le_bytes());
    }

    fn put64(bytes: &mut [u8], offset: usize, value: u64) {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn put32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn vector(bytes: &mut [u8], offset: usize, data: u64, length: u32) {
        put64(bytes, offset, data);
        put32(bytes, offset + 8, length);
        put32(bytes, offset + 12, length);
    }

    fn fixture() -> (InventoryLayout, Vec<u8>) {
        fixture_for(0xc55198a3, 0xad84b2ea)
    }

    fn fixture_for(count_mask: u32, check_mask: u32) -> (InventoryLayout, Vec<u8>) {
        let layout = InventoryLayout {
            global_registry_rva: 0x100,
            global_registry_offset: 0x138,
            resources: resources(0x300, 0),
            profile_descriptor_rva: 0x600,
            sync_offset: 0x140,
            misc_offset: 0x100,
            recipes_offset: 0x110,
            pending_offset: 0x120,
            count_mask,
            check_mask,
            count_rotation: 19,
            address_shift: 3,
        };
        let mut bytes = vec![0; 0x3000];
        vector(&mut bytes, 0x238, 0x400, 16);
        put32(&mut bytes, 0x400, 0x12345678);
        put64(&mut bytes, 0x408, 0x500);
        put64(&mut bytes, 0x500, 0x1000);
        put64(&mut bytes, 0x1008, layout.profile_descriptor_rva);
        put64(&mut bytes, 0x1010, 0x500);
        bytes[0x1140..0x114c].copy_from_slice(&[1; 12]);
        vector(&mut bytes, 0x1100, 0x2000, 32);
        vector(&mut bytes, 0x1110, 0x2100, 16);
        vector(&mut bytes, 0x1120, 0x2200, 0x58);
        for (address, descriptor, count) in [
            (0x2000, 0x9000, 22),
            (0x2010, 0x9100, 0),
            (0x2100, 0x9200, 1),
        ] {
            put64(&mut bytes, address, descriptor);
            encode_count(
                &mut bytes[address..address + 16],
                address as u64,
                count,
                layout,
            );
        }
        bytes[0x2200..0x220c].copy_from_slice(&[2; 12]);
        put64(&mut bytes, 0x2210, 0x9200);
        bytes[0x2218..0x2222].copy_from_slice(b"1789086000");
        bytes[0x2227] = 5;
        bytes[0x2237] = 15;
        bytes[0x2247] = 15;
        bytes[0x2257] = 15;
        (layout, bytes)
    }

    fn reader(layout: InventoryLayout, bytes: &[u8]) -> Reader {
        Reader {
            memory: crate::game_observer::memory::from_test_bytes(
                bytes,
                vec![Region {
                    start: 0,
                    end: bytes.len() as u64,
                    permissions: "rw-p".into(),
                    path: String::new(),
                }],
            ),
            base: 0,
            layout,
            names: [
                (0x9000, "/Lotus/ingredient".into()),
                (0x9200, "/Lotus/blueprint".into()),
            ]
            .into(),
        }
    }

    #[test]
    fn reads_absolute_stacks_and_pending_jobs() {
        let (adapter, bytes) = fixture();
        let mut reader = reader(adapter, &bytes);
        let first = reader.read().unwrap();
        assert_eq!(first.sync, "010101010101010101010101");
        assert_eq!(
            first.fields["MiscItems"],
            json!([{"ItemType":"/Lotus/ingredient","ItemCount":22}])
        );
        assert_eq!(
            first.fields["PendingRecipes"],
            json!([{
                "ItemId":{"$oid":"020202020202020202020202"},"ItemType":"/Lotus/blueprint",
                "CompletionDate":{"$date":{"$numberLong":"1789086000000"}}
            }])
        );
        assert_eq!(reader.read().unwrap(), first);
    }

    #[test]
    fn uses_discovered_registry_and_object_fields() {
        let (mut layout, mut bytes) = fixture();
        let expected = reader(layout, &bytes).read().unwrap();
        bytes.copy_within(0x238..0x248, 0x280);
        bytes[0x238..0x248].fill(0);
        bytes.copy_within(0x1008..0x1018, 0x1048);
        bytes[0x1008..0x1018].fill(0);
        layout.global_registry_offset = 0x180;
        layout.resources = resources(0x300, 0x40);
        assert_eq!(reader(layout, &bytes).read().unwrap(), expected);
    }

    #[test]
    fn updated_inventory_encoding_preserves_collection_shape() {
        let (layout, bytes) = fixture_for(0xac7e8740, 0x9c084a47);
        let actual = reader(layout, &bytes).read().unwrap();
        let (old, old_bytes) = fixture();
        assert_eq!(actual, reader(old, &old_bytes).read().unwrap());
        let wrong_encoding = InventoryLayout {
            count_mask: old.count_mask,
            check_mask: old.check_mask,
            count_rotation: old.count_rotation,
            ..layout
        };
        assert_eq!(
            reader(wrong_encoding, &bytes).read().unwrap_err(),
            "native inventory quantity check failed"
        );
    }

    #[test]
    fn quantities_are_address_dependent_and_integrity_checked() {
        for (mask, check) in [(0xc55198a3, 0xad84b2ea), (0xac7e8740, 0x9c084a47)] {
            let mut row = [0; 16];
            let layout = fixture_for(mask, check).0;
            for address in [0x1000, 0x3de9ae00, 0x10000000e0] {
                encode_count(&mut row, address, 22, layout);
                assert_eq!(quantity(&row, address, layout).unwrap(), 22);
                assert_ne!(quantity(&row, address + 16, layout).unwrap(), 22);
            }
            row[8] ^= 1;
            assert!(quantity(&row, 0x10000000e0, layout).is_err());
            encode_count(&mut row, 0x1000, u32::MAX, layout);
            assert!(quantity(&row, 0x1000, layout).is_err());
        }
    }

    #[test]
    fn profile_uses_type_and_backlink_not_rotating_registry_key() {
        let (mut layout, mut bytes) = fixture();
        put32(&mut bytes, 0x400, 0x87654321);
        layout.profile_descriptor_rva = 0x680;
        put64(&mut bytes, 0x1008, 0x680);
        assert!(reader(layout, &bytes).read().is_ok());

        let mut invalid = bytes.clone();
        put64(&mut invalid, 0x1010, 0x700);
        assert_eq!(
            reader(layout, &invalid).read().unwrap_err(),
            "invalid native player profile backlink"
        );

        let mut ambiguous = bytes.clone();
        vector(&mut ambiguous, 0x238, 0x400, 32);
        put64(&mut ambiguous, 0x418, 0x500);
        assert_eq!(
            reader(layout, &ambiguous).read().unwrap_err(),
            "ambiguous native player profile"
        );

        let mut absent = bytes.clone();
        put64(&mut absent, 0x500, 0);
        let mut waiting = reader(layout, &absent);
        assert_eq!(
            waiting.read().unwrap_err(),
            "native player profile is unavailable"
        );
        waiting.memory = reader(layout, &bytes).memory;
        assert!(waiting.read().is_ok());
    }

    #[test]
    fn changed_rotation_and_address_shift_decode_quantities() {
        let mut layout = fixture().0;
        layout.address_shift = 4;
        layout.count_rotation = 11;
        let mut row = [0; 16];
        encode_count(&mut row, 0x3de9ae00, 37, layout);
        assert_eq!(quantity(&row, 0x3de9ae00, layout).unwrap(), 37);
    }

    #[test]
    fn invalid_vectors_strings_and_sync_fail_without_partial_snapshot() {
        let (adapter, bytes) = fixture();
        for (offset, value) in [
            (0x1108, 17),
            (0x1108, u32::MAX),
            (0x110c, 0),
            (0x1128, 0x58 * 513),
        ] {
            let mut corrupt = bytes.clone();
            put32(&mut corrupt, offset, value);
            assert!(reader(adapter, &corrupt).read().is_err());
        }
        let mut corrupt = bytes.clone();
        corrupt[0x1140..0x114c].fill(0);
        assert!(reader(adapter, &corrupt).read().is_err());
        let mut corrupt = bytes.clone();
        corrupt[0x2227] = 16;
        assert!(reader(adapter, &corrupt).read().is_err());
    }

    #[test]
    fn detects_same_address_mutation_and_vector_reallocation() {
        let (adapter, bytes) = fixture();
        let before = reader(adapter, &bytes);
        let vector = Vector::read(&before.memory, 0x1100, 16, MAX_STACKS).unwrap();
        let mut changed = bytes.clone();
        encode_count(&mut changed[0x2000..0x2010], 0x2000, 27, adapter);
        assert!(vector.verify(&reader(adapter, &changed).memory).is_err());
        let mut changed = bytes;
        put64(&mut changed, 0x1100, 0x2500);
        assert!(vector.verify(&reader(adapter, &changed).memory).is_err());
    }

    #[test]
    #[ignore = "manual read-only benchmark; requires WF_GAME_PID"]
    fn benchmark_live_inventory_refresh() {
        let pid = std::env::var("WF_GAME_PID").unwrap().parse().unwrap();
        let mut reader = Reader::open(pid).unwrap();
        let cold = std::time::Instant::now();
        let snapshot = reader.read().unwrap();
        let cold = cold.elapsed();
        let mut samples = Vec::new();
        for _ in 0..20 {
            let start = std::time::Instant::now();
            reader.read().unwrap();
            samples.push(start.elapsed().as_micros());
        }
        samples.sort_unstable();
        eprintln!(
            "native inventory: cold={}us warm_median={}us warm_max={}us resources={} recipes={}",
            cold.as_micros(),
            samples[10],
            samples[19],
            snapshot.fields["MiscItems"].as_array().unwrap().len(),
            snapshot.fields["Recipes"].as_array().unwrap().len()
        );
    }
}
