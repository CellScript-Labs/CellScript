//! Finite ScriptRef code-hash and exact-32-byte-args read/write contracts.
//! These read bounded Script prefixes into private stack storage and copy only
//! the selected 32 bytes and length to independently checked caller buffers.
use super::*;

pub(super) fn kind(owner: &str) -> Option<(u32, bool)> {
    match owner {
        "runtime:__ckb_cell_lock_code_hash" => Some((2, false)),
        "runtime:__ckb_cell_type_code_hash" => Some((4, false)),
        "runtime:__ckb_cell_lock_args_hash" => Some((2, true)),
        "runtime:__ckb_cell_type_args_hash" => Some((4, true)),
        _ => None,
    }
}

pub(super) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf, owner: &str) -> Result<(), CheckerError> {
    use Label::*;
    let (field, args) = kind(owner).ok_or_else(|| invalid("unknown ScriptRef hash field"))?;
    let blocks = record.blocks.iter().filter(|block| block.owner_entry == owner).collect::<Vec<_>>();
    let start = blocks.iter().map(|block| block.range.start).min().ok_or_else(|| invalid("ScriptRef hash helper has no body"))?;
    let end = blocks.iter().map(|block| block.range.end).max().unwrap();
    if elf.control_flow.iter().any(|edge| start < edge.target && edge.target < end && !(start..end).contains(&edge.address)) {
        return Err(invalid("control flow enters a partial ScriptRef hash helper"));
    }
    let words = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect();
    let mut c = Decoder { pc: start, words: &words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
    // Independent layout constants for 53-byte prefixes / 128-byte args reads.
    let (frame, output, size, ra) = if args { (160, 136, 144, 152) } else { (96, 64, 72, 80) };
    c.addi(2, 2, -frame)?;
    c.hash_store(1, 2, ra, false)?;
    c.hash_store(11, 2, output, false)?;
    c.hash_store(12, 2, size, false)?;
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
    c.li(5, if args { 128 } else { 53 })?;
    c.hash_store(5, 2, 0, false)?;
    c.addi(10, 2, 8)?;
    c.addi(11, 2, 0)?;
    c.li(12, 0)?;
    c.addi(13, 6, 0)?;
    c.addi(14, 7, 0)?;
    c.li(15, field)?;
    c.constant(17, 2081)?;
    c.exact(0x73)?;
    c.branch(0, 10, 0, Ready)?;
    if !args {
        c.li(5, 3)?;
        c.sub(6, 10, 5)?;
        c.branch(0, 6, 0, Ready)?;
    }
    c.hash_jump(HashFailed)?;
    c.label(Ready);
    c.immediate(3, 3, 28, 2, 0)?;
    c.li(6, 49)?;
    c.register(3, 0, 7, 28, 6)?;
    c.branch(1, 7, 0, EndReady)?;
    stack_u32(&mut c, 8)?;
    c.li(6, 53)?;
    c.register(3, 0, 7, 5, 6)?;
    c.branch(1, 7, 0, EndReady)?;
    for (offset, expected) in [(12, 16), (16, 48), (20, 49)] {
        stack_u32(&mut c, offset)?;
        c.li(6, expected)?;
        c.sub(7, 5, 6)?;
        c.branch(1, 7, 0, EndReady)?;
    }
    if args {
        stack_u32(&mut c, 57)?;
        c.li(6, 32)?;
        c.sub(7, 5, 6)?;
        c.branch(1, 7, 0, HashBadOutput)?;
        c.li(6, 85)?;
        c.sub(7, 28, 6)?;
        c.branch(1, 7, 0, EndReady)?;
    }
    c.li(6, 0)?;
    c.label(HashCopy);
    c.li(7, 32)?;
    c.register(3, 0, 28, 6, 7)?;
    c.branch(0, 28, 0, HashCopied)?;
    c.addi(31, 2, if args { 61 } else { 24 })?;
    c.add(31, 31, 6)?;
    c.immediate(3, 4, 30, 31, 0)?;
    c.immediate(3, 3, 31, 2, output as i32)?;
    c.add(31, 31, 6)?;
    c.hash_store(30, 31, 0, true)?;
    c.addi(6, 6, 1)?;
    c.hash_jump(HashCopy)?;
    c.label(HashCopied);
    c.immediate(3, 3, 31, 2, size as i32)?;
    c.li(5, 32)?;
    c.hash_store(5, 31, 0, false)?;
    c.li(10, 0)?;
    c.hash_jump(Done)?;
    for (label, error) in [(Invalid, 44), (HashFailed, 34), (HashBadOutput, 38)] {
        c.label(label);
        c.li(10, error)?;
        c.hash_jump(Done)?;
    }
    c.label(EndReady);
    c.li(10, 34)?;
    c.label(Done);
    c.immediate(3, 3, 1, 2, ra as i32)?;
    c.addi(2, 2, frame)?;
    c.exact(0x00008067)?;
    if c.pc != end || c.branches.iter().any(|(label, target)| c.labels.get(label) != Some(target)) {
        return Err(invalid("ScriptRef hash helper extent or branch destination differs"));
    }
    if (start..end).step_by(4).any(|pc| !blocks.iter().any(|block| block.range.contains(pc))) {
        return Err(invalid("ScriptRef hash helper crosses another entry"));
    }
    Ok(())
}

fn stack_u32(c: &mut Decoder<'_>, offset: i32) -> Result<(), CheckerError> {
    c.immediate(3, 4, 5, 2, offset)?;
    for byte in 1..4 {
        c.immediate(3, 4, 29, 2, offset + byte)?;
        c.immediate(0x13, 1, 29, 29, byte * 8)?;
        c.register(6, 0, 5, 5, 29)?;
    }
    Ok(())
}
