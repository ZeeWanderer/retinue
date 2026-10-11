use pelite::pe64::{Pe, PeFile};
use serde::Serialize;

use super::super::executable;

const MANAGER: &str = "48 8b d9 48 8b 0d ${'} 48 85 c9 74 ? 48 8b d3 e8 ${'}";
const RESPONSE: &str = "
    4c 8b e9 [0-128]
    4d 8b 8d u4 4d 85 c9 0f 84 ????
    4d 8b 85 u4 49 8b 85 u4 49 8b c8 49 8b bd u4
    48 ff c8 48 d1 e9 48 23 c8 49 8b c0 83 e0 01
    48 8b 3c cf 48 8b 3c c7
    49 8d 41 ff 49 89 85 u4 48 85 c0 75 05 45 33 c0 eb 03 49 ff c0
    48 8b cb 4d 89 85 u4 e8 ???? 49 8d 8d u4 e8 ???? 4c 8d 67 u1
";
const CALLBACK: &str = "
    49 8b 44 24 u1 4d 8d 44 24 u1 41 0f b6 54 24 u1
    49 8d 4c 24 u1 ff 50 u1
";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ResponsePath {
    pub queue_table: u64,
    pub item_base: u64,
    pub body: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct Layout {
    pub manager_rva: u64,
    pub response: ResponsePath,
}

pub(crate) fn discover(bytes: &[u8]) -> Result<Layout, String> {
    let image = PeFile::from_bytes(bytes).map_err(|error| format!("Warframe PE: {error}"))?;
    let manager = executable::signature::<3>(image, "HTTP manager", MANAGER)?;
    let manager_rva = u64::from(manager[1]);
    executable::writable(image, manager_rva, 8)?;
    if !manager_rva.is_multiple_of(8) {
        return Err("unaligned HTTP manager binding".into());
    }
    let handler = executable::function_range(image, manager[2])?;
    let code = image.headers().code_range();
    if handler.start != manager[2]
        || handler.start < code.start
        || handler.end > code.end
        || handler.len() > 0x10000
    {
        return Err("invalid HTTP response handler extent".into());
    }
    let fields =
        executable::signature_in::<9>(image, "HTTP response queue", RESPONSE, handler.clone())?;
    let callback =
        executable::signature_in::<6>(image, "HTTP response callback", CALLBACK, handler)?;
    let table = fields[4];
    if !(0x20..0x1000).contains(&table)
        || !table.is_multiple_of(8)
        || fields[1] != table + 24
        || fields[2] != table + 16
        || fields[3] != table + 8
        || fields[5] != fields[1]
        || fields[6] != fields[2]
        || fields[7] >= table
        || !fields[7].is_multiple_of(8)
    {
        return Err("HTTP queue accessors disagree".into());
    }
    if [
        fields[8],
        callback[1],
        callback[2],
        callback[4],
        callback[5],
    ]
    .iter()
    .any(|&offset| offset == 0 || offset > 0x7f || !offset.is_multiple_of(8))
        || callback[3] > 0x7f
        || callback[1] != callback[4]
        || callback[2] + 16 > callback[1]
        || callback[3] >= callback[2]
    {
        return Err("invalid HTTP response callback layout".into());
    }
    Ok(Layout {
        manager_rva,
        response: ResponsePath {
            queue_table: table.into(),
            item_base: fields[8].into(),
            body: callback[2].into(),
        },
    })
}

#[cfg(test)]
mod tests;
