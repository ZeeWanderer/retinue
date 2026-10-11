use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use crate::game_observer::executable::fixture::{emit, put32};

fn memory(bytes: &[u8]) -> File {
    static ID: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "wfcli-account-{}-{}",
        std::process::id(),
        ID.fetch_add(1, Ordering::Relaxed)
    ));
    let file = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    std::fs::remove_file(path).unwrap();
    file.write_all_at(bytes, 0).unwrap();
    file
}

fn put64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn vector(bytes: &mut [u8], offset: usize, pointer: u64, size: u32) {
    put64(bytes, offset, pointer);
    put32(bytes, offset + 8, size);
    put32(bytes, offset + 12, size);
}

fn string(bytes: &mut [u8], offset: usize, value: &[u8]) {
    assert!(value.len() <= 15);
    bytes[offset..offset + 16].fill(0);
    bytes[offset..offset + value.len()].copy_from_slice(value);
    bytes[offset + 15] = 15 - value.len() as u8;
}

fn fixture() -> (Reader, Vec<u8>) {
    let mut bytes = layout::tests::fixture(0, 0);
    let layout = layout::discover(&bytes).unwrap();
    bytes.resize(0x7000, 0);
    vector(
        &mut bytes,
        0x3000 + layout.registry_offset as usize,
        0x3300,
        16,
    );
    put32(&mut bytes, 0x3300, 0xdeadbeef);
    put64(&mut bytes, 0x3308, 0x3400);
    put64(&mut bytes, 0x3400, 0x4000);
    put64(&mut bytes, 0x4000, 0x3600);
    put64(&mut bytes, 0x3600, layout.selector_rva);
    vector(
        &mut bytes,
        0x4000 + layout.profiles_offset as usize,
        0x3500,
        16,
    );
    for (entry, holder, object, identity) in
        [(0x3500, 0x3520, 0x5000, 1), (0x3508, 0x3528, 0x6000, 0)]
    {
        put64(&mut bytes, entry, holder as u64);
        put64(&mut bytes, holder, object as u64);
        put64(&mut bytes, object, 0x3800);
        put64(&mut bytes, object + 0x1d8, identity);
        string(
            &mut bytes,
            object + layout.platform_id_offset as usize,
            b"zzabcdef",
        );
        string(
            &mut bytes,
            object + layout.primary_id_offset as usize,
            b"zz123456",
        );
    }
    for slot in 0..4 {
        put64(&mut bytes, 0x3800 + slot * 8, layout.seed_getter_rva);
    }
    put64(&mut bytes, 0x3800 + layout.identity_slot as usize, 0x1c00);
    emit(&mut bytes, 0x1c00, "48 8b 81 d8 01 00 00 c3");
    (Reader { base: 0, layout }, bytes)
}

#[test]
fn selects_primary_profile_not_first_vector_entry() {
    let (reader, mut bytes) = fixture();
    string(
        &mut bytes,
        0x5000 + reader.layout.platform_id_offset as usize,
        b"zz777777",
    );
    assert_eq!(reader.read(&memory(&bytes)).unwrap(), 0xabcdef);
    string(
        &mut bytes,
        0x6000 + reader.layout.platform_id_offset as usize,
        b"",
    );
    assert_eq!(reader.read(&memory(&bytes)).unwrap(), 0x123456);
}

#[test]
fn decodes_identity_field_from_live_getter() {
    let (reader, mut bytes) = fixture();
    put32(&mut bytes, 0x1c03, 0x280);
    put64(&mut bytes, 0x5280, 1);
    put64(&mut bytes, 0x61d8, 1);
    assert_eq!(reader.read(&memory(&bytes)).unwrap(), 0xabcdef);
}

#[test]
fn rejects_duplicate_primary_profiles_and_managers() {
    let (reader, mut bytes) = fixture();
    put64(&mut bytes, 0x51d8, 0);
    assert!(
        reader
            .read(&memory(&bytes))
            .unwrap_err()
            .to_string()
            .contains("multiple primary")
    );
    put64(&mut bytes, 0x51d8, 1);
    vector(
        &mut bytes,
        0x3000 + reader.layout.registry_offset as usize,
        0x3300,
        32,
    );
    put64(&mut bytes, 0x3318, 0x3400);
    assert!(
        reader
            .read(&memory(&bytes))
            .unwrap_err()
            .to_string()
            .contains("multiple player profile managers")
    );
}

#[test]
fn rejects_invalid_getters_and_pointer_overflow() {
    for (offset, value) in [
        (0x3600, 0x1900),
        (0x5000, 0x3a00),
        (0x3818, u64::MAX),
        (0x4000, u64::MAX),
        (0x3508, u64::MAX),
    ] {
        let (reader, mut bytes) = fixture();
        put64(&mut bytes, offset, value);
        assert!(reader.read(&memory(&bytes)).is_err(), "offset {offset:x}");
    }
    let (reader, mut bytes) = fixture();
    bytes[0x1c00] = 0x90;
    assert!(
        reader
            .read(&memory(&bytes))
            .unwrap_err()
            .to_string()
            .contains("identity getter")
    );
}

#[test]
fn handles_inline_and_heap_strings_without_accepting_invalid_identifiers() {
    let (reader, mut bytes) = fixture();
    let address = 0x6000 + reader.layout.platform_id_offset as usize;
    let identifier = b"zz654321-account-identifier";
    bytes[0x6800..0x6800 + identifier.len()].copy_from_slice(identifier);
    put64(&mut bytes, address, 0x6800);
    put32(
        &mut bytes,
        address + 8,
        0xf000_0000 | identifier.len() as u32,
    );
    bytes[address + 15] = 0xff;
    assert_eq!(reader.read(&memory(&bytes)).unwrap(), 0x654321);
    put32(&mut bytes, address + 8, 257);
    assert!(reader.read(&memory(&bytes)).is_err());
    for value in [
        b"short".as_slice(),
        b"zzabcdef",
        b"zz12#456",
        b"zzABCDEFanything",
    ] {
        assert_eq!(
            seed(value).is_some(),
            value.starts_with(b"zzabcdef") || value.starts_with(b"zzABCDEF")
        );
    }
}

#[test]
fn detects_vector_mutation_and_rejects_invalid_bounds() {
    let file = memory(&[0; 128]);
    let mut bytes = [0; 128];
    vector(&mut bytes, 0, 32, 16);
    file.write_all_at(&bytes, 0).unwrap();
    let snapshot = Vector::read(&file, 0, 8, 32).unwrap();
    file.write_all_at(&[1], 32).unwrap();
    assert!(snapshot.verify(&file).is_err());
    for (pointer, size, capacity) in [(32, 7, 16), (32, 16, 8), (32, 16, 4096), (u64::MAX, 16, 16)]
    {
        vector(&mut bytes, 0, pointer, size);
        put32(&mut bytes, 12, capacity);
        assert!(Vector::read(&memory(&bytes), 0, 8, 32).is_err());
    }
}
