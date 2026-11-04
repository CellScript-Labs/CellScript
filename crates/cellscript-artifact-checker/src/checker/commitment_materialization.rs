//! Bind post-authentication values to the exact opening pointer and width.
//! Only the finite emitted memcpy or byte-assembly forms are admitted.
use super::*;

pub(super) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    contract: &CommitmentMachineContract,
    start: u64,
    authenticated_branch: u64,
    materialized: u64,
) -> Result<(), CheckerError> {
    let error = || commitment_machine_error("authenticated opening materialization lost its source, extent or control-flow binding");
    let opening = contract.opening.ok_or_else(error)?;
    let source = CommitmentMachineValue::StackLoadedAddress { stack_slot: i64::from(local_stack_offset(opening)?), displacement: 0 };
    if contract.width == 0 || contract.width > COMMITMENT_RUNTIME_SCRATCH_BYTES {
        return Err(error());
    }
    // Authentication is the only incoming branch. No earlier branch can enter
    // the copy or result store, and no intervening store can retag its source.
    if elf.control_flow.iter().any(|edge| {
        start <= edge.target
            && edge.target <= materialized
            && !(start <= edge.address && edge.address <= materialized)
            && !(edge.address == authenticated_branch && edge.target == start)
    }) || elf
        .instructions
        .iter()
        .any(|instruction| start <= instruction.address && instruction.address < materialized && instruction.word & 0x7f == 0x23)
    {
        return Err(error());
    }
    let copies = exact_runtime_calls(record, elf, "__cellscript_memcpy_fixed", start, materialized)?;
    if copies.len() > 1
        || elf.instructions.iter().any(|instruction| {
            start <= instruction.address
                && instruction.address < materialized
                && matches!(instruction.word & 0x7f, 0x63 | 0x6f | 0x67)
                && !copies.contains(&instruction.address)
        })
    {
        return Err(error());
    }
    if let [copy] = copies.as_slice() {
        let args = commitment_registers_before(elf, start, *copy);
        let stored = commitment_registers_before(elf, start, materialized)[5];
        let CommitmentMachineValue::StackAddress(buffer) = args[11] else { return Err(error()) };
        let typed = record.typed_semantics.entries.iter().find(|entry| entry.id == contract.owner).ok_or_else(error)?;
        let scalar_end = typed
            .locals
            .iter()
            .map(|local| u64::from(local.id))
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .and_then(|slots| slots.checked_mul(8))
            .ok_or_else(error)?;
        let frame_end = record
            .entries
            .iter()
            .find(|entry| entry.id == contract.owner)
            .and_then(|entry| u64::from(entry.frame_size_bytes).checked_sub(16))
            .ok_or_else(error)?;
        let buffer = u64::try_from(buffer).map_err(|_| error())?;
        if args[10] != source
            || args[12] != CommitmentMachineValue::Constant(contract.width)
            || stored != args[11]
            || buffer < scalar_end
            || buffer.checked_add(contract.width).is_none_or(|end| end > frame_end)
        {
            return Err(error());
        }
        return Ok(());
    }
    // Scalar openings assemble exactly their little-endian bytes. An arbitrary
    // value store after authentication is not evidence of this reconstruction.
    if contract.width > 8 {
        return Err(error());
    }
    let store = commitment_word(elf, materialized)?;
    let end = if is_sd(store, 5, 31, 0) { materialized.checked_sub(12).ok_or_else(error)? } else { materialized };
    let first = end.checked_sub(contract.width * 12).ok_or_else(error)?;
    if first < start || !is_addi(commitment_word(elf, first)?, 5, 0, 0) || commitment_registers_before(elf, start, first)[29] != source
    {
        return Err(error());
    }
    let mut cursor = first + 4;
    for index in 0..contract.width {
        if !is_lbu(commitment_word(elf, cursor)?, 6, 29, index as i32) {
            return Err(error());
        }
        cursor += 4;
        if index != 0 {
            if !is_slli(commitment_word(elf, cursor)?, 6, 6, (index * 8) as u32) {
                return Err(error());
            }
            cursor += 4;
        }
        if !is_or(commitment_word(elf, cursor)?, 5, 5, 6) {
            return Err(error());
        }
        cursor += 4;
    }
    if cursor != end {
        return Err(error());
    }
    Ok(())
}
