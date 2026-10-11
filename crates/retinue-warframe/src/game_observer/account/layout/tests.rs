use super::*;
use crate::game_observer::executable::fixture::{emit, image, put32, relative};

pub(in crate::game_observer::account) fn fixture(shift: usize, fields: u32) -> Vec<u8> {
    let mut bytes = image();
    let code = 0x1000 + shift;
    let label = b"Setting gGameRules\n\0";
    bytes[0x2000..0x2000 + label.len()].copy_from_slice(label);
    emit(
        &mut bytes,
        code,
        "48 8d 15 00 00 00 00 48 8b c8 e8 00 00 00 00 e8 00 00 00 00
         48 8d 55 00 41 b8 11 22 33 44 48 8b c8 e8 00 00 00 00",
    );
    relative(&mut bytes, code + 3, 0x2000);
    relative(&mut bytes, code + 16, code + 0x100);
    relative(&mut bytes, code + 34, code + 0x200);
    emit(
        &mut bytes,
        code + 0x100,
        "48 83 ec 28 48 8d 05 00 00 00 00 48 83 c4 28 c3",
    );
    relative(&mut bytes, code + 0x107, 0x3000 + shift);
    emit(
        &mut bytes,
        code + 0x200,
        "4c 8d b3 00 00 00 00 4d 8b 16 45 8b 46 08 49 8b da 4f 8d 0c 10
         49 c1 e8 04 90 48 8b 3e 4c 8b 74 24 58 48 39 7b 08",
    );
    put32(&mut bytes, code + 0x203, 0x138 + fields);

    let seed = code + 0x408;
    emit(
        &mut bytes,
        seed,
        "0f b6 91 00 00 00 00 80 fa ff 75 00 8b 81 00 00 00 00
         48 a9 ff ff ff 0f b8 00 00 00 00 eb 00 33 c0 80 fa 0f 0f 95 c0
         4c 8d 81 00 00 00 00 48 c1 e0 05 ba 0f 00 00 00 4c 03 c0
         4d 0f be 48 0f 49 2b d1 41 80 f9 ff 75 00 41 8b 40 08
         25 ff ff ff 0f eb 00 48 8b c2 48 83 f8 08 0f 82 00 00 00 00
         b9 06 00 00 00 41 80 f9 ff 75 00 48 3b c1 49 8d 50 02
         48 0f 47 c1 41 b8 10 00 00 00 33 d2 e8 00 00 00 00",
    );
    put32(&mut bytes, seed + 3, 0x147 + fields);
    put32(&mut bytes, seed + 14, 0x140 + fields);
    put32(&mut bytes, seed + 42, 0x118 + fields);
    let selector = code + 0x808;
    emit(
        &mut bytes,
        selector,
        "48 8b 99 00 00 00 00 48 8b f2 8b 81 00 00 00 00 48 8b f9 48 03 c3
         48 3b d8 74 00 48 8b 03 48 8b 08 48 85 c9 74 00 48 8b 01 ff 50 18
         48 3b c6 74 00 8b 87 00 00 00 00 48 83 c3 08 48 03 87 00 00 00 00
         48 3b d8 75 00",
    );
    for (position, value) in [
        (3, 0x230 + fields),
        (12, 0x238 + fields),
        (51, 0x238 + fields),
        (62, 0x230 + fields),
    ] {
        put32(&mut bytes, selector + position, value);
    }
    bytes[selector + 43] = 0x18 + fields as u8;
    put32(&mut bytes, 0x104, 16);
    put32(&mut bytes, 0x120, 0x2e00);
    put32(&mut bytes, 0x124, 24);
    for (index, (start, end)) in [(0x400, 0x500), (0x800, 0x900)].into_iter().enumerate() {
        put32(&mut bytes, 0x2e00 + index * 12, (code + start) as u32);
        put32(&mut bytes, 0x2e04 + index * 12, (code + end) as u32);
    }
    bytes
}

#[test]
fn derives_relocated_code_registry_fields_and_vtable_slot() {
    for (shift, fields) in [(0, 0), (0x80, 0x40)] {
        let found = discover(&fixture(shift, fields)).unwrap();
        assert_eq!(found.registry_rva, 0x3000 + shift as u64);
        assert_eq!(found.registry_offset, 0x138 + u64::from(fields));
        assert_eq!(found.selector_rva, 0x1800 + shift as u64);
        assert_eq!(found.seed_getter_rva, 0x1400 + shift as u64);
        assert_eq!(found.profiles_offset, 0x230 + u64::from(fields));
        assert_eq!(found.primary_id_offset, 0x118 + u64::from(fields));
        assert_eq!(found.platform_id_offset, 0x138 + u64::from(fields));
        assert_eq!(found.identity_slot, 0x18 + u64::from(fields));
    }
}

#[test]
fn rejects_inconsistent_fields_and_changed_registry_shape() {
    for (offset, value, reason) in [
        (0x1808 + 12, 0x248, "inconsistent player profile vector"),
        (0x1408 + 14, 0x148, "account identifier accessors disagree"),
        (0x1203, 0x139, "invalid registry vector offset"),
    ] {
        let mut bytes = fixture(0, 0);
        put32(&mut bytes, offset, value);
        assert_eq!(discover(&bytes).unwrap_err(), reason);
    }
    let mut bytes = fixture(0, 0);
    bytes[0x1218] = 3;
    assert!(
        discover(&bytes)
            .unwrap_err()
            .contains("registry vector layout")
    );
}

#[test]
fn rejects_ambiguous_native_methods() {
    let mut bytes = fixture(0, 0);
    bytes.copy_within(0x1400..0x1500, 0x1600);
    assert!(
        discover(&bytes)
            .unwrap_err()
            .contains("ambiguous account seed getter")
    );
}
