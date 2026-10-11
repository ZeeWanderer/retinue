use super::*;
use crate::game_observer::executable::fixture::{
    emit, image, put32, registry_setter, relative, resource_code, resources,
};

fn fixture(shift: usize, field_shift: u32) -> Vec<u8> {
    let mut bytes = image();
    for (address, text) in [
        (0x2000, "Setting gGameRules\n"),
        (0x2040, "LotusProfileData"),
        (0x2080, "/Lotus/Types/Game/"),
        (0x2100, "MiscItems"),
        (0x2120, "Recipes"),
        (0x2140, "PendingRecipes"),
        (0x2180, "Processing *FULL* Inventory JSON\n"),
    ] {
        bytes[address..address + text.len()].copy_from_slice(text.as_bytes());
    }
    let code = 0x1000 + shift;
    emit(
        &mut bytes,
        code,
        "48 8d 15 00 00 00 00 48 8b c8 e8 00 00 00 00 e8 00 00 00 00
         48 8d 55 00 41 b8 00 00 00 00 48 8b c8 e8 00 00 00 00",
    );
    relative(&mut bytes, code + 3, 0x2000);
    relative(&mut bytes, code + 16, code + 0x100);
    relative(&mut bytes, code + 34, code + 0x500);
    registry_setter(&mut bytes, code + 0x500, 0x138 + field_shift);
    put32(&mut bytes, code + 26, 0x11110000 + shift as u32);
    emit(
        &mut bytes,
        code + 0x100,
        "48 83 ec 28 48 8d 05 00 00 00 00 48 83 c4 28 c3",
    );
    relative(&mut bytes, code + 0x107, 0x3000 + shift);
    emit(
        &mut bytes,
        code + 0x200,
        "44 8b 01 48 8b da 48 8b 05 00 00 00 00 41 0f b7 c8 48 03 c9
         49 c1 e8 10 48 8b 0c c8 49 03 c8 48 89 0a",
    );
    relative(&mut bytes, code + 0x209, 0x3200 + shift);
    emit(
        &mut bytes,
        code + 0x300,
        "48 8d 15 00 00 00 00 48 8d 4c 24 50 e8 00 00 00 00
         48 8d 15 00 00 00 00 48 8d 4c 24 58 8b 18 e8 00 00 00 00
         44 8b c3 8b 10 48 8d 0d 00 00 00 00 c6 44 24 20 00 e8 00 00 00 00",
    );
    relative(&mut bytes, code + 0x303, 0x2040);
    relative(&mut bytes, code + 0x314, 0x2080);
    relative(&mut bytes, code + 0x32c, 0x3300 + shift);
    relative(&mut bytes, code + 0x336, code + 0xd00);
    resource_code(
        &mut bytes,
        code + 0xd00,
        resources(0x3200 + shift as u64, u64::from(field_shift / 8)),
    );

    for (address, label, offset) in [(code + 0x700, 0x2100, 0xd0), (code + 0x740, 0x2120, 0xe0)] {
        emit(
            &mut bytes,
            address,
            "48 8d 96 00 00 00 00 40 88 7c 24 20 45 0f b6 ce
             4c 8d 05 00 00 00 00 48 8b cb e8 00 00 00 00",
        );
        put32(&mut bytes, address + 3, offset);
        relative(&mut bytes, address + 19, label);
        relative(&mut bytes, address + 27, code + 0xc00);
    }
    emit(
        &mut bytes,
        code + 0x800,
        "48 8b c2 45 33 c9 48 8b d1 45 33 c0 48 8b c8 e9 00 00 00 00",
    );
    relative(&mut bytes, code + 0x810, code + 0x700);
    emit(
        &mut bytes,
        code + 0x900,
        "49 8d 8c 24 00 00 00 00 4c 8b c6 48 8d 15 00 00 00 00 e8 00 00 00 00",
    );
    put32(&mut bytes, code + 0x904, 0x800 + field_shift);
    relative(&mut bytes, code + 0x90e, 0x2140);
    relative(&mut bytes, code + 0x913, code + 0xe00);
    emit(
        &mut bytes,
        code + 0x930,
        "49 8d 94 24 00 00 00 00 48 8b ce e8 00 00 00 00",
    );
    put32(&mut bytes, code + 0x934, 0x500 + field_shift);
    relative(&mut bytes, code + 0x93c, code + 0x800);
    emit(
        &mut bytes,
        code + 0x980,
        "49 8d 94 24 00 00 00 00 48 8b ce e8 00 00 00 00 48 8b 06 48 8b ce ff 50 60
         90 90 90 90 48 8d 15 00 00 00 00",
    );
    put32(&mut bytes, code + 0x984, 0x700 + field_shift);
    relative(&mut bytes, code + 0x9a0, 0x2180);
    emit(
        &mut bytes,
        code + 0xc40,
        "4c 8d 53 0c 90 41 8b 0a 49 8b d2 48 c1 fa 03 8b c2 c1 c1 13 35 00 00 00 00 3b c1",
    );
    put32(&mut bytes, code + 0xc55, 0x12345678 + field_shift);
    emit(
        &mut bytes,
        code + 0xc90,
        "49 8b c8 41 8b 10 c1 c2 13 48 c1 f9 03 33 d1 48 8b c8 48 c1 f9 03 33 d1
         49 8d 48 04 c1 ca 13 89 10 81 f2 00 00 00 00 89 50 fc",
    );
    put32(&mut bytes, code + 0xcb3, 0x87654321 + field_shift);

    put32(&mut bytes, 0x104, 16);
    put32(&mut bytes, 0x120, 0x2e00);
    put32(&mut bytes, 0x124, 5 * 12);
    for (i, (start, end)) in [
        (0x700, 0x780),
        (0x900, 0xa00),
        (0xc00, 0xc20),
        (0xc20, 0xd00),
        (0xd00, 0xd80),
    ]
    .into_iter()
    .enumerate()
    {
        put32(&mut bytes, 0x2e00 + i * 12, (code + start) as u32);
        put32(&mut bytes, 0x2e04 + i * 12, (code + end) as u32);
    }
    bytes
}

