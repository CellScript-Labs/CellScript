//! Complete bounded WitnessArgs resolver contract, including raw ownership,
//! canonical table/BytesOpt lengths, absent fields and every rejecting return.
//! All three fixed-size reads use private stack slots and the same decoded
//! immutable source coordinate. No caller memory is written by this helper.
use super::*;

pub(super) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    use Label::*;
    let (start, end) = streaming_hashes::extent(record, elf, witness_exact_hash::RESOLVER)?;
    let words = elf.instructions.iter().map(|word| (word.address, word.word)).collect();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect();
    let mut c = Decoder { pc: start, words: &words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
    c.addi(2, 2, -96)?;
    for (register, offset) in [(10, 0), (11, 8), (12, 16)] {
        c.hash_store(register, 2, offset, false)?;
    }
    c.constant(5, 65536)?;
    c.branch(6, 5, 12, Cache(14))?;
    c.li(5, 3)?;
    c.branch(6, 5, 11, Invalid)?;
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
    c.hash_store(6, 2, 56, false)?;
    c.hash_store(7, 2, 64, false)?;
    c.hash_store(0, 2, 24, false)?;
    c.li(10, 0)?;
    c.addi(11, 2, 24)?;
    c.li(12, 0)?;
    c.addi(13, 6, 0)?;
    c.addi(14, 7, 0)?;
    load_status(&mut c, Cache(0))?;
    c.immediate(3, 3, 5, 2, 8)?;
    c.branch(0, 5, 0, Cache(1))?;
    c.immediate(3, 3, 5, 2, 24)?;
    c.li(6, 16)?;
    c.branch(6, 5, 6, Cache(11))?;
    c.hash_store(6, 2, 48, false)?;
    c.addi(10, 2, 32)?;
    c.addi(11, 2, 48)?;
    c.li(12, 0)?;
    c.immediate(3, 3, 13, 2, 56)?;
    c.immediate(3, 3, 14, 2, 64)?;
    load_status(&mut c, Cache(2))?;
    c.addi(28, 2, 32)?;
    c.u32_le(29, 28, 0, 30)?;
    c.immediate(3, 3, 5, 2, 24)?;
    c.branch(1, 29, 5, Cache(11))?;
    c.u32_le(29, 28, 4, 30)?;
    c.li(30, 16)?;
    c.branch(1, 29, 30, Cache(11))?;
    c.u32_le(29, 28, 4, 30)?;
    c.u32_le(30, 28, 8, 31)?;
    c.u32_le(31, 28, 12, 7)?;
    c.branch(6, 30, 29, Cache(11))?;
    c.branch(6, 31, 30, Cache(11))?;
    c.immediate(3, 3, 5, 2, 24)?;
    c.branch(6, 5, 31, Cache(12))?;
    c.hash_store(29, 2, 72, false)?;
    c.hash_store(30, 2, 80, false)?;
    c.immediate(3, 3, 5, 2, 8)?;
    c.li(6, 1)?;
    c.branch(0, 5, 6, Cache(3))?;
    c.li(6, 2)?;
    c.branch(0, 5, 6, Cache(4))?;
    c.hash_jump(Cache(5))?;
    c.label(Cache(3));
    c.hash_jump(Cache(6))?;
    c.label(Cache(4));
    c.hash_store(30, 2, 72, false)?;
    c.hash_store(31, 2, 80, false)?;
    c.hash_jump(Cache(6))?;
    c.label(Cache(5));
    c.hash_store(31, 2, 72, false)?;
    c.immediate(3, 3, 5, 2, 24)?;
    c.hash_store(5, 2, 80, false)?;
    c.label(Cache(6));
    c.immediate(3, 3, 29, 2, 72)?;
    c.immediate(3, 3, 30, 2, 80)?;
    c.sub(7, 30, 29)?;
    c.branch(0, 7, 0, Cache(13))?;
    c.li(28, 4)?;
    c.branch(6, 7, 28, Cache(11))?;
    c.hash_store(28, 2, 48, false)?;
    c.addi(10, 2, 32)?;
    c.addi(11, 2, 48)?;
    c.addi(12, 29, 0)?;
    c.immediate(3, 3, 13, 2, 56)?;
    c.immediate(3, 3, 14, 2, 64)?;
    load_status(&mut c, Cache(7))?;
    c.addi(28, 2, 32)?;
    c.u32_le(6, 28, 0, 30)?;
    c.immediate(3, 3, 29, 2, 72)?;
    c.immediate(3, 3, 30, 2, 80)?;
    c.sub(7, 30, 29)?;
    c.addi(7, 7, -4)?;
    c.branch(1, 6, 7, Cache(11))?;
    c.immediate(3, 3, 28, 2, 16)?;
    c.branch(6, 28, 6, Cache(14))?;
    c.addi(10, 29, 4)?;
    c.addi(11, 6, 0)?;
    c.hash_jump(Cache(8))?;
    c.label(Cache(1));
    c.immediate(3, 3, 11, 2, 24)?;
    c.immediate(3, 3, 5, 2, 16)?;
    c.branch(6, 5, 11, Cache(14))?;
    c.li(10, 0)?;
    c.label(Cache(8));
    c.immediate(3, 3, 12, 2, 56)?;
    c.immediate(3, 3, 13, 2, 64)?;
    c.li(14, 0)?;
    c.hash_jump(Done)?;
    for (label, error) in [(Invalid, 44), (HashFailed, 1), (Cache(11), 42), (Cache(12), 43), (Cache(13), 67), (Cache(14), 68)] {
        c.label(label);
        for register in 10..=13 {
            c.li(register, 0)?;
        }
        c.li(14, error)?;
        c.hash_jump(Done)?;
    }
    c.label(Done);
    c.addi(2, 2, 96)?;
    c.exact(0x8067)?;
    if c.pc != end || c.branches.iter().any(|(label, target)| c.labels.get(label) != Some(target)) {
        return Err(invalid("bounded WitnessArgs resolver extent or branch target differs"));
    }
    Ok(())
}

fn load_status(c: &mut Decoder<'_>, ready: Label) -> Result<(), CheckerError> {
    c.constant(17, 2074)?;
    c.exact(0x73)?;
    c.branch(0, 10, 0, ready)?;
    c.li(5, 3)?;
    c.branch(0, 10, 5, ready)?;
    c.hash_jump(Label::HashFailed)?;
    c.label(ready);
    Ok(())
}
