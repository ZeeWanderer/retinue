use std::path::Path;

use pelite::pe64::PeFile;
use serde::Serialize;

use super::super::executable::{self, descriptor};

mod fields;
pub(super) use fields::{ManifestFields, StoreFields};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(super) struct MetadataLayout {
    pub(super) global_registry_rva: u64,
    pub(super) global_registry_offset: u64,
    pub(super) resources: executable::ResourceLayout,
    pub(super) store: StoreFields,
    pub(super) manifests: ManifestFields,
    pub(super) store_manifest_descriptor_rva: u64,
    pub(super) variant_manifest_descriptor_rva: u64,
    pub(super) weapon_descriptor_rva: u64,
    pub(super) game_time_rva: u64,
    pub(super) game_rules_hash: u32,
}

pub(super) fn read(path: &Path) -> Result<(String, MetadataLayout), String> {
    let (hash, bytes) = executable::read(path)?;
    let layout = discover(&bytes)
        .map_err(|reason| format!("unsupported Warframe executable {hash} ({reason})"))?;
    Ok((hash, layout))
}

pub(super) fn discover(bytes: &[u8]) -> Result<MetadataLayout, String> {
    let image = PeFile::from_bytes(bytes).map_err(|error| format!("invalid PE: {error}"))?;
    let (global_registry_rva, global_registry_offset, game_rules_hash) =
        executable::registry(image)?;
    let (store, constructor) = descriptor(image, "StoreManifest", "/Lotus/Types/Game/")?;
    let (variants, variants_constructor) =
        descriptor(image, "VariantManifest", "/Lotus/Types/Game/Store/")?;
    let (weapon, weapon_constructor) = descriptor(image, "LotusWeapon", "/Lotus/Types/Game/")?;
    if constructor != variants_constructor || constructor != weapon_constructor {
        return Err("resource descriptors use different constructors".into());
    }
    let resources = executable::ResourceLayout::discover(image, constructor)?;
    let (store_fields, manifests, game_time_rva) = fields::discover(image)?;
    let layout = MetadataLayout {
        global_registry_rva,
        global_registry_offset,
        game_rules_hash,
        resources,
        store: store_fields,
        manifests,
        game_time_rva,
        store_manifest_descriptor_rva: store,
        variant_manifest_descriptor_rva: variants,
        weapon_descriptor_rva: weapon,
    };
    for (rva, size) in [
        (layout.global_registry_rva, global_registry_offset + 16),
        (resources.string_blocks_rva, 8),
        (layout.game_time_rva, 8),
        (
            layout.store_manifest_descriptor_rva,
            resources.descriptor_size(),
        ),
        (
            layout.variant_manifest_descriptor_rva,
            resources.descriptor_size(),
        ),
        (layout.weapon_descriptor_rva, resources.descriptor_size()),
    ] {
        executable::writable(image, rva, size)?;
    }
    Ok(layout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_observer::executable::fixture::{
        emit, image, put32, registry_setter, relative, resource_code, resources,
    };

    pub(super) fn fixture(shift: usize, hash: u32) -> Vec<u8> {
        let mut bytes = image();
        for (address, text) in [
            (0x2000, "Setting gGameRules\n"),
            (0x2020, "StoreManifest"),
            (0x2040, "VariantManifest"),
            (0x2060, "LotusWeapon"),
            (0x2080, "/Lotus/Types/Game/"),
            (0x20a0, "/Lotus/Types/Game/Store/"),
            (0x20d0, "StoreItem"),
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
        relative(&mut bytes, code + 34, code + 0x800);
        registry_setter(&mut bytes, code + 0x800, 0x138 + shift as u32);
        resource_code(
            &mut bytes,
            code + 0xd00,
            resources(0x3200 + shift as u64, shift as u64 / 2),
        );
        put32(&mut bytes, 0x104, 16);
        put32(&mut bytes, 0x120, 0x2e00);
        put32(&mut bytes, 0x124, 60);
        for (index, (start, end)) in [
            (code + 0x900, code + 0xa10),
            (code + 0xb00, code + 0xb30),
            (code + 0xb30, code + 0xb80),
            (code + 0xb80, code + 0xc00),
            (code + 0xd00, code + 0xd80),
        ]
        .into_iter()
        .enumerate()
        {
            put32(&mut bytes, 0x2e00 + index * 12, start as u32);
            put32(&mut bytes, 0x2e04 + index * 12, end as u32);
            put32(&mut bytes, 0x2e08 + index * 12, 0x2f00);
        }
        bytes[0x2f00] = 1;
        put32(&mut bytes, 0x2e20, 0x2f10);
        bytes[0x2f10] = 1 | (4 << 3);
        bytes.copy_within(0x2e0c..0x2e18, 0x2f14);
        put32(&mut bytes, code + 26, hash);
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
            "48 8b 81 e0 00 00 00 f2 0f 10 0d 00 00 00 00
              48 85 c0 74 0e 0f 57 c0 f2 48 0f 2a c0 66 0f 2f c1 77 1a
              48 8b 81 e8 00 00 00 48 85 c0 74 11
              0f 57 c0 f2 48 0f 2a c0 66 0f 2f c1 73 03 32 c0 c3
              0f b6 81 5c 01 00 00 c0 e8 05 24 01 c3",
        );
        relative(&mut bytes, code + 0x30b, 0x3210 + shift);
        for (index, name, parent) in [
            (0, 0x2020, 0x2080),
            (1, 0x2040, 0x20a0),
            (2, 0x2060, 0x2080),
            (3, 0x20d0, 0x2080),
        ] {
            let start = code + 0x400 + index * 0x100;
            emit(
                &mut bytes,
                start,
                "48 8d 15 00 00 00 00 48 8d 4c 24 50 e8 00 00 00 00
                  48 8d 15 00 00 00 00 48 8d 4c 24 58 8b 18 e8 00 00 00 00
                  44 8b c3 8b 10 48 8d 0d 00 00 00 00 c6 44 24 20 00 e8 00 00 00 00",
            );
            relative(&mut bytes, start + 3, name);
            relative(&mut bytes, start + 20, parent);
            relative(&mut bytes, start + 44, 0x3300 + shift + index * 0x100);
            relative(&mut bytes, start + 54, code + 0xd00);
        }
        fields::tests::fixture(&mut bytes, code, shift as u64 / 2);
        bytes
    }

    #[test]
    fn discovers_relocated_bindings_and_changed_atom_hashes() {
        for (shift, hash) in [(0, 0x27816687), (0x80, 0xd7e3ec85)] {
            assert_eq!(
                discover(&fixture(shift, hash)).unwrap(),
                MetadataLayout {
                    global_registry_rva: 0x3000 + shift as u64,
                    global_registry_offset: 0x138 + shift as u64,
                    resources: resources(0x3200 + shift as u64, shift as u64 / 2),
                    store: fields::tests::expected(shift as u64 / 2).0,
                    manifests: fields::tests::expected(shift as u64 / 2).1,
                    store_manifest_descriptor_rva: 0x3300 + shift as u64,
                    variant_manifest_descriptor_rva: 0x3400 + shift as u64,
                    weapon_descriptor_rva: 0x3500 + shift as u64,
                    game_time_rva: 0x3210 + shift as u64,
                    game_rules_hash: hash,
                }
            );
        }
    }

    #[test]
    fn accepts_repeated_bindings_but_rejects_conflicting_ones() {
        let mut bytes = fixture(0, 1);
        bytes.copy_within(0x1200..0x1222, 0x1180);
        relative(&mut bytes, 0x1189, 0x3200);
        assert!(discover(&bytes).is_ok());
        relative(&mut bytes, 0x1189, 0x3210);
        assert_eq!(
            discover(&bytes).unwrap_err(),
            "ambiguous string blocks signature"
        );
    }

    #[test]
    fn refuses_missing_anchors_changed_layout_and_non_data_bindings() {
        let original = fixture(0, 1);
        let mut bytes = original.clone();
        bytes[0x2000] = b'X';
        assert_eq!(
            discover(&bytes).unwrap_err(),
            "game registry signature not found"
        );
        bytes = original.clone();
        bytes[0x1303] = 0xf0;
        assert_eq!(
            discover(&bytes).unwrap_err(),
            "StoreItem accessors disagree"
        );
        bytes = original;
        relative(&mut bytes, 0x1209, 0x1000);
        assert!(
            discover(&bytes)
                .unwrap_err()
                .contains("outside writable image data")
        );
        assert!(
            discover(b"not an executable")
                .unwrap_err()
                .starts_with("invalid PE:")
        );
    }
}
