use super::*;

fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn relative(bytes: &mut [u8], at: usize, target: usize) {
    put32(bytes, at, (target as i32 - at as i32 - 4) as u32);
}

fn emit(bytes: &mut [u8], at: usize, code: &str) {
    for (i, word) in code.split_whitespace().enumerate() {
        bytes[at + i] = u8::from_str_radix(word, 16).unwrap();
    }
}

fn fixture(code_shift: usize, data_shift: usize, fields: u32) -> Vec<u8> {
    let mut bytes = vec![0; 0x4000];
    emit(&mut bytes, 0, "4d 5a");
    put32(&mut bytes, 0x3c, 0x80);
    emit(&mut bytes, 0x80, "50 45 00 00 64 86 03 00");
    emit(&mut bytes, 0x94, "f0 00 00 00 0b 02");
    put32(&mut bytes, 0x9c, 0x1000);
    put32(&mut bytes, 0xac, 0x1000);
    put32(&mut bytes, 0xd0, 0x4000);
    put32(&mut bytes, 0xd4, 0x1000);
    for (i, flags) in [0x6000_0020, 0x4000_0040, 0xc000_0040]
        .into_iter()
        .enumerate()
    {
        let header = 0x188 + i * 40;
        let rva = (i as u32 + 1) * 0x1000;
        for (offset, value) in [(8, 0x1000), (12, rva), (16, 0x1000), (20, rva), (36, flags)] {
            put32(&mut bytes, header + offset, value);
        }
    }
    let root = 0x1100 + code_shift;
    emit(
        &mut bytes,
        root,
        "57 48 83 ec 40 48 8b f1 33 ed 48 39 2d 00 00 00 00
        0f 85 22 01 00 00 b9 48 01 00 00 e8 00 00 00 00 48 8b f8",
    );
    relative(&mut bytes, root + 13, 0x3100 + data_shift);
    let ingame = root + 0x100;
    emit(
        &mut bytes,
        ingame,
        "48 8b 8e 90 02 00 00 ba 27 05 00 00 48 81 c1 28 01 00 00
        e8 00 00 00 00 85 c0 0f 85 11 01 00 00",
    );
    put32(&mut bytes, ingame + 3, 0x290 + fields);
    let server = root + 0x200;
    emit(
        &mut bytes,
        server,
        "48 c1 e9 20 49 d3 e0 49 03 d0 48 f7 da f0 48 01 15 00 00 00 00
        4d 2b c8 49 8b c9 e8 00 00 00 00 84 db 0f 84 22 11 00 00 49 8b 87 b0 05 00 00",
    );
    put32(&mut bytes, server + 43, 0x5b0 + fields);
    let gold = root + 0x300;
    emit(
        &mut bytes,
        gold,
        "81 79 08 fb d6 a0 dd 75 11 48 8b 82 b0 01 00 00
        8b 88 d0 39 00 00 eb 09 c7 42 10 00 00 00 00",
    );
    put32(&mut bytes, gold + 12, 0x1b0 + fields);
    put32(&mut bytes, gold + 18, 0x39d0 + fields);
    bytes[gold + 31..gold + 40].fill(0xcc);
    let visibility = root + 0x400;
    emit(
        &mut bytes,
        visibility,
        "66 0f 6e c0 8b 82 68 01 00 00 0f 5b c0 c1 e8 07 f3 0f 59 c1 a8 01 74 09",
    );
    put32(&mut bytes, visibility + 6, 0x168 + fields);
    let area = root + 0x600;
    emit(
        &mut bytes,
        area,
        "83 3d 00 00 00 00 ff 75 12 48 8d 05 00 00 00 00 48 89 05 00 00 00 00
        48 8d 0d 00 00 00 00 33 c0 48 89 05 00 00 00 00 48 89 05 00 00 00 00 e8 00 00 00 00
        48 8d 0d 00 00 00 00 e8 00 00 00 00 48 8d 05 00 00 00 00 48 83 c4 28 c3",
    );
    relative(&mut bytes, area + 66, 0x3300 + data_shift);
    bytes
}

#[test]
fn discovers_relocated_code_data_and_fields() {
    for (code, data, fields) in [(0, 0, 0), (0x200, 0x180, 16)] {
        let layout = Layout::discover(&fixture(code, data, fields)).unwrap();
        assert_eq!(layout.root_rva, 0x3100 + data as u64);
        let panels = layout.panels.unwrap();
        assert_eq!(panels.root_rva, 0x3300 + data as u64);
        assert_eq!(panels.flags_offset, 0x168 + fields);
        assert_eq!(layout.ingame_data_offset, 0x290 + fields);
        let gold = layout.gold.unwrap();
        assert_eq!(gold.server_offset, 0x5b0 + fields);
        assert_eq!(gold.data_offset, 0x1b0 + fields);
        assert_eq!(gold.value_offset, 0x39d0 + fields);
    }
}

#[test]
fn component_names_are_decoded_from_literal_return_getters() {
    let mut bytes = fixture(0, 0, 0);
    bytes[0x2200..0x2207].copy_from_slice(b"Player\0");
    emit(&mut bytes, 0x1900, "48 8d 05 00 00 00 00 c3");
    relative(&mut bytes, 0x1903, 0x2200);
    assert_eq!(
        Layout::discover(&bytes)
            .unwrap()
            .component_getters
            .get(&0x1900)
            .unwrap(),
        "Player"
    );
    bytes[0x1907] = 0x90;
    assert!(
        Layout::discover(&bytes)
            .unwrap()
            .component_getters
            .is_empty()
    );
}

#[test]
fn rejects_missing_ambiguous_or_invalid_bindings() {
    let mut bytes = fixture(0, 0, 0);
    bytes[0x1100] = 0;
    assert!(
        Layout::discover(&bytes)
            .unwrap_err()
            .contains("game root signature not found")
    );
    let mut bytes = fixture(0, 0, 0);
    bytes.copy_within(0x1100..0x1124, 0x1900);
    assert!(Layout::discover(&bytes).unwrap_err().contains("ambiguous"));
    for target in [0x1100, 0x2100, 0x3101, 0x4000] {
        let mut bytes = fixture(0, 0, 0);
        relative(&mut bytes, 0x110d, target);
        assert!(Layout::discover(&bytes).is_err());
    }
}

#[test]
fn excessive_fields_are_rejected() {
    for field in [0, 1, 0x10000, u32::MAX] {
        let mut bytes = fixture(0, 0, 0);
        put32(&mut bytes, 0x1203, field);
        assert!(Layout::discover(&bytes).is_err());
    }
}

#[test]
fn unrelated_bindings_do_not_disable_state_and_player_observation() {
    let mut bytes = fixture(0, 0, 0);
    bytes[0x1300] = 0;
    bytes[0x1500] = 0;
    let layout = Layout::discover(&bytes).unwrap();
    assert_eq!(layout.root_rva, 0x3100);
    assert!(layout.gold.unwrap_err().contains("server data"));
    assert!(layout.panels.unwrap_err().contains("UI visibility"));
}
