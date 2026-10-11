use super::*;

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn emit(bytes: &mut [u8], cursor: &mut usize, hex: &str) {
    for byte in hex.split_whitespace() {
        bytes[*cursor] = u8::from_str_radix(byte, 16).unwrap();
        *cursor += 1;
    }
}

fn relative(bytes: &mut [u8], cursor: &mut usize, opcode: &str, target: usize) {
    emit(bytes, cursor, opcode);
    put32(bytes, *cursor, (target as i32 - *cursor as i32 - 4) as u32);
    *cursor += 4;
}

fn registration(
    bytes: &mut [u8],
    at: usize,
    name: usize,
    thunk: usize,
    descriptor: usize,
    shift: usize,
) {
    let mut p = at;
    relative(bytes, &mut p, "48 8d 15", name);
    emit(bytes, &mut p, "48 8d 4c 24 50 e8 00 00 00 00");
    relative(bytes, &mut p, "48 8d 15", 0xa080 + shift);
    emit(
        bytes,
        &mut p,
        "48 8d 4c 24 58 8b 18 e8 00 00 00 00 c7 44 24 38 10 00 00 00",
    );
    relative(bytes, &mut p, "48 8d 0d", thunk);
    emit(
        bytes,
        &mut p,
        "c7 44 24 30 80 12 00 00 4c 8d 0d 00 00 00 00 48 89 4c 24 28 44 8b c3 8b 10",
    );
    relative(bytes, &mut p, "48 8d 0d", descriptor);
    emit(bytes, &mut p, "c6 44 24 20 00 e8 00 00 00 00");
}

