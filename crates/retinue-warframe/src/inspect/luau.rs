use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

mod analysis;
pub mod corpus;
pub mod evidence;
pub mod infer;
pub mod profile;
pub mod tracking;
use profile::{Layout as LuauLayout, Opcode as LuauOpcode, Profile};

const DECOMPILER_PROTOCOL: u8 = 1;

#[derive(Clone, Copy)]
enum TranscodeFormat {
    Warframe,
    Inspect,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ScriptInfo {
    pub adapter: String,
    pub executable_sha256: String,
    pub bytecode_version: u8,
    pub type_version: u8,
    pub byte_count: usize,
    pub opcode_coverage: Vec<OpcodeCoverage>,
    pub atom_constants: Vec<AtomCoverage>,
    pub strings: Vec<StringInfo>,
    pub prototypes: Vec<PrototypeInfo>,
    pub main_prototype: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StringInfo {
    pub index: usize,
    pub text: Option<String>,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PrototypeInfo {
    pub index: usize,
    pub code_offset: usize,
    pub max_stack_size: u8,
    pub parameters: u8,
    pub upvalues: u8,
    pub vararg: bool,
    pub flags: u8,
    pub type_info: Vec<u8>,
    pub words: Vec<u32>,
    pub uncertain_from_pc: Option<usize>,
    pub constants: Vec<Vec<u8>>,
    pub children: Vec<u64>,
    pub line_defined: u64,
    pub debug_name: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OpcodeCoverage {
    pub raw: u8,
    pub canonical: u8,
    pub name: String,
    pub instructions: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AtomCoverage {
    pub value: u32,
    pub hex: String,
    pub occurrences: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct DecompilerStatus {
    pub available: bool,
    pub compatible: bool,
    pub path: Option<PathBuf>,
    pub protocol: Option<u8>,
    pub version: Option<String>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
struct DecompilerDescription {
    protocol: u8,
    tool: String,
    version: String,
}

#[derive(Deserialize)]
struct DecompilerDiagnostic {
    protocol: u8,
    tool: String,
    ok: bool,
    error: Option<String>,
}

pub fn info(bytecode: &[u8], adapter: &Profile) -> Result<ScriptInfo, String> {
    let transcoded = transcode(bytecode, adapter.layout(), TranscodeFormat::Inspect)?;
    Ok(ScriptInfo {
        adapter: adapter.id.clone(),
        executable_sha256: adapter.executable_sha256.clone(),
        bytecode_version: transcoded.version,
        type_version: transcoded.type_version,
        byte_count: bytecode.len(),
        opcode_coverage: transcoded.coverage,
        atom_constants: transcoded.atoms,
        strings: transcoded
            .strings
            .into_iter()
            .enumerate()
            .map(|(index, bytes)| StringInfo {
                index: index + 1,
                text: String::from_utf8(bytes.clone()).ok(),
                bytes,
            })
            .collect(),
        prototypes: transcoded.prototypes,
        main_prototype: transcoded.main_prototype,
    })
}

pub fn disassemble(bytecode: &[u8], adapter: &Profile) -> Result<String, String> {
    use std::fmt::Write;
    let script = info(bytecode, adapter)?;
    let mut out = format!(
        "; Warframe Luau v{} profile {} ({})\n; Instruction fields: A B C, signed D; AUX retained separately\n",
        script.bytecode_version, adapter.id, adapter.status
    );
    for proto in &script.prototypes {
        writeln!(
            out,
            "\nprototype {}: params={} upvalues={} stack={} children={:?}",
            proto.index, proto.parameters, proto.upvalues, proto.max_stack_size, proto.children
        )
        .unwrap();
        for (index, constant) in analysis::constants(proto, &script, false)?
            .iter()
            .enumerate()
        {
            write!(out, "  K{index}: {constant}").unwrap();
            if let Some(atom) = analysis::atom(&proto.constants[index]) {
                write!(out, " ; __wf_atom_{atom:08x}").unwrap();
                if let Some(name) = adapter.symbols.get(&atom) {
                    write!(out, " = {name}").unwrap();
                }
            }
            out.push('\n');
        }
        for (pc, op) in analysis::instructions(proto, adapter)? {
            let word = proto.words[pc];
            write!(
                out,
                "  {pc:04} {:16} A={} B={} C={} D={} ; raw=0x{word:08x}",
                op.name,
                (word >> 8) as u8,
                (word >> 16) as u8,
                (word >> 24) as u8,
                (word >> 16) as i16
            )
            .unwrap();
            if op.has_aux {
                write!(out, " AUX=0x{:08x}", proto.words[pc + 1]).unwrap();
            }
            out.push('\n');
        }
    }
    Ok(out)
}

pub fn normalize(bytecode: &[u8], adapter: &Profile) -> Result<Vec<u8>, String> {
    let transcoded = transcode(bytecode, adapter.layout(), TranscodeFormat::Warframe)?;
    Ok(transcoded.bytes)
}

pub fn decompile(bytecode: &[u8], adapter: &Profile) -> Result<String, String> {
    let transcoded = transcode(bytecode, adapter.layout(), TranscodeFormat::Warframe)?;
    let helper = resolve_decompiler()?;
    let mut command = Command::new(&helper);
    for (hash, name) in &adapter.symbols {
        command.args(["--symbol", &format!("{hash:08x}={name}")]);
    }
    let mut child = command
        .arg("-")
        .args(["--diagnostics", "json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start {}: {error}", helper.display()))?;
    child
        .stdin
        .take()
        .expect("piped decompiler input")
        .write_all(&transcoded.bytes)
        .map_err(|error| format!("could not write decompiler input: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("could not wait for {}: {error}", helper.display()))?;
    let diagnostic = parse_json_line::<DecompilerDiagnostic>(&output.stderr)
        .map_err(|error| format!("invalid decompiler diagnostic: {error}"))?;
    if diagnostic.protocol != DECOMPILER_PROTOCOL || diagnostic.tool != "wf-luau-decompiler" {
        return Err(format!(
            "incompatible Warframe Luau decompiler protocol {} from {}",
            diagnostic.protocol, diagnostic.tool
        ));
    }
    if !output.status.success() || !diagnostic.ok {
        return Err(format!(
            "Warframe Luau decompiler failed: {}",
            diagnostic.error.as_deref().unwrap_or("unknown error")
        ));
    }
    let body = String::from_utf8(output.stdout)
        .map_err(|_| "Warframe Luau decompiler emitted non-UTF-8 source".to_owned())?;
    let mut header = format!(
        "-- Warframe Luau v{} adapter {}\n",
        transcoded.version, adapter.id
    );
    let unresolved = transcoded
        .atoms
        .iter()
        .filter(|a| !adapter.symbols.contains_key(&a.value))
        .count();
    if unresolved > 0 {
        header.push_str(&format!(
            "-- {} unresolved atom identifier(s) preserved as __wf_atom_XXXXXXXX\n",
            unresolved
        ));
    }
    Ok(format!("{}{}", header, body))
}

pub fn decompiler_status() -> DecompilerStatus {
    match resolve_decompiler() {
        Ok(path) => match Command::new(&path).arg("--describe").output() {
            Ok(output) if output.status.success() => {
                match serde_json::from_slice::<DecompilerDescription>(&output.stdout) {
                    Ok(description) => {
                        let compatible = description.protocol == DECOMPILER_PROTOCOL
                            && description.tool == "wf-luau-decompiler";
                        DecompilerStatus {
                            available: true,
                            compatible,
                            path: Some(path),
                            protocol: Some(description.protocol),
                            version: Some(description.version),
                            error: (!compatible).then(|| {
                                format!(
                                    "expected wf-luau-decompiler protocol {DECOMPILER_PROTOCOL}"
                                )
                            }),
                        }
                    }
                    Err(error) => DecompilerStatus {
                        available: true,
                        compatible: false,
                        path: Some(path),
                        protocol: None,
                        version: None,
                        error: Some(format!("invalid helper description: {error}")),
                    },
                }
            }
            Ok(output) => DecompilerStatus {
                available: false,
                compatible: false,
                path: Some(path),
                protocol: None,
                version: None,
                error: Some(String::from_utf8_lossy(&output.stderr).trim().to_owned()),
            },
            Err(error) => DecompilerStatus {
                available: false,
                compatible: false,
                path: Some(path),
                protocol: None,
                version: None,
                error: Some(error.to_string()),
            },
        },
        Err(error) => DecompilerStatus {
            available: false,
            compatible: false,
            path: None,
            protocol: None,
            version: None,
            error: Some(error),
        },
    }
}

fn parse_json_line<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, String> {
    let text = String::from_utf8_lossy(bytes);
    let line = text
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| "helper emitted no diagnostic".to_owned())?;
    serde_json::from_str(line).map_err(|error| error.to_string())
}

fn resolve_decompiler() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("WFINSPECT_LUAU_DECOMPILER") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path.clone()).ok_or_else(|| {
            format!(
                "WFINSPECT_LUAU_DECOMPILER is not a file: {}",
                path.display()
            )
        });
    }

    let name = format!("wf-luau-decompiler{}", std::env::consts::EXE_SUFFIX);
    if let Some(executable) = retinue_client::executable_path()
        && let Some(directory) = executable.parent()
    {
        let sibling = directory.join(&name);
        if sibling.is_file() {
            return Ok(sibling);
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join(&name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(format!(
        "{name} not found beside wfinspect or on PATH; set WFINSPECT_LUAU_DECOMPILER"
    ))
}

struct Transcoded {
    version: u8,
    type_version: u8,
    bytes: Vec<u8>,
    coverage: Vec<OpcodeCoverage>,
    atoms: Vec<AtomCoverage>,
    strings: Vec<Vec<u8>>,
    prototypes: Vec<PrototypeInfo>,
    main_prototype: u64,
}

fn transcode(
    bytecode: &[u8],
    dialect: LuauLayout<'_>,
    format: TranscodeFormat,
) -> Result<Transcoded, String> {
    let mut reader = Reader::new(bytecode);
    let mut output = Vec::with_capacity(bytecode.len());

    let version = reader.byte("bytecode version")?;
    if version != dialect.bytecode_version {
        return Err(format!(
            "adapter expects Warframe Luau v{}, found v{}",
            dialect.bytecode_version, version
        ));
    }
    output.push(version);

    let type_version = reader.byte("type version")?;
    if type_version != dialect.type_version {
        return Err(format!(
            "adapter expects Luau type version {}, found {}",
            dialect.type_version, type_version
        ));
    }
    output.push(type_version);

    let strings = copy_strings(&mut reader, &mut output)?;
    copy_userdata_remaps(&mut reader, &mut output, type_version)?;

    let proto_count = copy_count(&mut reader, &mut output, "prototype count")?;
    let mut coverage = BTreeMap::<u8, usize>::new();
    let mut atoms = BTreeMap::<u32, usize>::new();
    let mut prototypes = Vec::new();
    for proto in 0..proto_count {
        prototypes.push(transcode_proto(
            &mut reader,
            &mut output,
            dialect,
            format,
            proto,
            &mut coverage,
            &mut atoms,
        )?);
    }
    let main_prototype = reader.varint("main prototype")?;
    if main_prototype >= proto_count as u64 {
        return Err("main prototype index is out of range".into());
    }
    write_varint(&mut output, main_prototype);
    if !reader.is_empty() {
        return Err(format!(
            "{} trailing byte(s) after Luau chunk at offset {}",
            reader.remaining(),
            reader.offset
        ));
    }

    let coverage = coverage
        .into_iter()
        .map(|(raw, instructions)| {
            let opcode = opcode(dialect.opcodes, raw).expect("walk accepted only mapped opcodes");
            OpcodeCoverage {
                raw,
                canonical: opcode.canonical,
                name: opcode.name.clone(),
                instructions,
            }
        })
        .collect();
    let atoms = atoms
        .into_iter()
        .map(|(value, occurrences)| AtomCoverage {
            value,
            hex: format!("0x{value:08x}"),
            occurrences,
        })
        .collect();

    Ok(Transcoded {
        version,
        type_version,
        bytes: output,
        coverage,
        atoms,
        strings,
        prototypes,
        main_prototype,
    })
}

fn transcode_proto(
    reader: &mut Reader<'_>,
    output: &mut Vec<u8>,
    dialect: LuauLayout<'_>,
    format: TranscodeFormat,
    proto: usize,
    coverage: &mut BTreeMap<u8, usize>,
    atoms: &mut BTreeMap<u32, usize>,
) -> Result<PrototypeInfo, String> {
    let header = reader.bytes(4, "prototype header")?.to_vec();
    output.extend_from_slice(&header);
    let flags = reader.byte("prototype flags")?;
    output.push(flags);
    let type_bytes = copy_count(reader, output, "prototype type information length")?;
    let type_info = reader
        .bytes(type_bytes, "prototype type information")?
        .to_vec();
    output.extend_from_slice(&type_info);

    let code_count = reader.count("instruction word count")?;
    write_varint(output, code_count as u64);
    let mut words = Vec::with_capacity(code_count);
    let code_offset = reader.offset;
    for _ in 0..code_count {
        words.push(reader.u32("instruction word")?);
    }
    let raw_words = words.clone();
    let uncertain_from_pc = if matches!(format, TranscodeFormat::Inspect) {
        let mut pc = 0;
        while pc < words.len() {
            let Some(mapping) = opcode(dialect.opcodes, words[pc] as u8) else {
                break;
            };
            if mapping.has_aux && pc + 1 == words.len() {
                break;
            }
            *coverage.entry(mapping.raw).or_default() += 1;
            pc += if mapping.has_aux { 2 } else { 1 };
        }
        (pc < words.len()).then_some(pc)
    } else {
        transcode_code(&mut words, dialect.opcodes, proto, coverage)?;
        None
    };
    for word in words {
        output.extend_from_slice(&word.to_le_bytes());
    }

    let constant_count = copy_count(reader, output, "constant count")?;
    let mut constants = Vec::new();
    for constant in 0..constant_count {
        let start = reader.offset;
        transcode_constant(
            reader,
            output,
            dialect.boolean_bytes,
            proto,
            constant,
            atoms,
        )?;
        constants.push(reader.data[start..reader.offset].to_vec());
    }

    let child_count = copy_count(reader, output, "child prototype count")?;
    let mut children = Vec::new();
    for _ in 0..child_count {
        let child = reader.varint("child prototype")?;
        children.push(child);
        write_varint(output, child);
    }
    let line_defined = reader.varint("line defined")?;
    write_varint(output, line_defined);
    let debug_name = reader.varint("debug name")?;
    write_varint(output, debug_name);
    copy_line_info(reader, output, code_count)?;
    copy_debug_info(reader, output)?;
    Ok(PrototypeInfo {
        index: proto,
        code_offset,
        max_stack_size: header[0],
        parameters: header[1],
        upvalues: header[2],
        vararg: header[3] != 0,
        flags,
        type_info,
        words: raw_words,
        uncertain_from_pc,
        constants,
        children,
        line_defined,
        debug_name,
    })
}

fn transcode_code(
    words: &mut [u32],
    opcodes: &[LuauOpcode],
    proto: usize,
    coverage: &mut BTreeMap<u8, usize>,
) -> Result<(), String> {
    let mut pc = 0;
    while pc < words.len() {
        let raw = words[pc] as u8;
        let Some(mapping) = opcode(opcodes, raw) else {
            return Err(format!(
                "unmapped Warframe opcode 0x{raw:02x} in prototype {proto} at pc {pc}"
            ));
        };
        words[pc] = (words[pc] & !0xff) | u32::from(mapping.canonical);
        *coverage.entry(raw).or_default() += 1;
        pc += 1;
        if mapping.has_aux {
            if pc >= words.len() {
                return Err(format!(
                    "{} at prototype {proto} pc {} is missing its AUX word",
                    mapping.name,
                    pc - 1
                ));
            }
            pc += 1;
        }
    }
    Ok(())
}

fn transcode_constant(
    reader: &mut Reader<'_>,
    output: &mut Vec<u8>,
    boolean_bytes: usize,
    proto: usize,
    constant: usize,
    atoms: &mut BTreeMap<u32, usize>,
) -> Result<(), String> {
    let tag = reader.byte("constant tag")?;
    output.push(tag);
    match tag {
        0 => Ok(()),
        1 => {
            let bytes = reader.bytes(boolean_bytes, "boolean constant")?;
            let value = match bytes {
                [value] => u32::from(*value),
                [a, b, c, d] => u32::from_le_bytes([*a, *b, *c, *d]),
                _ => {
                    return Err(format!(
                        "unsupported Warframe boolean width: {boolean_bytes}"
                    ));
                }
            };
            output.extend_from_slice(bytes);
            if value > 1 {
                *atoms.entry(value).or_default() += 1;
            }
            Ok(())
        }
        2 => copy_fixed(reader, output, 8, "number constant"),
        3 => copy_varint(reader, output, "string constant"),
        4 => copy_fixed(reader, output, 4, "import constant"),
        5 => {
            let count = copy_count(reader, output, "table constant length")?;
            for _ in 0..count {
                copy_varint(reader, output, "table constant key")?;
            }
            Ok(())
        }
        6 => copy_varint(reader, output, "closure constant"),
        7 => copy_fixed(reader, output, 16, "vector constant"),
        8 => {
            let count = copy_count(reader, output, "initialized table constant length")?;
            for _ in 0..count {
                copy_varint(reader, output, "initialized table key")?;
                copy_fixed(reader, output, 4, "initialized table value")?;
            }
            Ok(())
        }
        _ => Err(format!(
            "unsupported Warframe constant tag {tag} in prototype {proto}, constant {constant}"
        )),
    }
}

fn copy_strings(reader: &mut Reader<'_>, output: &mut Vec<u8>) -> Result<Vec<Vec<u8>>, String> {
    let count = copy_count(reader, output, "string count")?;
    let mut strings = Vec::new();
    for _ in 0..count {
        let length = copy_count(reader, output, "string length")?;
        let bytes = reader.bytes(length, "string")?;
        strings.push(bytes.to_vec());
        output.extend_from_slice(bytes);
    }
    Ok(strings)
}

fn copy_userdata_remaps(
    reader: &mut Reader<'_>,
    output: &mut Vec<u8>,
    type_version: u8,
) -> Result<(), String> {
    if type_version != 3 {
        return Ok(());
    }
    loop {
        let index = reader.byte("userdata remap index")?;
        output.push(index);
        if index == 0 {
            return Ok(());
        }
        copy_varint(reader, output, "userdata remap name")?;
    }
}

fn copy_line_info(
    reader: &mut Reader<'_>,
    output: &mut Vec<u8>,
    code_count: usize,
) -> Result<(), String> {
    let present = reader.byte("line information flag")?;
    output.push(present);
    if present == 0 {
        return Ok(());
    }
    let gap = reader.byte("line information gap")?;
    output.push(gap);
    output.extend_from_slice(reader.bytes(code_count, "line deltas")?);
    let intervals = if code_count == 0 {
        0
    } else {
        let shift = usize::from(gap);
        if shift >= usize::BITS as usize {
            return Err(format!("invalid line information gap: {gap}"));
        }
        ((code_count - 1) >> shift) + 1
    };
    copy_fixed(
        reader,
        output,
        intervals.saturating_mul(4),
        "absolute line information",
    )
}

fn copy_debug_info(reader: &mut Reader<'_>, output: &mut Vec<u8>) -> Result<(), String> {
    let present = reader.byte("debug information flag")?;
    output.push(present);
    if present == 0 {
        return Ok(());
    }
    let local_count = copy_count(reader, output, "debug local count")?;
    for _ in 0..local_count {
        copy_varint(reader, output, "debug local name")?;
        copy_varint(reader, output, "debug local start")?;
        copy_varint(reader, output, "debug local end")?;
        output.push(reader.byte("debug local register")?);
    }
    let upvalue_count = copy_count(reader, output, "debug upvalue count")?;
    for _ in 0..upvalue_count {
        copy_varint(reader, output, "debug upvalue name")?;
    }
    Ok(())
}

fn copy_count(reader: &mut Reader<'_>, output: &mut Vec<u8>, name: &str) -> Result<usize, String> {
    let count = reader.count(name)?;
    write_varint(output, count as u64);
    Ok(count)
}

fn copy_varint(reader: &mut Reader<'_>, output: &mut Vec<u8>, name: &str) -> Result<(), String> {
    let value = reader.varint(name)?;
    write_varint(output, value);
    Ok(())
}

fn copy_fixed(
    reader: &mut Reader<'_>,
    output: &mut Vec<u8>,
    length: usize,
    name: &str,
) -> Result<(), String> {
    output.extend_from_slice(reader.bytes(length, name)?);
    Ok(())
}

fn write_varint(output: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            return;
        }
    }
}

fn opcode(opcodes: &[LuauOpcode], raw: u8) -> Option<&LuauOpcode> {
    opcodes.iter().find(|opcode| opcode.raw == raw)
}

struct Reader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn byte(&mut self, name: &str) -> Result<u8, String> {
        Ok(self.bytes(1, name)?[0])
    }

    fn u32(&mut self, name: &str) -> Result<u32, String> {
        let bytes: [u8; 4] = self.bytes(4, name)?.try_into().expect("four-byte slice");
        Ok(u32::from_le_bytes(bytes))
    }

    fn varint(&mut self, name: &str) -> Result<u64, String> {
        let start = self.offset;
        let mut value = 0u64;
        for shift in (0..=63).step_by(7) {
            let byte = self.byte(name)?;
            if shift == 63 && byte > 1 {
                return Err(format!("{name} at offset {start} is too large"));
            }
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(format!("{name} at offset {start} is too large"))
    }

    fn count(&mut self, name: &str) -> Result<usize, String> {
        let value = self.varint(name)?;
        let value = usize::try_from(value).map_err(|_| format!("{name} does not fit usize"))?;
        if value > self.data.len() {
            return Err(format!(
                "unreasonable {name} {value} at offset {}",
                self.offset
            ));
        }
        Ok(value)
    }

    fn bytes(&mut self, length: usize, name: &str) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| format!("{name} length overflows"))?;
        let bytes = self.data.get(self.offset..end).ok_or_else(|| {
            format!(
                "unexpected end of bytecode reading {name} at offset {}",
                self.offset
            )
        })?;
        self.offset = end;
        Ok(bytes)
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.offset
    }

    fn is_empty(&self) -> bool {
        self.offset == self.data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADAPTER: &str = "d01b5cb5cff5";

    pub(super) fn fixture(raw_opcode: u8, constant: u32) -> Vec<u8> {
        vec![
            9,
            3, // bytecode and type versions
            0, // strings
            0, // userdata remaps
            1, // prototypes
            1,
            0,
            0,
            0, // stack, params, upvalues, vararg
            0,
            0, // flags, type information
            1,
            raw_opcode,
            0,
            1,
            0, // one RETURN instruction
            1,
            1, // one tag-1 constant
            constant as u8,
            (constant >> 8) as u8,
            (constant >> 16) as u8,
            (constant >> 24) as u8,
            0, // children
            0, // line defined
            0, // debug name
            0, // line information
            0, // debug information
            0, // main prototype
        ]
    }

    #[test]
    fn transcodes_warframe_boolean_and_opcode() {
        let report = info(&fixture(0x29, 1), &Profile::builtin(ADAPTER).unwrap()).unwrap();
        assert_eq!(report.bytecode_version, 9);
        assert_eq!(report.prototypes.len(), 1);
        assert_eq!(report.opcode_coverage[0].name, "RETURN");
        assert_eq!(report.prototypes[0].uncertain_from_pc, None);
    }

    #[test]
    fn transcodes_setup_operations_without_rewriting_aux_words() {
        let adapter = Profile::builtin(ADAPTER).unwrap();
        for (raw, canonical, name, aux) in [
            (0x09, 41, "MULK", false),
            (0x0c, 75, "FASTCALL2K", true),
            (0x1a, 36, "DIV", false),
            (0x23, 28, "JUMPIFLE", true),
            (0x2b, 46, "OR", false),
            (0x2e, 18, "SETTABLEN", false),
            (0x33, 31, "JUMPIFNOTLE", true),
            (0x39, 11, "CLOSEUPVALS", false),
            (0x3c, 43, "MODK", false),
            (0x51, 48, "ORK", false),
            (0x53, 10, "SETUPVAL", false),
        ] {
            let mut words = vec![0x0302_0100 | raw];
            if aux {
                words.push(0xfedc_bafe);
            }
            words.push(0x0002_0129);
            let mut coverage = BTreeMap::new();
            transcode_code(&mut words, &adapter.mapping, 0, &mut coverage).unwrap();
            assert_eq!(words[0], 0x0302_0100 | canonical, "{name}");
            if aux {
                assert_eq!(words[1], 0xfedc_bafe, "{name}");
                assert!(
                    transcode_code(&mut [raw], &adapter.mapping, 0, &mut coverage)
                        .unwrap_err()
                        .contains("missing its AUX word")
                );
            }
            assert_eq!(*words.last().unwrap(), 0x0002_0116);
            assert_eq!(opcode(&adapter.mapping, raw as u8).unwrap().name, name);
        }
    }

    #[test]
    fn unknown_opcode_fails_with_location() {
        let report = info(&fixture(0xfe, 1), &Profile::builtin(ADAPTER).unwrap()).unwrap();
        assert_eq!(report.prototypes[0].uncertain_from_pc, Some(0));
        assert_eq!(report.prototypes[0].words[0] as u8, 0xfe);
        assert_eq!(report.prototypes[0].constants.len(), 1);
        let error = normalize(&fixture(0xfe, 1), &Profile::builtin(ADAPTER).unwrap()).unwrap_err();
        assert!(error.contains("opcode 0xfe"));
        assert!(error.contains("prototype 0 at pc 0"));
    }

    #[test]
    fn disassembly_preserves_atoms_and_uses_exact_opcode_profile() {
        let adapter = Profile::builtin(ADAPTER).unwrap();
        let out = disassemble(&fixture(0x29, 0x1234_5678), &adapter).unwrap();
        assert!(out.contains("__wf_atom_12345678"));
        assert!(out.contains("RETURN"));
        let forprep = adapter
            .mapping
            .iter()
            .find(|op| op.name == "FORGPREP")
            .unwrap()
            .raw;
        let out = disassemble(&fixture(forprep, 0), &adapter).unwrap();
        assert!(out.contains("FORGPREP"));
        assert!(!out.contains("IDIV"));
    }

    #[test]
    fn normalized_stream_preserves_warframe_atoms() {
        let raw = fixture(0x29, 0x1234_5678);
        let normalized = normalize(&raw, &Profile::builtin(ADAPTER).unwrap()).unwrap();
        assert_eq!(normalized.len(), raw.len());
        assert_eq!(normalized[0], 9);
        assert_eq!(&normalized[18..22], &0x1234_5678u32.to_le_bytes());
    }

    #[test]
    fn parses_last_helper_diagnostic_line() {
        let diagnostic = parse_json_line::<DecompilerDiagnostic>(
            b"warning\n{\"protocol\":1,\"tool\":\"wf-luau-decompiler\",\"ok\":true}\n",
        )
        .unwrap();
        assert_eq!(diagnostic.protocol, DECOMPILER_PROTOCOL);
        assert!(diagnostic.ok);
    }

    #[test]
    fn rejects_varints_larger_than_u64() {
        let mut bytes = vec![0x80; 9];
        bytes.push(2);
        let error = Reader::new(&bytes).varint("value").unwrap_err();
        assert!(error.contains("too large"));
    }
}
