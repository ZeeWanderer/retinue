use super::*;
use crate::game_observer::executable::fixture::{emit, put32, relative};

pub(crate) fn expected(shift: u64) -> (StoreFields, ManifestFields) {
    (
        StoreFields {
            category: 0x155 + shift,
            resource: 0x28 + shift,
            description: 0xfc + shift,
            flags: 0x15c + shift,
            exclusion: 0x15d + shift,
            start: 0xe0 + shift,
            expiry: 0xe8 + shift,
        },
        ManifestFields {
            store_vector: 0x38 + shift / 2,
            store_stride: 16 + shift / 4,
            variant_vector: 0x38 + shift / 2,
            variant_stride: 0x68 + shift / 8,
            variant_modes: 8 + shift / 8,
        },
    )
}

pub(crate) fn fixture(bytes: &mut [u8], code: usize, shift: u64) {
    let (store, manifests) = expected(shift);
    let names = [
        "mProductCategory",
        "mLocalizeDescTag",
        "mTypeName",
        "mShowInMarket",
        "mTintIconToTheme",
        "mExcludeFromCodex",
        "mEmbargoLiftDate",
        "mExpiryDate",
        "mStoreItems",
        "mWeaponVariantMap",
    ];
    for (index, name) in names.iter().enumerate() {
        let record = 0x2100 + index * 0x40;
        bytes[record + 7] = 0xa0;
        bytes[record + 8..record + 16].copy_from_slice(&((record + 16) as u64).to_le_bytes());
        bytes[record + 16..record + 16 + name.len()].copy_from_slice(name.as_bytes());
    }
    let mut cursor = code + 0x900;
    for (index, body, displacement, value, wide) in [
        (
            0,
            "48 8b cb ff 90 38 01 00 00 48 8d 97 00 00 00 00 48 8b cb e8 00 00 00 00",
            12,
            store.category,
            true,
        ),
        (
            1,
            "48 8b cb ff 90 38 01 00 00 48 8b 03 48 8d 97 00 00 00 00 48 8b cb ff 90 20 01 00 00",
            15,
            store.description,
            true,
        ),
        (
            2,
            "48 8b cb ff 90 38 01 00 00 48 8d 4f 00 48 8b d3",
            12,
            store.resource,
            false,
        ),
        (
            3,
            "48 8b cb ff 90 38 01 00 00 0f b6 87 00 00 00 00 48 8d 55 30 c0 e8 05 48 8b cb 24 01",
            12,
            store.flags,
            true,
        ),
        (
            4,
            "ff 90 38 01 00 00 48 8d b7 00 00 00 00 48 8b cb 0f b6 06 48 8d 55 30 d0 e8 24 01",
            9,
            store.exclusion,
            true,
        ),
        (
            5,
            "48 8b cb ff 90 38 01 00 00 0f b6 06 48 8d 55 30 24 01 48 8b cb",
            0,
            0,
            false,
        ),
        (
            6,
            "48 8b cb ff 90 38 01 00 00 48 8d 97 00 00 00 00 48 8b cb e8 00 00 00 00",
            12,
            store.start,
            true,
        ),
        (
            7,
            "48 8b cb ff 90 38 01 00 00 48 8d 97 00 00 00 00 48 8b cb e8 00 00 00 00",
            12,
            store.expiry,
            true,
        ),
    ] {
        emit(bytes, cursor, "48 8d 15 00 00 00 00");
        relative(bytes, cursor + 3, 0x2100 + index * 0x40);
        emit(bytes, cursor + 7, body);
        if wide {
            put32(bytes, cursor + 7 + displacement, value as u32);
        } else if value != 0 {
            bytes[cursor + 7 + displacement] = value as u8;
        }
        cursor += 7 + body.split_whitespace().count();
    }
    assert!(cursor <= code + 0xa10);
    put32(bytes, code + 0x303, store.start as u32);
    put32(bytes, code + 0x325, store.expiry as u32);
    put32(bytes, code + 0x342, store.flags as u32);

    let at = code + 0xa20;
    emit(
        bytes,
        at,
        "48 8d 15 00 00 00 00 48 8b cf ff 90 38 01 00 00 48 8d 53 00 45 33 c0 48 8b cf e8 00 00 00 00",
    );
    relative(bytes, at + 3, 0x2300);
    bytes[at + 19] = manifests.store_vector as u8;
    relative(bytes, at + 27, code + 0xb00);
    let at = code + 0xa60;
    emit(
        bytes,
        at,
        "48 8d 15 00 00 00 00 49 8b cc ff 90 38 01 00 00 48 8d 56 00 49 8b cc e8 00 00 00 00",
    );
    relative(bytes, at + 3, 0x2340);
    bytes[at + 19] = manifests.variant_vector as u8;
    relative(bytes, at + 24, code + 0xb80);
    let at = code + 0xb00;
    emit(
        bytes,
        at,
        "48 8b 1f 8b 7f 08 48 03 fb 4c 8d 05 00 00 00 00 48 8b d6 48 8b cb e8 00 00 00 00 48 83 c3 00 48 3b df 75 00",
    );
    relative(bytes, at + 12, 0x3600 + (code - 0x1000));
    bytes[at + 30] = manifests.store_stride as u8;
    emit(bytes, at + 0x40, "8b 47 08 48 c1 e8 00 48 89 44 24 28");
    bytes[at + 0x46] = manifests.store_stride.trailing_zeros() as u8;
    let at = code + 0xb80;
    emit(
        bytes,
        at,
        "48 8b 1f 8b 7f 08 48 03 fb 48 8b 03 48 8d 53 00 48 8d 4d af 48 89 45 a7 e8 00 00 00 00 48 8d 55 a7 48 8b ce e8 00 00 00 00 48 8b 06 48 8b ce ff 50 60 48 8d 4d af e8 00 00 00 00 48 83 c3 00 48 3b df 75 00",
    );
    bytes[at + 15] = manifests.variant_modes as u8;
    bytes[at + 62] = manifests.variant_stride as u8;
}

