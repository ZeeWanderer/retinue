use pelite::pe64::{Pe, PeFile};
use serde::Serialize;

use crate::game_observer::executable::{self, function_range, signature};

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct Layout {
    pub(crate) registry_rva: u64,
    pub(crate) registry_offset: u64,
    pub(crate) selector_rva: u64,
    pub(crate) profiles_offset: u64,
    pub(crate) identity_slot: u64,
    pub(crate) seed_getter_rva: u64,
    pub(crate) primary_id_offset: u64,
    pub(crate) platform_id_offset: u64,
    pub(crate) code_start: u64,
    pub(crate) code_end: u64,
}

const SELECTOR: &str = "
    ' 48 8b 99 u4 48 8b f2 8b 81 u4 48 8b f9 48 03 c3 48 3b d8 74 ?
    [0-16] 48 8b 03 48 8b 08 48 85 c9 74 ? 48 8b 01 ff 50 u1
    48 3b c6 74 ? 8b 87 u4 48 83 c3 08 48 03 87 u4 48 3b d8 75 ?
";
const SEED: &str = "
    ' 0f b6 91 u4 80 fa ff 75 ? 8b 81 u4 48 a9 ff ff ff 0f b8 00 00 00 00 eb ?
    33 c0 80 fa 0f 0f 95 c0 4c 8d 81 u4 48 c1 e0 u1
    ba 0f 00 00 00 4c 03 c0 4d 0f be 48 0f 49 2b d1 41 80 f9 ff 75 ?
    41 8b 40 08 25 ff ff ff 0f eb ? 48 8b c2 48 83 f8 08 0f 82 ????
    [0-16] b9 06 00 00 00 41 80 f9 ff 75 ?
    [0-32] 48 3b c1 49 8d 50 02 48 0f 47 c1
    [0-32] 41 b8 10 00 00 00 [0-16] 33 d2 e8 ????
";

pub(crate) fn discover(bytes: &[u8]) -> Result<Layout, String> {
    let image = PeFile::from_bytes(bytes).map_err(|error| format!("invalid PE: {error}"))?;
    let (registry_rva, registry_offset, _) = executable::registry(image)?;
    let selector = signature::<7>(image, "primary profile selector", SELECTOR)?;
    if !field(selector[2])
        || selector[3] != selector[2] + 8
        || selector[5] != selector[3]
        || selector[6] != selector[2]
        || selector[4] >= 512
        || !selector[4].is_multiple_of(8)
    {
        return Err("inconsistent player profile vector".into());
    }
    let seed = signature::<6>(image, "account seed getter", SEED)?;
    if !field(seed[4]) || !(4..=8).contains(&seed[5]) {
        return Err("invalid account identifier fields".into());
    }
    let platform = seed[4] + (1 << seed[5]);
    if seed[2] != platform + 15 || seed[3] != platform + 8 || !field(platform) {
        return Err("account identifier accessors disagree".into());
    }
    let code = image.headers().code_range();
    Ok(Layout {
        registry_rva,
        registry_offset,
        selector_rva: function_range(image, selector[0])?.start.into(),
        profiles_offset: selector[2].into(),
        identity_slot: selector[4].into(),
        seed_getter_rva: function_range(image, seed[0])?.start.into(),
        primary_id_offset: seed[4].into(),
        platform_id_offset: platform.into(),
        code_start: code.start.into(),
        code_end: code.end.into(),
    })
}

fn field(offset: u32) -> bool {
    (24..0x10000).contains(&offset) && offset.is_multiple_of(8)
}

#[cfg(test)]
pub(super) mod tests;
