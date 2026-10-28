//! Closed instruction contracts for exact Cell/Witness streaming BLAKE2b spans.
//! The wrapper proves offset <= total and len <= total-offset before calling
//! the core. The core reads <=128 bytes per block, checks the reported length,
//! zeroes final padding, and writes only the 32-byte caller output. Compression
//! retains the independently pinned complete-body contract used by instances.
use super::*;

const CORE: &str = "runtime:__cellscript_blake2b_transaction_span";
const COMPRESS: &str = "runtime:__cellscript_blake2b_compress";

pub(super) fn syscall(owner: &str) -> Option<u64> {
    match owner {
        "runtime:__ckb_cell_data_blake2b_span" => Some(2092),
        "runtime:__ckb_witness_blake2b_span" => Some(2074),
        _ => None,
    }
}

pub(super) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf, owner: &str) -> Result<(), CheckerError> {
    let syscall = syscall(owner).ok_or_else(|| invalid("unknown streaming hash wrapper"))?;
    let words = elf.instructions.iter().map(|word| (word.address, word.word)).collect();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect();
    let (start, end) = extent(record, elf, owner)?;
    let mut c = Decoder { pc: start, words: &words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
    use Label::*;
    c.addi(2, 2, -64)?;
    c.hash_store(1, 2, 56, false)?;
    for (register, offset) in [(11, 0), (12, 8), (13, 16)] {
        c.hash_store(register, 2, offset, false)?;
    }
    // Independently decode and whitelist source coordinates.
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
    if syscall == 2092 {
        c.li(5, 3)?;
        c.branch(0, 5, 7, Ready)?;
    }
    c.hash_jump(Invalid)?;
    c.label(Ready);
    c.hash_store(6, 2, 24, false)?;
    c.hash_store(7, 2, 32, false)?;
    c.hash_store(0, 2, 40, false)?;
    c.addi(10, 2, 48)?;
    c.addi(11, 2, 40)?;
    c.li(12, 0)?;
    c.addi(13, 6, 0)?;
    c.addi(14, 7, 0)?;
    c.constant(17, syscall)?;
    c.exact(0x73)?;
    c.branch(1, 10, 0, HashFailed)?;
    c.immediate(3, 3, 5, 2, 40)?;
    c.immediate(3, 3, 6, 2, 0)?;
    c.branch(6, 5, 6, HashFailed)?;
    c.sub(5, 5, 6)?;
    c.immediate(3, 3, 6, 2, 8)?;
    c.branch(6, 5, 6, HashFailed)?;
    for (register, offset) in [(10, 24), (11, 32), (12, 0), (13, 8), (14, 16)] {
        c.immediate(3, 3, register, 2, offset)?;
    }
    c.constant(15, syscall)?;
    call(&mut c, record, CORE)?;
    c.branch(1, 10, 0, HashFailed)?;
    c.hash_jump(Done)?;
    c.label(Invalid);
    c.li(10, 44)?;
    c.hash_jump(Done)?;
    c.label(HashFailed);
    c.li(10, if syscall == 2092 { 3 } else { 1 })?;
    c.label(Done);
    c.immediate(3, 3, 1, 2, 56)?;
    c.addi(2, 2, 64)?;
    c.exact(0x8067)?;
    finish(&c, end)?;
    validate_core(record, elf, &words, &targets)?;
    crate::instance_machine::validate_compression(record, elf)
}

pub(super) fn extent(record: &VerifiedLoweringRecord, elf: &ParsedElf, owner: &str) -> Result<(u64, u64), CheckerError> {
    let entry = record.entries.iter().find(|entry| entry.id == owner).ok_or_else(|| invalid("missing streaming hash entry"))?;
    let mut blocks = record.blocks.iter().filter(|block| block.owner_entry == owner).collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    let first = blocks.first().ok_or_else(|| invalid("empty streaming hash body"))?;
    let start = first.range.start;
    let end = blocks.last().unwrap().range.end;
    if first.id != entry.entry_block
        || blocks.windows(2).any(|pair| pair[0].range.end != pair[1].range.start)
        || elf.control_flow.iter().any(|edge| start < edge.target && edge.target < end && !(start..end).contains(&edge.address))
    {
        return Err(invalid("streaming hash partial entry or noncontiguous body"));
    }
    Ok((start, end))
}

fn call(c: &mut Decoder<'_>, record: &VerifiedLoweringRecord, owner: &str) -> Result<(), CheckerError> {
    let target = record
        .entries
        .iter()
        .find(|entry| entry.id == owner)
        .and_then(|entry| record.blocks.iter().find(|block| block.id == entry.entry_block))
        .ok_or_else(|| invalid("streaming hash dependency has no entry"))?
        .range
        .start;
    let upper = c.word()?;
    let address = c.pc;
    let lower = c.word()?;
    if upper & 0xfff != 0x97 || lower & 0x000f_ffff != 0x000080e7 || c.targets.get(&address) != Some(&target) {
        return Err(invalid("streaming hash calls an unchecked target"));
    }
    Ok(())
}
fn finish(c: &Decoder<'_>, end: u64) -> Result<(), CheckerError> {
    if c.pc != end || c.branches.iter().any(|(label, target)| c.labels.get(label) != Some(target)) {
        return Err(invalid("streaming hash extent or branch target differs"));
    }
    Ok(())
}

fn validate_core(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    words: &BTreeMap<u64, u32>,
    targets: &BTreeMap<u64, u64>,
) -> Result<(), CheckerError> {
    use Label::*;
    let (start, end) = extent(record, elf, CORE)?;
    let mut c = Decoder { pc: start, words, targets, labels: BTreeMap::new(), branches: Vec::new() };
    c.addi(2, 2, -400)?;
    c.hash_store(1, 2, 392, false)?;
    for (register, offset) in [(10, 360), (11, 368), (12, 320), (13, 328), (14, 336), (15, 384)] {
        c.hash_store(register, 2, offset, false)?;
    }
    c.hash_store(0, 2, 344, false)?;
    // RFC BLAKE2b IV, digest size 32, fanout/depth 1, CKB personalization.
    let iv: [u64; 8] = [
        0x6a09e667f3bcc908,
        0xbb67ae8584caa73b,
        0x3c6ef372fe94f82b,
        0xa54ff53a5f1d36f1,
        0x510e527fade682d1,
        0x9b05688c2b3e6c1f,
        0x1f83d9abfb41bd6b,
        0x5be0cd19137e2179,
    ];
    let mut initial = iv;
    initial[0] ^= 0x01010020;
    initial[6] ^= u64::from_le_bytes(*b"ckb-defa");
    initial[7] ^= u64::from_le_bytes(*b"ult-hash");
    for (index, value) in initial.into_iter().enumerate() {
        c.constant(5, value)?;
        c.hash_store(5, 2, (index * 8) as u32, false)?;
    }
    c.label(Cache(0));
    c.immediate(3, 3, 5, 2, 344)?;
    c.immediate(3, 3, 6, 2, 328)?;
    c.sub(7, 6, 5)?;
    c.branch(1, 7, 0, Cache(1))?;
    c.branch(0, 5, 0, Cache(1))?;
    c.hash_jump(Done)?;
    c.label(Cache(1));
    c.li(28, 128)?;
    c.register(3, 0, 29, 28, 7)?;
    c.branch(0, 29, 0, Cache(2))?;
    c.li(7, 128)?;
    c.hash_jump(Cache(3))?;
    c.label(Cache(2));
    c.label(Cache(3));
    c.hash_store(7, 2, 352, false)?;
    c.addi(5, 2, 192)?;
    c.addi(6, 2, 320)?;
    c.label(Cache(4));
    c.hash_store(0, 5, 0, false)?;
    c.addi(5, 5, 8)?;
    c.branch(1, 5, 6, Cache(4))?;
    c.immediate(3, 3, 5, 2, 352)?;
    c.branch(0, 5, 0, Ready)?;
    c.hash_store(5, 2, 376, false)?;
    c.addi(10, 2, 192)?;
    c.addi(11, 2, 376)?;
    c.immediate(3, 3, 12, 2, 320)?;
    c.immediate(3, 3, 5, 2, 344)?;
    c.add(12, 12, 5)?;
    for (register, offset) in [(13, 360), (14, 368), (17, 384)] {
        c.immediate(3, 3, register, 2, offset)?;
    }
    c.exact(0x73)?;
    c.branch(1, 10, 0, HashFailed)?;
    c.immediate(3, 3, 5, 2, 376)?;
    c.immediate(3, 3, 6, 2, 352)?;
    c.branch(6, 5, 6, HashFailed)?;
    c.hash_jump(Ready)?;
    c.label(HashFailed);
    c.immediate(3, 3, 1, 2, 392)?;
    c.addi(2, 2, 400)?;
    c.li(10, 1)?;
    c.exact(0x8067)?;
    c.label(Ready);
    for index in 0..8 {
        c.immediate(3, 3, 5, 2, index * 8)?;
        c.hash_store(5, 2, (64 + index * 8) as u32, false)?;
    }
    for (index, value) in iv.into_iter().enumerate() {
        c.constant(5, value)?;
        c.hash_store(5, 2, (128 + index * 8) as u32, false)?;
    }
    c.immediate(3, 3, 5, 2, 344)?;
    c.immediate(3, 3, 6, 2, 352)?;
    c.add(5, 5, 6)?;
    c.immediate(3, 3, 7, 2, 160)?;
    c.register(4, 0, 7, 7, 5)?;
    c.hash_store(7, 2, 160, false)?;
    c.immediate(3, 3, 7, 2, 168)?;
    c.hash_store(7, 2, 168, false)?;
    c.immediate(3, 3, 28, 2, 328)?;
    c.sub(29, 28, 5)?;
    c.branch(1, 29, 0, Cache(5))?;
    c.immediate(3, 3, 30, 2, 176)?;
    c.immediate(0x13, 4, 30, 30, -1)?;
    c.hash_store(30, 2, 176, false)?;
    c.label(Cache(5));
    c.addi(10, 2, 0)?;
    call(&mut c, record, COMPRESS)?;
    c.immediate(3, 3, 5, 2, 344)?;
    c.immediate(3, 3, 6, 2, 352)?;
    c.add(5, 5, 6)?;
    c.hash_store(5, 2, 344, false)?;
    c.branch(0, 6, 0, Done)?;
    c.hash_jump(Cache(0))?;
    c.label(Done);
    c.immediate(3, 3, 31, 2, 336)?;
    for index in 0..4 {
        c.immediate(3, 3, 5, 2, index * 8)?;
        c.hash_store(5, 31, (index * 8) as u32, false)?;
    }
    c.immediate(3, 3, 1, 2, 392)?;
    c.addi(2, 2, 400)?;
    c.li(10, 0)?;
    c.exact(0x8067)?;
    finish(&c, end)
}
