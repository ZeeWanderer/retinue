use pelite::pattern;
use pelite::pe64::{Pe, PeFile};

use crate::game_observer::adapter::ScaleformLayout;
use crate::game_observer::executable::{self, function_range, signature, signature_in};

const FLASH: &str = r#"
    48 8d 15 ${ "FlashInstanceImpl" 00 }
    48 8d 4c 24 50 e8 ???? 48 8d 15 ${ "/EE/Types/UISys/" 00 }
    48 8d 4c 24 58 8b 18 e8 ???? c7 44 24 38 10 00 00 00
    48 8d 0d ${ e9 ${'} } c7 44 24 30 ???? 4c 8d 0d ????
    48 89 4c 24 28 44 8b c3 8b 10 48 8d 0d ${'} c6 44 24 20 00 e8 ????
"#;
const FLASH_VTABLE: &str = "e8 ???? 48 8d 05 ${'} 33 ff 48 89 03";
const RESOURCE_LOOKUP: &str = "
    e8 ${'} 4c 8d 05 ${'} 48 8b c8 4c 89 44 24 28
    4c 8d 0d ???? 48 8d 55 ? c6 44 24 20 01 e8 ????
";
const RESOURCE_MANAGER: &str = "
    33 d2 48 8d 0d ${ 48 8d 0d ${'} e9 ${'} } 0f 57 c0
    [0-256] 48 8d 05 ${'} 48 83 c4 28 c3
";
const RESOURCE_VECTOR: &str = "
    48 8b bb u4 48 85 ff 74 ? 8b b3 u4 48 03 f7 48 3b fe 74 ?
    48 8b 0f 83 41 08 ff 75 ? e8 ????
    48 83 c7 10 48 3b fe 75 ? 48 8b 8b u4 e8 ????
";
const ROOT: &str = "
    45 33 ff 48 8d 05 ${'} 48 89 01 48 8d 05 ${'} 48 89 41 10
    49 8b f0 44 89 79 08 48 8b da 4c 89 79 18 48 8b f9
    44 89 79 20 4c 89 79 28 4c 89 79 30 4c 89 79 38
    [0-192] 4c 89 bf a0 00 00 00
";
const CONTAINER: &str = "
    e8 ${'} 48 8d 05 ${'} 45 33 f6 48 89 03 48 8d 05 ${'}
    48 89 43 10 4c 89 b3 28 01 00 00
    [0-96] 4c 89 b3 30 01 00 00 [0-8] 4c 89 b3 38 01 00 00
";
const TEXT: &str = "
    e8 ${'} 48 8d 05 ${'} c6 87 28 01 00 00 01 48 89 07
    45 33 f6 48 8d 05 ${'} 48 89 47 10 4c 89 b7 30 01 00 00
";

pub(crate) fn discover(bytes: &[u8]) -> Result<ScaleformLayout, String> {
    let image = PeFile::from_bytes(bytes).map_err(|error| format!("invalid PE: {error}"))?;
    let (flash_type, flash_vtable) = flash_binding(image)?;
    let lookup = resource_lookup(image, flash_type)?;
    let manager = signature_in::<4>(
        image,
        "resource manager",
        RESOURCE_MANAGER,
        function_range(image, lookup)?,
    )?;
    if manager[1] != manager[3] {
        return Err("resource manager cleanup and accessor disagree".into());
    }
    let vector = signature_in::<4>(
        image,
        "resource registry vector",
        RESOURCE_VECTOR,
        function_range(image, manager[2])?,
    )?;
    if !(8..0x1000).contains(&vector[1])
        || !vector[1].is_multiple_of(8)
        || vector[2] != vector[1] + 8
        || vector[3] != vector[1]
    {
        return Err("inconsistent resource registry vector".into());
    }
    let registry = u64::from(manager[1]) + u64::from(vector[1]);
    executable::writable(image, registry, 16)?;

    let root = signature::<3>(image, "movie root constructor", ROOT)?;
    let container = signature::<4>(image, "display container constructor", CONTAINER)?;
    let text = signature::<4>(image, "display text constructor", TEXT)?;
    if container[1] != text[1] {
        return Err("display constructors do not share their base class".into());
    }
    let tables = [
        (flash_vtable, 4),
        (root[1], 4),
        (root[2], 1),
        (container[2], 4),
        (container[3], 1),
        (text[2], 4),
        (text[3], 1),
    ];
    for (index, &(table, entries)) in tables.iter().enumerate() {
        vtable(image, table, entries)?;
        if tables[..index].iter().any(|&(other, _)| other == table) {
            return Err("Scaleform classes have overlapping vtables".into());
        }
    }
    Ok(ScaleformLayout {
        registry_vector_rva: registry,
        flash_instance_type_rva: flash_type.into(),
        flash_instance_vtable_rva: flash_vtable.into(),
        root_vtable_rva: root[1].into(),
        root_secondary_vtable_rva: root[2].into(),
        container_vtable_rva: container[2].into(),
        container_secondary_vtable_rva: container[3].into(),
        text_vtable_rva: text[2].into(),
        text_secondary_vtable_rva: text[3].into(),
    })
}

pub(crate) fn flash_instance_vtable(image: PeFile<'_>) -> Result<u64, String> {
    Ok(flash_binding(image)?.1.into())
}

fn flash_binding(image: PeFile<'_>) -> Result<(u32, u32), String> {
    let flash = signature::<3>(image, "Flash instance registration", FLASH)?;
    executable::writable(image, flash[2].into(), 0x30)?;
    let flash_vtable = signature_in::<2>(
        image,
        "Flash instance constructor",
        FLASH_VTABLE,
        function_range(image, flash[1])?,
    )?;
    vtable(image, flash_vtable[1], 4)?;
    Ok((flash[2], flash_vtable[1]))
}

fn resource_lookup(image: PeFile<'_>, descriptor: u32) -> Result<u32, String> {
    let pattern = pattern::parse(RESOURCE_LOOKUP).map_err(|error| error.to_string())?;
    let scanner = image.scanner();
    let mut matches = scanner.matches(&pattern, image.headers().code_range());
    let mut found = None;
    let mut values = [0; 3];
    while matches.next(&mut values) {
        if values[2] != descriptor {
            continue;
        }
        if found.is_some_and(|previous| previous != values[1]) {
            return Err("ambiguous Flash resource manager".into());
        }
        found = Some(values[1]);
    }
    found.ok_or_else(|| "Flash resource manager signature not found".into())
}

fn vtable(image: PeFile<'_>, rva: u32, entries: usize) -> Result<(), String> {
    if !rva.is_multiple_of(8)
        || !image.section_headers().iter().any(|section| {
            section.Characteristics & 0xe000_0000 == 0x4000_0000
                && rva >= section.VirtualAddress
                && u64::from(rva) + entries as u64 * 8
                    <= u64::from(section.VirtualAddress) + u64::from(section.VirtualSize)
        })
    {
        return Err(format!(
            "Scaleform vtable 0x{rva:x} is outside read-only image data"
        ));
    }
    let targets = image
        .derva_slice::<u64>(rva, entries)
        .map_err(|error| format!("Scaleform vtable 0x{rva:x}: {error}"))?;
    if targets.iter().any(|&address| {
        address
            .checked_sub(image.optional_header().ImageBase)
            .is_none_or(|target| {
                !image.section_headers().iter().any(|section| {
                    section.Characteristics & 0x2000_0000 != 0
                        && target >= u64::from(section.VirtualAddress)
                        && target
                            < u64::from(section.VirtualAddress) + u64::from(section.VirtualSize)
                })
            })
    }) {
        return Err(format!("Scaleform vtable 0x{rva:x} has a non-code target"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
