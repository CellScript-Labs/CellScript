//! Closed machine contract for the existing C256 product comparison closure.
//! Each top-level requirement only reads its u128 operands. All multiply/add
//! outputs belong to its own fixed frame; no incoming pointer authorizes a
//! write. Validate every instruction and nested call before using that effect.
use super::*;

const LOAD: &str = "runtime:__cellscript_load_u64_le";
const MULTIPLY: &str = "runtime:__cellscript_mul_u128_to_u256";
const ADD: &str = "runtime:__cellscript_add_u256";

fn kind(owner: &str) -> Option<(bool, bool)> {
    match owner {
        "runtime:__c256_require_u128_product_eq" => Some((false, true)),
        "runtime:__c256_require_u128_product_lte" => Some((false, false)),
        "runtime:__c256_require_u128_sum2_products_eq" => Some((true, true)),
        "runtime:__c256_require_u128_sum2_products_lte" => Some((true, false)),
        _ => None,
    }
}

pub(super) fn operands(owner: &str) -> Option<usize> {
    kind(owner).map(|(sum, _)| if sum { 8 } else { 4 })
}

pub(super) fn validate_all(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<(), CheckerError> {
    for entry in record.entries.iter().filter(|entry| kind(&entry.id).is_some()) {
        validate(record, elf, &entry.id)?;
    }
    Ok(())
}

pub(super) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf, owner: &str) -> Result<(), CheckerError> {
    let (sum, equality) = kind(owner).ok_or_else(|| invalid("unknown C256 product requirement"))?;
    let words = elf.instructions.iter().map(|word| (word.address, word.word)).collect();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect();
    for dependency in [LOAD, MULTIPLY] {
        check_body(record, elf, dependency, &words, &targets, |c| if dependency == LOAD { load(c) } else { multiply(c, record) })?;
    }
    if sum {
        check_body(record, elf, ADD, &words, &targets, add)?;
    }
    check_body(record, elf, owner, &words, &targets, |c| requirement(c, record, sum, equality))
}

