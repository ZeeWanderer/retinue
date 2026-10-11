use super::*;
use crate::game_observer::executable::fixture::{emit, image, put32, relative};

fn fixture(shift: usize, vector: u32) -> Vec<u8> {
    let mut bytes = image();
    bytes[0xb0..0xb8].copy_from_slice(&0x1_4000_0000_u64.to_le_bytes());
    for (offset, label) in [(0x2000, "FlashInstanceImpl"), (0x2040, "/EE/Types/UISys/")] {
        bytes[offset..offset + label.len()].copy_from_slice(label.as_bytes());
    }
    let code = 0x1000 + shift;
    emit(
        &mut bytes,
        code,
        "
        48 8d 15 00 00 00 00 48 8d 4c 24 50 e8 00 00 00 00
        48 8d 15 00 00 00 00 48 8d 4c 24 58 8b 18 e8 00 00 00 00
        c7 44 24 38 10 00 00 00 48 8d 0d 00 00 00 00 c7 44 24 30 50 54 00 00
        4c 8d 0d 00 00 00 00 48 89 4c 24 28 44 8b c3 8b 10
        48 8d 0d 00 00 00 00 c6 44 24 20 00 e8 00 00 00 00",
    );
    relative(&mut bytes, code + 3, 0x2000);
    relative(&mut bytes, code + 20, 0x2040);
    relative(&mut bytes, code + 47, code + 0x100);
    relative(&mut bytes, code + 79, 0x3200 + shift);
    emit(&mut bytes, code + 0x100, "e9 00 00 00 00");
    relative(&mut bytes, code + 0x101, code + 0x120);
    emit(
        &mut bytes,
        code + 0x120,
        "e8 00 00 00 00 48 8d 05 00 00 00 00 33 ff 48 89 03",
    );
    relative(&mut bytes, code + 0x128, 0x2400 + shift);
    emit(
        &mut bytes,
        code + 0x200,
        "
        e8 00 00 00 00 4c 8d 05 00 00 00 00 48 8b c8 4c 89 44 24 28
        4c 8d 0d 00 00 00 00 48 8d 55 e0 c6 44 24 20 01 e8 00 00 00 00",
    );
    relative(&mut bytes, code + 0x201, code + 0x300);
    relative(&mut bytes, code + 0x208, 0x3200 + shift);
    emit(
        &mut bytes,
        code + 0x300,
        "33 d2 48 8d 0d 00 00 00 00 0f 57 c0",
    );
    relative(&mut bytes, code + 0x305, code + 0x400);
    emit(
        &mut bytes,
        code + 0x320,
        "48 8d 05 00 00 00 00 48 83 c4 28 c3",
    );
    relative(&mut bytes, code + 0x323, 0x3400 + shift);
    emit(
        &mut bytes,
        code + 0x400,
        "48 8d 0d 00 00 00 00 e9 00 00 00 00",
    );
    relative(&mut bytes, code + 0x403, 0x3400 + shift);
    relative(&mut bytes, code + 0x408, code + 0x500);
    emit(
        &mut bytes,
        code + 0x500,
        "
        48 8b bb 00 00 00 00 48 85 ff 74 31 8b b3 00 00 00 00
        48 03 f7 48 3b fe 74 17 48 8b 0f 83 41 08 ff 75 05 e8 00 00 00 00
        48 83 c7 10 48 3b fe 75 e9 48 8b 8b 00 00 00 00 e8 00 00 00 00",
    );
    put32(&mut bytes, code + 0x503, vector);
    put32(&mut bytes, code + 0x50e, vector + 8);
    put32(&mut bytes, code + 0x534, vector);
    emit(
        &mut bytes,
        code + 0x600,
        "
        45 33 ff 48 8d 05 00 00 00 00 48 89 01 48 8d 05 00 00 00 00
        48 89 41 10 49 8b f0 44 89 79 08 48 8b da 4c 89 79 18 48 8b f9
        44 89 79 20 4c 89 79 28 4c 89 79 30 4c 89 79 38 4c 89 bf a0 00 00 00",
    );
    relative(&mut bytes, code + 0x606, 0x2440 + shift);
    relative(&mut bytes, code + 0x610, 0x2480 + shift);
    emit(
        &mut bytes,
        code + 0x800,
        "
        e8 00 00 00 00 48 8d 05 00 00 00 00 45 33 f6 48 89 03
        48 8d 05 00 00 00 00 48 89 43 10 4c 89 b3 28 01 00 00
        4c 89 b3 30 01 00 00 4c 89 b3 38 01 00 00",
    );
    relative(&mut bytes, code + 0x801, code + 0xc00);
    relative(&mut bytes, code + 0x808, 0x24c0 + shift);
    relative(&mut bytes, code + 0x815, 0x2500 + shift);
    emit(
        &mut bytes,
        code + 0xa00,
        "
        e8 00 00 00 00 48 8d 05 00 00 00 00 c6 87 28 01 00 00 01
        48 89 07 45 33 f6 48 8d 05 00 00 00 00 48 89 47 10 4c 89 b7 30 01 00 00",
    );
    relative(&mut bytes, code + 0xa01, code + 0xc00);
    relative(&mut bytes, code + 0xa08, 0x2540 + shift);
    relative(&mut bytes, code + 0xa1c, 0x2580 + shift);
    for index in 0..7 {
        for entry in 0..4 {
            let offset = 0x2400 + shift + index * 0x40 + entry * 8;
            bytes[offset..offset + 8]
                .copy_from_slice(&(0x1_4000_0000 + code as u64 + 0xc00).to_le_bytes());
        }
    }
    put32(&mut bytes, 0x104, 16);
    put32(&mut bytes, 0x120, 0x2e00);
    put32(&mut bytes, 0x124, 3 * 12);
    for (i, start) in [0x120, 0x300, 0x500].into_iter().enumerate() {
        put32(&mut bytes, 0x2e00 + i * 12, (code + start) as u32);
        put32(&mut bytes, 0x2e04 + i * 12, (code + start + 0x80) as u32);
    }
    bytes
}

