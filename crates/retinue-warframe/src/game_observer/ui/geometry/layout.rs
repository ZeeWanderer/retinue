use std::collections::BTreeMap;

use iced_x86::{Decoder, DecoderOptions, Instruction, Mnemonic, OpKind, Register};
use pelite::pe64::{Pe, PeFile};
use serde::{Deserialize, Serialize};

use crate::game_observer::{adapter::ScaleformLayout, executable};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Layout {
    pub matrix: usize,
    pub parent: usize,
    pub parent_control: usize,
    pub name: usize,
    pub alpha: usize,
    pub flags: usize,
    pub leaf_definitions: BTreeMap<u64, usize>,
    pub definition_rects: BTreeMap<u64, usize>,
}

impl Layout {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.matrix > 0x1000
            || !self.matrix.is_multiple_of(4)
            || self.parent_control != self.matrix + 64
            || self.parent != self.matrix + 72
            || self.name != self.matrix + 80
            || self.alpha != self.matrix + 120
            || self.flags <= self.alpha + 8
            || self.flags > 0x2000
            || self.leaf_definitions.is_empty()
            || self.leaf_definitions.len() > 128
            || self.definition_rects.is_empty()
            || self.definition_rects.len() > 128
            || self.leaf_definitions.iter().any(|(&rva, &offset)| {
                !valid_rva(rva)
                    || !(self.flags + 1..=0x1000).contains(&offset)
                    || !offset.is_multiple_of(8)
            })
            || self.definition_rects.iter().any(|(&rva, &offset)| {
                !valid_rva(rva) || !(0x10..=0x1000).contains(&offset) || !offset.is_multiple_of(4)
            })
        {
            return Err("invalid display geometry bindings".into());
        }
        Ok(())
    }
}

fn valid_rva(rva: u64) -> bool {
    (0x1000..0xffff_ff00).contains(&rva) && rva.is_multiple_of(8)
}

pub(crate) fn discover(bytes: &[u8], display: ScaleformLayout) -> Result<Layout, String> {
    let image = PeFile::from_bytes(bytes).map_err(|error| error.to_string())?;
    let base = super::super::layout::display_base(image)?;
    let range = executable::function_range(image, base)?;
    let code = image
        .derva_slice::<u8>(range.start, (range.end - range.start) as usize)
        .map_err(|error| error.to_string())?;
    let mut layout = constructor(code)?;
    let text_table = u32::try_from(display.text_vtable_rva).map_err(|_| "invalid text vtable")?;
    let bounds = *image
        .derva::<u64>(text_table + 0x240)
        .map_err(|error| error.to_string())?;
    let bounds_code = code_at(image, bounds, 64).ok_or("missing display bounds method")?;
    // This shared method delegates to the definition, then applies the local matrix.
    let instructions = decode(bounds_code);
    let calls: Vec<_> = instructions
        .iter()
        .filter(|i| i.mnemonic() == Mnemonic::Call)
        .filter(|i| i.op0_kind() == OpKind::Memory)
        .map(|i| i.memory_displacement64())
        .collect();
    if calls != [0x258, 0x28]
        || !instructions.iter().any(|i| {
            i.mnemonic() == Mnemonic::Lea
                && i.memory_base() == Register::RBX
                && i.memory_displacement64() == layout.matrix as u64
        })
    {
        return Err("unsupported display bounds delegation".into());
    }

    for section in image
        .section_headers()
        .iter()
        .filter(|s| s.Characteristics & 0xe000_0000 == 0x4000_0000)
    {
        let bytes = image
            .derva_slice::<u8>(section.VirtualAddress, section.SizeOfRawData as usize)
            .map_err(|error| error.to_string())?;
        for (index, entry) in bytes.chunks_exact(8).enumerate() {
            let target = u64::from_le_bytes(entry.try_into().unwrap());
            let rva = section.VirtualAddress as u64 + index as u64 * 8;
            if target == bounds && index >= 0x240 / 8 {
                let table = rva - 0x240;
                let accessor = image.derva::<u64>((table + 0x258) as u32).ok().copied();
                if let Some(offset) = accessor
                    .and_then(|a| code_at(image, a, 16))
                    .and_then(pointer_accessor)
                {
                    layout.leaf_definitions.insert(table, offset);
                }
            }
            if index >= 5
                && let Some(offset) = code_at(image, target, 16).and_then(rect_accessor)
            {
                layout.definition_rects.insert(rva - 0x28, offset);
            }
        }
    }
    if !layout
        .leaf_definitions
        .contains_key(&display.text_vtable_rva)
        || layout.definition_rects.is_empty()
    {
        return Err("display definition bounds not discovered".into());
    }
    layout.validate()?;
    Ok(layout)
}

