use iced_x86::{Code, Decoder, DecoderOptions, Instruction, Mnemonic, OpKind, Register};
use pelite::pe64::{Pe, PeFile};
use serde::Serialize;

use super::super::executable::{self, ResourceLayout};

const ANCHOR: &str = r#"' 48 8d 15 ${ "AddAttachedUpgradesToWeapon: " 00 }"#;
const SLOTS: &str = "
    49 8b 07 49 8b cf ff 90 u4 48 85 c0 74 ? [0-16]
    49 8b 07 48 8b d3 49 8b cf ff 90 u4 [0-128]
    49 8b 07 49 8b cf 48 ff c3 ff 90 u4 48 3b d8 72 ?
";

#[derive(Clone, Copy, Debug, Serialize)]
pub(super) struct Layout {
    pub caller_rva: u32,
    pub count_slot: u64,
    pub index_slot: u64,
    pub weapon_descriptor_rva: u64,
    pub resources: ResourceLayout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(super) struct Accessors {
    pub count_rva: u32,
    pub index_rva: u32,
    pub vector_offset: u64,
    pub stride: usize,
    pub entry_holder_offset: usize,
    pub holder_target_offset: u64,
    pub count_code_bytes: usize,
    pub index_code_bytes: usize,
}

impl Layout {
    pub fn discover(image: PeFile<'_>) -> Result<Self, String> {
        let anchor = executable::signature::<2>(image, "attached upgrades anchor", ANCHOR)?;
        let caller = executable::function_range(image, anchor[1])?;
        if caller.end - caller.start > 64 * 1024 {
            return Err("attached upgrades caller exceeds code limit".into());
        }
        section(
            image,
            caller.start.into(),
            (caller.end - caller.start) as usize,
            true,
        )?;
        let slots =
            executable::signature_in::<4>(image, "attached upgrades loop", SLOTS, caller.clone())?;
        if slots[1] != slots[3]
            || slots[1] == slots[2]
            || ![slots[1], slots[2]]
                .iter()
                .all(|&slot| (8..0x4000).contains(&slot) && slot.is_multiple_of(8))
        {
            return Err("attached upgrades virtual slots disagree".into());
        }
        // Only the named weapon registration and shared resource accessors are needed.
        let (weapon_descriptor_rva, constructor) =
            executable::descriptor(image, "LotusWeapon", "/Lotus/Types/Game/")?;
        let resources = ResourceLayout::discover(image, constructor)?;
        executable::writable(image, weapon_descriptor_rva, resources.descriptor_size())?;
        Ok(Self {
            caller_rva: caller.start,
            count_slot: slots[1].into(),
            index_slot: slots[2].into(),
            weapon_descriptor_rva,
            resources,
        })
    }
}

impl Accessors {
    pub fn decode(image: PeFile<'_>, count_rva: u32, index_rva: u32) -> Result<Self, String> {
        let (count, count_code_bytes) = instructions(image, count_rva, 3)?;
        let (index, index_code_bytes) = instructions(image, index_rva, 11)?;
        let invalid = || "unrecognized attached upgrades accessors".to_owned();
        let (length_offset, stride) = count_fields(&count).ok_or_else(invalid)?;
        let (index_length, index_stride) = count_fields(&index).ok_or_else(invalid)?;
        if count[2].code() != Code::Retnq
            || index_length != length_offset
            || index_stride != stride
            || !registers(&index[2], Mnemonic::Cmp, Register::RDX, Register::RAX)
            || index[3].mnemonic() != Mnemonic::Jb
            || index[3].near_branch_target() != index[6].ip()
            || !registers(&index[4], Mnemonic::Xor, Register::EAX, Register::EAX)
            || index[5].code() != Code::Retnq
            || !load(&index[6], Register::RAX, Register::RCX, Register::None)
            || !load(&index[8], Register::RAX, Register::RAX, Register::RDX)
            || !load(&index[9], Register::RAX, Register::RAX, Register::None)
            || index[10].code() != Code::Retnq
        {
            return Err(invalid());
        }
        let multiplier = if registers(&index[7], Mnemonic::Add, Register::RDX, Register::RDX) {
            2
        } else if index[7].mnemonic() == Mnemonic::Shl
            && index[7].op0_kind() == OpKind::Register
            && index[7].op0_register() == Register::RDX
            && index[7].op1_kind() == OpKind::Immediate8
            && index[7].immediate8() <= 3
        {
            1usize << index[7].immediate8()
        } else {
            return Err(invalid());
        };
        let vector_offset = index[6].memory_displacement64();
        let entry_holder_offset = index[8].memory_displacement64();
        let holder_target_offset = index[9].memory_displacement64();
        if !(0x20..0x10000).contains(&vector_offset)
            || !vector_offset.is_multiple_of(8)
            || vector_offset + 8 != length_offset
            || index[8].memory_index_scale() as usize * multiplier != stride
            || entry_holder_offset
                .checked_add(8)
                .is_none_or(|end| end > stride as u64)
            || !entry_holder_offset.is_multiple_of(8)
            || holder_target_offset > 0x40
            || !holder_target_offset.is_multiple_of(8)
        {
            return Err("inconsistent attached upgrades vector layout".into());
        }
        Ok(Self {
            count_rva,
            index_rva,
            vector_offset,
            stride,
            entry_holder_offset: entry_holder_offset as usize,
            holder_target_offset,
            count_code_bytes,
            index_code_bytes,
        })
    }
}

fn count_fields(code: &[Instruction]) -> Option<(u64, usize)> {
    let load_length = &code[0];
    let shift = &code[1];
    (load(load_length, Register::EAX, Register::RCX, Register::None)
        && shift.mnemonic() == Mnemonic::Shr
        && shift.op0_kind() == OpKind::Register
        && matches!(shift.op0_register(), Register::RAX | Register::EAX)
        && shift.op1_kind() == OpKind::Immediate8
        && (3..=6).contains(&shift.immediate8()))
    .then(|| {
        (
            load_length.memory_displacement64(),
            1usize << shift.immediate8(),
        )
    })
}

fn registers(instruction: &Instruction, mnemonic: Mnemonic, a: Register, b: Register) -> bool {
    instruction.mnemonic() == mnemonic
        && instruction.op0_kind() == OpKind::Register
        && instruction.op1_kind() == OpKind::Register
        && instruction.op0_register() == a
        && instruction.op1_register() == b
}

fn load(instruction: &Instruction, target: Register, base: Register, index: Register) -> bool {
    instruction.mnemonic() == Mnemonic::Mov
        && instruction.op0_kind() == OpKind::Register
        && instruction.op0_register() == target
        && instruction.op1_kind() == OpKind::Memory
        && instruction.memory_base() == base
        && instruction.memory_index() == index
        && instruction.segment_prefix() == Register::None
}

fn instructions(
    image: PeFile<'_>,
    rva: u32,
    count: usize,
) -> Result<(Vec<Instruction>, usize), String> {
    section(image, rva.into(), 1, true)?;
    let bytes = image
        .slice(rva, 1, 1)
        .map_err(|e| format!("accessor code: {e}"))?;
    let mut decoder = Decoder::with_ip(
        64,
        &bytes[..bytes.len().min(96)],
        rva.into(),
        DecoderOptions::NONE,
    );
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        let instruction = decoder.decode();
        if instruction.is_invalid()
            || instruction.has_lock_prefix()
            || instruction.has_rep_prefix()
            || instruction.has_repne_prefix()
        {
            return Err("invalid attached upgrades accessor code".into());
        }
        result.push(instruction);
    }
    section(image, rva.into(), decoder.position(), true)?;
    Ok((result, decoder.position()))
}

pub(super) fn section(image: PeFile<'_>, rva: u64, size: usize, code: bool) -> Result<(), String> {
    let flags = if code { 0x6000_0000 } else { 0x4000_0000 };
    if image.section_headers().iter().any(|s| {
        s.Characteristics & 0xe000_0000 == flags
            && rva >= u64::from(s.VirtualAddress)
            && rva
                .checked_add(size as u64)
                .is_some_and(|end| end <= u64::from(s.VirtualAddress) + u64::from(s.VirtualSize))
    }) {
        Ok(())
    } else {
        Err(format!(
            "equipment binding 0x{rva:x} outside {}",
            if code {
                "executable code"
            } else {
                "read-only image data"
            }
        ))
    }
}
