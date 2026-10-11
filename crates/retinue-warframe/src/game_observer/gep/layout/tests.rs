use super::*;
use crate::game_observer::executable::fixture::{emit, image, put32, relative};

fn fixture(code_shift: usize, data_shift: usize, field_shift: u32) -> Vec<u8> {
    let mut bytes = image();
    let caller = 0x1100 + code_shift;
    let handler = 0x1200 + code_shift;
    let queue = handler + 0x23;
    let callback = handler + 0x200;
    emit(
        &mut bytes,
        caller,
        "48 8b d9 48 8b 0d 00 00 00 00 48 85 c9 74 08 48 8b d3 e8 00 00 00 00 c3",
    );
    relative(&mut bytes, caller + 6, 0x3100 + data_shift);
    relative(&mut bytes, caller + 19, handler);
    emit(&mut bytes, handler + 0x20, "4c 8b e9");
    emit(
        &mut bytes,
        queue,
        "4d 8b 8d b0 00 00 00 4d 85 c9 0f 84 00 00 00 00
         4d 8b 85 a8 00 00 00 49 8b 85 a0 00 00 00 49 8b c8 49 8b bd 98 00 00 00
         48 ff c8 48 d1 e9 48 23 c8 49 8b c0 83 e0 01 48 8b 3c cf 48 8b 3c c7
         49 8d 41 ff 49 89 85 b0 00 00 00 48 85 c0 75 05 45 33 c0 eb 03 49 ff c0
         48 8b cb 4d 89 85 a8 00 00 00 e8 00 00 00 00
         49 8d 8d 80 00 00 00 e8 00 00 00 00 4c 8d 67 18",
    );
    for (offset, value) in [
        (3, 0xb0),
        (19, 0xa8),
        (26, 0xa0),
        (36, 0x98),
        (70, 0xb0),
        (93, 0xa8),
        (105, 0x80),
    ] {
        put32(&mut bytes, queue + offset, value + field_shift);
    }
    bytes[queue + 117] = 0x18 + field_shift as u8;
    emit(
        &mut bytes,
        callback,
        "49 8b 44 24 50 4d 8d 44 24 38 41 0f b6 54 24 18 49 8d 4c 24 50 ff 50 20 c3",
    );
    for offset in [4, 9, 15, 20] {
        bytes[callback + offset] += field_shift as u8;
    }
    put32(&mut bytes, 0x104, 16);
    put32(&mut bytes, 0x120, 0x2800);
    put32(&mut bytes, 0x124, 12);
    put32(&mut bytes, 0x2800, handler as u32);
    put32(&mut bytes, 0x2804, (callback + 25) as u32);
    bytes
}

#[test]
fn discovers_relocated_code_data_and_fields() {
    for (code, data, fields) in [(0, 0, 0), (0x200, 0x180, 16)] {
        let layout = discover(&fixture(code, data, fields)).unwrap();
        assert_eq!(layout.manager_rva, 0x3100 + data as u64);
        assert_eq!(
            layout.response,
            ResponsePath {
                queue_table: u64::from(0x98 + fields),
                item_base: u64::from(0x18 + fields),
                body: u64::from(0x38 + fields),
            }
        );
    }
}

#[test]
fn rejects_missing_and_conflicting_managers() {
    let mut bytes = fixture(0, 0, 0);
    bytes[0x1100] = 0x90;
    assert!(
        discover(&bytes)
            .unwrap_err()
            .contains("HTTP manager signature not found")
    );
    let mut bytes = fixture(0, 0, 0);
    bytes.copy_within(0x1100..0x1117, 0x1600);
    relative(&mut bytes, 0x1606, 0x3108);
    relative(&mut bytes, 0x1613, 0x1200);
    assert!(
        discover(&bytes)
            .unwrap_err()
            .contains("ambiguous HTTP manager")
    );
    relative(&mut bytes, 0x1606, 0x3100);
    assert!(discover(&bytes).is_ok());
}

#[test]
fn rejects_invalid_image_bindings() {
    for target in [0x1800, 0x2800, 0x3101, 0x3ffc, 0x8000] {
        let mut bytes = fixture(0, 0, 0);
        relative(&mut bytes, 0x1106, target);
        assert!(discover(&bytes).is_err(), "manager {target:x}");
    }
    for target in [0x1201, 0x1418, 0x3000] {
        let mut bytes = fixture(0, 0, 0);
        relative(&mut bytes, 0x1113, target);
        assert!(discover(&bytes).is_err(), "handler {target:x}");
    }
    let mut bytes = fixture(0, 0, 0);
    put32(&mut bytes, 0x2804, 0x2100);
    assert!(discover(&bytes).unwrap_err().contains("extent"));
}

#[test]
fn rejects_changed_queue_relationships() {
    for offset in [3, 19, 26, 36, 70, 93, 105] {
        let mut bytes = fixture(0, 0, 0);
        put32(&mut bytes, 0x1223 + offset, 0x100);
        assert!(discover(&bytes).unwrap_err().contains("accessors disagree"));
    }
    for offset in [0x1220, 0x1223, 0x1256] {
        let mut bytes = fixture(0, 0, 0);
        bytes[offset] ^= 1;
        assert!(discover(&bytes).is_err());
    }
}

#[test]
fn rejects_changed_callback_relationships() {
    for (offset, value) in [(4, 0x58), (9, 0x48), (15, 0x40), (20, 0x58), (23, 0x81)] {
        let mut bytes = fixture(0, 0, 0);
        bytes[0x1400 + offset] = value;
        assert!(discover(&bytes).unwrap_err().contains("callback layout"));
    }
    let mut bytes = fixture(0, 0, 0);
    bytes[0x1223 + 117] = 0xf8;
    assert!(discover(&bytes).is_err());
}

#[test]
fn rejects_conflicting_layouts_within_handler() {
    let mut bytes = fixture(0, 0, 0);
    bytes.copy_within(0x1400..0x1418, 0x13b0);
    bytes[0x13b9] = 0x30;
    assert!(
        discover(&bytes)
            .unwrap_err()
            .contains("ambiguous HTTP response callback")
    );
    let mut bytes = fixture(0, 0, 0);
    bytes.copy_within(0x1220..0x1299, 0x1300);
    bytes[0x1303 + 117] = 0x20;
    assert!(
        discover(&bytes)
            .unwrap_err()
            .contains("ambiguous HTTP response queue")
    );
}

#[test]
fn does_not_scan_past_handler_or_into_data() {
    let mut bytes = fixture(0, 0, 0);
    put32(&mut bytes, 0x2804, 0x1400);
    assert!(
        discover(&bytes)
            .unwrap_err()
            .contains("callback signature not found")
    );
    let mut bytes = fixture(0, 0, 0);
    bytes.copy_within(0x1100..0x1117, 0x2500);
    bytes[0x1100] = 0x90;
    assert!(
        discover(&bytes)
            .unwrap_err()
            .contains("manager signature not found")
    );
}