fn constructor(bytes: &[u8]) -> Result<Layout, String> {
    let instructions = decode(bytes);
    let stores: Vec<_> = instructions
        .iter()
        .filter(|i| {
            i.mnemonic() == Mnemonic::Mov
                && i.op0_kind() == OpKind::Memory
                && i.memory_base() == Register::RBX
                && matches!(i.op1_kind(), OpKind::Immediate32 | OpKind::Immediate32to64)
                && i.immediate(1) == 0x3f800000
        })
        .map(|i| i.memory_displacement64() as usize)
        .collect();
    let &[matrix, m1, m2, m3, color, c1, c2, alpha] = stores.as_slice() else {
        return Err("unsupported display matrix/color constructor".into());
    };
    if matrix > 0x1000
        || [m1, m2, m3] != [matrix + 20, matrix + 40, matrix + 60]
        || [c1, c2, alpha] != [color + 8, color + 16, color + 24]
        || color != matrix + 96
    {
        return Err("inconsistent display matrix/color layout".into());
    }
    let parent_control = matrix + 64;
    let parent = matrix + 72;
    let name = matrix + 80;
    let stored = |offset, register| {
        instructions.iter().any(|i| {
            i.mnemonic() == Mnemonic::Mov
                && i.op0_kind() == OpKind::Memory
                && i.memory_base() == Register::RBX
                && i.memory_displacement64() == offset as u64
                && i.op1_kind() == OpKind::Register
                && i.op1_register() == register
        })
    };
    if !stored(parent_control, Register::RBP)
        || !stored(parent, Register::RDI)
        || !stored(name, Register::BPL)
        || !instructions.iter().any(|i| {
            i.mnemonic() == Mnemonic::Mov
                && i.op0_kind() == OpKind::Memory
                && i.memory_base() == Register::RBX
                && i.memory_displacement64() == (name + 15) as u64
                && i.op1_kind() == OpKind::Immediate8
                && i.immediate8() == 15
        })
    {
        return Err("unsupported display parent/name layout".into());
    }
    let flags: Vec<_> = instructions
        .iter()
        .filter(|i| {
            i.mnemonic() == Mnemonic::Movzx
                && i.op0_register() == Register::EAX
                && i.op1_kind() == OpKind::Memory
                && i.memory_base() == Register::RBX
        })
        .map(|i| i.memory_displacement64() as usize)
        .collect();
    let &[flags] = flags.as_slice() else {
        return Err("ambiguous display flags".into());
    };
    if flags <= alpha + 8 || flags > 0x2000 || !stored(flags, Register::AL) {
        return Err("invalid display flags layout".into());
    }
    Ok(Layout {
        matrix,
        parent,
        parent_control,
        name,
        alpha,
        flags,
        leaf_definitions: BTreeMap::new(),
        definition_rects: BTreeMap::new(),
    })
}

fn code_at(image: PeFile<'_>, address: u64, length: usize) -> Option<&[u8]> {
    let rva = u32::try_from(address.checked_sub(image.optional_header().ImageBase)?).ok()?;
    if !image.headers().code_range().contains(&rva) {
        return None;
    }
    image.derva_slice::<u8>(rva, length).ok()
}

fn decode(bytes: &[u8]) -> Vec<Instruction> {
    let mut result = Vec::new();
    for instruction in Decoder::new(64, bytes, DecoderOptions::NONE) {
        let end = matches!(instruction.mnemonic(), Mnemonic::Ret | Mnemonic::Int3);
        result.push(instruction);
        if end {
            break;
        }
    }
    result
}

fn pointer_accessor(bytes: &[u8]) -> Option<usize> {
    let instructions = decode(bytes);
    let [load, ret] = instructions.as_slice() else {
        return None;
    };
    let offset = load.memory_displacement64() as usize;
    (load.mnemonic() == Mnemonic::Mov
        && load.op0_register() == Register::RAX
        && load.op1_kind() == OpKind::Memory
        && load.memory_base() == Register::RCX
        && load.memory_index() == Register::None
        && (0x100..=0x1000).contains(&offset)
        && offset.is_multiple_of(8)
        && ret.mnemonic() == Mnemonic::Ret)
        .then_some(offset)
}

fn rect_accessor(bytes: &[u8]) -> Option<usize> {
    if !bytes.starts_with(&[0x0f, 0x10, 0x41]) && !bytes.starts_with(&[0x0f, 0x10, 0x81]) {
        return None;
    }
    let instructions = decode(bytes);
    let [load, store, ret] = instructions.as_slice() else {
        return None;
    };
    let offset = load.memory_displacement64() as usize;
    (load.mnemonic() == Mnemonic::Movups
        && load.op0_register() == Register::XMM0
        && load.op1_kind() == OpKind::Memory
        && load.memory_base() == Register::RCX
        && load.memory_index() == Register::None
        && (0x10..=0x1000).contains(&offset)
        && offset.is_multiple_of(4)
        && store.mnemonic() == Mnemonic::Movups
        && store.op0_kind() == OpKind::Memory
        && store.memory_base() == Register::RDX
        && store.memory_index() == Register::None
        && store.memory_displacement64() == 0
        && store.op1_register() == Register::XMM0
        && ret.mnemonic() == Mnemonic::Ret)
        .then_some(offset)
}

#[cfg(test)]
mod tests;
