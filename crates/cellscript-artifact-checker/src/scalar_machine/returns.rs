//! Bind bounded-body scalar and direct collection-byte returns through the
//! actual shared frame restore. General aggregate producers remain separate.
use super::*;

pub(super) const PREFIX: &str = ".Lallocated_return_";

fn byte_width(ty: &str) -> Option<u32> {
    if ty == "u128" {
        return Some(16);
    }
    ty.strip_prefix("[u8; ")?.strip_suffix(']')?.parse().ok().filter(|width| (1..=8).contains(width))
}

fn copied_return(
    c: &mut Cursor<'_>,
    value: &TypedSemanticOperand,
    locals: &BTreeMap<u32, &TypedSemanticLocal>,
    frame: u32,
) -> Result<(), CheckerError> {
    let width = byte_width(&value.ty).ok_or_else(|| invalid("unsupported collection return width"))?;
    let local = value.local.and_then(|id| locals.get(&id)).ok_or_else(|| invalid("collection return local missing"))?;
    let offset = slot(local, frame)?;
    if width == 16 {
        c.stack_load(29, offset)?; // t4: copied value pointer
        for (register, start) in [(10, 0), (11, 8)] {
            c.constant(register, 0)?;
            for byte in 0..8 {
                c.load(31, 29, start + byte, true)?;
                if byte != 0 {
                    c.immediate(0x13, 1, 31, 31, byte as i32 * 8)?;
                }
                c.bit_or(register, register, 31)?;
            }
        }
    } else {
        c.constant(10, 0)?;
        for byte in 0..width {
            c.stack_load(29, offset)?;
            c.load(28, 29, byte, true)?;
            if byte != 0 {
                c.immediate(0x13, 1, 28, 28, byte as i32 * 8)?;
            }
            c.bit_or(10, 10, 28)?;
        }
    }
    Ok(())
}

fn tail(record: &VerifiedLoweringRecord, machine: &Machine<'_>, owner: &str, start: u64, frame: u32) -> Result<(), CheckerError> {
    let block = record
        .blocks
        .iter()
        .find(|block| block.owner_entry == owner && block.range.start == start)
        .ok_or_else(|| invalid("scalar return has no owned epilogue"))?;
    if block.range.end - start > 256 {
        return Err(invalid("scalar return epilogue exceeds its finite extent"));
    }
    for pc in (start..block.range.end).step_by(4) {
        if flow::checked_return_tail(pc, block.range.end, frame, machine.words, machine.targets) {
            return Ok(());
        }
        let word = *machine.words.get(&pc).ok_or_else(|| invalid("scalar return epilogue is truncated"))?;
        let destination = word >> 7 & 31;
        // Cached-read functions may first restore s5..s11 using t6 for large
        // stack offsets. Their cache values have separate lifetime proofs;
        // here only pure reloads/scratch calculations can precede the exact
        // frame restore, and none may overwrite the returned a0 value.
        let cache_reload = word & 0x707f == 0x3003 && (21..=27).contains(&destination) && matches!(word >> 15 & 31, 2 | 31);
        let scratch = destination == 31
            && match word & 0x7f {
                0x37 => true,
                0x13 => matches!(word >> 15 & 31, 0 | 31),
                0x33 => matches!(word >> 15 & 31, 0 | 2 | 31) && matches!(word >> 20 & 31, 0 | 2 | 31),
                _ => false,
            };
        if !cache_reload && !scratch {
            return Err(invalid("scalar return epilogue changes the result or bypasses its frame restore"));
        }
    }
    Err(invalid("scalar return epilogue has no complete frame restore"))
}

