//! Finite input OutPoint transaction-hash output contract. The helper reads a
//! 36-byte OutPoint into its private frame, then publishes only its first 32
//! bytes and the length through separately checked caller-owned buffers.
use super::*;

pub(super) const OWNER: &str = "runtime:__ckb_input_out_point_tx_hash";

pub(super) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    use Label::*;
    let blocks = record.blocks.iter().filter(|block| block.owner_entry == OWNER).collect::<Vec<_>>();
    let start = blocks.iter().map(|block| block.range.start).min().ok_or_else(|| invalid("OutPoint hash helper has no body"))?;
    let end = blocks.iter().map(|block| block.range.end).max().unwrap();
    if elf.control_flow.iter().any(|edge| start < edge.target && edge.target < end && !(start..end).contains(&edge.address)) {
        return Err(invalid("control flow enters a partial OutPoint hash helper"));
    }
    let words = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect();
    let mut c = Decoder { pc: start, words: &words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
    c.addi(2, 2, -80)?;
    c.hash_store(1, 2, 72, false)?;
    c.hash_store(11, 2, 8, false)?;
    c.hash_store(12, 2, 16, false)?;
    c.branch(0, 11, 0, HashBadOutput)?;
    c.branch(0, 12, 0, HashBadOutput)?;
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
    c.li(5, 1)?;
    c.branch(0, 7, 5, Ready)?;
    c.constant(5, 0x0100_0000_0000_0001)?;
    c.branch(0, 7, 5, Ready)?;
    c.hash_jump(Invalid)?;
    c.label(Ready);
    c.li(5, 36)?;
    c.hash_store(5, 2, 24, false)?;
    c.addi(10, 2, 32)?;
    c.addi(11, 2, 24)?;
    c.li(12, 0)?;
    c.addi(13, 6, 0)?;
    c.addi(14, 7, 0)?;
    c.li(15, 0)?;
    c.constant(17, 2083)?;
    c.exact(0x73)?;
    c.branch(1, 10, 0, HashFailed)?;
    c.immediate(3, 3, 5, 2, 24)?;
    c.li(6, 36)?;
    c.sub(7, 5, 6)?;
    c.branch(1, 7, 0, HashFailed)?;
    c.li(5, 0)?;
    c.label(HashCopy);
    c.li(6, 32)?;
    c.register(3, 0, 7, 5, 6)?;
    c.branch(0, 7, 0, HashCopied)?;
    c.addi(28, 2, 32)?;
    c.add(28, 28, 5)?;
    c.immediate(3, 4, 29, 28, 0)?;
    c.immediate(3, 3, 30, 2, 8)?;
    c.add(30, 30, 5)?;
    c.hash_store(29, 30, 0, true)?;
    c.addi(5, 5, 1)?;
    c.hash_jump(HashCopy)?;
    c.label(HashCopied);
    c.immediate(3, 3, 5, 2, 16)?;
    c.li(6, 32)?;
    c.hash_store(6, 5, 0, false)?;
    c.li(10, 0)?;
    c.hash_jump(Done)?;
    c.label(Invalid);
    c.li(10, 44)?;
    c.hash_jump(Done)?;
    c.label(HashBadOutput);
    c.li(10, 18)?;
    c.hash_jump(Done)?;
    c.label(HashFailed);
    c.li(10, 33)?;
    c.label(Done);
    c.immediate(3, 3, 1, 2, 72)?;
    c.addi(2, 2, 80)?;
    c.exact(0x00008067)?;
    if c.pc != end || c.branches.iter().any(|(label, target)| c.labels.get(label) != Some(target)) {
        return Err(invalid("OutPoint hash helper extent or branch destination differs"));
    }
    if (start..end).step_by(4).any(|pc| !blocks.iter().any(|block| block.range.contains(pc))) {
        return Err(invalid("OutPoint hash helper crosses another entry"));
    }
    Ok(())
}