#[test]
fn discovers_relocated_bindings_changed_fields_and_encoding() {
    for (shift, fields) in [(0, 0), (0x80, 0x200)] {
        let actual = discover(&fixture(shift, fields)).unwrap();
        assert_eq!(
            actual,
            InventoryLayout {
                global_registry_rva: 0x3000 + shift as u64,
                global_registry_offset: 0x138 + fields as u64,
                resources: resources(0x3200 + shift as u64, u64::from(fields / 8)),
                profile_descriptor_rva: 0x3300 + shift as u64,
                sync_offset: 0x700 + fields as u64,
                misc_offset: 0x5d0 + fields as u64,
                recipes_offset: 0x5e0 + fields as u64,
                pending_offset: 0x800 + fields as u64,
                count_mask: 0x12345678 + fields,
                check_mask: 0x87654321 + fields,
                count_rotation: 19,
                address_shift: 3,
            }
        );
    }
}

#[test]
fn discovers_changed_rotation_and_address_salt() {
    let mut bytes = fixture(0, 0);
    for offset in [0x1c53, 0x1c98, 0x1cae] {
        bytes[offset] = 11;
    }
    for offset in [0x1c4e, 0x1c9c, 0x1ca5] {
        bytes[offset] = 4;
    }
    let layout = discover(&bytes).unwrap();
    assert_eq!(layout.count_rotation, 11);
    assert_eq!(layout.address_shift, 4);
}

#[test]
fn rejects_inconsistent_or_ambiguous_quantity_operations() {
    let mut bytes = fixture(0, 0);
    bytes[0x1cae] = 12;
    assert!(
        discover(&bytes)
            .unwrap_err()
            .contains("inconsistent native quantity")
    );
    let mut bytes = fixture(0, 0);
    bytes.copy_within(0x1c40..0x1c5b, 0x1d50);
    put32(&mut bytes, 0x1d65, 99);
    assert_eq!(
        discover(&bytes).unwrap_err(),
        "ambiguous stack quantity signature"
    );
}

#[test]
fn rejects_missing_anchors_wrong_calls_and_overlapping_fields() {
    let mut bytes = fixture(0, 0);
    bytes[0x2100] = b'X';
    assert_eq!(
        discover(&bytes).unwrap_err(),
        "MiscItems signature not found"
    );
    let mut bytes = fixture(0, 0);
    relative(&mut bytes, 0x175b, 0x1c10);
    assert_eq!(
        discover(&bytes).unwrap_err(),
        "inventory stack serializers disagree"
    );
    let mut bytes = fixture(0, 0);
    relative(&mut bytes, 0x1810, 0x1710);
    assert!(
        discover(&bytes)
            .unwrap_err()
            .contains("profile does not call")
    );
    let mut bytes = fixture(0, 0);
    put32(&mut bytes, 0x1904, 0x5d0);
    assert!(discover(&bytes).unwrap_err().contains("overlapping"));
}

#[test]
fn rejects_non_data_descriptors_and_unrelated_functions() {
    let mut bytes = fixture(0, 0);
    relative(&mut bytes, 0x132c, 0x1500);
    assert!(
        discover(&bytes)
            .unwrap_err()
            .contains("outside writable image data")
    );
    let mut bytes = fixture(0, 0);
    put32(&mut bytes, 0x2e04, 0x1740);
    assert_eq!(
        discover(&bytes).unwrap_err(),
        "inventory stack serializers disagree"
    );
    let mut bytes = fixture(0, 0);
    put32(&mut bytes, 0x2e0c, 0x1600);
    assert!(
        discover(&bytes)
            .unwrap_err()
            .contains("function table order")
    );
    assert!(discover(b"invalid PE").unwrap_err().contains("invalid PE"));
}
