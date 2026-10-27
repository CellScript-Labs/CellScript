//! Finite write-effect contracts for current-Script and raw transaction hashes.
//! The caller proves disjoint private output/length buffers before applying the
//! summary. No other caller memory may be written by the recognized helper.
use super::*;

pub(super) fn syscall(owner: &str) -> Option<u32> {
    match owner {
        "runtime:__ckb_transaction_hash" => Some(2061),
        "runtime:__ckb_current_script_hash" => Some(2062),
        _ => None,
    }
}

pub(super) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf, owner: &str) -> Result<(), CheckerError> {
    use Label::*;
    let syscall = syscall(owner).ok_or_else(|| invalid("unknown context hash helper"))?;
    let blocks = record.blocks.iter().filter(|block| block.owner_entry == owner).collect::<Vec<_>>();
    let start = blocks.iter().map(|block| block.range.start).min().ok_or_else(|| invalid("context hash helper has no body"))?;
    let end = blocks.iter().map(|block| block.range.end).max().unwrap();
    if elf.control_flow.iter().any(|edge| start < edge.target && edge.target < end && !(start..end).contains(&edge.address)) {
        return Err(invalid("control flow enters a partial context hash helper"));
    }
    let words = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect();
    let mut c = Decoder { pc: start, words: &words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
    c.addi(2, 2, -16)?;
    c.hash_store(1, 2, 8, false)?;
    c.hash_store(11, 2, 0, false)?;
    c.li(5, 32)?;
    c.hash_store(5, 11, 0, false)?;
    c.li(12, 0)?;
    c.constant(17, u64::from(syscall))?;
    c.exact(0x73)?;
    c.branch(1, 10, 0, HashFailed)?;
    c.immediate(3, 3, 31, 2, 0)?;
    c.immediate(3, 3, 5, 31, 0)?;
    c.li(6, 32)?;
    if syscall == 2062 {
        c.sub(7, 5, 6)?;
        c.branch(1, 7, 0, Invalid)?;
    } else {
        c.branch(1, 5, 6, Invalid)?;
    }
    c.li(10, 0)?;
    c.hash_jump(Done)?;
    c.label(HashFailed);
    c.li(10, 1)?;
    c.hash_jump(Done)?;
    c.label(Invalid);
    c.li(10, 18)?;
    c.label(Done);
    c.immediate(3, 3, 1, 2, 8)?;
    c.addi(2, 2, 16)?;
    c.exact(0x00008067)?;
    if c.pc != end || c.branches.iter().any(|(label, target)| c.labels.get(label) != Some(target)) {
        return Err(invalid("context hash helper extent or branch destination differs"));
    }
    if (start..end).step_by(4).any(|pc| !blocks.iter().any(|block| block.range.contains(pc))) {
        return Err(invalid("context hash helper crosses another entry"));
    }
    Ok(())
}
