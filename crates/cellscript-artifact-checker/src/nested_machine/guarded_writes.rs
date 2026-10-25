//! Exact spill/guard/store/restore syntax shared by transport and memory proofs.
//! Callers independently evaluate the private-frame bounds at the two guards;
//! this parser alone does not authorize a write or establish pointer provenance.
use super::*;

pub(super) struct Sequence {
    pub guard: u64,
    pub end: u64,
}

pub(super) fn sequence(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    words: &BTreeMap<u64, u32>,
    pc: u64,
) -> Result<Option<Sequence>, CheckerError> {
    if words.get(&pc) != Some(&0xfe010113) {
        // addi sp, sp, -32
        return Ok(None);
    }
    let mut saved = Vec::new();
    for index in 0..3u32 {
        let Some(&word) = words.get(&(pc + 4 + u64::from(index) * 4)) else { return Ok(None) };
        let offset = (((word >> 7 & 31) | (word >> 25 << 5)) as i32) << 20 >> 20;
        let register = (word >> 20 & 31) as usize;
        if word & 0x707f != 0x3023
            || word >> 15 & 31 != 2
            || offset != (index * 8) as i32
            || matches!(register, 0 | 2)
            || saved.contains(&register)
        {
            return Ok(None);
        }
        saved.push(register);
    }
    let mut at = pc + 16;
    for _ in 0..32 {
        let Some(&word) = words.get(&at) else { return Ok(None) };
        if word & 0x7f == 0x63 {
            let Some((&upper, &store)) = words.get(&(at + 4)).zip(words.get(&(at + 8))) else { return Ok(None) };
            let pointer = word >> 15 & 31;
            let offset = (((store >> 7 & 31) | (store >> 25 << 5)) as i32) << 20 >> 20;
            if word & 0x707f != 0x6063
                || upper & 0x707f != 0x6063
                || store & 0x7f != 0x23
                || upper >> 20 & 31 != pointer
                || store >> 15 & 31 != pointer
                || offset != 0
                || store >> 12 & 7 > 3
                || saved.contains(&((store >> 20 & 31) as usize))
                || store >> 20 & 31 == 2
            {
                return Ok(None);
            }
            for branch in [at, at + 4] {
                if !elf.control_flow.iter().find(|flow| flow.address == branch).is_some_and(|flow| {
                    record.verifier_failure_exits.iter().any(|exit| exit.address == flow.target && exit.code == 21)
                }) {
                    return Ok(None);
                }
            }
            let restore = at + 12;
            for (index, register) in saved.iter().enumerate() {
                let expected = (index as u32 * 8) << 20 | 2 << 15 | 3 << 12 | (*register as u32) << 7 | 3;
                if words.get(&(restore + index as u64 * 4)) != Some(&expected) {
                    return Ok(None);
                }
            }
            if words.get(&(restore + 12)) != Some(&0x02010113) {
                // addi sp, sp, 32
                return Ok(None);
            }
            let end = restore + 16;
            let owner = record.blocks.iter().find(|block| block.range.contains(pc)).map(|block| block.owner_entry.as_str());
            if owner.is_none()
                || !record.blocks.iter().any(|block| Some(block.owner_entry.as_str()) == owner && block.range.contains(end - 4))
                || record
                    .blocks
                    .iter()
                    .any(|block| block.range.start < end && block.range.end > pc && Some(block.owner_entry.as_str()) != owner)
                || elf.control_flow.iter().any(|flow| flow.target > pc && flow.target < end)
            {
                return Err(invalid("control flow bypasses a guarded write spill or restore"));
            }
            return Ok(Some(Sequence { guard: at, end }));
        }
        let rd = (word >> 7 & 31) as usize;
        let kind = word >> 12 & 7;
        if !saved.contains(&rd)
            || !matches!(word & 0x7f, 0x37)
                && !matches!((word & 0x7f, kind), (0x13 | 0x1b, 0))
                && !(word & 0x7f == 0x33 && kind == 0 && word >> 25 == 0)
        {
            return Ok(None);
        }
        at += 4;
    }
    Ok(None)
}
