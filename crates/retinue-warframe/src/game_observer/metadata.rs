use std::collections::{BTreeMap, HashMap, HashSet};

use memchr::memmem;
use serde::Serialize;

use super::ProcessIdentity;
use super::executable::ResourceLayout;
use super::memory::{ExecutableIdentity, ProcessMemory, Region, identify_process};
use layout::{ManifestFields, MetadataLayout};

mod layout;

const MAX_STORE_ENTRIES: usize = 100_000;
const MAX_VARIANT_ENTRIES: usize = 50_000;
const SCAN_CHUNK_SIZE: usize = 8 * 1024 * 1024;
const GAME_RULES_PREFIX: usize = 0x2000;
const PLAYER_POWER_SUIT: &str = "/Lotus/Types/Game/PowerSuits/PlayerPowerSuit";

#[derive(Debug, Serialize)]
pub struct GameMetadata {
    schema: u32,
    executable: ExecutableIdentity,
    archimedea: ArchimedeaMetadata,
}

#[derive(Debug, Serialize)]
struct ArchimedeaMetadata {
    catalog: Catalog,
    owned_suit_items: Vec<String>,
    owned_weapon_items: Vec<String>,
    suit_aliases: Vec<Alias>,
    weapon_aliases: Vec<Alias>,
}

