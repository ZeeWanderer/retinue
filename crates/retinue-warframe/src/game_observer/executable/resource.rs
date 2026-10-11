use pelite::pe64::PeFile;
use serde::Serialize;

use super::{function_range, signature, signature_in, string_blocks, writable};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ResourceLayout {
    pub(crate) string_blocks_rva: u64,
    pub(crate) object_type_offset: u64,
    pub(crate) object_holder_offset: u64,
    pub(crate) parent_offset: u64,
    pub(crate) name_prefix_offset: u64,
    pub(crate) name_leaf_offset: u64,
}

const HEADER: &str = "
    33 d2 48 8d 05 ???? 48 89 01 48 8d 05 ${'}
    48 89 41 u1 48 8b c1 48 89 51 u1 ff 05 ${'}
    c7 41 ? ff ff ff ff 89 51 ? 80 61 ? e0 88 51 ? c7 41 ? 01 00 00 00 c3
";
const DESCRIPTOR: &str = "
    33 d2 89 47 u1 48 8d 05 ???? 89 57 ?
    48 89 57 u1 89 57 ? 48 89 77 u1 48 8b 74 24 ?
";
const NAME: &str = "
    48 8b 41 u1 48 85 c0 74 ? 8b 00 89 02
    8b 41 u1 89 42 04 48 8b c2 c3
    89 02 8b 41 u1 89 42 04 48 8b c2 c3
";

impl ResourceLayout {
    pub(crate) fn discover(image: PeFile<'_>, constructor: u32) -> Result<Self, String> {
        let header = signature::<5>(image, "resource object header", HEADER)?;
        let descriptor = signature_in::<4>(
            image,
            "resource descriptor fields",
            DESCRIPTOR,
            function_range(image, constructor)?,
        )?;
        let name = signature::<4>(image, "resource name fields", NAME)?;
        if Some(header[4]) != header[1].checked_add(8)
            || !separate(&[(header[2], 8), (header[3], 8)])
            || !separate(&[(descriptor[1], 4), (descriptor[2], 8), (descriptor[3], 8)])
            || name[1] != descriptor[2]
            || name[2] != descriptor[1]
            || name[3] != descriptor[1]
        {
            return Err("resource accessors disagree".into());
        }
        writable(image, header[1].into(), 16)?;
        Ok(Self {
            string_blocks_rva: string_blocks(image)?,
            object_type_offset: header[3].into(),
            object_holder_offset: header[2].into(),
            parent_offset: descriptor[3].into(),
            name_prefix_offset: descriptor[2].into(),
            name_leaf_offset: descriptor[1].into(),
        })
    }

    pub(crate) fn descriptor_size(self) -> u64 {
        (self.parent_offset + 8)
            .max(self.name_prefix_offset + 8)
            .max(self.name_leaf_offset + 4)
    }
}

fn separate(fields: &[(u32, u32)]) -> bool {
    fields.iter().enumerate().all(|(index, &(offset, size))| {
        (8..0x80).contains(&offset)
            && offset.is_multiple_of(size)
            && fields[..index]
                .iter()
                .all(|&(other, width)| offset >= other + width || other >= offset + size)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_observer::executable::fixture::{
        emit, image, put32, relative, resource_code, resources,
    };

    fn fixture(fields: u64) -> Vec<u8> {
        let mut bytes = image();
        emit(
            &mut bytes,
            0x1000,
            "44 8b 01 48 8b da 48 8b 05 00 00 00 00 41 0f b7 c8 48 03 c9
             49 c1 e8 10 48 8b 0c c8 49 03 c8 48 89 0a",
        );
        relative(&mut bytes, 0x1009, 0x3000);
        resource_code(&mut bytes, 0x1300, resources(0x3000, fields));
        put32(&mut bytes, 0x104, 16);
        put32(&mut bytes, 0x120, 0x2e00);
        put32(&mut bytes, 0x124, 12);
        put32(&mut bytes, 0x2e00, 0x1300);
        put32(&mut bytes, 0x2e04, 0x1340);
        bytes
    }

    #[test]
    fn discovers_changed_resource_fields() {
        for fields in [0, 0x20, 0x40] {
            let bytes = fixture(fields);
            let image = PeFile::from_bytes(&bytes).unwrap();
            assert_eq!(
                ResourceLayout::discover(image, 0x1300).unwrap(),
                resources(0x3000, fields)
            );
        }
    }

    #[test]
    fn rejects_disagreeing_getters_and_overlapping_fields() {
        for (offset, value) in [
            (0x1503, 0x20),
            (0x150f, 0x30),
            (0x151b, 0x30),
            (0x141d, 0x10),
            (0x141d, 0x80),
            (0x1319, 0x10),
        ] {
            let mut bytes = fixture(0);
            bytes[offset] = value;
            assert_eq!(
                ResourceLayout::discover(PeFile::from_bytes(&bytes).unwrap(), 0x1300).unwrap_err(),
                "resource accessors disagree"
            );
        }
        let mut bytes = fixture(0);
        relative(&mut bytes, 0x1420, 0x3c10);
        assert_eq!(
            ResourceLayout::discover(PeFile::from_bytes(&bytes).unwrap(), 0x1300).unwrap_err(),
            "resource accessors disagree"
        );
    }

    #[test]
    fn rejects_unrelated_constructor_and_invalid_holder_binding() {
        let mut bytes = fixture(0);
        assert!(ResourceLayout::discover(PeFile::from_bytes(&bytes).unwrap(), 0x1000).is_err());
        relative(&mut bytes, 0x140f, 0x1000);
        relative(&mut bytes, 0x1420, 0x1008);
        assert!(
            ResourceLayout::discover(PeFile::from_bytes(&bytes).unwrap(), 0x1300)
                .unwrap_err()
                .contains("outside writable image data")
        );
    }
}
