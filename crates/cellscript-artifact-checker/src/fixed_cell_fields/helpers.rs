//! Caller storage preservation for the existing finite membership helper.
//! This checks its memory/return boundary, not role authorization semantics.
use super::*;
use crate::checker::{block_for_address, is_sd, last_register_definition_before, register_constant_before};

/// Existing inline capacity/hash observations must preserve received Cell data.
/// Their two actual write ranges are independently bounded and disjoint from
/// scalar locals, saved state and every checked Cell read. No role predicate
/// or syscall-success interpretation is inferred from this memory proof.
pub(super) fn check_owner_fixed_observation(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    instructions: &BTreeMap<u64, u32>,
    storage: &CheckedFixedCellParameterStorage,
    owner: &str,
    address: u64,
    frame: u32,
) -> Result<(), CheckerError> {
    let block = block_for_address(record, address).ok_or_else(|| invalid("unowned observation syscall"))?;
    let capacity_bytes = match register_constant_before(elf, block, address, 17) {
        Some(2062) => 32,
        Some(2081) => match register_constant_before(elf, block, address, 15) {
            Some(0) => 8,
            // Pinned CKB CellField: LockHash=3 and TypeHash=5.
            Some(3 | 5) => 32,
            _ => return Err(invalid("Cell-field observation lacks a fixed capacity/hash field")),
        },
        _ => return Err(invalid("owner syscall lacks a proved Cell-read or fixed observation boundary")),
    };
    let definition = |register| {
        last_register_definition_before(elf, block, address, register).ok_or_else(|| invalid("missing observation pointer definition"))
    };
    let (buffer_at, buffer) = definition(10)?;
    let (_, size) = definition(11)?;
    let (_, byte_offset) = definition(12)?;
    let direct = |instruction: u32, register| {
        let offset = (instruction as i32) >> 20;
        if offset < 0 || !is_addi(instruction, register, 2, offset) {
            return Err(invalid("observation pointer has an unproved base"));
        }
        Ok(offset as u32)
    };
    let buffer = direct(buffer, 10)?;
    let size = direct(size, 11)?;
    let scalar_end = storage
        .reads()
        .reads()
        .iter()
        .filter(|read| read.entry == owner)
        .map(|read| read.size_offset)
        .min()
        .ok_or_else(|| invalid("observation owner lacks bounded scalar storage"))?;
    let overlaps = |start: u32, width: u32, other: u32, other_width: u32| start < other + other_width && other < start + width;
    if !is_addi(byte_offset, 12, 0, 0)
        || buffer < scalar_end
        || size < scalar_end
        || buffer.checked_add(capacity_bytes).is_none_or(|end| end > frame - 16)
        || size.checked_add(8).is_none_or(|end| end > frame - 16)
        || overlaps(buffer, capacity_bytes, size, 8)
        || storage.reads().reads().iter().filter(|read| read.entry == owner).any(|read| {
            overlaps(buffer, capacity_bytes, read.buffer_offset, 512)
                || overlaps(size, 8, read.buffer_offset, 512)
                || overlaps(buffer, capacity_bytes, read.size_offset, 8)
                || overlaps(size, 8, read.size_offset, 8)
        })
    {
        return Err(invalid("observation write ranges overlap checked Cell/scalar/saved storage"));
    }
    let capacity = instructions
        .range(..address)
        .rev()
        .find(|(_, instruction)| {
            if **instruction & 0x7f != 0x23 || ((**instruction >> 15) & 31) != 2 {
                return false;
            }
            let offset = (((**instruction >> 25) << 5) | ((**instruction >> 7) & 31)) as i32;
            let offset = (offset << 20) >> 20;
            let width = 1u32 << ((**instruction >> 12) & 7);
            offset >= 0 && overlaps(offset as u32, width, size, 8)
        })
        .map(|(&address, &instruction)| (address, instruction))
        .ok_or_else(|| invalid("observation lacks capacity initialization"))?;
    let initialization = capacity.0.checked_sub(4).ok_or_else(|| invalid("truncated observation initialization"))?;
    if !is_sd(capacity.1, 5, 2, size as i32)
        || capacity.0 >= buffer_at
        || !is_addi(word(elf, initialization)?, 5, 0, capacity_bytes as i32)
        || elf.control_flow.iter().any(|flow| {
            (initialization <= flow.address && flow.address < address) || (initialization < flow.target && flow.target <= address)
        })
    {
        return Err(invalid("observation capacity or initialization dominance differs"));
    }
    Ok(())
}

