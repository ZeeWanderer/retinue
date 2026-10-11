use std::ops::Range;
use std::path::Path;

use pelite::pattern;
use pelite::pe64::{Pe, PeFile};
use sha2::{Digest, Sha256};

#[cfg(test)]
pub(crate) mod fixture;
mod resource;
pub(super) use resource::ResourceLayout;

const GLOBAL_REGISTRY: &str = r#"
    48 8d 15 ${ "Setting gGameRules" 0a 00 }
    48 8b c8 e8 ????
    e8 ${ [0-96] 48 8d 05 ${'} 48 83 c4 28 c3 }
    [0-12] 41 b8 u4 48 8b c8 e8 ${'}
"#;
const STRING_BLOCKS: &str = "
    44 8b 01 48 8b da 48 8b 05 ${'}
    41 0f b7 c8 48 03 c9 49 c1 e8 10 48 8b 0c c8 49 03 c8 48 89 0a
";

pub(super) fn read(path: &Path) -> Result<(String, Vec<u8>), String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("could not read Warframe executable: {error}"))?;
    Ok((format!("{:x}", Sha256::digest(&bytes)), bytes))
}

pub(super) fn registry(image: PeFile<'_>) -> Result<(u64, u64, u32), String> {
    let registry = signature::<4>(image, "game registry", GLOBAL_REGISTRY)?;
    if !image.headers().code_range().contains(&registry[3]) {
        return Err("registry setter is outside game code".into());
    }
    let end = registry[3]
        .checked_add(512)
        .filter(|&end| end <= image.headers().code_range().end)
        .ok_or("registry setter is outside game code")?;
    let fields = signature_in::<2>(
        image,
        "registry vector layout",
        "4c 8d b3 u4 4d 8b 16 45 8b 46 08 49 8b da 4f 8d 0c 10 49 c1 e8 04
         [0-192] 48 8b 3e 4c 8b 74 24 ? 48 39 7b 08",
        registry[3]..end,
    )?;
    if !(24..0x10000).contains(&fields[1]) || !fields[1].is_multiple_of(8) {
        return Err("invalid registry vector offset".into());
    }
    writable(image, registry[1].into(), u64::from(fields[1]) + 16)?;
    Ok((registry[1].into(), fields[1].into(), registry[2]))
}

pub(super) fn string_blocks(image: PeFile<'_>) -> Result<u64, String> {
    let rva = signature::<2>(image, "string blocks", STRING_BLOCKS)?[1].into();
    writable(image, rva, 8)?;
    Ok(rva)
}

pub(super) fn descriptor(
    image: PeFile<'_>,
    name: &str,
    parent: &str,
) -> Result<(u64, u32), String> {
    let pattern = format!(
        r#"
        48 8d 15 ${{ "{name}" 00 }}
        48 8d 4c 24 50 e8 ???? 48 8d 15 ${{ "{parent}" 00 }}
        48 8d 4c 24 58 8b 18 e8 ????
        [0-40] 44 8b c3 8b 10 48 8d 0d ${{'}} c6 44 24 20 00 e8 ${{'}}
    "#
    );
    let found = signature::<3>(image, name, &pattern)?;
    let rva = found[1].into();
    writable(image, rva, 0x30)?;
    Ok((rva, found[2]))
}

pub(super) fn writable(image: PeFile<'_>, rva: u64, size: u64) -> Result<(), String> {
    if image.section_headers().iter().any(|section| {
        section.Characteristics & 0xa000_0000 == 0x8000_0000
            && rva >= u64::from(section.VirtualAddress)
            && rva.checked_add(size).is_some_and(|end| {
                end <= u64::from(section.VirtualAddress) + u64::from(section.VirtualSize)
            })
    }) {
        Ok(())
    } else {
        Err(format!("binding 0x{rva:x} is outside writable image data"))
    }
}

pub(super) fn signature<const N: usize>(
    image: PeFile<'_>,
    label: &str,
    source: &str,
) -> Result<[u32; N], String> {
    signature_in(image, label, source, image.headers().code_range())
}

pub(super) fn signature_in<const N: usize>(
    image: PeFile<'_>,
    label: &str,
    source: &str,
    range: Range<u32>,
) -> Result<[u32; N], String> {
    let pattern = pattern::parse(source).map_err(|error| format!("{label} pattern: {error}"))?;
    let scanner = image.scanner();
    let mut matches = scanner.matches(&pattern, range);
    let mut first = [0; N];
    if !matches.next(&mut first) {
        return Err(format!("{label} signature not found"));
    }
    let mut next = [0; N];
    while matches.next(&mut next) {
        if next[1..] != first[1..] {
            return Err(format!("ambiguous {label} signature"));
        }
    }
    Ok(first)
}

pub(super) fn function_range(image: PeFile<'_>, rva: u32) -> Result<Range<u32>, String> {
    let table = image
        .exception()
        .map_err(|error| format!("PE function table: {error}"))?;
    if !table.check_sorted() {
        return Err("invalid PE function table order".into());
    }
    let rows = table.image();
    rows.get(
        rows.partition_point(|row| row.BeginAddress <= rva)
            .wrapping_sub(1),
    )
    .filter(|row| rva < row.EndAddress)
    .map(|row| row.BeginAddress..row.EndAddress)
    .ok_or_else(|| format!("no PE function contains 0x{rva:x}"))
}