#[test]
fn discovers_relocated_code_data_and_registry_field() {
    for (shift, field) in [(0, 0x80), (0x60, 0xa0)] {
        let actual = discover(&fixture(shift, field)).unwrap();
        let shift = shift as u64;
        assert_eq!(
            actual,
            ScaleformLayout {
                registry_vector_rva: 0x3400 + shift + u64::from(field),
                flash_instance_type_rva: 0x3200 + shift,
                flash_instance_vtable_rva: 0x2400 + shift,
                root_vtable_rva: 0x2440 + shift,
                root_secondary_vtable_rva: 0x2480 + shift,
                container_vtable_rva: 0x24c0 + shift,
                container_secondary_vtable_rva: 0x2500 + shift,
                text_vtable_rva: 0x2540 + shift,
                text_secondary_vtable_rva: 0x2580 + shift,
            }
        );
    }
}

#[test]
fn rejects_wrong_manager_and_display_relationships() {
    let mut bytes = fixture(0, 0x80);
    relative(&mut bytes, 0x1323, 0x3450);
    assert!(discover(&bytes).unwrap_err().contains("accessor disagree"));
    let mut bytes = fixture(0, 0x80);
    put32(&mut bytes, 0x150e, 0x90);
    assert!(discover(&bytes).unwrap_err().contains("registry vector"));
    let mut bytes = fixture(0, 0x80);
    relative(&mut bytes, 0x1a01, 0x1c10);
    assert!(discover(&bytes).unwrap_err().contains("base class"));
}

#[test]
fn rejects_missing_ambiguous_and_structurally_changed_anchors() {
    let mut bytes = fixture(0, 0x80);
    bytes[0x2000] = b'X';
    assert!(
        discover(&bytes)
            .unwrap_err()
            .contains("registration signature not found")
    );
    let mut bytes = fixture(0, 0x80);
    bytes[0x1617] = 0x18;
    assert!(
        discover(&bytes)
            .unwrap_err()
            .contains("root constructor signature not found")
    );
    let mut bytes = fixture(0, 0x80);
    bytes.copy_within(0x1a00..0x1a2b, 0x1d00);
    relative(&mut bytes, 0x1d01, 0x1c00);
    relative(&mut bytes, 0x1d08, 0x25c0);
    relative(&mut bytes, 0x1d1c, 0x2600);
    assert!(
        discover(&bytes)
            .unwrap_err()
            .contains("ambiguous display text")
    );
}

#[test]
fn rejects_invalid_sections_vtable_targets_and_aliases() {
    let mut bytes = fixture(0, 0x80);
    relative(&mut bytes, 0x104f, 0x1100);
    assert!(
        discover(&bytes)
            .unwrap_err()
            .contains("outside writable image")
    );
    let mut bytes = fixture(0, 0x80);
    bytes[0x2400..0x2408].copy_from_slice(&0x1_4000_3400_u64.to_le_bytes());
    assert!(discover(&bytes).unwrap_err().contains("non-code target"));
    let mut bytes = fixture(0, 0x80);
    relative(&mut bytes, 0x1a08, 0x2400);
    assert!(
        discover(&bytes)
            .unwrap_err()
            .contains("overlapping vtables")
    );
    assert!(discover(b"invalid PE").unwrap_err().contains("invalid PE"));
}
