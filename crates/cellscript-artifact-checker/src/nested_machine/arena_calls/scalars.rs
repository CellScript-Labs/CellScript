//! Exact evaluated scalar argument loads/stores. The lifetime interpreter
//! separately preserves these facts to the consuming call along every path.
use super::*;
use crate::cell_selection::machine::cursor::Cursor;

const PREFIX: &str = ".Lallocated_call_";

pub(super) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    calls: &BTreeMap<u64, Call>,
) -> Result<BTreeMap<u64, Vec<ScalarStage>>, CheckerError> {
    let words = elf.instructions.iter().map(|i| (i.address, i.word)).collect::<BTreeMap<_, _>>();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect::<BTreeMap<_, _>>();
    let mut incoming = BTreeMap::<u64, Vec<u64>>::new();
    for edge in &elf.control_flow {
        incoming.entry(edge.target).or_default().push(edge.address);
    }
    let mut blocks = record.blocks.iter().collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    let mut current = BTreeMap::new();
    let mut locations = BTreeMap::new();
    let mut boundaries = BTreeMap::new();
    let mut invocations = BTreeMap::new();
    for block in &blocks {
        let owner = block.owner_entry.as_str();
        if let Some(id) = block.lowering_block_id {
            current.insert(owner, id);
        }
        locations.insert(block.range.start, (owner, current.get(owner).copied()));
        for label in &block.allocated_lifecycle_labels {
            let Some(suffix) = label.strip_prefix(PREFIX) else { continue };
            let parts = suffix.split('_').collect::<Vec<_>>();
            let nonce = parts[0].parse::<u64>().map_err(|_| invalid("invalid scalar call marker identity"))?;
            if parts.len() == 2 && parts[1] == "invoke" {
                if invocations.insert((owner, nonce), block.range.start).is_some() {
                    return Err(invalid("duplicate scalar call invocation"));
                }
            } else if parts.len() == 3 && matches!(parts[2], "begin" | "end") {
                let position = parts[1].parse::<u32>().map_err(|_| invalid("invalid scalar argument ordinal"))?;
                if boundaries.insert((owner, nonce, position, parts[2]), block.range.start).is_some() {
                    return Err(invalid("duplicate scalar argument boundary"));
                }
            } else {
                return Err(invalid("unknown scalar call boundary"));
            }
        }
    }
    let typed = record.typed_semantics.entries.iter().map(|entry| (entry.id.as_str(), entry)).collect::<BTreeMap<_, _>>();
    let mut observed = BTreeSet::new();
    let mut ends = BTreeMap::<u64, Vec<ScalarStage>>::new();
    let mut ranges = Vec::new();
    let mut work = 0usize;
    for ((owner, nonce), invoke) in invocations {
        let instruction = *words.get(&invoke).ok_or_else(|| invalid("scalar invocation has no instruction"))?;
        let call_pc = if instruction & 0xfff == 0xef {
            invoke
        } else if instruction & 0xfff == 0x97 && words.get(&(invoke + 4)).is_some_and(|word| word & 0xfffff == 0x080e7) {
            invoke + 4
        } else {
            return Err(invalid("scalar invocation is not a linked direct call"));
        };
        let call = calls
            .get(&call_pc)
            .filter(|call| call.owner == owner && !call.scalars.is_empty())
            .ok_or_else(|| invalid("scalar invocation has no consuming typed call"))?;
        if !observed.insert(call_pc) {
            return Err(invalid("scalar invocation reused"));
        }
        let entry = typed[owner];
        let locals = entry.locals.iter().map(|local| (local.id, local)).collect::<BTreeMap<_, _>>();
        let lowering = record.entries.iter().find(|entry| entry.id == owner).ok_or_else(|| invalid("scalar caller has no frame"))?;
        let frame = owned_frame(record, elf, lowering)?;
        let mut previous = 0;
        for argument in &call.scalars {
            let start = boundaries
                .remove(&(owner, nonce, argument.position, "begin"))
                .ok_or_else(|| invalid("scalar call preparation missing"))?;
            let end = boundaries
                .remove(&(owner, nonce, argument.position, "end"))
                .ok_or_else(|| invalid("scalar call preparation end missing"))?;
            if start > end || end - start > 256 || end > invoke || start < previous {
                return Err(invalid("scalar call preparation reordered, oversized or detached"));
            }
            previous = end;
            for pc in [start, end] {
                if locations.range(..=pc).next_back().map(|(_, location)| *location) != Some((owner, Some(call.block))) {
                    return Err(invalid("scalar call preparation leaves its typed block"));
                }
            }
            // A source-only zero-length span is possible when the previous SD
            // already leaves the required value in the argument register. The
            // cursor proves that adjacent store and forbids an alternate entry.
            for (_, sources) in incoming.range((std::ops::Bound::Excluded(start), std::ops::Bound::Included(end))) {
                work += sources.len();
                if work > 4_000_000 || sources.iter().any(|pc| !(start..end).contains(pc)) {
                    return Err(invalid("control flow bypasses scalar argument preparation"));
                }
            }
            let register = if argument.word < 8 { 10 + argument.word } else { 5 };
            let mut c = Cursor::new(start, end, &words, &targets);
            if let Some(id) = argument.operand.local {
                let local = locals.get(&id).ok_or_else(|| invalid("scalar call operand has no local"))?;
                let slot = local
                    .source_id
                    .checked_mul(8)
                    .and_then(|slot| u32::try_from(slot).ok())
                    .ok_or_else(|| invalid("scalar call operand slot overflows"))?;
                if local.ty != argument.operand.ty || slot.checked_add(8).is_none_or(|end| end > frame.saturating_sub(16)) {
                    return Err(invalid("scalar call operand leaves its typed frame"));
                }
                c.stack_load(register, slot)?;
            } else {
                use crate::TypedSemanticConstant as K;
                let value = match argument.operand.constant.as_ref() {
                    Some(K::Bool(value)) => u64::from(*value),
                    Some(K::U8(value) | K::U16(value) | K::U32(value) | K::U64(value)) => {
                        value.parse().map_err(|_| invalid("invalid scalar call constant"))?
                    }
                    _ => return Err(invalid("scalar call argument lacks an admitted source")),
                };
                c.constant(register, value)?;
            }
            if argument.word >= 8 {
                let offset = i64::from(argument.word - 8) * 8 - i64::from(call.outgoing);
                if offset >= 0 || offset < -i64::from(call.outgoing) {
                    return Err(invalid("scalar argument leaves outgoing area"));
                }
                if (-2048..=2047).contains(&offset) {
                    let displacement = offset as u32 & 0xfff;
                    c.exact(
                        ((displacement >> 5) << 25) | (register << 20) | (2 << 15) | (3 << 12) | ((displacement & 31) << 7) | 0x23,
                    )?;
                } else {
                    c.constant(31, offset as u64)?;
                    c.add(31, 2, 31)?;
                    c.store(register, 31, 0, false)?;
                }
            }
            c.finish()?;
            ranges.push((start, end));
            ends.entry(end).or_default().push(ScalarStage {
                owner: owner.to_string(),
                call: call_pc,
                word: argument.word,
                outgoing: call.outgoing,
            });
        }
    }
    ranges.sort_unstable();
    if !boundaries.is_empty()
        || ranges.windows(2).any(|pair| pair[0].1 > pair[1].0)
        || calls.iter().any(|(pc, call)| !call.scalars.is_empty() && !observed.contains(pc))
    {
        return Err(invalid("orphan, overlapping or missing scalar call preparations"));
    }
    Ok(ends)
}
