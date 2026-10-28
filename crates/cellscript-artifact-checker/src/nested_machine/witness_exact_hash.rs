//! Exact-32-byte WitnessArgs wrappers. The resolver must independently prove
//! private memory effects; this wrapper publishes one bounded caller buffer.
use super::*;

pub(super) const RESOLVER: &str = "runtime:__cellscript_witness_bounded_resolve";

pub(super) fn field(owner: &str) -> Option<u32> {
    match owner {
        "runtime:__ckb_witness_lock_exact32" => Some(1),
        "runtime:__ckb_witness_input_type_exact32" => Some(2),
        "runtime:__ckb_witness_output_type_exact32" => Some(3),
        _ => None,
    }
}

pub(super) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf, owner: &str) -> Result<(), CheckerError> {
    use Label::*;
    let field = field(owner).ok_or_else(|| invalid("unknown exact WitnessArgs byte helper"))?;
    let blocks = record.blocks.iter().filter(|block| block.owner_entry == owner).collect::<Vec<_>>();
    let start = blocks.iter().map(|block| block.range.start).min().ok_or_else(|| invalid("WitnessArgs helper has no body"))?;
    let end = blocks.iter().map(|block| block.range.end).max().unwrap();
    if elf.control_flow.iter().any(|edge| start < edge.target && edge.target < end && !(start..end).contains(&edge.address)) {
        return Err(invalid("control flow enters a partial WitnessArgs helper"));
    }
    let resolver = record.entries.iter().find(|entry| entry.id == RESOLVER).ok_or_else(|| invalid("missing witness resolver"))?;
    let resolver_start = record
        .blocks
        .iter()
        .find(|block| block.id == resolver.entry_block)
        .ok_or_else(|| invalid("missing witness resolver entry block"))?
        .range
        .start;
    let words = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect();
    let mut c = Decoder { pc: start, words: &words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
    c.addi(2, 2, -48)?;
    c.hash_store(1, 2, 40, false)?;
    c.hash_store(11, 2, 0, false)?;
    c.li(11, field)?;
    c.li(12, 32)?;
    // The internal assembler lowers `call` to AUIPC ra + JALR ra, ra.
    let upper = c.word()?;
    let call = c.pc;
    let lower = c.word()?;
    if upper & 0xfff != 0x97 || lower & 0x000f_ffff != 0x000080e7 || c.targets.get(&call) != Some(&resolver_start) {
        return Err(invalid("WitnessArgs helper does not call its checked resolver"));
    }
    c.branch(0, 14, 0, Ready)?;
    c.hash_jump(Invalid)?;
    c.label(Ready);
    c.li(5, 32)?;
    c.branch(1, 11, 5, HashBadOutput)?;
    for (register, offset) in [(10, 8), (12, 16), (13, 24), (5, 32)] {
        c.hash_store(register, 2, offset, false)?;
    }
    c.immediate(3, 3, 10, 2, 0)?;
    c.addi(11, 2, 32)?;
    for (register, offset) in [(12, 8), (13, 16), (14, 24)] {
        c.immediate(3, 3, register, 2, offset)?;
    }
    c.constant(17, 2074)?;
    c.exact(0x73)?;
    c.branch(0, 10, 0, EndReady)?;
    c.li(5, 3)?;
    c.branch(0, 10, 5, EndReady)?;
    c.hash_jump(HashFailed)?;
    c.label(Invalid);
    c.li(5, 67)?;
    c.branch(0, 14, 5, HashBadOutput)?;
    c.li(5, 68)?;
    c.branch(0, 14, 5, HashBadOutput)?;
    c.addi(10, 14, 0)?;
    c.hash_jump(Done)?;
    c.label(HashBadOutput);
    c.li(10, 4)?;
    c.hash_jump(Done)?;
    c.label(HashFailed);
    c.li(10, 1)?;
    c.hash_jump(Done)?;
    c.label(EndReady);
    c.li(10, 0)?;
    c.label(Done);
    c.immediate(3, 3, 1, 2, 40)?;
    c.addi(2, 2, 48)?;
    c.exact(0x00008067)?;
    if c.pc != end || c.branches.iter().any(|(label, target)| c.labels.get(label) != Some(target)) {
        return Err(invalid("WitnessArgs helper extent or branch destination differs"));
    }
    if (start..end).step_by(4).any(|pc| !blocks.iter().any(|block| block.range.contains(pc))) {
        return Err(invalid("WitnessArgs helper crosses another entry"));
    }
    Ok(())
}
