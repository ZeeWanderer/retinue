use super::*;

fn constructor_bytes(shift: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    for offset in [0x78, 0x8c, 0xa0, 0xb4, 0xd8, 0xe0, 0xe8, 0xf0] {
        bytes.extend([0x48, 0xc7, 0x83]);
        bytes.extend((offset + shift).to_le_bytes());
        bytes.extend(0x3f800000_u32.to_le_bytes());
    }
    for (opcode, offset) in [
        ([0x48, 0x89, 0xab], 0xb8),
        ([0x48, 0x89, 0xbb], 0xc0),
        ([0x40, 0x88, 0xab], 0xc8),
    ] {
        bytes.extend(opcode);
        bytes.extend((offset + shift).to_le_bytes());
    }
    bytes.extend([0xc6, 0x83]);
    bytes.extend((0xd7 + shift).to_le_bytes());
    bytes.push(15);
    bytes.extend([0x0f, 0xb6, 0x83]);
    bytes.extend((0x127 + shift).to_le_bytes());
    bytes.extend([0x88, 0x83]);
    bytes.extend((0x127 + shift).to_le_bytes());
    bytes.push(0xc3);
    bytes
}

#[test]
fn derives_shifted_layout_without_addresses() {
    for shift in [0, 0x40] {
        let layout = constructor(&constructor_bytes(shift)).unwrap();
        assert_eq!(layout.matrix, 0x78 + shift as usize);
        assert_eq!(layout.parent_control, 0xb8 + shift as usize);
        assert_eq!(layout.parent, 0xc0 + shift as usize);
        assert_eq!(layout.name, 0xc8 + shift as usize);
        assert_eq!(layout.alpha, 0xf0 + shift as usize);
        assert_eq!(layout.flags, 0x127 + shift as usize);
    }
}

#[test]
fn rejects_changed_or_ambiguous_constructor() {
    let mut bytes = constructor_bytes(0);
    bytes[3] += 1;
    assert!(constructor(&bytes).is_err());
    let mut bytes = constructor_bytes(0);
    bytes.splice(0..0, [0x0f, 0xb6, 0x83, 0x24, 1, 0, 0]);
    assert!(constructor(&bytes).is_err());
}

#[test]
fn accepts_only_plain_native_accessors() {
    assert_eq!(
        pointer_accessor(&[0x48, 0x8b, 0x81, 0x30, 1, 0, 0, 0xc3]),
        Some(0x130)
    );
    assert_eq!(
        pointer_accessor(&[0x48, 0x8b, 0x81, 0x38, 1, 0, 0, 0xc3]),
        Some(0x138)
    );
    assert_eq!(
        pointer_accessor(&[0x48, 0x8b, 0x81, 0x30, 1, 0, 0, 0x90, 0xc3]),
        None
    );
    assert_eq!(
        rect_accessor(&[0x0f, 0x10, 0x41, 0x50, 0x0f, 0x11, 2, 0xc3]),
        Some(0x50)
    );
    assert_eq!(
        rect_accessor(&[0x0f, 0x10, 0x81, 0x80, 0, 0, 0, 0x0f, 0x11, 2, 0xc3]),
        Some(0x80)
    );
    assert_eq!(
        rect_accessor(&[0x0f, 0x10, 0x41, 0x50, 0x0f, 0x11, 3, 0xc3]),
        None
    );
}