// A small PE with real RIP-relative references and chained unwind entries. The
// core accepts the Flash result separately; the offline test exercises its helper.
fn fixture(base: u64, shift: usize) -> Vec<u8> {
    let mut bytes = vec![0; 0x18000];
    emit(&mut bytes, &mut 0, "4d 5a");
    put32(&mut bytes, 0x3c, 0x80);
    emit(&mut bytes, &mut 0x80, "50 45 00 00 64 86 03 00");
    emit(&mut bytes, &mut 0x94, "f0 00 00 00 0b 02");
    bytes[0xb0..0xb8].copy_from_slice(&base.to_le_bytes());
    for (offset, value) in [
        (0x9c, 0x9000),
        (0xac, 0x1000),
        (0xd0, 0x18000),
        (0xd4, 0x1000),
        (0x104, 16),
    ] {
        put32(&mut bytes, offset, value);
    }
    for (index, (address, size, flags)) in [
        (0x1000, 0x9000, 0x6000_0020),
        (0xa000, 0xa000, 0x4000_0040),
        (0x14000, 0x4000, 0xc000_0040),
    ]
    .into_iter()
    .enumerate()
    {
        for (offset, value) in [
            (8, size),
            (12, address),
            (16, size),
            (20, address),
            (36, flags),
        ] {
            put32(&mut bytes, 0x188 + index * 40 + offset, value);
        }
    }
    let rows = [
        (0x2000, 0x2100, None),
        (0x3000, 0x3010, None),
        (0x3010, 0x3400, Some(1)),
        (0x4000, 0x4050, None),
        (0x4050, 0x4100, Some(3)),
        (0x4100, 0x4400, Some(4)),
        (0x5000, 0x5100, None),
        (0x5200, 0x5400, None),
        (0x5600, 0x5800, None),
        (0x5a00, 0x5c00, None),
    ];
    put32(&mut bytes, 0x120, 0xb000 + shift as u32);
    put32(&mut bytes, 0x124, rows.len() as u32 * 12);
    for (i, &(begin, end, parent)) in rows.iter().enumerate() {
        let unwind = 0xb800 + shift + i * 32;
        for (offset, value) in [(0, begin + shift), (4, end + shift), (8, unwind)] {
            put32(&mut bytes, 0xb000 + shift + i * 12 + offset, value as u32);
        }
        bytes[unwind] = if parent.is_some() { 0x21 } else { 1 };
        if let Some(parent) = parent {
            for (offset, value) in [
                (0, rows[parent].0 + shift),
                (4, rows[parent].1 + shift),
                (8, 0xb800 + shift + parent * 32),
            ] {
                put32(&mut bytes, unwind + 4 + offset, value as u32);
            }
        }
    }
    for (offset, string) in [
        (0xa000, "LotusHudRedux"),
        (0xa040, "LotusHudStatus"),
        (0xa080, "/Lotus/Types/Game/"),
        (0xa100, "TargetHit"),
        (0xa120, ".Label"),
    ] {
        bytes[offset + shift..offset + shift + string.len()].copy_from_slice(string.as_bytes());
    }
    for (i, table) in [0xa200, 0xa240, 0xa280].into_iter().enumerate() {
        for slot in 0..4 {
            let at = table + shift + slot * 8;
            bytes[at..at + 8]
                .copy_from_slice(&(base + 0x2000 + shift as u64 + i as u64).to_le_bytes());
        }
    }
    registration(
        &mut bytes,
        0x1000 + shift,
        0xa000 + shift,
        0x1200 + shift,
        0x14100 + shift,
        shift,
    );
    registration(
        &mut bytes,
        0x1100 + shift,
        0xa040 + shift,
        0x1220 + shift,
        0x14200 + shift,
        shift,
    );
    relative(&mut bytes, &mut (0x1200 + shift), "e9", 0x2000 + shift);
    relative(&mut bytes, &mut (0x1220 + shift), "e9", 0x3000 + shift);
    let mut p = 0x2000 + shift;
    emit(
        &mut bytes,
        &mut p,
        "48 8b d9 e8 00 00 00 00 33 ff 48 8d 35 00 00 00 00",
    );
    relative(&mut bytes, &mut p, "48 8d 05", 0xa200 + shift);
    emit(
        &mut bytes,
        &mut p,
        "48 89 03 48 89 73 28 ff 05 00 00 00 00 48 89 73 30",
    );
    p = 0x3010 + shift;
    emit(
        &mut bytes,
        &mut p,
        "4c 8b f1 e8 00 00 00 00 45 33 ed 49 8d 9e e8 02 00 00",
    );
    relative(&mut bytes, &mut p, "48 8d 05", 0xa240 + shift);
    emit(
        &mut bytes,
        &mut p,
        "b9 10 00 00 00 49 89 06 66 45 89 ae d8 02 00 00
        4d 89 ae 30 0c 00 00 4d 89 ae 38 0c 00 00
        49 8d 8e 30 0c 00 00 ba 37 00 00 00 e8 00 00 00 00
        49 8b 9e 30 0c 00 00 41 8b be 38 0c 00 00 48 03 fb 48 3b df",
    );
    let allocation = p;
    emit(
        &mut bytes,
        &mut p,
        "ba 01 00 00 00 b9 90 01 00 00 e8 00 00 00 00 48 8b f0 48 8d 48 60
        44 88 28 4c 89 68 04 4c 89 68 0c 4c 89 68 14 4c 89 68 1c 4c 89 68 24
        44 88 68 30 c6 40 3f 0f 44 89 68 40 c7 40 44 00 00 80 3f
        66 c7 40 48 01 00 44 88 68 4a 44 89 68 4c 44 88 68 50 e8 00 00 00 00
        4c 89 a6 60 01 00 00 ff 05 00 00 00 00 c7 86 68 01 00 00 ff ff ff ff
        66 44 89 ae 6c 01 00 00 c6 86 6e 01 00 00 01 44 88 ae 70 01 00 00
        66 c7 86 7f 01 00 00 0f 00 c6 86 8f 01 00 00 0f e8 00 00 00 00
        48 89 30 c7 40 0c 01 00 00 00 c7 40 08 01 00 00 00 48 8b 0b 48 89 03
        48 83 c3 08 48 3b df",
    );
    relative(&mut bytes, &mut p, "0f 85", allocation);
    p = 0x4000 + shift;
    relative(&mut bytes, &mut p, "48 8b 05", 0x14000 + shift);
    emit(
        &mut bytes,
        &mut p,
        "48 8d 55 bf 48 8b 49 28 48 ff c8 49 89 7b 18
        48 a9 fd ff ff ff 4d 89 63 20 4d 89 6b e8 41 0f 94 c5
        48 8b 09 4d 89 73 e0 4d 89 7b d8 49 89 73 10",
    );
    relative(&mut bytes, &mut p, "e8", 0x5000 + shift);
    p = 0x4050 + shift;
    emit(
        &mut bytes,
        &mut p,
        "49 8b 06 48 89 83 a8 03 00 00 41 8b 46 08 89 83 b0 03 00 00
        41 8b 46 0c 89 83 b4 03 00 00",
    );
    p = 0x4100 + shift;
    relative(&mut bytes, &mut p, "48 8d 15", 0xa100 + shift);
    emit(&mut bytes, &mut p, "48 8b 8b a8 03 00 00");
    relative(&mut bytes, &mut p, "4c 8d 05", 0xa120 + shift);
    emit(
        &mut bytes,
        &mut p,
        "44 8b f8 48 8b 14 f9 48 8b 0a 48 8d 55 bf 89 81 68 01 00 00 48 8d 4d ff e8 00 00 00 00
        48 8b 8b a8 03 00 00 48 8b f0 48 b8 00 00 00 10 00 00 00 ff
        48 8b 14 f9 4c 8b 32 49 39 86 78 01 00 00 72 16
        49 8b 8e 70 01 00 00 e8 00 00 00 00 48 b8 00 00 00 10 00 00 00 ff
        0f 10 06 41 0f 11 86 70 01 00 00",
    );
    emit(
        &mut bytes,
        &mut (0x5000 + shift),
        "4c 8b 81 30 0c 00 00 4c 8d 4c 24 40 8b 89 38 0c 00 00",
    );
    p = 0x5200 + shift;
    relative(&mut bytes, &mut p, "48 8b 05", 0x14000 + shift);
    emit(
        &mut bytes,
        &mut p,
        "4c 8b fa 48 ff c8 4c 8b e9 48 a9 fd ff ff ff
        41 0f b7 85 d8 02 00 00 66 89 86 6c 01 00 00 b8 d3 4d 62 10
        41 0f b7 8d d8 02 00 00 ff c1 f7 e1 c1 ea 06 69 c2 e8 03 00 00
        2b c8 66 41 89 8d d8 02 00 00",
    );
    emit(
        &mut bytes,
        &mut (0x5600 + shift),
        "48 8b 03 4c 8b 30 41 80 be 6e 01 00 00 00 0f 85 00 00 00 00 45 0f 2f 56 1c
        48 8b 9e a8 03 00 00 8b be b0 03 00 00
        41 80 7e 3f ff 4d 8d 46 30 48 8b 46 30 48 8b 08 48 8b 01
        41 8b 46 40 41 8b 56 4c 49 8d 4e 60",
    );
    p = 0x5a00 + shift;
    relative(&mut bytes, &mut p, "48 8b 05", 0x14000 + shift);
    emit(
        &mut bytes,
        &mut p,
        "48 89 b4 24 50 01 00 00 40 32 f6 48 ff c8
        4c 89 bc 24 40 01 00 00 0f 29 b4 24 30 01 00 00 48 a9 fd ff ff ff
        49 8b 9e 30 0c 00 00 41 8b be 38 0c 00 00
        80 7d 63 00 74 4b 48 8b 85 60 01 00 00 48 8b 10 48 85 d2",
    );
    bytes
}