pub(super) fn validate<'a>(
    record: &'a VerifiedLoweringRecord,
    elf: &ParsedElf,
    machine: &Machine<'_>,
    required: &mut flow::Markers<'a>,
) -> Result<(), CheckerError> {
    let mut expected = BTreeMap::new();
    for entry in record.typed_semantics.entries.iter().filter(|entry| covered_entry(entry)) {
        for block in &entry.blocks {
            if block.terminator == "return"
                && block.runtime_error.is_none()
                && let Some(op) = block.operations.last().filter(|op| op.opcode == "return")
                && let [value] = op.operands.as_slice()
                && (scalar(&value.ty)
                    || (byte_width(&value.ty).is_some()
                        && value.local.is_some_and(|id| {
                            block.operations.iter().any(|producer| {
                                matches!(producer.detail, TypedSemanticOperationDetail::AllocatedValue { .. })
                                    && producer.destinations.contains(&id)
                            })
                        })))
            {
                expected.insert((entry.id.as_str(), block.id), (entry, op, value));
            }
        }
    }
    let mut boundaries = BTreeMap::new();
    let mut attribution = BTreeMap::new();
    let mut current = BTreeMap::new();
    let mut blocks = record.blocks.iter().collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    for block in blocks {
        let owner = block.owner_entry.as_str();
        if let Some(id) = block.lowering_block_id {
            current.insert(owner, id);
        }
        attribution.insert(block.range.start, (owner, current.get(owner).copied()));
        for label in &block.allocated_lifecycle_labels {
            let Some(suffix) = label.strip_prefix(PREFIX) else { continue };
            let parts = suffix.split('_').collect::<Vec<_>>();
            if parts.len() != 3 || !matches!(parts[2], "begin" | "end") {
                return Err(invalid("invalid scalar return marker"));
            }
            let id = parts[0].parse::<u32>().map_err(|_| invalid("invalid scalar return block"))?;
            let nonce = parts[1].parse::<u32>().map_err(|_| invalid("invalid scalar return nonce"))?;
            if boundaries.insert((owner, id, nonce, parts[2]), block.range.start).is_some() {
                return Err(invalid("duplicate scalar return boundary"));
            }
        }
    }
    let mut incoming = BTreeMap::<u64, Vec<u64>>::new();
    for (&source, &destination) in machine.targets {
        incoming.entry(destination).or_default().push(source);
    }
    let mut work = 0usize;
    let mut consumed = 0;
    for (&(owner, block, nonce, edge), &start) in &boundaries {
        if edge != "begin" {
            continue;
        }
        let &end = boundaries.get(&(owner, block, nonce, "end")).ok_or_else(|| invalid("scalar return end is absent"))?;
        if start >= end || end - start > 512 {
            return Err(invalid("empty, reversed or excessive scalar return range"));
        }
        let (entry, op, value) = expected.remove(&(owner, block)).ok_or_else(|| invalid("extra scalar return site"))?;
        for pc in (start..end).step_by(4) {
            if attribution.range(..=pc).next_back().map(|(_, value)| *value) != Some((owner, Some(block))) {
                return Err(invalid("scalar return leaves its typed block"));
            }
        }
        for (_, sources) in incoming.range((std::ops::Bound::Excluded(start), std::ops::Bound::Excluded(end))) {
            work += sources.len();
            if work > 4_000_000 || sources.iter().any(|pc| !(start..end).contains(pc)) {
                return Err(invalid("scalar return preparation can be bypassed or exceeds its proof budget"));
            }
        }
        let lowering =
            record.entries.iter().find(|entry| entry.id == owner).ok_or_else(|| invalid("scalar return owner is absent"))?;
        let frame = crate::nested_machine::owned_frame(record, elf, lowering)?;
        let locals = entry.locals.iter().map(|local| (local.id, local)).collect();
        let mut c = Cursor::new(start, end, machine.words, machine.targets);
        if scalar(&value.ty) {
            operand(&mut c, value, &locals, frame, 10)?;
        } else {
            copied_return(&mut c, value, &locals, frame)?;
        }
        if lowering.kind == crate::EntryKind::Lock {
            let success = c.label();
            c.branch(1, 10, 0, success)?;
            c.constant(10, 5)?;
            if c.jump_target()? != machine.abort {
                return Err(invalid("false scalar lock return does not abort"));
            }
            c.bind(success)?;
            c.constant(10, 0)?;
        }
        let control = c.pc;
        let epilogue = c.jump_target()?;
        c.finish()?;
        tail(record, machine, owner, epilogue, frame)?;
        if required.insert((owner, block, flow::operation_position(op.index, 0)?), (start, control, 0)).is_some() {
            return Err(invalid("scalar return overlaps another required operation"));
        }
        consumed += 2;
    }
    if !expected.is_empty() || consumed != boundaries.len() {
        return Err(invalid("missing scalar return or orphan return boundary"));
    }
    Ok(())
}
