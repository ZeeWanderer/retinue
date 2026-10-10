mod layout;

use pelite::pe64::{Pe, PeFile};
use serde::Serialize;
use serde_json::{Value, json};

use super::{memory::ProcessMemory, metadata};
use layout::{Accessors, Layout};

const MAX_UPGRADES: usize = 128;
const MAX_ANCESTORS: usize = 32;

#[derive(Debug, Serialize)]
struct Resource {
    address: u64,
    vtable: u64,
    descriptor: u64,
    holder: u64,
    path: String,
}

#[derive(Serialize)]
struct Upgrade {
    index: usize,
    entry_address: u64,
    #[serde(flatten)]
    resource: Resource,
}

#[derive(Default)]
struct Checks {
    bytes: Vec<(u64, Vec<u8>)>,
    names: Vec<(u64, String)>,
}

impl Checks {
    fn bytes(
        &mut self,
        memory: &ProcessMemory,
        address: u64,
        size: usize,
    ) -> Result<Vec<u8>, String> {
        let value = read(memory, address, size)?;
        self.bytes.push((address, value.clone()));
        Ok(value)
    }

    fn pointer(&mut self, memory: &ProcessMemory, address: u64) -> Result<u64, String> {
        Ok(u64::from_le_bytes(
            self.bytes(memory, address, 8)?.try_into().unwrap(),
        ))
    }

    fn resource(
        &mut self,
        memory: &ProcessMemory,
        image: PeFile<'_>,
        base: u64,
        layout: Layout,
        address: u64,
    ) -> Result<Resource, String> {
        pointer(address)?;
        let vtable = self.pointer(memory, address)?;
        pointer(vtable)?;
        layout::section(image, rva(base, vtable)?.into(), 8, false)?;
        let first_method = self.pointer(memory, vtable)?;
        layout::section(image, rva(base, first_method)?.into(), 1, true)?;
        let descriptor =
            self.pointer(memory, add(address, layout.resources.object_type_offset)?)?;
        pointer(descriptor)?;
        let holder = self.pointer(memory, add(address, layout.resources.object_holder_offset)?)?;
        pointer(holder)?;
        let path = metadata::resource_name(memory, base, layout.resources, descriptor)?;
        if !(path.starts_with("/Lotus/") || path.starts_with("/EE/"))
            || path.len() > 2048
            || path.chars().any(char::is_control)
        {
            return Err("invalid equipment resource path".into());
        }
        self.names.push((descriptor, path.clone()));
        Ok(Resource {
            address,
            vtable,
            descriptor,
            holder,
            path,
        })
    }

    fn verify(&self, memory: &ProcessMemory, base: u64, layout: Layout) -> Result<(), String> {
        for (descriptor, expected) in &self.names {
            if metadata::resource_name(memory, base, layout.resources, *descriptor)? != *expected {
                return Err("equipment resource identity changed during read; retry".into());
            }
        }
        // Reverse order checks leaves before their owning vector and root identity.
        for (address, expected) in self.bytes.iter().rev() {
            if read(memory, *address, expected.len())? != *expected {
                return Err(format!(
                    "equipment changed during read at 0x{address:x}; retry"
                ));
            }
        }
        Ok(())
    }
}

pub(crate) fn inspect(
    memory: &ProcessMemory,
    executable: &[u8],
    weapon: u64,
) -> Result<Value, String> {
    let image = PeFile::from_bytes(executable).map_err(|e| format!("invalid equipment PE: {e}"))?;
    let layout = Layout::discover(image)?;
    let base = memory.image_base().ok_or("game image base unavailable")?;
    add(base, image.optional_header().SizeOfImage.into())?;
    let (report, checks) = snapshot(memory, image, base, layout, weapon)?;
    checks.verify(memory, base, layout)?;
    Ok(report)
}

