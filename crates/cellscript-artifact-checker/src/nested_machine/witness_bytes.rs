//! Exact raw witness slices: whitelist the source, request exactly 32 bytes,
//! check both syscall status and reported remaining length, and restore SP/RA.
//! The output is caller-owned; no digest or zero-padding contract is implied.
use super::*;

pub(super) const OWNER: &str = "runtime:__ckb_witness_bytes32";

pub(super) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    use Label::*;
    let entry = record.entries.iter().find(|entry| entry.id == OWNER).ok_or_else(|| invalid("missing witness bytes32 entry"))?;
    let mut blocks = record.blocks.iter().filter(|block| block.owner_entry == OWNER).collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    let first = blocks.first().ok_or_else(|| invalid("empty witness bytes32 helper"))?;
    let start = first.range.start;
    let end = blocks.last().unwrap().range.end;
    if first.id != entry.entry_block
        || blocks.windows(2).any(|pair| pair[0].range.end != pair[1].range.start)
        || elf.control_flow.iter().any(|edge| start < edge.target && edge.target < end && !(start..end).contains(&edge.address))
    {
        return Err(invalid("witness bytes32 partial entry or noncontiguous body"));
    }
    let words = elf.instructions.iter().map(|word| (word.address, word.word)).collect();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect();
    let mut c = Decoder { pc: start, words: &words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
    c.addi(2, 2, -32)?;
    c.hash_store(11, 2, 0, false)?;
    c.hash_store(13, 2, 8, false)?;
    c.hash_store(1, 2, 24, false)?;
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
    for source in [1, 2, 0x0100_0000_0000_0001, 0x0100_0000_0000_0002] {
        c.constant(5, source)?;
        c.branch(0, 5, 7, Ready)?;
    }
    c.hash_jump(Invalid)?;
    c.label(Ready);
    c.li(5, 32)?;
    c.hash_store(5, 2, 16, false)?;
    c.immediate(3, 3, 10, 2, 8)?;
    c.addi(11, 2, 16)?;
    c.immediate(3, 3, 12, 2, 0)?;
    c.addi(13, 6, 0)?;
    c.addi(14, 7, 0)?;
    c.constant(17, 2074)?;
    c.exact(0x73)?;
    c.branch(1, 10, 0, HashFailed)?;
    c.immediate(3, 3, 5, 2, 16)?;
    c.li(6, 32)?;
    c.branch(6, 5, 6, HashFailed)?;
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
    c.exact(0x8067)?;
    if c.pc != end || c.branches.iter().any(|(label, target)| c.labels.get(label) != Some(target)) {
        return Err(invalid("witness bytes32 extent or branch target differs"));
    }
    Ok(())
}
