//! Logical view of the assembler's conditional-branch relaxation.
//!
//! A far conditional branch is encoded as the inverted condition skipping one
//! JAL x0. Only the policy pattern matcher consumes this view. ELF decoding,
//! byte digests, CFG coverage and source-map validation retain real addresses.

use std::collections::{BTreeMap, BTreeSet};

use crate::{ParsedElf, VerifiedLoweringRecord};

pub(crate) fn normalize_relaxed_branches(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
) -> Result<Option<(VerifiedLoweringRecord, ParsedElf)>, String> {
    let targets: BTreeMap<_, _> = elf.control_flow.iter().map(|flow| (flow.address, flow.target)).collect();
    let mut replacements = BTreeMap::new();
    let mut removed = BTreeSet::new();
    for pair in elf.instructions.windows(2) {
        let branch = pair[0];
        let jump = pair[1];
        if branch.word & 0x7f == 0x63
            && jump.address == branch.address + 4
            && jump.word & 0xfff == 0x06f
            && targets.get(&branch.address) == Some(&(branch.address + 8))
            && record.blocks.iter().any(|block| block.range.contains(branch.address) && block.range.contains(jump.address))
        {
            let target = *targets.get(&jump.address).ok_or("relaxed branch has no decoded JAL target")?;
            replacements.insert(branch.address, target);
            removed.insert(jump.address);
        }
    }
    if removed.is_empty() {
        return Ok(None);
    }
    // Entering the second instruction separately would invalidate treating the
    // pair as one conditional transfer. Never erase such a machine boundary.
    if elf.control_flow.iter().any(|flow| removed.contains(&flow.target))
        || record.blocks.iter().any(|block| removed.contains(&block.range.start) || removed.contains(&block.range.end))
        || removed.contains(&elf.entry)
    {
        return Err("a relaxed-branch JAL is independently reachable or splits a declared block".into());
    }
    let relocate = |address: u64| -> Result<u64, String> {
        if removed.contains(&address) {
            return Err("evidence refers to the interior of a relaxed branch".into());
        }
        address.checked_sub(4 * removed.range(..address).count() as u64).ok_or_else(|| "logical instruction address underflow".into())
    };
    let mut logical_elf = elf.clone();
    logical_elf.instructions.retain(|instruction| !removed.contains(&instruction.address));
    for instruction in &mut logical_elf.instructions {
        if replacements.contains_key(&instruction.address) {
            // BEQ/BNE, BLT/BGE and BLTU/BGEU differ in funct3's low bit.
            instruction.word ^= 1 << 12;
        }
        instruction.address = relocate(instruction.address)?;
    }
    logical_elf.control_flow.retain(|flow| !removed.contains(&flow.address));
    for flow in &mut logical_elf.control_flow {
        flow.target = relocate(replacements.get(&flow.address).copied().unwrap_or(flow.target))?;
        flow.address = relocate(flow.address)?;
    }
    for adjustment in &mut logical_elf.stack_adjustments {
        adjustment.address = relocate(adjustment.address)?;
    }
    for address in &mut logical_elf.syscall_addresses {
        *address = relocate(*address)?;
    }
    logical_elf.entry = relocate(logical_elf.entry)?;
    logical_elf.instruction_count = logical_elf.instructions.len() as u64;
    logical_elf.text.size -= removed.len() as u64 * 4;

    let mut logical_record = record.clone();
    logical_record.text_range.start = relocate(logical_record.text_range.start)?;
    logical_record.text_range.end = relocate(logical_record.text_range.end)?;
    for block in &mut logical_record.blocks {
        block.range.start = relocate(block.range.start)?;
        block.range.end = relocate(block.range.end)?;
    }
    for site in &mut logical_record.syscall_sites {
        site.address = relocate(site.address)?;
    }
    for exit in logical_record.runtime_error_exits.iter_mut().chain(&mut logical_record.verifier_failure_exits) {
        exit.address = relocate(exit.address)?;
    }
    Ok(Some((logical_record, logical_elf)))
}
