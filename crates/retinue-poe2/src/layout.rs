use std::collections::BTreeMap;
use std::path::Path;

use pelite::pe64::{Pe, PeFile};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// GEP 312's SmartFind anchors, with operands captured instead of build-specific addresses.
const ROOT: &str = "57 48 83 ec 40 48 8b f1 33 ed 48 39 2d ${'}
    0f 85 ? 01 00 00 b9 48 01 00 00 e8 ???? 48 8b f8";
const INGAME_DATA: &str = "48 8b 8e u4 ba 27 05 00 00 48 81 c1 ? 01 00 00
    e8 ???? 85 c0 0f 85 ?? 00 00";
const SERVER_DATA: &str = "48 c1 e9 20 49 d3 e0 49 03 d0 48 f7 da f0 48 01 15 ????
    4d 2b c8 49 8b c9 e8 ???? 84 db 0f 84 ?? 00 00 49 8b 87 u4";
const GOLD: &str = "81 79 08 fb d6 a0 dd 75 ? 48 8b 82 u4 8b 88 u4 eb ? c7 ? ? 00 00 00 00";
const UI_ROOT: &str = "83 3d ???? ff 75 ? 48 8d 05 ???? 48 89 05 ???? 48 8d 0d ????
    33 c0 48 89 05 ???? 48 89 05 ???? e8 ???? 48 8d 0d ???? e8 ????
    48 8d 05 ${'} 48 83 c4 28 c3";
const VISIBILITY: &str = "66 0f 6e c0 8b 82 u4 0f 5b c0 c1 e8 07 f3 0f 59 c1 a8 01 74 09";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GoldLayout {
    pub server_offset: u32,
    pub data_offset: u32,
    pub value_offset: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PanelLayout {
    pub root_rva: u64,
    pub flags_offset: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Layout {
    pub root_rva: u64,
    pub ingame_data_offset: u32,
    pub gold: Result<GoldLayout, String>,
    pub panels: Result<PanelLayout, String>,
    pub component_getters: BTreeMap<u32, String>,
    pub image_size: u32,
    pub image_sha256: String,
}

impl Layout {
    pub fn read(path: &Path) -> Result<Self, String> {
        let metadata = path.metadata().map_err(|e| e.to_string())?;
        if metadata.len() > 256 * 1024 * 1024 {
            return Err("PoE2 executable exceeds 256 MiB".into());
        }
        Self::discover(&std::fs::read(path).map_err(|e| e.to_string())?)
    }

    pub fn discover(bytes: &[u8]) -> Result<Self, String> {
        let image = PeFile::from_bytes(bytes).map_err(|e| format!("PoE2 PE: {e}"))?;
        let root = signature::<2>(image, "game root", ROOT)?[1];
        let ingame = signature::<2>(image, "in-game data", INGAME_DATA)?[1];
        section(image, root, 8, true)?;
        let gold = (|| {
            let server_offset = signature::<2>(image, "server data", SERVER_DATA)?[1];
            let fields = signature::<3>(image, "gold", GOLD)?;
            let gold = GoldLayout {
                server_offset,
                data_offset: fields[1],
                value_offset: fields[2],
            };
            gold.validate()?;
            Ok(gold)
        })();
        let panels = (|| {
            let root = signature::<2>(image, "UI root", UI_ROOT)?[1];
            section(image, root, 24, true)?;
            let flags_offset = signature::<2>(image, "UI visibility", VISIBILITY)?[1];
            field(flags_offset, 4)?;
            Ok(PanelLayout {
                root_rva: root.into(),
                flags_offset,
            })
        })();
        let layout = Self {
            root_rva: root.into(),
            ingame_data_offset: ingame,
            gold,
            panels,
            component_getters: component_getters(image)?,
            image_size: image.optional_header().SizeOfImage,
            image_sha256: format!("{:x}", Sha256::digest(bytes)),
        };
        layout.validate()?;
        Ok(layout)
    }

    pub fn validate(&self) -> Result<(), String> {
        let image_pointer = |rva: u64| {
            rva.is_multiple_of(8)
                && rva >= 4096
                && rva
                    .checked_add(24)
                    .is_some_and(|end| end <= u64::from(self.image_size))
        };
        if !image_pointer(self.root_rva)
            || self.image_sha256.len() != 64
            || !self.image_sha256.bytes().all(|c| c.is_ascii_hexdigit())
            || self.component_getters.len() > 128
            || self.component_getters.iter().any(|(rva, name)| {
                *rva < 4096
                    || *rva >= self.image_size
                    || !["Stats", "Player", "Life", "Positioned"].contains(&name.as_str())
            })
        {
            return Err("invalid PoE2 native layout".into());
        }
        field(self.ingame_data_offset, 8)?;
        if let Ok(gold) = &self.gold {
            gold.validate()?;
        }
        if let Ok(panels) = &self.panels {
            if !image_pointer(panels.root_rva) {
                return Err("invalid PoE2 UI root".into());
            }
            field(panels.flags_offset, 4)?;
        }
        Ok(())
    }
}

impl GoldLayout {
    fn validate(&self) -> Result<(), String> {
        field(self.server_offset, 8)?;
        field(self.data_offset, 8)?;
        field(self.value_offset, 4)
    }
}

fn field(offset: u32, alignment: u32) -> Result<(), String> {
    if !(8..0x10000).contains(&offset) || !offset.is_multiple_of(alignment) {
        Err("invalid PoE2 field offset".into())
    } else {
        Ok(())
    }
}

fn component_getters(image: PeFile<'_>) -> Result<BTreeMap<u32, String>, String> {
    let scanner = image.scanner();
    let mut result = BTreeMap::new();
    for name in ["Stats", "Player", "Life", "Positioned"] {
        let pattern = pelite::pattern::parse(&format!("48 8d 05 ${{ \"{name}\" 00 }} c3"))
            .map_err(|e| e.to_string())?;
        let mut matches = scanner.matches(&pattern, image.headers().code_range());
        let mut found = [0];
        while matches.next(&mut found) {
            result.insert(found[0], name.into());
            if result.len() > 128 {
                return Err("too many PoE2 component name getters".into());
            }
        }
    }
    Ok(result)
}

fn signature<const N: usize>(
    image: PeFile<'_>,
    label: &str,
    source: &str,
) -> Result<[u32; N], String> {
    let pattern = pelite::pattern::parse(source).map_err(|e| format!("{label} pattern: {e}"))?;
    let scanner = image.scanner();
    let mut matches = scanner.matches(&pattern, image.headers().code_range());
    let mut found = [0; N];
    if !matches.next(&mut found) {
        return Err(format!("PoE2 {label} signature not found"));
    }
    if matches.next(&mut [0; N]) {
        return Err(format!("ambiguous PoE2 {label} signature"));
    }
    Ok(found)
}

fn section(image: PeFile<'_>, rva: u32, size: u32, writable: bool) -> Result<(), String> {
    if image.section_headers().iter().any(|section| {
        section.Characteristics & 0x2000_0000 == 0
            && section.Characteristics & 0x4000_0000 != 0
            && (!writable || section.Characteristics & 0x8000_0000 != 0)
            && rva >= section.VirtualAddress
            && u64::from(rva) + u64::from(size)
                <= u64::from(section.VirtualAddress) + u64::from(section.VirtualSize)
    }) {
        Ok(())
    } else {
        Err(format!(
            "PoE2 binding 0x{rva:x} is outside expected image data"
        ))
    }
}

#[cfg(test)]
mod tests;
