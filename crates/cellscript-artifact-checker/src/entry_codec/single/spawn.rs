//! Complete bounded hex4 spawn/wait helper and fail-closed caller recognition.
use super::*;

pub(crate) fn verify(record: &VerifiedLoweringRecord, elf: &ParsedElf, owner: &str, index: u64) -> Result<(), CheckerError> {
    if let Some((logical, decoded)) = crate::policy_machine::normalize_relaxed_branches(record, elf).map_err(invalid)? {
        return verify_logical(&logical, &decoded, owner, index);
    }
    verify_logical(record, elf, owner, index)
}
fn verify_logical(record: &VerifiedLoweringRecord, elf: &ParsedElf, owner: &str, index: u64) -> Result<(), CheckerError> {
    let helper = record
        .entries
        .iter()
        .find(|e| e.name == "__ckb_spawn_wait_cell_dep_hex4")
        .ok_or_else(|| invalid("missing spawn/wait machine helper"))?;
    let start = entry_start(record, &helper.id)?;
    let mut c = Cursor { elf, address: start };
    let owned = record.blocks.iter().filter(|b| b.owner_entry == helper.id).collect::<Vec<_>>();
    if elf.control_flow.iter().any(|f| {
        owned.iter().any(|b| b.range.contains(f.target)) && !owned.iter().any(|b| b.range.contains(f.address)) && f.target != start
    }) {
        return Err(invalid("incoming flow bypasses spawn/wait guards"));
    }
    let labels = |prefix: &str, count: usize| -> Result<Vec<u64>, CheckerError> {
        let mut addresses = owned
            .iter()
            .filter(|b| {
                b.machine_label
                    .as_deref()
                    .and_then(|l| l.strip_prefix(prefix))
                    .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
            })
            .map(|b| b.range.start)
            .collect::<Vec<_>>();
        addresses.sort();
        if addresses.len() != count {
            return Err(invalid("spawn/wait block cardinality differs"));
        }
        Ok(addresses)
    };
    let fail = labels(".Lexec_hex4_failed_", 1)?[0];
    let done = labels(".Lspawn_hex4_done_", 1)?[0];
    let scans = labels(".Lexec_hex4_encode_", 4)?;
    let ends = labels(".Lexec_hex4_encoded_", 4)?;
    let digits = labels(".Lexec_hex4_digit_", 8)?;
    let chars = labels(".Lexec_hex4_char_", 8)?;
    c.i(2, 2, -624)?;
    c.store(1, 2, 560, false)?;
    c.store(10, 2, 568, false)?;
    c.li(5, 256)?;
    for register in 12..=16 {
        c.b(5, register, 6, fail)?;
    }
    c.r(5, 13, 14, 0)?;
    c.r(5, 5, 15, 0)?;
    c.r(5, 5, 16, 0)?;
    c.b(5, 12, 1, fail)?;
    c.i(6, 11, 0)?;
    c.i(7, 2, 0)?;
    for item in 0..4 {
        c.store(7, 2, 520 + item as i32 * 8, false)?;
        c.i(28, 13 + item as u32, 0)?;
        c.at(scans[item])?;
        c.b(28, 0, 0, ends[item])?;
        c.load(29, 6, 0, true)?;
        for half in 0..2 {
            if half == 0 {
                c.shift(30, 29, 4, true)?;
            } else {
                c.expect(|w| w == (29 << 15 | 5 << 12 | 30 << 7 | 0x13) || is_addi(w, 30, 29, 0))?;
            }
            c.li(31, 15)?;
            c.r(30, 30, 31, 0x7000)?;
            c.li(31, 10)?;
            c.b(30, 31, 6, digits[item * 2 + half])?;
            c.i(30, 30, 87)?;
            c.jump(chars[item * 2 + half])?;
            c.at(digits[item * 2 + half])?;
            c.i(30, 30, 48)?;
            c.at(chars[item * 2 + half])?;
            c.store(30, 7, half as i32, true)?;
        }
        c.i(6, 6, 1)?;
        c.i(7, 7, 2)?;
        c.i(28, 28, -1)?;
        c.jump(scans[item])?;
        c.at(ends[item])?;
        c.store(0, 7, 0, true)?;
        c.i(7, 7, 1)?;
    }
    c.store(0, 2, 552, false)?;
    c.load(10, 2, 568, false)?;
    c.li(11, 3)?;
    c.li(12, 0)?;
    c.li(13, 0)?;
    c.li(5, 4)?;
    c.store(5, 2, 576, false)?;
    c.i(5, 2, 520)?;
    c.store(5, 2, 584, false)?;
    c.i(5, 2, 608)?;
    c.store(5, 2, 592, false)?;
    for offset in [600, 608, 616] {
        c.store(0, 2, offset, false)?;
    }
    c.i(14, 2, 576)?;
    c.li(15, 0)?;
    c.li(17, 2601)?;
    c.expect(|w| w == 0x73)?;
    c.b(10, 0, 1, fail)?;
    c.load(10, 2, 608, false)?;
    c.i(11, 2, 616)?;
    c.li(17, 2602)?;
    c.expect(|w| w == 0x73)?;
    c.b(10, 0, 1, fail)?;
    c.load(5, 2, 616, true)?;
    c.b(5, 0, 1, fail)?;
    c.li(10, 0)?;
    c.jump(done)?;
    c.at(fail)?;
    c.li(10, 1)?;
    c.at(done)?;
    c.load(1, 2, 560, false)?;
    c.i(2, 2, 624)?;
    c.expect(|w| w == 0x00008067)?;
    if owned.iter().any(|b| b.range.start < start || b.range.end > c.address) {
        return Err(invalid("unchecked spawn/wait helper code"));
    }
    let calls = elf
        .control_flow
        .iter()
        .filter(|f| f.target == start && record.blocks.iter().any(|b| b.owner_entry == owner && b.range.contains(f.address)))
        .collect::<Vec<_>>();
    if calls.len() != 1 {
        return Err(invalid("expected one actual selected child invocation"));
    }
    let call = calls[0];
    let block = crate::checker::block_for_address(record, call.address).ok_or_else(|| invalid("missing spawn caller block"))?;
    if crate::checker::register_constant_before(elf, block, call.address, 10) != Some(index) {
        return Err(invalid("actual spawn dependency differs from the selected direct index"));
    }
    let abort = record.entries.iter().find(|e| e.name == "__cellscript_abort").ok_or_else(|| invalid("missing caller abort"))?;
    let mut c = Cursor { elf, address: call.address };
    c.expect(is_jalr_call)?;
    c.b(10, 0, 0, call.address + 12)?;
    c.jump(entry_start(record, &abort.id)?)?;
    Ok(())
}
