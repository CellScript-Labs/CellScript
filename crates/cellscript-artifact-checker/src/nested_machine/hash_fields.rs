//! Complete finite contracts for full Cell-field hash helpers. A successful
//! helper writes 32 bytes plus an eight-byte size into caller-owned buffers;
//! no other caller memory is written. Call sites still prove both extents.
use super::*;

pub(super) fn field(owner: &str) -> Option<u32> {
    match owner {
        "runtime:__ckb_cell_data_hash_field" => Some(1),
        "runtime:__ckb_cell_lock_hash" => Some(3),
        "runtime:__ckb_cell_type_hash" => Some(5),
        _ => None,
    }
}

pub(super) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf, owner: &str, field: u32) -> Result<(), CheckerError> {
    use Label::*;
    let blocks = record.blocks.iter().filter(|block| block.owner_entry == owner).collect::<Vec<_>>();
    let start = blocks.iter().map(|block| block.range.start).min().ok_or_else(|| invalid("hash helper has no body"))?;
    let end = blocks.iter().map(|block| block.range.end).max().unwrap();
    if elf.control_flow.iter().any(|edge| start < edge.target && edge.target < end && !(start..end).contains(&edge.address)) {
        return Err(invalid("control flow enters a partial hash helper"));
    }
    let words = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect();
    let mut c = Decoder { pc: start, words: &words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
    c.addi(2, 2, -80)?;
    c.hash_store(1, 2, 72, false)?;
    c.hash_store(11, 2, 56, false)?;
    c.hash_store(12, 2, 64, false)?;
    c.branch(0, 11, 0, HashBadOutput)?;
    c.branch(0, 12, 0, HashBadOutput)?;
    // Decode the source-view's upper tag and lower global index.
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
    c.li(5, 32)?;
    c.hash_store(5, 2, 8, false)?;
    c.addi(10, 2, 16)?;
    c.addi(11, 2, 8)?;
    c.li(12, 0)?;
    c.addi(13, 6, 0)?;
    c.addi(14, 7, 0)?;
    c.li(15, field)?;
    c.constant(17, 2081)?;
    c.exact(0x73)?;
    c.branch(1, 10, 0, HashFailed)?;
    c.immediate(3, 3, 5, 2, 8)?;
    c.li(6, 32)?;
    c.sub(7, 5, 6)?;
    c.branch(1, 7, 0, HashFailed)?;
    c.li(6, 0)?;
    c.label(HashCopy);
    c.li(7, 32)?;
    c.register(3, 0, 28, 6, 7)?;
    c.branch(0, 28, 0, HashCopied)?;
    c.addi(31, 2, 16)?;
    c.add(31, 31, 6)?;
    c.immediate(3, 4, 30, 31, 0)?;
    c.immediate(3, 3, 31, 2, 56)?;
    c.add(31, 31, 6)?;
    c.hash_store(30, 31, 0, true)?;
    c.addi(6, 6, 1)?;
    c.hash_jump(HashCopy)?;
    c.label(HashCopied);
    c.immediate(3, 3, 31, 2, 64)?;
    c.li(5, 32)?;
    c.hash_store(5, 31, 0, false)?;
    c.li(10, 0)?;
    c.hash_jump(Done)?;
    c.label(Invalid);
    c.li(10, 44)?;
    c.hash_jump(Done)?;
    c.label(HashBadOutput);
    c.li(10, 18)?;
    c.hash_jump(Done)?;
    c.label(HashFailed);
    c.li(10, 44)?;
    c.label(Done);
    c.immediate(3, 3, 1, 2, 72)?;
    c.addi(2, 2, 80)?;
    c.exact(0x00008067)?;
    if c.pc != end || c.branches.iter().any(|(label, target)| c.labels.get(label) != Some(target)) {
        return Err(invalid("hash helper extent or branch destination differs"));
    }
    if (start..end).step_by(4).any(|pc| !blocks.iter().any(|block| block.range.contains(pc))) {
        return Err(invalid("hash helper crosses another entry"));
    }
    Ok(())
}

impl Decoder<'_> {
    fn hash_store(&mut self, source: u32, base: u32, offset: u32, byte: bool) -> Result<(), CheckerError> {
        self.exact(
            ((offset >> 5) << 25) | (source << 20) | (base << 15) | (if byte { 0 } else { 3 } << 12) | ((offset & 31) << 7) | 0x23,
        )
    }
    fn hash_jump(&mut self, label: Label) -> Result<(), CheckerError> {
        let target = self.jump()?;
        self.branches.push((label, target));
        Ok(())
    }
}
