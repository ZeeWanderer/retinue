use pelite::pe64::PeFile;
use serde::Serialize;

use super::super::executable::{self, function_range, signature, signature_in};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct InventoryLayout {
    pub(crate) global_registry_rva: u64,
    pub(crate) global_registry_offset: u64,
    pub(crate) resources: executable::ResourceLayout,
    pub(crate) profile_descriptor_rva: u64,
    pub(crate) sync_offset: u64,
    pub(crate) misc_offset: u64,
    pub(crate) recipes_offset: u64,
    pub(crate) pending_offset: u64,
    pub(crate) count_mask: u32,
    pub(crate) check_mask: u32,
    pub(crate) count_rotation: u32,
    pub(crate) address_shift: u32,
}

const PENDING: &str = r#"
    49 8d 8c 24 u4 4c 8b c6
    48 8d 15 ${ "PendingRecipes" 00 } e8 ${'}
"#;
const INVENTORY: &str = "
    49 8d 94 24 u4 48 8b ce
    e8 ${ 48 8b c2 45 33 c9 48 8b d1 45 33 c0 48 8b c8 e9 ${'} }
";
const SYNC: &str = r#"
    49 8d 94 24 u4 48 8b ce e8 ???? 48 8b 06 48 8b ce ff 50 60
    [0-64] 48 8d 15 ${ "Processing *FULL* Inventory JSON" 0a 00 }
"#;
const COUNT: &str = "
    4c 8d 53 0c [0-16]
    41 8b 0a 49 8b d2 48 c1 fa u1 8b c2 c1 c1 u1 35 u4 3b c1
";
const RELOCATE_COUNT: &str = "
    49 8b c8 41 8b 10 c1 c2 u1 48 c1 f9 u1 33 d1
    48 8b c8 48 c1 f9 u1 33 d1 49 8d 48 04 c1 ca u1
    89 10 81 f2 u4 89 50 fc
";

pub(crate) fn discover(bytes: &[u8]) -> Result<InventoryLayout, String> {
    let image = PeFile::from_bytes(bytes).map_err(|error| format!("invalid PE: {error}"))?;
    let (global_registry_rva, global_registry_offset, _) = executable::registry(image)?;
    let (profile_descriptor_rva, descriptor_constructor) =
        executable::descriptor(image, "LotusProfileData", "/Lotus/Types/Game/")?;
    let resources = executable::ResourceLayout::discover(image, descriptor_constructor)?;
    executable::writable(image, profile_descriptor_rva, resources.descriptor_size())?;
    let misc = stack(image, "MiscItems")?;
    let recipes = stack(image, "Recipes")?;
    let inventory_function = function_range(image, misc[0])?;
    if misc[2] != recipes[2] || !inventory_function.contains(&recipes[0]) {
        return Err("inventory stack serializers disagree".into());
    }
    let pending = signature::<3>(image, "PendingRecipes", PENDING)?;
    let profile_function = function_range(image, pending[0])?;
    let inventory = signature_in::<3>(
        image,
        "embedded inventory",
        INVENTORY,
        profile_function.clone(),
    )?;
    if inventory[2] != inventory_function.start {
        return Err("profile does not call the named inventory serializer".into());
    }
    let sync = signature_in::<2>(image, "full inventory sync", SYNC, profile_function)?;

    // The helper has separate unwind fragments. Bound matching from its verified
    // entry, and require both the zero test and address-dependent relocation.
    let helper = misc[2];
    if function_range(image, helper)?.start != helper {
        return Err("inventory stack helper is not a function entry".into());
    }
    let range = helper
        ..helper
            .checked_add(0x300)
            .ok_or("invalid stack helper range")?;
    let count = signature_in::<4>(image, "stack quantity", COUNT, range.clone())?;
    let relocated = signature_in::<6>(image, "stack quantity relocation", RELOCATE_COUNT, range)?;
    if !(1..64).contains(&count[1])
        || !(1..32).contains(&count[2])
        || relocated[1] != count[2]
        || relocated[2] != count[1]
        || relocated[3] != count[1]
        || relocated[4] != count[2]
    {
        return Err("inconsistent native quantity encoding".into());
    }
    let layout = InventoryLayout {
        global_registry_rva,
        global_registry_offset,
        resources,
        profile_descriptor_rva,
        sync_offset: sync[1].into(),
        misc_offset: u64::from(inventory[1]) + u64::from(misc[1]),
        recipes_offset: u64::from(inventory[1]) + u64::from(recipes[1]),
        pending_offset: pending[1].into(),
        count_mask: count[3],
        check_mask: relocated[5],
        count_rotation: count[2],
        address_shift: count[1],
    };
    let fields = [
        (layout.sync_offset, 12),
        (layout.misc_offset, 16),
        (layout.recipes_offset, 16),
        (layout.pending_offset, 16),
    ];
    for (i, &(offset, size)) in fields.iter().enumerate() {
        if !(0x18..0x10_0000).contains(&offset)
            || !offset.is_multiple_of(4)
            || fields[..i]
                .iter()
                .any(|&(other, width)| offset < other + width && other < offset + size)
        {
            return Err("invalid or overlapping native inventory fields".into());
        }
    }
    Ok(layout)
}

fn stack(image: PeFile<'_>, name: &str) -> Result<[u32; 3], String> {
    signature(
        image,
        name,
        &format!(
            r#"
        48 8d 96 u4 40 88 7c 24 20 45 0f b6 ce
        4c 8d 05 ${{ "{name}" 00 }} 48 8b cb e8 ${{'}}
    "#
        ),
    )
}

#[cfg(test)]
mod tests;
