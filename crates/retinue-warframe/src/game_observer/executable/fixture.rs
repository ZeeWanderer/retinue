pub(crate) fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn emit(bytes: &mut [u8], offset: usize, hex: &str) {
    for (index, byte) in hex.split_whitespace().enumerate() {
        bytes[offset + index] = u8::from_str_radix(byte, 16).unwrap();
    }
}

pub(crate) fn relative(bytes: &mut [u8], offset: usize, target: usize) {
    put32(bytes, offset, (target as i32 - offset as i32 - 4) as u32);
}

pub(crate) fn image() -> Vec<u8> {
    let mut bytes = vec![0; 0x4000];
    emit(&mut bytes, 0, "4d 5a");
    put32(&mut bytes, 0x3c, 0x80);
    emit(&mut bytes, 0x80, "50 45 00 00 64 86 03 00");
    emit(&mut bytes, 0x94, "f0 00 00 00 0b 02");
    put32(&mut bytes, 0x9c, 0x1000);
    put32(&mut bytes, 0xac, 0x1000);
    put32(&mut bytes, 0xd0, 0x4000);
    put32(&mut bytes, 0xd4, 0x1000);
    for (index, flags) in [0x6000_0020, 0x4000_0040, 0xc000_0040]
        .into_iter()
        .enumerate()
    {
        let header = 0x188 + index * 40;
        let address = (index as u32 + 1) * 0x1000;
        for (offset, value) in [
            (8, 0x1000),
            (12, address),
            (16, 0x1000),
            (20, address),
            (36, flags),
        ] {
            put32(&mut bytes, header + offset, value);
        }
    }
    bytes
}

pub(crate) fn registry_setter(bytes: &mut [u8], address: usize, offset: u32) {
    emit(
        bytes,
        address,
        "4c 8d b3 00 00 00 00 4d 8b 16 45 8b 46 08 49 8b da 4f 8d 0c 10
         49 c1 e8 04 90 48 8b 3e 4c 8b 74 24 58 48 39 7b 08",
    );
    put32(bytes, address + 3, offset);
}

pub(crate) fn resources(strings: u64, fields: u64) -> super::ResourceLayout {
    super::ResourceLayout {
        string_blocks_rva: strings,
        object_type_offset: 8 + fields,
        object_holder_offset: 16 + fields,
        parent_offset: 24 + fields,
        name_prefix_offset: 16 + fields,
        name_leaf_offset: 44 + fields,
    }
}

pub(crate) fn resource_code(bytes: &mut [u8], address: usize, layout: super::ResourceLayout) {
    emit(
        bytes,
        address,
        "33 d2 89 47 00 48 8d 05 00 00 00 00 89 57 0c
         48 89 57 00 89 57 24 48 89 77 00 48 8b 74 24 48 c3",
    );
    bytes[address + 4] = layout.name_leaf_offset as u8;
    bytes[address + 18] = layout.name_prefix_offset as u8;
    bytes[address + 25] = layout.parent_offset as u8;
    let header = address + 0x100;
    emit(
        bytes,
        header,
        "33 d2 48 8d 05 00 00 00 00 48 89 01 48 8d 05 00 00 00 00
         48 89 41 00 48 8b c1 48 89 51 00 ff 05 00 00 00 00
         c7 41 18 ff ff ff ff 89 51 1c 80 61 20 e0 88 51 21 c7 41 22 01 00 00 00 c3",
    );
    bytes[header + 22] = layout.object_holder_offset as u8;
    bytes[header + 29] = layout.object_type_offset as u8;
    relative(bytes, header + 15, 0x3c00);
    relative(bytes, header + 32, 0x3c08);
    let name = address + 0x200;
    emit(
        bytes,
        name,
        "48 8b 41 00 48 85 c0 74 0e 8b 00 89 02 8b 41 00 89 42 04 48 8b c2 c3
         89 02 8b 41 00 89 42 04 48 8b c2 c3",
    );
    bytes[name + 3] = layout.name_prefix_offset as u8;
    bytes[name + 15] = layout.name_leaf_offset as u8;
    bytes[name + 27] = layout.name_leaf_offset as u8;
}