pub(super) fn check_membership_frame(record: &VerifiedLoweringRecord, elf: &ParsedElf, target: u64) -> Result<(), CheckerError> {
    let owner = "runtime:__cellscript_require_cell_membership";
    if target != entry_start(record, owner)? || !is_addi(word(elf, target)?, 2, 2, -96) {
        return Err(invalid("membership call lacks its actual private frame"));
    }
    let instructions = owned_instructions(record, elf, owner, 256)?;
    let flows = elf.control_flow.iter().map(|flow| (flow.address, flow.target)).collect::<BTreeMap<_, _>>();
    if elf.control_flow.iter().any(|flow| {
        (instructions.contains_key(&flow.address) && (!instructions.contains_key(&flow.target) || flow.target <= flow.address))
            || (instructions.contains_key(&flow.target) && !instructions.contains_key(&flow.address) && flow.target != target)
    }) {
        return Err(invalid("membership helper has escaping, backward or incoming interior flow"));
    }
    let mut restore = None;
    let mut returns = Vec::new();
    let mut syscalls = 0;
    for (&address, &instruction) in &instructions {
        let opcode = instruction & 0x7f;
        let rd = (instruction >> 7) & 31;
        let rs1 = (instruction >> 15) & 31;
        if instruction == 0x00008067 {
            returns.push(address);
            continue;
        }
        if !matches!(opcode, 0x6f | 0x67) && !instructions.contains_key(&(address + 4)) {
            return Err(invalid("membership helper falls out of its owned instruction range"));
        }
        if opcode == 0x67 || (opcode == 0x6f && rd != 0) {
            return Err(invalid("membership helper makes an unproved call"));
        }
        if matches!(opcode, 0x03 | 0x13 | 0x17 | 0x1b | 0x33 | 0x37 | 0x3b | 0x6f) {
            if rd == 2 {
                if address == target && is_addi(instruction, 2, 2, -96) {
                    continue;
                }
                if is_addi(instruction, 2, 2, 96) && word(elf, address + 4)? == 0x00008067 && restore.replace(address).is_none() {
                    continue;
                }
                return Err(invalid("membership helper changes its private stack base"));
            }
            if !matches!(rd, 0 | 5..=7 | 10..=17) {
                return Err(invalid("membership helper clobbers protected caller registers"));
            }
        }
        if matches!(opcode, 0x03 | 0x23) {
            let offset = if opcode == 0x03 {
                (instruction as i32) >> 20
            } else {
                let offset = (((instruction >> 25) << 5) | ((instruction >> 7) & 31)) as i32;
                (offset << 20) >> 20
            };
            let width = 1u32 << (((instruction >> 12) & 7) & 3);
            if rs1 != 2 || offset < 0 || offset as u32 + width > 96 {
                return Err(invalid("membership helper accesses outside its private frame"));
            }
        }
        if instruction == 0x00000073 {
            syscalls += 1;
            let block = block_for_address(record, address).ok_or_else(|| invalid("unowned membership syscall"))?;
            if !matches!(register_constant_before(elf, block, address, 17), Some(2062 | 2081)) {
                return Err(invalid("membership helper uses an unproved syscall"));
            }
            let definition = |register| {
                last_register_definition_before(elf, block, address, register)
                    .ok_or_else(|| invalid("missing membership syscall pointer definition"))
            };
            let (buffer_at, buffer) = definition(10)?;
            let (_, size) = definition(11)?;
            let (_, byte_offset) = definition(12)?;
            if !(is_addi(buffer, 10, 2, 8) || is_addi(buffer, 10, 2, 40))
                || !is_addi(size, 11, 2, 0)
                || !is_addi(byte_offset, 12, 0, 0)
            {
                return Err(invalid("membership syscall escapes its private buffers"));
            }
            let capacity = instructions
                .range(..address)
                .rev()
                .find(|(_, instruction)| {
                    **instruction & 0x7f == 0x23
                        && ((**instruction >> 15) & 31) == 2
                        && (((**instruction >> 25) << 5) | ((**instruction >> 7) & 31)) < 8
                })
                .map(|(&address, &instruction)| (address, instruction))
                .ok_or_else(|| invalid("missing membership capacity store"))?;
            if !is_sd(capacity.1, 5, 2, 0)
                || capacity.0 >= buffer_at
                || !is_addi(word(elf, capacity.0 - 4)?, 5, 0, 32)
                || elf.control_flow.iter().any(|flow| {
                    (capacity.0 - 4 <= flow.address && flow.address < address)
                        || (capacity.0 - 4 < flow.target && flow.target <= address)
                })
            {
                return Err(invalid("membership capacity or initialization dominance differs"));
            }
        }
    }
    let restore = restore.ok_or_else(|| invalid("membership helper never restores its private frame"))?;
    let bypass = reachable_without(&instructions, &flows, target, Some(restore), &BTreeSet::new());
    if returns.len() != 1 || syscalls != 3 || returns.iter().any(|address| bypass.contains(address)) {
        return Err(invalid("membership helper lacks its bounded, restored return"));
    }
    Ok(())
}
