use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::{
    PrototypeInfo, Reader, ScriptInfo,
    corpus::hash,
    profile::{Opcode, Profile},
};

pub(super) fn instructions<'a>(
    proto: &PrototypeInfo,
    profile: &'a Profile,
) -> Result<Vec<(usize, &'a Opcode)>, String> {
    let mut result = Vec::new();
    let mut pc = 0;
    while pc < proto.words.len() {
        let raw = proto.words[pc] as u8;
        let op = profile
            .mapping
            .iter()
            .find(|op| op.raw == raw)
            .ok_or_else(|| {
                format!(
                    "unmapped opcode 0x{raw:02x}, prototype {}, PC {pc}",
                    proto.index
                )
            })?;
        if op.has_aux && pc + 1 >= proto.words.len() {
            return Err(format!("truncated AUX, prototype {}, PC {pc}", proto.index));
        }
        result.push((pc, op));
        pc += if op.has_aux { 2 } else { 1 };
    }
    Ok(result)
}

pub(super) fn operand_mask(op: &Opcode) -> u32 {
    if op.predicted_slot() {
        0x00ff_ff00
    } else {
        0xffff_ff00
    }
}

pub(super) fn atom(constant: &[u8]) -> Option<u32> {
    if constant.len() != 5 || constant[0] != 1 {
        return None;
    }
    let value = u32::from_le_bytes(constant[1..].try_into().ok()?);
    (value > 1).then_some(value)
}

pub(super) fn constants(
    proto: &PrototypeInfo,
    script: &ScriptInfo,
    hide_atoms: bool,
) -> Result<Vec<Value>, String> {
    proto
        .constants
        .iter()
        .map(|constant| {
            if let Some(id) = atom(constant) {
                return Ok(if hide_atoms {
                    json!({"atom": "unresolved"})
                } else {
                    json!({"atom": id})
                });
            }
            if constant.first() == Some(&3) {
                let index = Reader::new(&constant[1..]).varint("string constant")?;
                let value = script
                    .strings
                    .iter()
                    .find(|s| s.index as u64 == index)
                    .ok_or_else(|| format!("invalid string index {index}"))?;
                let contents = value
                    .text
                    .as_ref()
                    .map(|text| json!(text))
                    .unwrap_or_else(|| json!({"bytes": value.bytes}));
                return Ok(json!({"string": contents}));
            }
            Ok(json!({"raw": constant}))
        })
        .collect()
}

pub(super) fn header(proto: &PrototypeInfo) -> Value {
    json!([
        proto.max_stack_size,
        proto.parameters,
        proto.upvalues,
        proto.vararg,
        proto.flags,
        proto.type_info
    ])
}

pub(super) fn projection(
    proto: &PrototypeInfo,
    script: &ScriptInfo,
    profile: &Profile,
    hide_atoms: bool,
) -> Result<Value, String> {
    let mut words = proto.words.clone();
    for (pc, op) in instructions(proto, profile)? {
        words[pc] = (words[pc] & operand_mask(op)) | u32::from(op.canonical);
    }
    Ok(json!({"header": header(proto), "words": words,
        "constants": constants(proto, script, hide_atoms)?, "children": proto.children}))
}

pub(super) fn fingerprint(value: &Value) -> String {
    hash(&serde_json::to_vec(value).expect("JSON value"))
}

pub(super) fn dependencies(script: &ScriptInfo) -> BTreeSet<String> {
    script
        .strings
        .iter()
        .filter_map(|s| s.text.as_deref())
        .filter_map(|text| {
            if text.starts_with('/') && text.ends_with(".lua") {
                return Some(text.to_owned());
            }
            if (text.starts_with("Lotus.") || text.starts_with("EE."))
                && text
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._".contains(&c))
            {
                return Some(format!("/{}.lua", text.replace('.', "/")));
            }
            None
        })
        .collect()
}

#[cfg(test)]
pub(super) fn fixture(words: Vec<u32>, constants: Vec<Vec<u8>>) -> ScriptInfo {
    ScriptInfo {
        adapter: "test".into(),
        executable_sha256: "a".repeat(64),
        bytecode_version: 9,
        type_version: 3,
        byte_count: 0,
        opcode_coverage: vec![],
        atom_constants: vec![],
        strings: vec![],
        prototypes: vec![PrototypeInfo {
            index: 0,
            code_offset: 0,
            max_stack_size: 8,
            parameters: 0,
            upvalues: 0,
            vararg: false,
            flags: 0,
            type_info: vec![],
            words,
            uncertain_from_pc: None,
            constants,
            children: vec![],
            line_defined: 0,
            debug_name: 0,
        }],
        main_prototype: 0,
    }
}