fn discover_fixture(bytes: &[u8], shift: usize) -> Result<Layout, String> {
    discover_image(PeFile::from_bytes(bytes).unwrap(), 0xa280 + shift as u64)
}

#[test]
fn relocates_bindings_and_follows_chained_unwind_to_mode() {
    for (base, shift) in [(0x140000000, 0), (0x7ff400000000, 0x1000)] {
        let bytes = fixture(base, shift);
        let layout = discover_fixture(&bytes, shift).unwrap();
        assert_eq!(layout.schema, SCHEMA);
        assert_eq!(layout.hud_vtable_rva, 0xa200 + shift as u64);
        assert_eq!(layout.native_vtable_rva, 0xa240 + shift as u64);
        assert_eq!(layout.flash_vtable_rva, 0xa280 + shift as u64);
        assert_eq!(layout.mode_rva, 0x14000 + shift as u64);
        assert_eq!(layout.evidence.pool_initializer, 0x4000 + shift as u32);
    }
}

#[test]
fn rejects_ambiguous_anchors_even_when_bindings_agree() {
    let mut bytes = fixture(0x140000000, 0);
    registration(&mut bytes, 0x1400, 0xa000, 0x1200, 0x14100, 0);
    assert!(
        discover_fixture(&bytes, 0)
            .unwrap_err()
            .contains("ambiguous LotusHudRedux")
    );
    let mut bytes = fixture(0x140000000, 0);
    relative(&mut bytes, &mut 0x1500, "48 8d 15", 0xa100);
    assert!(
        discover_fixture(&bytes, 0)
            .unwrap_err()
            .contains("ambiguous TargetHit")
    );
}

