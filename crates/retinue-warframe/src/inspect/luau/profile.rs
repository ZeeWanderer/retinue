use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Profile {
    pub schema: u8,
    pub id: String,
    pub executable_sha256: String,
    pub bytecode_version: u8,
    pub type_version: u8,
    pub boolean_bytes: usize,
    pub status: String,
    pub mapping: Vec<Opcode>,
    #[serde(default)]
    pub symbols: BTreeMap<u32, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Opcode {
    pub raw: u8,
    pub canonical: u8,
    pub name: String,
    pub has_aux: bool,
}

#[derive(Clone, Copy)]
pub(super) struct Layout<'a> {
    pub bytecode_version: u8,
    pub type_version: u8,
    pub boolean_bytes: usize,
    pub opcodes: &'a [Opcode],
}

const BUILTINS: [&str; 2] = [
    include_str!("profiles/d01b5cb5cff5.json"),
    include_str!("profiles/45fa6ad0769c.json"),
];

impl Profile {
    pub fn load(path: &Path) -> Result<Self, String> {
        Self::parse(&std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?)
    }

    pub fn parse(json: &str) -> Result<Self, String> {
        let profile: Self =
            serde_json::from_str(json).map_err(|e| format!("decode profile: {e}"))?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn builtin(key: &str) -> Result<Self, String> {
        Self::builtins()
            .into_iter()
            .find(|p| p.id == key || p.executable_sha256 == key)
            .ok_or_else(|| {
                format!(
                    "no script profile for {key}; supply --profile or capture raw scripts first"
                )
            })
    }

    pub fn builtins() -> Vec<Self> {
        BUILTINS
            .iter()
            .map(|json| Self::parse(json).expect("checked-in script profile"))
            .collect()
    }

    pub fn unmapped(executable_sha256: String) -> Self {
        Self {
            schema: 1,
            id: executable_sha256.chars().take(12).collect(),
            executable_sha256,
            bytecode_version: 9,
            type_version: 3,
            boolean_bytes: 4,
            status: "unmapped".into(),
            mapping: Vec::new(),
            symbols: BTreeMap::new(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1
            || self.bytecode_version != 9
            || self.type_version != 3
            || self.boolean_bytes != 4
        {
            return Err("unsupported script profile schema or bytecode layout".into());
        }
        if self.executable_sha256.len() != 64
            || !self
                .executable_sha256
                .bytes()
                .all(|c| c.is_ascii_hexdigit())
        {
            return Err("script profile requires an executable SHA-256".into());
        }
        if !matches!(self.status.as_str(), "reviewed" | "candidate" | "unmapped") {
            return Err("profile status must be reviewed, candidate or unmapped".into());
        }
        let mut raw = BTreeSet::new();
        let mut canonical = BTreeSet::new();
        for entry in &self.mapping {
            let expected = Opcode::new(entry.raw, entry.canonical)?;
            if entry.name != expected.name || entry.has_aux != expected.has_aux {
                return Err(format!(
                    "opcode 0x{:02x}: name/AUX disagrees with canonical operation",
                    entry.raw
                ));
            }
            if !raw.insert(entry.raw) || !canonical.insert(entry.canonical) {
                return Err("opcode map is not one-to-one".into());
            }
        }
        if self
            .symbols
            .iter()
            .any(|(hash, name)| *hash <= 1 || !identifier(name))
        {
            return Err("atom symbols need non-boolean hashes and identifier names".into());
        }
        Ok(())
    }

    pub fn require_executable(&self, hash: &str) -> Result<(), String> {
        if self.executable_sha256 != hash {
            Err(format!(
                "profile belongs to {}, not {hash}",
                self.executable_sha256
            ))
        } else {
            Ok(())
        }
    }

    pub(super) fn layout(&self) -> Layout<'_> {
        Layout {
            bytecode_version: self.bytecode_version,
            type_version: self.type_version,
            boolean_bytes: self.boolean_bytes,
            opcodes: &self.mapping,
        }
    }
}

impl Opcode {
    pub fn new(raw: u8, canonical: u8) -> Result<Self, String> {
        let name = NAMES
            .get(canonical as usize)
            .ok_or_else(|| format!("unsupported canonical opcode {canonical}"))?;
        let has_aux = matches!(canonical, 7 | 8 | 12 | 15 | 16 | 20 | 27..=32 | 53 | 55 | 58 | 60 | 66 | 74 | 75 | 77..=80 | 83..=85);
        Ok(Self {
            raw,
            canonical,
            name: (*name).into(),
            has_aux,
        })
    }

    pub(super) fn predicted_slot(&self) -> bool {
        matches!(self.canonical, 7 | 8 | 15 | 16 | 20)
    }
}

fn identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && bytes.all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

const NAMES: [&str; 86] = [
    "NOP",
    "BREAK",
    "LOADNIL",
    "LOADB",
    "LOADN",
    "LOADK",
    "MOVE",
    "GETGLOBAL",
    "SETGLOBAL",
    "GETUPVAL",
    "SETUPVAL",
    "CLOSEUPVALS",
    "GETIMPORT",
    "GETTABLE",
    "SETTABLE",
    "GETTABLEKS",
    "SETTABLEKS",
    "GETTABLEN",
    "SETTABLEN",
    "NEWCLOSURE",
    "NAMECALL",
    "CALL",
    "RETURN",
    "JUMP",
    "JUMPBACK",
    "JUMPIF",
    "JUMPIFNOT",
    "JUMPIFEQ",
    "JUMPIFLE",
    "JUMPIFLT",
    "JUMPIFNOTEQ",
    "JUMPIFNOTLE",
    "JUMPIFNOTLT",
    "ADD",
    "SUB",
    "MUL",
    "DIV",
    "MOD",
    "POW",
    "ADDK",
    "SUBK",
    "MULK",
    "DIVK",
    "MODK",
    "POWK",
    "AND",
    "OR",
    "ANDK",
    "ORK",
    "CONCAT",
    "NOT",
    "MINUS",
    "LENGTH",
    "NEWTABLE",
    "DUPTABLE",
    "SETLIST",
    "FORNPREP",
    "FORNLOOP",
    "FORGLOOP",
    "FORGPREP_INEXT",
    "FASTCALL3",
    "FORGPREP_NEXT",
    "NATIVECALL",
    "GETVARARGS",
    "DUPCLOSURE",
    "PREPVARARGS",
    "LOADKX",
    "JUMPX",
    "FASTCALL",
    "COVERAGE",
    "CAPTURE",
    "SUBRK",
    "DIVRK",
    "FASTCALL1",
    "FASTCALL2",
    "FASTCALL2K",
    "FORGPREP",
    "JUMPXEQKNIL",
    "JUMPXEQKB",
    "JUMPXEQKN",
    "JUMPXEQKS",
    "IDIV",
    "IDIVK",
    "GETUDATAKS",
    "SETUDATAKS",
    "NAMECALLUDATA",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_do_not_enable_native_adapters() {
        for p in Profile::builtins() {
            p.validate().unwrap();
        }
        let current = Profile::builtin("45fa6ad0769c").unwrap();
        assert!(crate::game_observer::adapter::resolve(&current.executable_sha256).is_none());
        assert_eq!(current.mapping.len(), 72);
    }

    #[test]
    fn malformed_maps_and_wrong_build_fail() {
        let mut p = Profile::builtin("d01b5cb5cff5").unwrap();
        assert!(p.require_executable(&"a".repeat(64)).is_err());
        p.mapping.push(p.mapping[0].clone());
        assert!(p.validate().is_err());
        p.mapping.pop();
        p.mapping[0].has_aux = !p.mapping[0].has_aux;
        assert!(p.validate().is_err());
    }
}