#[test]
fn rejects_missing_names_disagreement_and_invalid_fields() {
    let original = super::super::tests::fixture(0, 1);
    for (offset, value, error) in [
        (
            0x2150,
            u32::from(b'X'),
            "mLocalizeDescTag signature not found",
        ),
        (0x1913, 0xe0, "invalid or overlapping StoreItem fields"),
        (0x1303, 0xe8, "StoreItem accessors disagree"),
    ] {
        let mut bytes = original.clone();
        put32(&mut bytes, offset, value);
        assert_eq!(
            discover(PeFile::from_bytes(&bytes).unwrap()).unwrap_err(),
            error
        );
    }
    let mut bytes = original.clone();
    bytes[0x1b1e] = 24;
    assert_eq!(
        discover(PeFile::from_bytes(&bytes).unwrap()).unwrap_err(),
        "StoreManifest entry sizes disagree"
    );
    let mut bytes = original;
    relative(&mut bytes, 0x1b0c, 0x3500);
    assert_eq!(
        discover(PeFile::from_bytes(&bytes).unwrap()).unwrap_err(),
        "StoreManifest serializer has a different item type"
    );
}

#[test]
fn rejects_conflicting_serializers() {
    let mut bytes = super::super::tests::fixture(0, 1);
    bytes.copy_within(0x191f..0x1942, 0x1c80);
    relative(&mut bytes, 0x1c83, 0x2140);
    put32(&mut bytes, 0x1c96, 0x100);
    put32(&mut bytes, 0x2e04, 0x1d00);
    put32(&mut bytes, 0x124, 12);
    assert_eq!(
        discover(PeFile::from_bytes(&bytes).unwrap()).unwrap_err(),
        "ambiguous mLocalizeDescTag signature"
    );
}

#[test]
fn excludes_neighboring_serializers_but_keeps_chained_fragments() {
    let mut bytes = super::super::tests::fixture(0, 1);
    emit(&mut bytes, 0x1bd0, "8b 47 08 48 c1 e8 05 48 89 44 24 28");
    let image = PeFile::from_bytes(&bytes).unwrap();
    assert_eq!(helper_range(image, 0x1b00).unwrap(), 0x1b00..0x1b80);
    assert_eq!(discover(image).unwrap().1, expected(0).1);

    emit(&mut bytes, 0x1b60, "8b 47 08 48 c1 e8 05 48 89 44 24 28");
    assert_eq!(
        discover(PeFile::from_bytes(&bytes).unwrap()).unwrap_err(),
        "ambiguous StoreManifest entry count signature"
    );
}

#[test]
fn handles_nested_unwind_chains_and_odd_code_counts() {
    let mut bytes = super::super::tests::fixture(0, 1);
    bytes[0x2f12] = 1;
    bytes.copy_within(0x2e0c..0x2e18, 0x2f18);
    put32(&mut bytes, 0x2e2c, 0x2f40);
    bytes[0x2f40] = 1 | (4 << 3);
    bytes.copy_within(0x2e18..0x2e24, 0x2f44);
    assert_eq!(
        helper_range(PeFile::from_bytes(&bytes).unwrap(), 0x1b00).unwrap(),
        0x1b00..0x1c00
    );
}

#[test]
fn rejects_invalid_unwind_boundaries_instead_of_scanning_neighbors() {
    let original = super::super::tests::fixture(0, 1);
    for (offset, value) in [(0x2f14, 0x1b30), (0x2f18, 0x1b40), (0x2f1c, 0x2f10)] {
        let mut bytes = original.clone();
        put32(&mut bytes, offset, value);
        assert_eq!(
            helper_range(PeFile::from_bytes(&bytes).unwrap(), 0x1b00).unwrap_err(),
            "invalid manifest unwind chain"
        );
    }
    let mut bytes = original.clone();
    bytes[0x2f10] = 3 | (4 << 3);
    assert_eq!(
        helper_range(PeFile::from_bytes(&bytes).unwrap(), 0x1b00).unwrap_err(),
        "unsupported manifest chained unwind info"
    );
    let mut bytes = original;
    put32(&mut bytes, 0x2e1c, 0x2100);
    put32(&mut bytes, 0x124, 36);
    assert_eq!(
        helper_range(PeFile::from_bytes(&bytes).unwrap(), 0x1b00).unwrap_err(),
        "manifest unwind chain exceeds bounds"
    );
}