#[test]
fn rejects_changed_record_schema_and_unsupported_code() {
    for (before, byte, after, error) in [
        ("b9 90 01 00 00", 1, 0xa0, "record allocation"),
        ("44 89 68 4c", 3, 0x54, "record allocation"),
        ("66 44 89 ae 6c 01 00 00", 4, 0x70, "record allocation"),
        ("ba 37 00 00 00", 1, 0x38, "55-slot"),
        ("69 c2 e8 03 00 00", 2, 0xe9, "counter writer"),
        ("41 80 7e 3f ff", 3, 0x47, "consumer text"),
        ("80 7d 63 00", 2, 0x64, "anchor updater"),
        ("48 8b d9", 0, 0x90, "LotusHudRedux"),
    ] {
        let mut bytes = fixture(0x140000000, 0);
        let needle = before
            .split_whitespace()
            .map(|v| u8::from_str_radix(v, 16).unwrap())
            .collect::<Vec<_>>();
        let at = bytes
            .windows(needle.len())
            .position(|v| v == needle)
            .unwrap();
        bytes[at + byte] = after;
        assert!(
            discover_fixture(&bytes, 0).unwrap_err().contains(error),
            "{before}"
        );
    }
}

#[test]
fn rejects_bad_binding_sections_mode_disagreement_and_unwind_cycles() {
    let mut bytes = fixture(0x140000000, 0);
    bytes[0xa200..0xa208].copy_from_slice(&0x140014000_u64.to_le_bytes());
    assert!(
        discover_fixture(&bytes, 0)
            .unwrap_err()
            .contains("PE section")
    );
    let mut bytes = fixture(0x140000000, 0);
    relative(&mut bytes, &mut 0x5200, "48 8b 05", 0x14008);
    assert!(
        discover_fixture(&bytes, 0)
            .unwrap_err()
            .contains("mode references disagree")
    );
    let mut bytes = fixture(0x140000000, 0);
    let row = bytes[0xb000 + 5 * 12..0xb000 + 6 * 12].to_vec();
    bytes[0xb800 + 5 * 32 + 4..0xb800 + 5 * 32 + 16].copy_from_slice(&row);
    assert!(discover_fixture(&bytes, 0).unwrap_err().contains("cyclic"));
}

#[test]
fn malformed_unwind_ranges_return_errors_without_subtraction_panics() {
    let bytes = fixture(0x140000000, 0);
    let image = PeFile::from_bytes(&bytes).unwrap();
    for end in [0x1fff, 0x2000] {
        let rows = [RUNTIME_FUNCTION {
            BeginAddress: 0x2000,
            EndAddress: end,
            UnwindData: 0xb800,
        }];
        assert_eq!(
            unwind_root(image, &rows, &rows[0]).unwrap_err(),
            "invalid combat unwind range"
        );
    }
}

#[test]
#[ignore = "requires WF_COMBAT_PE and WF_COMBAT_REPORT; offline executable only"]
fn offline_pe_report() {
    let path = std::env::var("WF_COMBAT_PE").expect("WF_COMBAT_PE");
    let output = std::env::var("WF_COMBAT_REPORT").expect("WF_COMBAT_REPORT");
    let bytes = std::fs::read(&path).unwrap();
    let layout = discover(&bytes).unwrap();
    let report = serde_json::json!({
        "executable": path, "sha256": format!("{:x}", Sha256::digest(&bytes)),
        "layout": layout, "validation": "offline PE code only; no live memory or input",
    });
    std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}
