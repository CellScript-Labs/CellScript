//! Independent finite contracts for the four legacy fallible scalar getters.
//! Validate both the complete helper and every incoming call's a1 status guard.
//! The compiler's return-convention registry is intentionally not consumed.
use super::*;

const HELPERS: &[(&str, Option<(u32, u32)>)] = &[
    ("runtime:__ckb_input_out_point_index", Some((32, 4))),
    ("runtime:__ckb_input_out_point_tx_hash_low", Some((0, 8))),
    ("runtime:__ckb_cell_lock_hash_low", None),
    ("runtime:__ckb_cell_type_hash_low", None),
];

pub(crate) fn validate_all(record: &VerifiedLoweringRecord, elf: &ParsedElf, terminal_sink: Option<&str>) -> Result<(), CheckerError> {
    let words: BTreeMap<_, _> = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect();
    let targets: BTreeMap<_, _> = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect();
    for &(owner, out_point) in HELPERS {
        let Some(entry) = record.entries.iter().find(|entry| entry.id == owner) else { continue };
        let mut blocks = record.blocks.iter().filter(|block| block.owner_entry == owner).collect::<Vec<_>>();
        blocks.sort_by_key(|block| block.range.start);
        let first = blocks.first().ok_or_else(|| invalid("scalar status helper has no body"))?;
        let start = first.range.start;
        let end = blocks.last().unwrap().range.end;
        if first.id != entry.entry_block || blocks.windows(2).any(|pair| pair[0].range.end != pair[1].range.start) {
            return Err(invalid("scalar status helper entry or ownership differs"));
        }
        let mut c = Decoder { pc: start, words: &words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
        if end - start == 12 {
            c.li(10, 1)?;
            c.addi(11, 10, 0)?;
            c.exact(0x00008067)?;
        } else {
            validate_body(&mut c, owner, out_point)?;
        }
        if c.pc != end || c.branches.iter().any(|(label, target)| c.labels.get(label) != Some(target)) {
            return Err(invalid("scalar status helper extent or branch destination differs"));
        }
        for edge in &elf.control_flow {
            if start < edge.target && edge.target < end && !(start..end).contains(&edge.address) {
                return Err(invalid("control flow enters a partial scalar status helper"));
            }
            if edge.target != start {
                continue;
            }
            let word = words[&edge.address];
            if !matches!(word & 0x7f, 0x6f | 0x67) || word >> 7 & 31 != 1 {
                return Err(invalid("scalar status helper entered without a checked returning call"));
            }
            let abort = terminal_sink
                .and_then(|id| record.blocks.iter().find(|block| block.id == id))
                .ok_or_else(|| invalid("scalar status caller has no verified abort sink"))?
                .range
                .start;
            let mut guard =
                Decoder { pc: edge.address + 4, words: &words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
            guard.branch(0, 11, 0, Label::Done)?;
            guard.addi(10, 11, 0)?;
            if guard.jump()? != abort {
                return Err(invalid("nonzero scalar status does not terminate at the verified sink"));
            }
            if guard.branches[0].1 != guard.pc {
                return Err(invalid("scalar status success branch skips a different region"));
            }
        }
    }
    Ok(())
}

fn validate_body(c: &mut Decoder<'_>, owner: &str, out_point: Option<(u32, u32)>) -> Result<(), CheckerError> {
    use Label::*;
    let frame = if out_point.is_some() { 64 } else { 32 };
    c.addi(2, 2, -frame)?;
    c.hash_store(1, 2, (frame - 8) as u32, false)?;
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
    if out_point.is_some() {
        c.li(5, 1)?;
        c.branch(0, 7, 5, Ready)?;
        c.constant(5, 0x0100_0000_0000_0001)?;
        c.branch(0, 7, 5, Ready)?;
        c.hash_jump(Invalid)?;
        c.label(Ready);
    }
    c.li(5, if out_point.is_some() { 36 } else { 8 })?;
    c.hash_store(5, 2, 8, false)?;
    c.addi(10, 2, 16)?;
    c.addi(11, 2, 8)?;
    c.li(12, 0)?;
    c.addi(13, 6, 0)?;
    c.addi(14, 7, 0)?;
    c.li(
        15,
        if out_point.is_some() {
            0
        } else if owner.ends_with("cell_lock_hash_low") {
            3
        } else {
            5
        },
    )?;
    c.constant(17, if out_point.is_some() { 2083 } else { 2081 })?;
    c.exact(0x73)?;
    c.branch(1, 10, 0, HashFailed)?;
    if let Some((offset, width)) = out_point {
        c.immediate(3, 3, 5, 2, 8)?;
        c.li(6, 36)?;
        c.sub(7, 5, 6)?;
        c.branch(1, 7, 0, HashFailed)?;
        c.addi(29, 2, 16)?;
        c.li(10, 0)?;
        for byte in 0..width {
            c.immediate(3, 4, 28, 29, (offset + byte) as i32)?;
            if byte != 0 {
                c.immediate(0x13, 1, 28, 28, (byte * 8) as i32)?;
            }
            c.register(6, 0, 10, 10, 28)?;
        }
    } else {
        c.immediate(3, 3, 10, 2, 16)?;
    }
    c.li(11, 0)?;
    c.hash_jump(Done)?;
    c.label(Invalid);
    c.li(10, 44)?;
    c.addi(11, 10, 0)?;
    c.hash_jump(Done)?;
    c.label(HashFailed);
    c.li(10, 44)?;
    c.addi(11, 10, 0)?;
    c.label(Done);
    c.immediate(3, 3, 1, 2, frame - 8)?;
    c.addi(2, 2, frame)?;
    c.exact(0x00008067)
}
