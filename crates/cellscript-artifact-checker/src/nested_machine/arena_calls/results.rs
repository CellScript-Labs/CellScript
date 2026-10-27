//! Bind scalar call results to the caller's typed destination. Start at the
//! resolved call's continuation; no source marker can move this proof past a
//! clobber or an incorrect outgoing-stack restoration. The callee's returned
//! value and subsequent preservation of the destination are separate proofs.
use super::*;
use crate::cell_selection::machine::cursor::Cursor;

pub(super) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf, calls: &BTreeMap<u64, Call>) -> Result<(), CheckerError> {
    let words = elf.instructions.iter().map(|i| (i.address, i.word)).collect::<BTreeMap<_, _>>();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect::<BTreeMap<_, _>>();
    let entries = record.typed_semantics.entries.iter().map(|entry| (entry.id.as_str(), entry)).collect::<BTreeMap<_, _>>();
    let mut locations = BTreeMap::new();
    let mut current = BTreeMap::new();
    let mut blocks = record.blocks.iter().collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    for block in blocks {
        if let Some(id) = block.lowering_block_id {
            current.insert(block.owner_entry.as_str(), id);
        }
        locations.insert(block.range.start, (block.owner_entry.as_str(), current.get(block.owner_entry.as_str()).copied()));
    }
    let mut incoming = BTreeMap::<u64, Vec<u64>>::new();
    for edge in &elf.control_flow {
        incoming.entry(edge.target).or_default().push(edge.address);
    }
    let mut frames = BTreeMap::new();
    let mut work = 0usize;
    for (&pc, call) in calls {
        let Some(destination) = call.result else { continue };
        let caller = entries[call.owner.as_str()];
        let callee = entries[call.callee.as_str()];
        let local = caller
            .locals
            .iter()
            .find(|local| local.id == destination)
            .filter(|local| local.ty == callee.return_type)
            .ok_or_else(|| invalid("scalar call result differs from its typed destination"))?;
        let frame = if let Some(&frame) = frames.get(call.owner.as_str()) {
            frame
        } else {
            let entry = record
                .entries
                .iter()
                .find(|entry| entry.id == call.owner)
                .ok_or_else(|| invalid("scalar result caller has no frame"))?;
            let frame = owned_frame(record, elf, entry)?;
            frames.insert(call.owner.as_str(), frame);
            frame
        };
        let slot = local
            .source_id
            .checked_mul(8)
            .and_then(|slot| u32::try_from(slot).ok())
            .ok_or_else(|| invalid("scalar result slot overflows"))?;
        if slot.checked_add(8).is_none_or(|end| end > frame.saturating_sub(16)) {
            return Err(invalid("scalar result leaves its caller's private frame"));
        }
        let start = pc.checked_add(4).ok_or_else(|| invalid("scalar result address overflows"))?;
        let limit = start.checked_add(256).ok_or_else(|| invalid("scalar result range overflows"))?;
        let mut cursor = Cursor::new(start, limit, &words, &targets);
        if call.outgoing != 0 {
            cursor.stack_adjust(i64::from(call.outgoing))?;
        }
        cursor.stack_store(10, slot)?;
        let end = cursor.pc;
        for address in (start..end).step_by(4) {
            work += 1;
            if work > 4_000_000
                || locations.range(..=address).next_back().map(|(_, location)| *location)
                    != Some((call.owner.as_str(), Some(call.block)))
            {
                return Err(invalid("scalar result leaves its typed call block or proof budget"));
            }
        }
        for (_, sources) in incoming.range(start..end) {
            work += sources.len();
            if work > 4_000_000 || sources.iter().any(|source| !(start..end).contains(source)) {
                return Err(invalid("control flow enters a scalar result without its call"));
            }
        }
    }
    Ok(())
}
