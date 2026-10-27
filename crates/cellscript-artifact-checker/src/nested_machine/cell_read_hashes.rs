//! Consensus data-hash and fixed-slice contracts, including direct builtin calls.
//! Capacity, reported remaining length and published length are independently
//! bound to instructions; compiler metadata does not authorize memory writes.
use super::*;

pub(super) const SLICE: &str = "runtime:__ckb_cell_data_bytes32";

pub(crate) fn validate_all(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    for entry in &record.entries {
        match entry.id.as_str() {
            "runtime:__ckb_cell_data_hash_field" => hash_fields::validate(record, elf, &entry.id, 1)?,
            SLICE => validate(record, elf, &entry.id)?,
            _ => {}
        }
    }
    Ok(())
}

fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf, owner: &str) -> Result<(), CheckerError> {
    use Label::*;
    let entry = record.entries.iter().find(|entry| entry.id == owner).ok_or_else(|| invalid("missing Cell read helper"))?;
    let mut blocks = record.blocks.iter().filter(|block| block.owner_entry == owner).collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    let first = blocks.first().ok_or_else(|| invalid("empty Cell read helper"))?;
    let start = first.range.start;
    let end = blocks.last().unwrap().range.end;
    if first.id != entry.entry_block
        || blocks.windows(2).any(|pair| pair[0].range.end != pair[1].range.start)
        || elf.control_flow.iter().any(|edge| start < edge.target && edge.target < end && !(start..end).contains(&edge.address))
    {
        return Err(invalid("Cell read helper has partial-entry or noncontiguous code"));
    }
    let words = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect();
    let mut c = Decoder { pc: start, words: &words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
    // A disabled target may only fail without touching memory.
    if end - start == 8 {
        c.li(10, 1)?;
        c.exact(0x00008067)?;
        return Ok(());
    }
    c.addi(2, 2, -32)?;
    c.hash_store(1, 2, 24, false)?;
    for (reg, offset) in [(11, 0), (12, 8), (13, 16)] {
        c.hash_store(reg, 2, offset, false)?;
    }
    c.branch(0, 12, 0, HashFailed)?;
    c.branch(0, 13, 0, HashFailed)?;
    c.li(5, 32)?;
    c.hash_store(5, 13, 0, false)?;
    c.immediate(0x13, 5, 5, 10, 32)?;
    c.immediate(0x13, 1, 6, 10, 32)?;
    c.immediate(0x13, 5, 6, 6, 32)?;
    c.branch(0, 5, 0, Invalid)?;
    c.li(30, 4)?;
    c.branch(7, 30, 5, HashDirect)?;
    c.li(30, 6)?;
    c.branch(6, 30, 5, Invalid)?;
    c.addi(7, 5, -4)?;
    c.constant(30, 0x0100_0000_0000_0000)?;
    c.register(6, 0, 7, 7, 30)?;
    c.hash_jump(HashDecoded)?;
    c.label(HashDirect);
    c.addi(7, 5, 0)?;
    c.label(HashDecoded);
    for (reg, offset) in [(10, 8), (11, 16), (12, 0)] {
        c.immediate(3, 3, reg, 2, offset)?;
    }
    c.addi(13, 6, 0)?;
    c.addi(14, 7, 0)?;
    c.constant(17, 2092)?;
    c.exact(0x73)?;
    c.branch(0, 10, 0, Ready)?;
    c.li(5, 3)?;
    c.sub(6, 10, 5)?;
    c.branch(1, 6, 0, HashFailed)?;
    c.label(Ready);
    c.immediate(3, 3, 5, 2, 16)?;
    c.immediate(3, 3, 7, 5, 0)?;
    c.li(6, 32)?;
    c.branch(6, 7, 6, HashFailed)?;
    c.hash_store(6, 5, 0, false)?;
    c.li(10, 0)?;
    c.hash_jump(Done)?;
    c.label(Invalid);
    c.li(10, 44)?;
    c.hash_jump(Done)?;
    c.label(HashFailed);
    c.li(10, 1)?;
    c.label(Done);
    c.immediate(3, 3, 1, 2, 24)?;
    c.addi(2, 2, 32)?;
    c.exact(0x00008067)?;
    if c.pc != end || c.branches.iter().any(|(label, target)| c.labels.get(label) != Some(target)) {
        return Err(invalid("Cell read helper extent or branch destination differs"));
    }
    Ok(())
}
