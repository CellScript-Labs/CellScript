//! Complete physical-slot proof for a scalar forwarding Lock with an owned
//! exact-read cache. Source identities are not physical offsets when the
//! scalar allocator reuses a dead parameter slot for the returned predicate.
//! This deliberately finite grammar checks every instruction and read/write;
//! other bodies retain the conservative source-identity boundary.
use super::*;

pub(super) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    owner: &str,
    base: i64,
    frame: i64,
    cache: &exact_cache::Checked,
    abort: Option<u64>,
) -> Result<(), CheckerError> {
    let typed = record
        .typed_semantics
        .entries
        .iter()
        .find(|entry| entry.id == owner)
        .ok_or_else(|| invalid("cache forwarder has no typed entry"))?;
    let entry =
        record.entries.iter().find(|entry| entry.id == owner).ok_or_else(|| invalid("cache forwarder has no machine entry"))?;
    let scalar = |ty: &str| matches!(ty, "bool" | "u8" | "u16" | "u32" | "i32" | "u64");
    if typed.kind != "lock"
        || entry.kind != crate::EntryKind::Lock
        || typed.return_type != "bool"
        || typed.params.is_empty()
        || typed.params.len() > 8
        || typed.params.iter().any(|param| !scalar(&param.ty) || param.reference || param.mutable)
        || typed.blocks.len() != 1
        || typed.blocks[0].operations.len() != 2
        || !typed.blocks[0].successors.is_empty()
        || typed.blocks[0].terminator != "return"
        || entry.outgoing_argument_bytes != 0
        || entry.fixed_result_abi.is_some()
        || !entry.fixed_result_calls.is_empty()
    {
        return Err(invalid("cache overlap lacks the complete scalar forwarding Lock profile"));
    }
    let operations = &typed.blocks[0].operations;
    let call = &operations[0];
    let returned = &operations[1];
    let target = call.call.as_ref().ok_or_else(|| invalid("cache forwarder has no typed call"))?;
    let destination = call.destinations.first().copied().ok_or_else(|| invalid("cache forwarder has no result"))?;
    if call.opcode != "call"
        || call.destinations.len() != 1
        || target.contract != "typed-local"
        || target.return_type != "bool"
        || target.params != typed.params.iter().map(|param| param.ty.clone()).collect::<Vec<_>>()
        || call.operands.len() != typed.params.len()
        || call
            .operands
            .iter()
            .zip(&typed.params)
            .any(|(operand, param)| operand.local != Some(param.binding_id) || operand.ty != param.ty || operand.constant.is_some())
        || returned.opcode != "return"
        || !returned.destinations.is_empty()
        || returned.operands.len() != 1
        || returned.operands[0].local != Some(destination)
        || returned.operands[0].ty != "bool"
        || returned.operands[0].constant.is_some()
        || !typed.locals.iter().any(|local| local.id == destination && local.ty == "bool")
    {
        return Err(invalid("cache forwarder does not return its sole scalar call result"));
    }
    let callee = record
        .entries
        .iter()
        .find(|entry| entry.name == target.target && entry.kind == crate::EntryKind::Helper)
        .ok_or_else(|| invalid("cache forwarder target is not an ordinary helper"))?;
    let callee_start = record
        .blocks
        .iter()
        .find(|block| block.id == callee.entry_block)
        .ok_or_else(|| invalid("cache forwarder target has no prologue"))?
        .range
        .start;
    let (start, end) = streaming_hashes::extent(record, elf, owner)?;
    let words = elf.instructions.iter().map(|word| (word.address, word.word)).collect();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect();
    let mut c = Decoder { pc: start, words: &words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
    adjust(&mut c, -frame)?;
    slot(&mut c, 1, frame - 8, false)?;
    slot(&mut c, 8, frame - 16, false)?;
    address(&mut c, 8, frame)?;
    for (register, offset) in [(0, base), (0, base + 8), (27, base + 16)] {
        slot(&mut c, register, offset, false)?;
    }
    if cache.hot {
        for (index, register) in [26, 25, 24, 23, 22, 21].into_iter().enumerate() {
            slot(&mut c, register, base + 24 + index as i64 * 8, false)?;
        }
    }
    for way in 0..exact_cache::WAYS {
        slot(&mut c, 0, base + cache.header() + way * exact_cache::ENTRY_BYTES + 24, false)?;
    }
    address(&mut c, 27, base)?;
    if cache.hot {
        c.constant(26, 0)?;
    }
    let mut slots = Vec::new();
    for index in 0..typed.params.len() {
        let offset = scalar_store(&mut c, 10 + index as u32, base)?;
        if slots.contains(&offset) {
            return Err(invalid("cache forwarder parameter spills alias"));
        }
        slots.push(offset);
    }
    if !record
        .blocks
        .iter()
        .any(|block| block.owner_entry == owner && block.lowering_block_id == Some(typed.blocks[0].id) && block.range.start == c.pc)
    {
        return Err(invalid("cache forwarder body bypasses its parameter spills"));
    }
    for (index, &offset) in slots.iter().enumerate() {
        slot(&mut c, 10 + index as u32, offset, true)?;
    }
    if !crate::checker::is_auipc(c.word()?, 1)
        || !crate::checker::is_jalr_call(c.word()?)
        || c.targets.get(&(c.pc - 4)) != Some(&callee_start)
    {
        return Err(invalid("cache forwarder call target or register ABI changed"));
    }
    let result_slot = scalar_store(&mut c, 10, base)?;
    // Store/load forwarding may retain a0. The diagnostic strategy reloads
    // that same freshly written physical slot; no other read is admitted.
    if c.words.get(&c.pc).is_some_and(|word| crate::checker::is_ld(*word, 10, 2, result_slot as i32)) {
        slot(&mut c, 10, result_slot, true)?;
    }
    c.branch(1, 10, 0, Label::Ready)?;
    c.constant(10, 5)?;
    if Some(c.jump()?) != abort {
        return Err(invalid("cache forwarder false predicate does not abort"));
    }
    c.label(Label::Ready);
    c.constant(10, 0)?;
    if c.jump()? != c.pc {
        return Err(invalid("cache forwarder success bypasses its epilogue"));
    }
    if cache.hot {
        for (index, register) in [26, 25, 24, 23, 22, 21].into_iter().enumerate().rev() {
            slot(&mut c, register, base + 24 + index as i64 * 8, true)?;
        }
    }
    slot(&mut c, 27, base + 16, true)?;
    slot(&mut c, 1, frame - 8, true)?;
    slot(&mut c, 8, frame - 16, true)?;
    adjust(&mut c, frame)?;
    c.exact(0x8067)?;
    if c.pc != end || c.branches.iter().any(|(label, target)| c.labels.get(label) != Some(target)) {
        return Err(invalid("cache forwarder has unchecked instructions or branch destinations"));
    }
    Ok(())
}

fn scalar_store(c: &mut Decoder<'_>, source: u32, base: i64) -> Result<i64, CheckerError> {
    let word = c.word()?;
    let offset = i64::from(((((word >> 7 & 31) | (word >> 25 << 5)) as i32) << 20) >> 20);
    if word & 0x01fff07f != (source << 20 | 2 << 15 | 3 << 12 | 0x23)
        || offset < 0
        || offset % 8 != 0
        || offset.checked_add(8).is_none_or(|end| end > base)
    {
        return Err(invalid("cache forwarder scalar store aliases the cache or lacks a private slot"));
    }
    Ok(offset)
}

fn address(c: &mut Decoder<'_>, register: u32, offset: i64) -> Result<(), CheckerError> {
    if (-2048..=2047).contains(&offset) {
        c.addi(register, 2, offset as i32)
    } else {
        c.constant(register, offset as u64)?;
        c.add(register, 2, register)
    }
}

fn adjust(c: &mut Decoder<'_>, delta: i64) -> Result<(), CheckerError> {
    if (-2048..=2047).contains(&delta) {
        c.addi(2, 2, delta as i32)
    } else {
        c.constant(31, delta as u64)?;
        c.add(2, 2, 31)
    }
}

fn slot(c: &mut Decoder<'_>, register: u32, offset: i64, load: bool) -> Result<(), CheckerError> {
    let (base, offset) = if (-2048..=2047).contains(&offset) {
        (2, offset as i32)
    } else {
        address(c, 31, offset)?;
        (31, 0)
    };
    if load {
        c.immediate(3, 3, register, base, offset)
    } else {
        c.exact(((offset as u32 & 0xfe0) << 20) | register << 20 | base << 15 | 3 << 12 | (offset as u32 & 31) << 7 | 0x23)
    }
}
