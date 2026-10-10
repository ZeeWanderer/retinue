use std::io::Read;
use std::ops::Range;

use pelite::pattern;
use pelite::pe64::{Pe, PeFile, image::RUNTIME_FUNCTION};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::OpenedSource;

pub(super) const SCHEMA: &str = "hud_damage_v1";
const MAX_EXECUTABLE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
pub(super) struct Layout {
    pub schema: &'static str,
    pub hud_vtable_rva: u64,
    pub native_vtable_rva: u64,
    pub flash_vtable_rva: u64,
    pub mode_rva: u64,
    pub evidence: Evidence,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct Evidence {
    hud_constructor: u32,
    native_constructor: u32,
    pool_initializer: u32,
    pool_getter: u32,
    counter_writer: u32,
    consumer: u32,
    anchor_updater: u32,
}

// These instruction contracts define hud_damage_v1, not an executable fingerprint.
// Displacements governing the decoded fields are deliberately not wildcards.
const HUD_CTOR: &str = "
    48 8b d9 e8 ???? 33 ff 48 8d 35 ???? 48 8d 05 ${'}
    48 89 03 48 89 73 28 ff 05 ???? 48 89 73 30
";
const NATIVE_CTOR: &str = "
    4c 8b f1 e8 ???? 45 33 ed 49 8d 9e e8 02 00 00
    48 8d 05 ${'} b9 10 00 00 00 49 89 06
";
const POOL_ALLOC: &str = "
    49 8d 8e 30 0c 00 00 ba 37 00 00 00 [0-80] e8 ????
    49 8b 9e 30 0c 00 00 41 8b be 38 0c 00 00 48 03 fb 48 3b df
";
const RECORD_ALLOC: &str = "
    ba 01 00 00 00 b9 90 01 00 00 e8 ???? 48 8b f0 48 8d 48 60
    44 88 28 4c 89 68 04 4c 89 68 0c 4c 89 68 14 4c 89 68 1c 4c 89 68 24
    44 88 68 30 c6 40 3f 0f 44 89 68 40 c7 40 44 00 00 80 3f
    66 c7 40 48 01 00 44 88 68 4a 44 89 68 4c 44 88 68 50 e8 ????
    4c 89 a6 60 01 00 00 ff 05 ???? c7 86 68 01 00 00 ff ff ff ff
    66 44 89 ae 6c 01 00 00 c6 86 6e 01 00 00 01 44 88 ae 70 01 00 00
    66 c7 86 7f 01 00 00 0f 00 c6 86 8f 01 00 00 0f e8 ????
    48 89 30 c7 40 0c 01 00 00 00 c7 40 08 01 00 00 00 48 8b 0b 48 89 03
    [0-48] 48 83 c3 08 48 3b df 0f 85 ${'}
";
const INITIALIZER_MODE: &str = "
    48 8b 05 ${'} 48 8d 55 bf 48 8b 49 28 48 ff c8 49 89 7b 18
    48 a9 fd ff ff ff 4d 89 63 20 4d 89 6b e8 41 0f 94 c5
    48 8b 09 4d 89 73 e0 4d 89 7b d8 49 89 73 10 e8 ${'}
";
const HUD_POOL_COPY: &str = "
    49 8b 06 48 89 83 a8 03 00 00 41 8b 46 08 89 83 b0 03 00 00
    41 8b 46 0c 89 83 b4 03 00 00
";
const LABEL: &str = r#"
    48 8b 8b a8 03 00 00 4c 8d 05 ${ ".Label" 00 } 44 8b f8
    48 8b 14 f9 48 8b 0a 48 8d 55 bf 89 81 68 01 00 00 48 8d 4d ff e8 ????
    48 8b 8b a8 03 00 00 48 8b f0 48 b8 00 00 00 10 00 00 00 ff
    48 8b 14 f9 4c 8b 32 49 39 86 78 01 00 00 72 ?
    49 8b 8e 70 01 00 00 e8 ???? 48 b8 00 00 00 10 00 00 00 ff
    0f 10 06 41 0f 11 86 70 01 00 00
"#;
const COUNTER: &str = "
    41 0f b7 85 d8 02 00 00 66 89 86 6c 01 00 00 b8 d3 4d 62 10
    41 0f b7 8d d8 02 00 00 ff c1 f7 e1 c1 ea 06 69 c2 e8 03 00 00
    2b c8 66 41 89 8d d8 02 00 00
";
const CONSUMER: &str = "
    48 8b 03 4c 8b 30 41 80 be 6e 01 00 00 00 0f 85 ???? 45 0f 2f 56 1c
";
const ANCHOR: &str = "80 7d 63 00 74 ? 48 8b 85 60 01 00 00 48 8b 10 48 85 d2";

pub(super) fn read(source: &OpenedSource) -> Result<Layout, String> {
    let identity = source
        .metadata
        .identity
        .as_ref()
        .ok_or("combat source has no executable identity")?;
    if identity.pid != source.metadata.pid || identity.pid != source.memory.pid() {
        return Err("combat source process identity disagrees".into());
    }
    let file = std::fs::File::open(&identity.executable.path).map_err(|error| {
        format!(
            "combat requires its identified executable {}: {error}",
            identity.executable.path.display()
        )
    })?;
    let mut bytes = Vec::new();
    file.take(MAX_EXECUTABLE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_EXECUTABLE_BYTES {
        return Err("combat executable exceeds 256 MiB limit".into());
    }
    if bytes.len() as u64 != identity.executable.size
        || format!("{:x}", Sha256::digest(&bytes)) != identity.executable.sha256
    {
        return Err("combat executable differs from source identity".into());
    }
    discover(&bytes)
}

fn discover(bytes: &[u8]) -> Result<Layout, String> {
    let image = PeFile::from_bytes(bytes).map_err(|error| format!("invalid combat PE: {error}"))?;
    let flash = crate::game_observer::ui::layout::flash_instance_vtable(image)
        .map_err(|error| format!("combat Flash binding: {error}"))?;
    discover_image(image, flash)
        .map_err(|error| format!("unsupported combat schema ({SCHEMA}): {error}"))
}

fn discover_image(image: PeFile<'_>, flash_vtable_rva: u64) -> Result<Layout, String> {
    let hud = constructor(image, "LotusHudRedux", HUD_CTOR)?;
    let native = constructor(image, "LotusHudStatus", NATIVE_CTOR)?;
    let native_range = function(image, native.0)?;
    for (label, code) in [
        ("native counter initialization", "66 45 89 ae d8 02 00 00"),
        (
            "native pool initialization",
            "4d 89 ae 30 0c 00 00 4d 89 ae 38 0c 00 00",
        ),
        ("55-slot native pool", POOL_ALLOC),
    ] {
        signature::<1>(image, label, code, native_range.clone())?;
    }
    let allocation = signature::<2>(image, "record allocation", RECORD_ALLOC, native_range)?;
    if allocation[0] != allocation[1] {
        return Err("record allocation loop does not return to its allocator".into());
    }
    let initializer = signature::<1>(
        image,
        "TargetHit anchor",
        r#"48 8d 15 ${ "TargetHit" 00 }"#,
        image.headers().code_range(),
    )?[0];
    let initializer = function(image, initializer)?;
    let mode = signature::<3>(
        image,
        "display mode and native pool accessor",
        INITIALIZER_MODE,
        initializer.clone(),
    )?;
    signature::<1>(image, "HUD pool copy", HUD_POOL_COPY, initializer.clone())?;
    signature::<1>(image, "HUD slot label", LABEL, initializer.clone())?;
    signature::<1>(
        image,
        "native pool accessor",
        "4c 8b 81 30 0c 00 00 4c 8d 4c 24 40 8b 89 38 0c 00 00",
        function(image, mode[2])?,
    )?;
    section(image, mode[1].into(), 8, 0xc000_0000)?;

    let writer = anchored_function(image, "counter writer", COUNTER)?;
    let writer_mode = signature::<2>(
        image,
        "counter writer mode",
        "48 8b 05 ${'} 4c 8b fa 48 ff c8 4c 8b e9 48 a9 fd ff ff ff",
        writer.clone(),
    )?;
    let consumer = anchored_function(image, "record consumer", CONSUMER)?;
    for (label, code) in [
        (
            "consumer pool",
            "48 8b 9e a8 03 00 00 [0-32] 8b be b0 03 00 00",
        ),
        (
            "consumer text",
            "41 80 7e 3f ff 4d 8d 46 30 48 8b 46 30 48 8b 08 48 8b 01",
        ),
        ("consumer color", "41 8b 46 40"),
        ("consumer critical tier", "41 8b 56 4c 49 8d 4e 60"),
    ] {
        signature::<1>(image, label, code, consumer.clone())?;
    }
    let updater = anchored_function(image, "anchor updater", ANCHOR)?;
    signature::<1>(
        image,
        "updater pool",
        "49 8b 9e 30 0c 00 00 41 8b be 38 0c 00 00",
        updater.clone(),
    )?;
    let updater_mode = signature::<2>(
        image,
        "updater mode",
        "48 8b 05 ${'} 48 89 b4 24 50 01 00 00 40 32 f6 48 ff c8 4c 89 bc 24 40 01 00 00 0f 29 b4 24 30 01 00 00 48 a9 fd ff ff ff",
        updater.clone(),
    )?;
    if writer_mode[1] != mode[1] || updater_mode[1] != mode[1] {
        return Err("display mode references disagree".into());
    }
    let tables = [u64::from(hud.1), u64::from(native.1), flash_vtable_rva];
    for (index, &table) in tables.iter().enumerate() {
        vtable(image, table)?;
        if tables[..index].contains(&table) {
            return Err("combat types share a vtable".into());
        }
    }
    Ok(Layout {
        schema: SCHEMA,
        hud_vtable_rva: hud.1.into(),
        native_vtable_rva: native.1.into(),
        flash_vtable_rva,
        mode_rva: mode[1].into(),
        evidence: Evidence {
            hud_constructor: hud.0,
            native_constructor: native.0,
            pool_initializer: initializer.start,
            pool_getter: mode[2],
            counter_writer: writer.start,
            consumer: consumer.start,
            anchor_updater: updater.start,
        },
    })
}

fn constructor(image: PeFile<'_>, name: &str, code: &str) -> Result<(u32, u32), String> {
    let registration = format!(
        r#"
        48 8d 15 ${{ "{name}" 00 }} 48 8d 4c 24 50 e8 ????
        48 8d 15 ${{ "/Lotus/Types/Game/" 00 }} 48 8d 4c 24 58 8b 18 e8 ????
        c7 44 24 38 ???? 48 8d 0d ${{ e9 ${{'}} }} c7 44 24 30 ????
        4c 8d 0d ???? 48 89 4c 24 28 44 8b c3 8b 10 48 8d 0d ${{'}}
        c6 44 24 20 00 e8 ????
    "#
    );
    let found = signature::<3>(image, name, &registration, image.headers().code_range())?;
    section(image, found[2].into(), 0x30, 0xc000_0000)?;
    let range = function(image, found[1])?;
    if range.start != found[1] {
        return Err(format!("{name} factory does not target a function entry"));
    }
    Ok((found[1], signature::<2>(image, name, code, range)?[1]))
}

fn signature<const N: usize>(
    image: PeFile<'_>,
    label: &str,
    code: &str,
    range: Range<u32>,
) -> Result<[u32; N], String> {
    let pattern = pattern::parse(code).map_err(|error| format!("{label}: {error}"))?;
    let scanner = image.scanner();
    let mut matches = scanner.matches(&pattern, range);
    let mut result = [0; N];
    if !matches.next(&mut result) {
        return Err(format!("{label} code not found"));
    }
    if matches.next(&mut [0; N]) {
        return Err(format!("ambiguous {label} code"));
    }
    section(image, result[0].into(), 1, 0x6000_0000)?;
    Ok(result)
}

fn anchored_function(image: PeFile<'_>, label: &str, code: &str) -> Result<Range<u32>, String> {
    function(
        image,
        signature::<1>(image, label, code, image.headers().code_range())?[0],
    )
}

fn section(image: PeFile<'_>, rva: u64, size: u64, flags: u32) -> Result<(), String> {
    if image.section_headers().iter().any(|section| {
        section.Characteristics & 0xe000_0000 == flags
            && rva >= section.VirtualAddress.into()
            && rva.checked_add(size).is_some_and(|end| {
                end <= u64::from(section.VirtualAddress) + u64::from(section.VirtualSize)
            })
    }) {
        Ok(())
    } else {
        Err(format!("invalid combat PE section at 0x{rva:x}+0x{size:x}"))
    }
}

fn vtable(image: PeFile<'_>, rva: u64) -> Result<(), String> {
    section(image, rva, 32, 0x4000_0000)?;
    let rva = u32::try_from(rva).map_err(|_| "vtable RVA overflow")?;
    if !rva.is_multiple_of(8) {
        return Err("unaligned combat vtable".into());
    }
    for &address in image
        .derva_slice::<u64>(rva, 4)
        .map_err(|e| e.to_string())?
    {
        let target = address
            .checked_sub(image.optional_header().ImageBase)
            .ok_or("invalid combat vtable target")?;
        section(image, target, 1, 0x6000_0000)?;
    }
    Ok(())
}

// Chained unwind records describe fragments, not independent functions. Only join
// adjacent entries with the same validated root; never scan past another function.
fn function(image: PeFile<'_>, pc: u32) -> Result<Range<u32>, String> {
    let table = image
        .exception()
        .map_err(|e| format!("combat unwind table: {e}"))?;
    if !table.check_sorted() {
        return Err("invalid combat unwind order".into());
    }
    let rows = table.image();
    let index = rows
        .partition_point(|row| row.BeginAddress <= pc)
        .wrapping_sub(1);
    let row = rows
        .get(index)
        .filter(|row| pc < row.EndAddress)
        .ok_or("combat code lacks unwind entry")?;
    let root = unwind_root(image, rows, row)?;
    let start = rows[root].BeginAddress;
    let mut end = rows[root].EndAddress;
    for (offset, row) in rows[root + 1..].iter().enumerate() {
        if row.BeginAddress != end || unwind_root(image, rows, row)? != root {
            break;
        }
        if offset >= 63 || row.EndAddress.saturating_sub(start) > 65536 {
            return Err("combat function exceeds unwind bounds".into());
        }
        end = row.EndAddress;
    }
    if !(start..end).contains(&pc) || end - start > 65536 {
        return Err("unsupported combat function fragments".into());
    }
    section(image, start.into(), (end - start).into(), 0x6000_0000)?;
    Ok(start..end)
}

fn unwind_root(
    image: PeFile<'_>,
    rows: &[RUNTIME_FUNCTION],
    row: &RUNTIME_FUNCTION,
) -> Result<usize, String> {
    let mut index = rows
        .binary_search_by_key(&row.BeginAddress, |row| row.BeginAddress)
        .map_err(|_| "missing combat unwind parent")?;
    let mut seen = Vec::new();
    for _ in 0..16 {
        if seen.contains(&index) {
            return Err("cyclic combat unwind chain".into());
        }
        seen.push(index);
        let row = &rows[index];
        let length = row
            .EndAddress
            .checked_sub(row.BeginAddress)
            .filter(|&length| length != 0)
            .ok_or("invalid combat unwind range")?;
        section(image, row.BeginAddress.into(), length.into(), 0x6000_0000)?;
        section(image, row.UnwindData.into(), 4, 0x4000_0000)?;
        let header = image
            .slice(row.UnwindData, 4, 1)
            .map_err(|e| e.to_string())?;
        let flags = header[0] >> 3;
        if header[0] & 7 != 1 || flags > 4 {
            return Err("unsupported combat unwind encoding".into());
        }
        if flags != 4 {
            return Ok(index);
        }
        let tail = row
            .UnwindData
            .checked_add(4 + 2 * ((u32::from(header[2]) + 1) & !1))
            .ok_or("combat unwind overflow")?;
        section(image, tail.into(), 12, 0x4000_0000)?;
        let parent = image
            .derva::<RUNTIME_FUNCTION>(tail)
            .map_err(|e| e.to_string())?;
        index = rows
            .binary_search_by_key(&parent.BeginAddress, |row| row.BeginAddress)
            .map_err(|_| "missing combat unwind parent")?;
        if rows[index].EndAddress != parent.EndAddress
            || rows[index].UnwindData != parent.UnwindData
        {
            return Err("inconsistent combat unwind parent".into());
        }
    }
    Err("combat unwind chain exceeds 16 links".into())
}

#[cfg(test)]
mod tests;