fn snapshot(
    memory: &ProcessMemory,
    image: PeFile<'_>,
    base: u64,
    layout: Layout,
    weapon: u64,
) -> Result<(Value, Checks), String> {
    let mut checks = Checks::default();
    let weapon = checks.resource(memory, image, base, layout, weapon)?;
    let mut current = weapon.descriptor;
    let expected = add(base, layout.weapon_descriptor_rva)?;
    let mut ancestors = Vec::new();
    while current != expected {
        pointer(current)?;
        if ancestors.len() >= MAX_ANCESTORS || ancestors.contains(&current) {
            return Err("invalid weapon resource ancestry".into());
        }
        ancestors.push(current);
        current = checks.pointer(memory, add(current, layout.resources.parent_offset)?)?;
        if current == 0 {
            return Err("selected resource is not a LotusWeapon".into());
        }
    }
    let mut methods = Vec::new();
    for slot in [layout.count_slot, layout.index_slot] {
        let address = add(weapon.vtable, slot)?;
        layout::section(image, rva(base, address)?.into(), 8, false)?;
        let method = checks.pointer(memory, address)?;
        methods.push(rva(base, method)?);
    }
    let accessors = Accessors::decode(image, methods[0], methods[1])?;
    for (rva, size) in [
        (accessors.count_rva, accessors.count_code_bytes),
        (accessors.index_rva, accessors.index_code_bytes),
    ] {
        let observed = checks.bytes(memory, add(base, rva.into())?, size)?;
        let compiled = image
            .slice(rva, size, 1)
            .map_err(|e| format!("equipment accessor: {e}"))?;
        if observed != compiled[..size] {
            return Err("equipment accessor code differs from identified executable".into());
        }
    }
    let header_address = add(weapon.address, accessors.vector_offset)?;
    let header = checks.bytes(memory, header_address, 16)?;
    let data = u64::from_le_bytes(header[..8].try_into().unwrap());
    let used = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    let capacity = u32::from_le_bytes(header[12..16].try_into().unwrap()) as usize;
    if !used.is_multiple_of(accessors.stride)
        || !capacity.is_multiple_of(accessors.stride)
        || used > capacity
        || capacity > MAX_UPGRADES * accessors.stride
        || (capacity != 0 && data == 0)
        || (data != 0 && pointer(data).is_err())
        || data.checked_add(capacity as u64).is_none()
    {
        return Err("invalid attached upgrades vector header".into());
    }
    let entries = if used == 0 {
        Vec::new()
    } else {
        checks.bytes(memory, data, used)?
    };
    let mut upgrades = Vec::with_capacity(used / accessors.stride);
    for (index, entry) in entries.chunks_exact(accessors.stride).enumerate() {
        let offset = accessors.entry_holder_offset;
        let holder = u64::from_le_bytes(entry[offset..offset + 8].try_into().unwrap());
        pointer(holder)?;
        let address = checks.pointer(memory, add(holder, accessors.holder_target_offset)?)?;
        let resource = checks.resource(memory, image, base, layout, address)?;
        if resource.holder != holder {
            return Err("attached upgrade holder backlink disagrees".into());
        }
        upgrades.push(Upgrade {
            index,
            entry_address: add(data, (index * accessors.stride) as u64)?,
            resource,
        });
    }
    Ok((
        json!({
            "weapon": weapon,
            "attached_upgrades": upgrades,
            "raw_rank": {"status": "unavailable", "reason": "Artifact rank ownership is not bound; reference counters are not rank"},
            "bindings": {"image_base": base, "layout": layout, "accessors": accessors},
            "vector": {"address": header_address, "data": data, "used_bytes": used, "capacity_bytes": capacity, "count": upgrades.len()},
            "limits": {"attached_upgrades": MAX_UPGRADES, "ancestors": MAX_ANCESTORS},
            "consistency": "identity, header and entries reread; not an atomic snapshot"
        }),
        checks,
    ))
}

fn pointer(value: u64) -> Result<(), String> {
    if value == 0 || value >= 0x0000_8000_0000_0000 || !value.is_multiple_of(8) {
        Err(format!("invalid equipment pointer 0x{value:x}"))
    } else {
        Ok(())
    }
}

fn add(address: u64, offset: u64) -> Result<u64, String> {
    address
        .checked_add(offset)
        .ok_or_else(|| "equipment address overflow".into())
}

fn rva(base: u64, address: u64) -> Result<u32, String> {
    address
        .checked_sub(base)
        .and_then(|v| v.try_into().ok())
        .ok_or_else(|| "equipment binding outside game image".into())
}

fn read(memory: &ProcessMemory, address: u64, size: usize) -> Result<Vec<u8>, String> {
    if !memory.supports_read_range(address, size) {
        return Err(format!(
            "equipment range unavailable at 0x{address:x} ({size} bytes)"
        ));
    }
    let mut bytes = vec![0; size];
    memory
        .read_exact_at(&mut bytes, address)
        .map_err(|e| format!("equipment read at 0x{address:x}: {e}"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests;