#[derive(Debug, Default, Serialize)]
struct Catalog {
    suits: Vec<String>,
    primaries: Vec<String>,
    secondaries: Vec<String>,
    melees: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct Alias {
    source: String,
    canonical: String,
}

#[derive(Clone, Debug)]
struct StoreEntry {
    category: u8,
    path: String,
    descriptor: u64,
    description: u32,
    excluded: bool,
    eligible: bool,
}

#[derive(Clone, Debug)]
struct DescriptorNode {
    address: u64,
    name: String,
    parent: u64,
}

pub fn capture(pid: u32) -> Result<GameMetadata, String> {
    let identity = identify_process(pid)?;
    capture_for_identity(pid, identity)
}

pub(super) fn inspect_image(bytes: &[u8]) -> Result<serde_json::Value, String> {
    layout::discover(bytes).map(|bindings| serde_json::json!(bindings))
}

pub fn capture_for_identity(pid: u32, identity: ProcessIdentity) -> Result<GameMetadata, String> {
    if identity.pid != pid {
        return Err("Warframe process identity PID mismatch".to_owned());
    }
    let (hash, layout) = layout::read(&identity.executable.path)?;
    if hash != identity.executable.sha256 {
        return Err("Warframe executable changed during metadata discovery".to_owned());
    }
    let memory = crate::game_observer::memory::open(pid)?;
    let base = memory
        .image_base()
        .ok_or_else(|| "Warframe executable mapping not found".to_owned())?;
    let store = read_store_entries(&memory, base, layout)?;
    let variants = read_variant_manifest(&memory, base, layout)?;
    let archimedea = build_archimedea(&memory, base, layout, &store, &variants)?;
    Ok(GameMetadata {
        schema: 2,
        executable: identity.executable,
        archimedea,
    })
}

fn build_archimedea(
    memory: &ProcessMemory,
    base: u64,
    layout: MetadataLayout,
    store: &[StoreEntry],
    variants: &HashMap<u64, u64>,
) -> Result<ArchimedeaMetadata, String> {
    let mut catalog = Catalog::default();
    let mut owned_suit_items = Vec::new();
    let mut owned_weapon_items = Vec::new();
    let mut weapon_aliases = Vec::new();
    for entry in store {
        if entry.category == 3 {
            owned_suit_items.push(entry.path.clone());
            if !entry.excluded {
                catalog.suits.push(entry.path.clone());
            }
            continue;
        }
        if is_owned_weapon(entry)
            && let Some(canonical) =
                normalized_weapon(memory, base, layout, entry.descriptor, variants)?
        {
            owned_weapon_items.push(entry.path.clone());
            if canonical != entry.path {
                weapon_aliases.push(Alias {
                    source: entry.path.clone(),
                    canonical,
                });
            }
        }
        if entry.description == 0
            || !entry.eligible
            || entry.excluded
            || !is_catalog_weapon(&entry.path)
        {
            continue;
        }
        match entry.category {
            0 => catalog.secondaries.push(entry.path.clone()),
            1 => catalog.primaries.push(entry.path.clone()),
            5 => catalog.melees.push(entry.path.clone()),
            _ => {}
        }
    }
    if [
        &catalog.suits,
        &catalog.primaries,
        &catalog.secondaries,
        &catalog.melees,
    ]
    .iter()
    .any(|items| items.len() < 3 || items.iter().any(|name| !name.starts_with("/Lotus/")))
    {
        return Err("invalid Archimedea equipment pools".to_owned());
    }
    Ok(ArchimedeaMetadata {
        catalog,
        owned_suit_items,
        owned_weapon_items,
        suit_aliases: suit_aliases(memory, base, layout, store)?,
        weapon_aliases,
    })
}

fn is_owned_weapon(entry: &StoreEntry) -> bool {
    let modular = entry.path.contains("Modular");
    entry.description != 0
        && (!entry.excluded || modular)
        && (is_owned_base_weapon(&entry.path) || modular)
}

fn is_owned_base_weapon(path: &str) -> bool {
    ![
        "MK1",
        "StartingRifle",
        "Wraith",
        "Vandal",
        "Prisma",
        "Syndicate",
        "Modular",
        "Gear",
    ]
    .iter()
    .any(|token| path.contains(token))
}

fn is_catalog_weapon(path: &str) -> bool {
    is_owned_base_weapon(path) && !path.contains("Prime") && !path.contains("Bayonet/TnBayonet")
}

fn read_store_entries(
    memory: &ProcessMemory,
    base: u64,
    layout: MetadataLayout,
) -> Result<Vec<StoreEntry>, String> {
    let game_time = read_f64(memory, base + layout.game_time_rva)?;
    if !game_time.is_finite() {
        return Err("invalid Warframe game time".to_owned());
    }
    let holder = global_object(
        memory,
        base + layout.global_registry_rva + layout.global_registry_offset,
        layout.game_rules_hash,
    )?;
    let game_rules = non_null(read_u64(memory, holder)?, "gGameRules")?;
    let (entries, count) = store_manifest(
        memory,
        game_rules,
        base + layout.store_manifest_descriptor_rva,
        layout.resources,
        layout.manifests,
    )?;
    let mut result = Vec::new();
    for index in 0..count {
        let holder = read_u64(
            memory,
            entries + index as u64 * layout.manifests.store_stride,
        )?;
        if holder == 0 {
            continue;
        }
        let item = read_u64(memory, holder)?;
        if item == 0 {
            continue;
        }
        let fields = layout.store;
        let category = read_u8(memory, item.saturating_add(fields.category))?;
        if !matches!(category, 0 | 1 | 3 | 5) {
            continue;
        }
        let resource = non_null(
            read_u64(memory, item.saturating_add(fields.resource))?,
            "StoreItem resource",
        )?;
        result.push(StoreEntry {
            category,
            path: resource_name(memory, base, layout.resources, resource)?,
            descriptor: resource,
            description: read_u32(memory, item.saturating_add(fields.description))?,
            excluded: read_u8(memory, item.saturating_add(fields.exclusion))? & 1 != 0,
            eligible: store_item_eligible(
                read_i64(memory, item.saturating_add(fields.start))?,
                read_i64(memory, item.saturating_add(fields.expiry))?,
                u32::from(read_u8(memory, item.saturating_add(fields.flags))?),
                game_time,
            ),
        });
    }
    Ok(result)
}

fn store_manifest(
    memory: &ProcessMemory,
    game_rules: u64,
    descriptor: u64,
    resources: ResourceLayout,
    manifests: ManifestFields,
) -> Result<(u64, usize), String> {
    let mut fields = [0; GAME_RULES_PREFIX];
    memory
        .read_exact_at(&mut fields, game_rules)
        .map_err(|error| format!("could not read gGameRules fields: {error}"))?;
    let mut candidates = BTreeMap::new();
    for field in fields.chunks_exact(8) {
        let mut candidate = u64::from_le_bytes(field.try_into().unwrap());
        for _depth in 0..=2 {
            if candidate == 0
                || !candidate.is_multiple_of(8)
                || !readable_range(memory, candidate, 0x48)
            {
                break;
            }
            if read_u64(
                memory,
                candidate.saturating_add(resources.object_type_offset),
            )
            .ok()
            .is_some_and(|actual| descriptor_inherits(memory, actual, descriptor, resources))
                && let Some(shape) = manifest_shape(
                    memory,
                    candidate,
                    manifests.store_vector,
                    manifests.store_stride,
                    MAX_STORE_ENTRIES,
                )
            {
                candidates.insert(candidate, shape);
            }
            candidate = match read_u64(memory, candidate) {
                Ok(next) if next != candidate => next,
                _ => break,
            };
        }
    }
    let candidates = candidates.into_iter().collect::<Vec<_>>();
    if candidates.len() != 1 {
        return Err(format!(
            "expected one StoreManifest candidate, found {}",
            candidates.len()
        ));
    }
    Ok(candidates[0].1)
}

fn descriptor_inherits(
    memory: &ProcessMemory,
    mut actual: u64,
    expected: u64,
    resources: ResourceLayout,
) -> bool {
    for _ in 0..32 {
        if actual == expected {
            return true;
        }
        if actual == 0 || !readable_range(memory, actual, resources.parent_offset + 8) {
            return false;
        }
        let Ok(parent) = read_u64(memory, actual.saturating_add(resources.parent_offset)) else {
            return false;
        };
        if parent == actual {
            return false;
        }
        actual = parent;
    }
    false
}

fn suit_aliases(
    memory: &ProcessMemory,
    base: u64,
    layout: MetadataLayout,
    entries: &[StoreEntry],
) -> Result<Vec<Alias>, String> {
    let mut groups = Vec::new();
    for entry in entries.iter().filter(|entry| entry.category == 3) {
        let chain = descriptor_chain(memory, base, layout.resources, entry.descriptor)?;
        if let Some(pair) = chain
            .windows(2)
            .find(|pair| pair[1].name == PLAYER_POWER_SUIT)
        {
            groups.push((entry.path.clone(), pair[0].address));
        }
    }
    Ok(suit_aliases_from_groups(&groups))
}

fn suit_aliases_from_groups(groups: &[(String, u64)]) -> Vec<Alias> {
    let mut canonical = HashMap::new();
    let mut result = Vec::new();
    for (path, group) in groups {
        let target = canonical.entry(*group).or_insert_with(|| path.clone());
        if target != path {
            result.push(Alias {
                source: path.clone(),
                canonical: target.clone(),
            });
        }
    }
    result
}

fn read_variant_manifest(
    memory: &ProcessMemory,
    base: u64,
    layout: MetadataLayout,
) -> Result<HashMap<u64, u64>, String> {
    let descriptor = base + layout.variant_manifest_descriptor_rva;
    let mut candidates = Vec::new();
    for manifest in instances_with_descriptor(memory, descriptor, layout.resources)? {
        if let Some((vector, count)) = manifest_shape(
            memory,
            manifest,
            layout.manifests.variant_vector,
            layout.manifests.variant_stride,
            MAX_VARIANT_ENTRIES,
        ) {
            let score = variant_score(memory, base, layout, vector, count).unwrap_or(0);
            if score >= 24 {
                candidates.push((score, manifest, vector, count));
            }
        }
    }
    candidates.sort_unstable_by(|left, right| right.cmp(left));
    let Some(&(score, _resource, vector, count)) = candidates.first() else {
        return Err("VariantManifest is not loaded".to_owned());
    };
    if candidates
        .get(1)
        .is_some_and(|candidate| candidate.0 == score)
    {
        return Err("ambiguous VariantManifest candidates".to_owned());
    }
    let mut variants = HashMap::new();
    for index in 0..count {
        let entry = vector + index as u64 * layout.manifests.variant_stride;
        if let Some(target) = variant_target(memory, entry, layout.manifests.variant_modes)? {
            variants.insert(read_u64(memory, entry)?, target);
        }
    }
    Ok(variants)
}

fn normalized_weapon(
    memory: &ProcessMemory,
    base: u64,
    layout: MetadataLayout,
    descriptor: u64,
    variants: &HashMap<u64, u64>,
) -> Result<Option<String>, String> {
    let chain = descriptor_chain(memory, base, layout.resources, descriptor)?;
    let Some(target) =
        variant_target_from_chain(&chain, base + layout.weapon_descriptor_rva, variants)
    else {
        return Ok(None);
    };
    resource_name(memory, base, layout.resources, target).map(Some)
}

fn descriptor_chain(
    memory: &ProcessMemory,
    base: u64,
    resources: ResourceLayout,
    descriptor: u64,
) -> Result<Vec<DescriptorNode>, String> {
    let mut chain = Vec::new();
    let mut current = descriptor;
    let mut seen = HashSet::new();
    while current != 0 {
        if chain.len() >= 32 || !seen.insert(current) {
            return Err("invalid Warframe resource inheritance chain".into());
        }
        let parent = read_u64(memory, current.saturating_add(resources.parent_offset))?;
        chain.push(DescriptorNode {
            address: current,
            name: resource_name(memory, base, resources, current)?,
            parent,
        });
        current = parent;
    }
    Ok(chain)
}

fn variant_target_from_chain(
    chain: &[DescriptorNode],
    weapon_descriptor: u64,
    variants: &HashMap<u64, u64>,
) -> Option<u64> {
    let weapon_index = chain
        .iter()
        .position(|node| node.address == weapon_descriptor);
    for (index, node) in chain.iter().enumerate() {
        if let Some(target) = variants.get(&node.address) {
            return Some(*target);
        }
        if node.name.contains("Base")
            || node.parent == 0
            || node.parent == node.address
            || weapon_index.is_none_or(|weapon_index| index > weapon_index)
        {
            return None;
        }
    }
    None
}

fn store_item_eligible(start: i64, end: i64, flags: u32, game_time: f64) -> bool {
    (start == 0 || start as f64 <= game_time)
        && (end == 0 || end as f64 >= game_time)
        && flags & 0x20 != 0
}

fn variant_score(
    memory: &ProcessMemory,
    base: u64,
    layout: MetadataLayout,
    vector: u64,
    count: usize,
) -> Result<usize, String> {
    let mut previous = 0;
    let mut decoded = 0;
    let mut mapped = 0;
    for index in 0..count.min(32) {
        let entry = vector + index as u64 * layout.manifests.variant_stride;
        let key = read_u64(memory, entry)?;
        if key == 0 || key < previous {
            return Ok(0);
        }
        previous = key;
        if resource_name(memory, base, layout.resources, key)?.starts_with("/Lotus/") {
            decoded += 1;
        }
        if let Some(target) = variant_target(memory, entry, layout.manifests.variant_modes)? {
            if resource_name(memory, base, layout.resources, target)?.starts_with("/Lotus/") {
                mapped += 1;
            }
        }
    }
    Ok(decoded + mapped * 2)
}

fn instances_with_descriptor(
    memory: &ProcessMemory,
    descriptor: u64,
    resources: ResourceLayout,
) -> Result<Vec<u64>, String> {
    let needle = descriptor.to_le_bytes();
    let mut instances = Vec::new();
    for region in memory
        .regions()
        .iter()
        .filter(|region| private_writable(region))
    {
        let mut cursor = region.start;
        let mut tail = Vec::new();
        while cursor < region.end {
            let size = (region.end - cursor).min(SCAN_CHUNK_SIZE as u64) as usize;
            let mut chunk = vec![0; size];
            if memory.read_exact_at(&mut chunk, cursor).is_err() {
                break;
            }
            let mut data = Vec::with_capacity(tail.len() + chunk.len());
            data.extend_from_slice(&tail);
            data.extend_from_slice(&chunk);
            let start = cursor.saturating_sub(tail.len() as u64);
            for offset in memmem::find_iter(&data, &needle) {
                let address = start + offset as u64;
                if let Some(instance) = address.checked_sub(resources.object_type_offset)
                    && read_u64(memory, address).ok() == Some(descriptor)
                {
                    instances.push(instance);
                }
            }
            tail.clear();
            tail.extend_from_slice(&chunk[chunk.len().saturating_sub(7)..]);
            cursor += size as u64;
        }
    }
    instances.sort_unstable();
    instances.dedup();
    Ok(instances)
}

pub(super) fn global_object(memory: &ProcessMemory, vector: u64, key: u32) -> Result<u64, String> {
    let mut header = [0; 16];
    memory
        .read_exact_at(&mut header, vector)
        .map_err(|error| error.to_string())?;
    let entries = u64::from_le_bytes(header[..8].try_into().unwrap());
    let byte_length = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    let capacity = u32::from_le_bytes(header[12..].try_into().unwrap()) as usize;
    if byte_length > capacity
        || capacity > 0x10000
        || !byte_length.is_multiple_of(16)
        || (byte_length != 0 && entries == 0)
        || entries.checked_add(byte_length as u64).is_none()
    {
        return Err("invalid Warframe global registry".to_owned());
    }
    let mut bytes = vec![0; byte_length];
    memory
        .read_exact_at(&mut bytes, entries)
        .map_err(|error| error.to_string())?;
    let mut found = None;
    for entry in bytes.chunks_exact(16) {
        if u32::from_le_bytes(entry[..4].try_into().unwrap()) == key {
            let holder = non_null(
                u64::from_le_bytes(entry[8..].try_into().unwrap()),
                "Warframe global",
            )?;
            if found.replace(holder).is_some() {
                return Err("ambiguous Warframe global".into());
            }
        }
    }
    let mut current = vec![0; byte_length];
    let mut current_header = [0; 16];
    memory
        .read_exact_at(&mut current, entries)
        .map_err(|error| error.to_string())?;
    memory
        .read_exact_at(&mut current_header, vector)
        .map_err(|error| error.to_string())?;
    if header != current_header || bytes != current {
        return Err("Warframe global registry changed during read".into());
    }
    found.ok_or_else(|| format!("Warframe global 0x{key:08x} not found"))
}

fn manifest_shape(
    memory: &ProcessMemory,
    manifest: u64,
    offset: u64,
    entry_size: u64,
    maximum: usize,
) -> Option<(u64, usize)> {
    let (entries, byte_length) =
        vector_header(memory, manifest.checked_add(offset)?, entry_size, maximum).ok()?;
    if entries == 0 || byte_length == 0 {
        return None;
    }
    let count = (byte_length / entry_size) as usize;
    (count <= maximum && readable_range(memory, entries, byte_length)).then_some((entries, count))
}

fn vector_header(
    memory: &ProcessMemory,
    address: u64,
    stride: u64,
    maximum: usize,
) -> Result<(u64, u64), String> {
    let mut bytes = [0; 16];
    memory
        .read_exact_at(&mut bytes, address)
        .map_err(|error| error.to_string())?;
    let data = u64::from_le_bytes(bytes[..8].try_into().unwrap());
    let length = u64::from(u32::from_le_bytes(bytes[8..12].try_into().unwrap()));
    let capacity = u64::from(u32::from_le_bytes(bytes[12..].try_into().unwrap()));
    if !length.is_multiple_of(stride)
        || length > capacity
        || capacity > maximum as u64 * stride
        || (length != 0 && data == 0)
        || data.checked_add(length).is_none()
    {
        return Err("invalid metadata vector".into());
    }
    Ok((data, length))
}

fn variant_target(memory: &ProcessMemory, entry: u64, modes: u64) -> Result<Option<u64>, String> {
    let (data, length) = vector_header(memory, entry.saturating_add(modes), 8, 512)?;
    if length == 0 {
        return Ok(None);
    }
    let target = read_u64(memory, data)?;
    Ok((target != 0).then_some(target))
}

pub(super) fn resource_name(
    memory: &ProcessMemory,
    base: u64,
    resources: ResourceLayout,
    resource: u64,
) -> Result<String, String> {
    let blocks = read_u64(memory, base + resources.string_blocks_rva)?;
    let first_pointer = read_u64(
        memory,
        resource.saturating_add(resources.name_prefix_offset),
    )?;
    let first = if first_pointer == 0 {
        0
    } else {
        read_u32(memory, first_pointer)?
    };
    let second = read_u32(memory, resource.saturating_add(resources.name_leaf_offset))?;
    Ok(token_part(memory, blocks, first)? + &token_part(memory, blocks, second)?)
}

fn token_part(memory: &ProcessMemory, blocks: u64, token: u32) -> Result<String, String> {
    let slot = blocks
        .checked_add(u64::from(token & 0xffff) * 16)
        .ok_or("Warframe string block address overflow")?;
    let block = read_u64(memory, slot)?;
    let address = block
        .checked_add(u64::from(token >> 16))
        .ok_or("Warframe string address overflow")?;
    read_c_string(memory, address, 1024)
}

fn read_c_string(memory: &ProcessMemory, address: u64, limit: usize) -> Result<String, String> {
    let mut bytes = Vec::new();
    while bytes.len() < limit {
        let size = (limit - bytes.len()).min(64);
        let mut chunk = vec![0; size];
        let offset = address
            .checked_add(bytes.len() as u64)
            .ok_or("Warframe string address overflow")?;
        let count = memory
            .read_at(&mut chunk, offset)
            .map_err(|error| format!("could not read Warframe string: {error}"))?;
        if count == 0 {
            break;
        }
        chunk.truncate(count);
        if let Some(end) = chunk.iter().position(|byte| *byte == 0) {
            bytes.extend_from_slice(&chunk[..end]);
            return String::from_utf8(bytes)
                .map_err(|error| format!("invalid Warframe resource name: {error}"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Err("unterminated Warframe resource name".to_owned())
}

fn containing(regions: &[Region], address: u64, size: u64) -> Option<&Region> {
    let end = address.checked_add(size)?;
    regions
        .iter()
        .find(|region| region.start <= address && end <= region.end)
}

fn readable_range(memory: &ProcessMemory, address: u64, size: u64) -> bool {
    containing(memory.regions(), address, size)
        .is_some_and(|region| region.permissions.starts_with('r'))
}

fn private_writable(region: &Region) -> bool {
    region.permissions.starts_with("rw")
        && region.permissions.ends_with('p')
        && (region.path.is_empty() || region.path.starts_with('['))
}

fn non_null(value: u64, name: &str) -> Result<u64, String> {
    (value != 0)
        .then_some(value)
        .ok_or_else(|| format!("null {name} pointer"))
}

fn read_u8(memory: &ProcessMemory, address: u64) -> Result<u8, String> {
    let mut bytes = [0; 1];
    memory
        .read_exact_at(&mut bytes, address)
        .map_err(|error| format!("could not read Warframe memory at 0x{address:x}: {error}"))?;
    Ok(bytes[0])
}

fn read_u32(memory: &ProcessMemory, address: u64) -> Result<u32, String> {
    let mut bytes = [0; 4];
    memory
        .read_exact_at(&mut bytes, address)
        .map_err(|error| format!("could not read Warframe memory at 0x{address:x}: {error}"))?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_i64(memory: &ProcessMemory, address: u64) -> Result<i64, String> {
    let mut bytes = [0; 8];
    memory
        .read_exact_at(&mut bytes, address)
        .map_err(|error| format!("could not read Warframe memory at 0x{address:x}: {error}"))?;
    Ok(i64::from_le_bytes(bytes))
}

fn read_f64(memory: &ProcessMemory, address: u64) -> Result<f64, String> {
    let mut bytes = [0; 8];
    memory
        .read_exact_at(&mut bytes, address)
        .map_err(|error| format!("could not read Warframe memory at 0x{address:x}: {error}"))?;
    Ok(f64::from_le_bytes(bytes))
}

fn read_u64(memory: &ProcessMemory, address: u64) -> Result<u64, String> {
    let mut bytes = [0; 8];
    memory
        .read_exact_at(&mut bytes, address)
        .map_err(|error| format!("could not read Warframe memory at 0x{address:x}: {error}"))?;
    Ok(u64::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_observer::executable::fixture::{put32, resources};

    fn put64(bytes: &mut [u8], offset: usize, value: u64) {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn manifest_fixture() -> Vec<u8> {
        let mut bytes = vec![0; 0x5000];
        put64(&mut bytes, 0x3000, 0x3100);
        put64(&mut bytes, 0x3100, 0x3200);
        put64(&mut bytes, 0x3208, 0x4000);
        put64(&mut bytes, 0x3238, 0x4100);
        put64(&mut bytes, 0x3240, 32);
        put32(&mut bytes, 0x3244, 32);
        bytes
    }

    fn memory(bytes: &[u8]) -> ProcessMemory {
        crate::game_observer::memory::from_test_bytes(
            bytes,
            vec![Region {
                start: 0,
                end: bytes.len() as u64,
                permissions: "rw-p".into(),
                path: String::new(),
            }],
        )
    }

    fn find_manifest(bytes: &[u8], descriptor: u64) -> Result<(u64, usize), String> {
        store_manifest(
            &memory(bytes),
            0x100,
            descriptor,
            resources(0, 0),
            manifests(),
        )
    }

    fn manifests() -> ManifestFields {
        ManifestFields {
            store_vector: 0x38,
            store_stride: 16,
            variant_vector: 0x38,
            variant_stride: 0x68,
            variant_modes: 8,
        }
    }

    fn name_fixture() -> Vec<u8> {
        let mut bytes = vec![0; 0x3000];
        put64(&mut bytes, 0x100, 0x200);
        put64(&mut bytes, 0x200, 0x300);
        let names = b"\0/Lotus/\0Fixture\0";
        bytes[0x300..0x300 + names.len()].copy_from_slice(names);
        bytes
    }

    #[test]
    fn resource_names_use_discovered_fields() {
        for shift in [0, 0x40] {
            let mut bytes = name_fixture();
            let layout = resources(0x100, shift);
            let prefix = 0x600 + layout.name_prefix_offset as usize;
            let leaf = 0x600 + layout.name_leaf_offset as usize;
            put64(&mut bytes, prefix, 0x400);
            put32(&mut bytes, 0x400, 1 << 16);
            put32(&mut bytes, leaf, 9 << 16);
            assert_eq!(
                resource_name(&memory(&bytes), 0, layout, 0x600).unwrap(),
                "/Lotus/Fixture"
            );
            put64(&mut bytes, prefix, 0);
            assert_eq!(
                resource_name(&memory(&bytes), 0, layout, 0x600).unwrap(),
                "Fixture"
            );
        }
    }

    #[test]
    fn resource_strings_accept_short_reads_but_require_termination_and_utf8() {
        assert_eq!(
            read_c_string(&memory(b"Fixture\0"), 0, 1024).unwrap(),
            "Fixture"
        );
        assert!(read_c_string(&memory(b"Fixture"), 0, 1024).is_err());
        assert!(read_c_string(&memory(b"Fixture\0"), 0, 7).is_err());
        assert!(read_c_string(&memory(b"\xff\0"), 0, 1024).is_err());
    }

    #[test]
    fn resource_tokens_reject_address_overflow() {
        let memory = memory(&(u64::MAX - 3).to_le_bytes());
        assert!(
            token_part(&memory, u64::MAX - 3, 1)
                .unwrap_err()
                .contains("overflow")
        );
        assert!(
            token_part(&memory, 0, 8 << 16)
                .unwrap_err()
                .contains("overflow")
        );
    }

    #[test]
    fn inheritance_rejects_cycles_and_excessive_depth() {
        let mut bytes = name_fixture();
        let layout = resources(0x100, 0x40);
        for index in 0..32 {
            let address = 0x600 + index * 0x100;
            let parent = if index == 31 { 0 } else { address + 0x100 };
            put64(
                &mut bytes,
                address + layout.parent_offset as usize,
                parent as u64,
            );
        }
        assert_eq!(
            descriptor_chain(&memory(&bytes), 0, layout, 0x600)
                .unwrap()
                .len(),
            32
        );
        for parent in [0x600, 0x700, 0x2600] {
            put64(&mut bytes, 0x2500 + layout.parent_offset as usize, parent);
            assert_eq!(
                descriptor_chain(&memory(&bytes), 0, layout, 0x600).unwrap_err(),
                "invalid Warframe resource inheritance chain"
            );
        }
    }

    #[test]
    fn store_manifest_follows_type_not_game_rules_field_offset() {
        for offset in [0x4e0, 0xbd0, 0xbe0, 0xf98, 0x1800] {
            for pointer in [0x3000, 0x3100, 0x3200] {
                let mut bytes = manifest_fixture();
                put64(&mut bytes, 0x100 + offset, pointer);
                assert_eq!(find_manifest(&bytes, 0x4000), Ok((0x4100, 2)));
                put64(&mut bytes, 0x108, 0x3200);
                assert_eq!(find_manifest(&bytes, 0x4000), Ok((0x4100, 2)));
            }
        }
    }

    #[test]
    fn store_manifest_rejects_wrong_types_bad_vectors_and_ambiguous_instances() {
        let mut bytes = manifest_fixture();
        put64(&mut bytes, 0x100, 0x3200);
        assert!(find_manifest(&bytes, 0x4008).is_err());
        put64(&mut bytes, 0x3240, 31);
        assert!(find_manifest(&bytes, 0x4000).is_err());
        put64(&mut bytes, 0x3240, 0x10000);
        assert!(find_manifest(&bytes, 0x4000).is_err());
        put64(&mut bytes, 0x3240, 32);
        bytes.copy_within(0x3200..0x3248, 0x3300);
        put64(&mut bytes, 0x108, 0x3300);
        assert!(find_manifest(&bytes, 0x4000).is_err());
    }

    #[test]
    fn store_manifest_accepts_derived_resources() {
        let mut bytes = manifest_fixture();
        put64(&mut bytes, 0x100, 0x3100);
        put64(&mut bytes, 0x3208, 0x3800);
        put64(&mut bytes, 0x3818, 0x3900);
        put64(&mut bytes, 0x3918, 0x4000);
        assert_eq!(find_manifest(&bytes, 0x4000), Ok((0x4100, 2)));
        put64(&mut bytes, 0x3918, 0x3800);
        assert!(find_manifest(&bytes, 0x4000).is_err());
        put64(&mut bytes, 0x3918, u64::MAX);
        assert!(find_manifest(&bytes, 0x4000).is_err());
    }

    #[test]
    fn store_manifest_pointer_walk_is_bounded() {
        let mut bytes = manifest_fixture();
        put64(&mut bytes, 0x100, 0x2800);
        put64(&mut bytes, 0x2800, 0x3000);
        assert!(find_manifest(&bytes, 0x4000).is_err());
        put64(&mut bytes, 0x3000, 0x2800);
        assert!(find_manifest(&bytes, 0x4000).is_err());
    }

    #[test]
    fn store_manifest_uses_derived_type_and_parent_fields() {
        let mut bytes = manifest_fixture();
        let layout = resources(0, 0x40);
        put64(&mut bytes, 0x100, 0x3100);
        put64(&mut bytes, 0x3208, 0);
        put64(
            &mut bytes,
            0x3200 + layout.object_type_offset as usize,
            0x3800,
        );
        put64(&mut bytes, 0x3800 + layout.parent_offset as usize, 0x4000);
        assert_eq!(
            store_manifest(&memory(&bytes), 0x100, 0x4000, layout, manifests()),
            Ok((0x4100, 2))
        );
        assert!(find_manifest(&bytes, 0x4000).is_err());
    }

    #[test]
    fn manifest_uses_discovered_vector_position_and_stride() {
        let mut bytes = manifest_fixture();
        let mut layout = manifests();
        layout.store_vector = 0x68;
        layout.store_stride = 32;
        bytes.copy_within(0x3238..0x3248, 0x3268);
        bytes[0x3238..0x3248].fill(0);
        put64(&mut bytes, 0x100, 0x3200);
        assert_eq!(
            store_manifest(&memory(&bytes), 0x100, 0x4000, resources(0, 0), layout),
            Ok((0x4100, 1))
        );
        assert!(find_manifest(&bytes, 0x4000).is_err());
        put32(&mut bytes, 0x3274, 16);
        assert!(store_manifest(&memory(&bytes), 0x100, 0x4000, resources(0, 0), layout).is_err());
    }

    #[test]
    fn variant_modes_require_a_valid_vector_and_follow_its_first_entry() {
        for offset in [8, 24] {
            let mut bytes = vec![0; 0x1000];
            let header = 0x100 + offset;
            assert_eq!(
                variant_target(&memory(&bytes), 0x100, offset as u64),
                Ok(None)
            );
            put64(&mut bytes, header, 0x300);
            put32(&mut bytes, header + 8, 16);
            put32(&mut bytes, header + 12, 16);
            put64(&mut bytes, 0x300, 0x500);
            put64(&mut bytes, 0x308, 0x600);
            assert_eq!(
                variant_target(&memory(&bytes), 0x100, offset as u64),
                Ok(Some(0x500))
            );
            for length in [1, 24, u32::MAX] {
                put32(&mut bytes, header + 8, length);
                assert!(variant_target(&memory(&bytes), 0x100, offset as u64).is_err());
            }
            put32(&mut bytes, header + 8, 16);
            put64(&mut bytes, header, u64::MAX);
            assert!(variant_target(&memory(&bytes), 0x100, offset as u64).is_err());
        }
    }

    #[test]
    fn global_registry_rejects_ambiguity_and_invalid_bounds() {
        let mut bytes = vec![0; 0x1000];
        put64(&mut bytes, 0x200, 0x300);
        put32(&mut bytes, 0x208, 16);
        put32(&mut bytes, 0x20c, 32);
        put32(&mut bytes, 0x300, 42);
        put64(&mut bytes, 0x308, 0x500);
        assert_eq!(global_object(&memory(&bytes), 0x200, 42), Ok(0x500));
        assert!(global_object(&memory(&bytes), 0x200, 43).is_err());
        bytes.copy_within(0x300..0x310, 0x310);
        put32(&mut bytes, 0x208, 32);
        assert_eq!(
            global_object(&memory(&bytes), 0x200, 42).unwrap_err(),
            "ambiguous Warframe global"
        );
        for length in [17, 48, u32::MAX] {
            put32(&mut bytes, 0x208, length);
            assert!(global_object(&memory(&bytes), 0x200, 42).is_err());
        }
        put32(&mut bytes, 0x208, 16);
        put64(&mut bytes, 0x200, u64::MAX);
        assert!(global_object(&memory(&bytes), 0x200, 42).is_err());
    }

    #[test]
    fn catalog_and_owned_filters_match_archimedea_scripts() {
        assert!(is_catalog_weapon("/Lotus/Weapons/Tenno/Rifle/HeavyRifle"));
        assert!(!is_catalog_weapon("/Lotus/Weapons/Tenno/Rifle/BratonPrime"));
        assert!(!is_catalog_weapon(
            "/Lotus/Weapons/Syndicates/SteelMeridian/SMHek"
        ));
        assert!(!is_catalog_weapon("/Lotus/Weapons/Tenno/Bayonet/TnBayonet"));
        assert!(is_owned_base_weapon(
            "/Lotus/Weapons/Tenno/Rifle/BratonPrime"
        ));
        assert!(is_owned_base_weapon(
            "/Lotus/Weapons/Tenno/Bayonet/TnBayonet"
        ));
    }

    #[test]
    fn owned_weapon_filter_uses_local_rules_and_keeps_modular_weapons() {
        let entry = |path: &str, excluded, eligible| StoreEntry {
            category: 1,
            path: path.to_owned(),
            descriptor: 0,
            description: 1,
            excluded,
            eligible,
        };
        assert!(is_owned_weapon(&entry(
            "/Lotus/Weapons/Tenno/Rifle/HeavyRifle",
            false,
            true
        )));
        assert!(is_owned_weapon(&entry(
            "/Lotus/Weapons/Tenno/Rifle/BratonPrime",
            false,
            true
        )));
        assert!(!is_owned_weapon(&entry(
            "/Lotus/Weapons/Tenno/Rifle/BratonWraith",
            false,
            true
        )));
        assert!(is_owned_weapon(&entry(
            "/Lotus/Weapons/SolarisUnited/ModularPrimary",
            true,
            true
        )));
        assert!(is_owned_weapon(&entry(
            "/Lotus/Weapons/Tenno/Rifle/HeavyRifle",
            false,
            false
        )));
    }

    #[test]
    fn eligibility_uses_native_flag_and_time_window() {
        assert!(store_item_eligible(0, 0, 0x20, 100.0));
        assert!(store_item_eligible(100, 100, 0x20, 100.0));
        assert!(!store_item_eligible(101, 0, 0x20, 100.0));
        assert!(!store_item_eligible(0, 99, 0x20, 100.0));
        assert!(!store_item_eligible(0, 0, 0x20000, 100.0));
    }

    #[test]
    fn variant_lookup_walks_parents_but_stops_at_base_or_weapon_boundary() {
        let chain = vec![
            node(1, "Variant", 2),
            node(2, "Family", 3),
            node(3, "Weapon", 4),
            node(4, "Resource", 0),
        ];
        assert_eq!(
            variant_target_from_chain(&chain, 3, &HashMap::from([(2, 9)])),
            Some(9)
        );
        assert_eq!(
            variant_target_from_chain(&chain, 3, &HashMap::from([(4, 9)])),
            Some(9)
        );
        let base = vec![node(1, "VariantBase", 2), node(2, "Weapon", 0)];
        assert_eq!(
            variant_target_from_chain(&base, 2, &HashMap::from([(2, 9)])),
            None
        );
    }

    #[test]
    fn suit_aliases_use_first_store_item_in_each_base_suit_group() {
        let groups = vec![
            ("/Lotus/Powersuits/Cowgirl/Cowgirl".to_owned(), 0x200),
            ("/Lotus/Powersuits/Cowgirl/MesaPrime".to_owned(), 0x200),
            ("/Lotus/Powersuits/Mag/Mag".to_owned(), 0x300),
        ];
        assert_eq!(
            suit_aliases_from_groups(&groups),
            vec![Alias {
                source: "/Lotus/Powersuits/Cowgirl/MesaPrime".to_owned(),
                canonical: "/Lotus/Powersuits/Cowgirl/Cowgirl".to_owned(),
            }]
        );
    }

    fn node(address: u64, name: &str, parent: u64) -> DescriptorNode {
        DescriptorNode {
            address,
            name: name.to_owned(),
            parent,
        }
    }
}