fn check_body(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    owner: &str,
    words: &BTreeMap<u64, u32>,
    targets: &BTreeMap<u64, u64>,
    body: impl FnOnce(&mut Decoder<'_>) -> Result<(), CheckerError>,
) -> Result<(), CheckerError> {
    // This also rejects external entry into the middle of any dependency and
    // records whose block inventory hides a gap or a different prologue.
    let (start, end) = streaming_hashes::extent(record, elf, owner)?;
    let mut c = Decoder { pc: start, words, targets, labels: BTreeMap::new(), branches: Vec::new() };
    body(&mut c).map_err(|error| invalid(format!("C256 closure {owner}: {error}")))?;
    if c.pc != end || c.branches.iter().any(|(label, target)| c.labels.get(label) != Some(target)) {
        return Err(invalid("C256 closure extent or branch target differs"));
    }
    Ok(())
}

fn call(c: &mut Decoder<'_>, record: &VerifiedLoweringRecord, owner: &str) -> Result<(), CheckerError> {
    let target = record
        .entries
        .iter()
        .find(|entry| entry.id == owner)
        .and_then(|entry| record.blocks.iter().find(|block| block.id == entry.entry_block))
        .ok_or_else(|| invalid("C256 dependency has no complete entry"))?
        .range
        .start;
    let upper = c.word()?;
    let at = c.pc;
    let lower = c.word()?;
    if upper & 0xfff != 0x97 || lower & 0x000f_ffff != 0x000080e7 || c.targets.get(&at) != Some(&target) {
        return Err(invalid("C256 closure calls an unchecked target"));
    }
    Ok(())
}

fn ld(c: &mut Decoder<'_>, register: u32, base: u32, offset: i32) -> Result<(), CheckerError> {
    c.immediate(3, 3, register, base, offset)
}

fn sd(c: &mut Decoder<'_>, register: u32, base: u32, offset: u32) -> Result<(), CheckerError> {
    c.hash_store(register, base, offset, false)
}

fn load(c: &mut Decoder<'_>) -> Result<(), CheckerError> {
    c.li(11, 0)?;
    for byte in 0..8 {
        c.immediate(3, 4, 5, 10, byte)?;
        if byte != 0 {
            c.immediate(0x13, 1, 5, 5, byte * 8)?;
        }
        c.register(6, 0, 11, 11, 5)?;
    }
    c.addi(10, 11, 0)?;
    c.exact(0x8067)
}

fn multiply(c: &mut Decoder<'_>, record: &VerifiedLoweringRecord) -> Result<(), CheckerError> {
    c.addi(2, 2, -96)?;
    for (register, offset) in [(1, 88), (10, 0), (11, 8), (12, 16)] {
        sd(c, register, 2, offset)?;
    }
    for (pointer, byte, destination) in [(0, 0, 24), (0, 8, 32), (8, 0, 40), (8, 8, 48)] {
        ld(c, 10, 2, pointer)?;
        if byte != 0 {
            c.addi(10, 10, byte)?;
        }
        call(c, record, LOAD)?;
        sd(c, 10, 2, destination)?;
    }
    // Four 64x64 products, with both cross-product carries accumulated before
    // the high limb is written. Every output offset is exactly 0/8/16/24.
    ld(c, 5, 2, 24)?;
    ld(c, 6, 2, 40)?;
    c.register(0, 1, 7, 5, 6)?;
    c.register(3, 1, 28, 5, 6)?;
    sd(c, 7, 2, 56)?;
    ld(c, 5, 2, 24)?;
    ld(c, 6, 2, 48)?;
    c.register(0, 1, 29, 5, 6)?;
    c.register(3, 1, 30, 5, 6)?;
    ld(c, 5, 2, 32)?;
    ld(c, 6, 2, 40)?;
    c.register(0, 1, 31, 5, 6)?;
    c.register(3, 1, 13, 5, 6)?;
    c.add(5, 28, 29)?;
    c.register(3, 0, 14, 5, 28)?;
    c.add(6, 5, 31)?;
    c.register(3, 0, 15, 6, 5)?;
    c.add(14, 14, 15)?;
    sd(c, 6, 2, 64)?;
    ld(c, 5, 2, 32)?;
    ld(c, 6, 2, 48)?;
    c.register(0, 1, 15, 5, 6)?;
    c.register(3, 1, 16, 5, 6)?;
    c.add(7, 30, 13)?;
    c.register(3, 0, 17, 7, 30)?;
    c.add(28, 7, 15)?;
    c.register(3, 0, 29, 28, 7)?;
    c.add(30, 28, 14)?;
    c.register(3, 0, 31, 30, 28)?;
    sd(c, 30, 2, 72)?;
    c.add(5, 16, 17)?;
    c.add(5, 5, 29)?;
    c.add(5, 5, 31)?;
    sd(c, 5, 2, 80)?;
    ld(c, 5, 2, 16)?;
    for (source, destination) in [(56, 0), (64, 8), (72, 16), (80, 24)] {
        ld(c, 6, 2, source)?;
        sd(c, 6, 5, destination)?;
    }
    ld(c, 1, 2, 88)?;
    c.addi(2, 2, 96)?;
    c.exact(0x8067)
}

fn add(c: &mut Decoder<'_>) -> Result<(), CheckerError> {
    c.li(13, 0)?;
    for limb in [0, 8, 16, 24] {
        ld(c, 5, 10, limb)?;
        ld(c, 6, 11, limb)?;
        c.add(7, 5, 6)?;
        c.register(3, 0, 28, 7, 5)?;
        c.add(7, 7, 13)?;
        c.register(3, 0, 29, 7, 13)?;
        sd(c, 7, 12, limb as u32)?;
        c.add(13, 28, 29)?;
    }
    c.addi(10, 13, 0)?;
    c.exact(0x8067)
}

fn requirement(c: &mut Decoder<'_>, record: &VerifiedLoweringRecord, sum: bool, equality: bool) -> Result<(), CheckerError> {
    use Label::{Done, Invalid, Item, Ready};
    let frame = if sum { 272 } else { 112 };
    let saved_ra = frame - 8;
    c.addi(2, 2, -frame)?;
    sd(c, 1, 2, saved_ra as u32)?;
    let arity = if sum { 8 } else { 4 };
    if sum {
        for word in 0..arity {
            sd(c, 10 + word, 2, word * 8)?;
            c.branch(0, 10 + word, 0, Invalid)?;
        }
    } else {
        for word in 0..arity {
            sd(c, 10 + word, 2, word * 8)?;
        }
        for word in 0..arity {
            c.branch(0, 10 + word, 0, Invalid)?;
        }
    }
    for (left, right, output) in [(0, 8, if sum { 64 } else { 32 }), (16, 24, if sum { 96 } else { 64 })] {
        ld(c, 10, 2, left)?;
        ld(c, 11, 2, right)?;
        c.addi(12, 2, output)?;
        call(c, record, MULTIPLY)?;
    }
    if sum {
        for (register, offset) in [(10, 64), (11, 96), (12, 128)] {
            c.addi(register, 2, offset)?;
        }
        call(c, record, ADD)?;
        c.branch(1, 10, 0, Item)?;
        for (left, right, output) in [(32, 40, 160), (48, 56, 192)] {
            ld(c, 10, 2, left)?;
            ld(c, 11, 2, right)?;
            c.addi(12, 2, output)?;
            call(c, record, MULTIPLY)?;
        }
        for (register, offset) in [(10, 160), (11, 192), (12, 224)] {
            c.addi(register, 2, offset)?;
        }
        call(c, record, ADD)?;
        c.branch(1, 10, 0, Item)?;
    }
    let (left, right) = if sum { (128, 224) } else { (32, 64) };
    for limb in [24, 16, 8, 0] {
        ld(c, 5, 2, left + limb)?;
        ld(c, 6, 2, right + limb)?;
        if equality {
            c.sub(7, 5, 6)?;
            c.branch(1, 7, 0, Item)?;
        } else {
            c.register(3, 0, 7, 5, 6)?;
            c.branch(1, 7, 0, Ready)?;
            c.register(3, 0, 7, 6, 5)?;
            c.branch(1, 7, 0, Item)?;
        }
    }
    c.label(Ready);
    c.li(10, 0)?;
    c.hash_jump(Done)?;
    c.label(Invalid);
    c.li(10, 18)?;
    c.hash_jump(Done)?;
    c.label(Item);
    c.li(10, 49)?;
    c.label(Done);
    ld(c, 1, 2, saved_ra)?;
    c.addi(2, 2, frame)?;
    c.exact(0x8067)
}
