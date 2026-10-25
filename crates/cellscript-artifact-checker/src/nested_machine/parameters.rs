//! Independently checked incoming helper words and their immutable frame slots.
//! Arena spills do not establish caller pointer provenance or borrowed reads.
use super::*;

/// Resume after independently checked mixed-signature decoders, then interpret
/// every remaining prologue instruction. No marker can authorize a spill bypass.
pub(super) fn finish_prefix(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    entry: &crate::LoweringEntry,
    seed: Option<&lifetime::Seed>,
    words: &BTreeMap<u64, u32>,
) -> Result<PrefixMachine, CheckerError> {
    let start =
        record.blocks.iter().find(|block| block.id == entry.entry_block).ok_or_else(|| invalid("helper has no prologue"))?.range.start;
    let end = record
        .blocks
        .iter()
        .filter(|block| block.owner_entry == entry.id && block.lowering_block_id.is_some())
        .map(|block| block.range.start)
        .min()
        .ok_or_else(|| invalid("helper has no typed body"))?;
    let frame = owned_frame(record, elf, entry)?;
    let mut machine = seed.map_or_else(|| PrefixMachine::new(start, frame), |seed| seed.machine.clone());
    let prefix_start = machine.pc;
    if prefix_start < start || prefix_start > end || machine.frame != i64::from(frame) {
        return Err(invalid("helper prefix lies outside its owning frame/body"));
    }
    if elf.control_flow.iter().any(|flow| {
        flow.target < end
            && (flow.target > prefix_start
                || (prefix_start > start && flow.target == prefix_start && (flow.address < start || flow.address >= prefix_start)))
    }) {
        return Err(invalid("control flow bypasses a helper parameter spill"));
    }
    if elf.control_flow.iter().any(|flow| {
        flow.target != start
            && record.blocks.iter().any(|block| block.owner_entry == entry.id && block.range.contains(flow.target))
            && !record.blocks.iter().any(|block| block.owner_entry == entry.id && block.range.contains(flow.address))
    }) {
        return Err(invalid("external control flow bypasses the helper prologue"));
    }
    machine.execute_until(end, words)?;
    Ok(machine)
}

pub(super) fn allocated(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    seeds: &mut BTreeMap<String, lifetime::Seed>,
) -> Result<(), CheckerError> {
    let owners = crate::allocated_values::owner_types(&record.typed_semantics);
    if owners.is_empty() {
        return Ok(());
    }
    let words = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect();
    for typed in &record.typed_semantics.entries {
        let is_owner = |ty: &str| ty.strip_prefix('&').is_some_and(|ty| owners.contains(ty));
        if typed.kind != "helper" || !typed.params.iter().any(|param| is_owner(&param.ty)) {
            continue;
        }
        let entry =
            record.entries.iter().find(|entry| entry.id == typed.id).ok_or_else(|| invalid("arena helper has no machine entry"))?;
        let machine = finish_prefix(record, elf, entry, seeds.get(&typed.id), &words)?;
        let scalar_end = typed
            .locals
            .iter()
            .map(|local| local.source_id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .and_then(|count| count.checked_mul(8))
            .and_then(|offset| i64::try_from(offset).ok())
            .ok_or_else(|| invalid("arena helper scalar frame overflows"))?;
        let mut word = 0u32;
        let mut slots = Vec::new();
        let mut spans = BTreeMap::new();
        for param in &typed.params {
            let width = crate::fixed_results::parameter_word_width(record, typed, param);
            if is_owner(&param.ty) {
                if width != 2 {
                    return Err(invalid("arena helper requires pointer and extent ABI words"));
                }
                let local = typed
                    .locals
                    .iter()
                    .find(|local| local.id == param.binding_id)
                    .ok_or_else(|| invalid("arena parameter has no typed local"))?;
                let pointer = local
                    .source_id
                    .checked_mul(8)
                    .and_then(|slot| i64::try_from(slot).ok())
                    .ok_or_else(|| invalid("arena parameter pointer slot overflows"))?;
                // Lexical readonly loans can share this physical slot. Bind
                // its actual incoming origin and protect the bytes, rather
                // than requiring one semantic name per machine slot.
                if pointer.checked_add(8).is_none_or(|end| end > machine.frame.saturating_sub(16))
                    || machine.memory.get(&(pointer - machine.frame)) != Some(&Origin::Incoming(word))
                {
                    return Err(invalid(format!(
                        "arena pointer spill differs from its typed incoming argument in {} at slot {pointer}",
                        typed.name
                    )));
                }
                let length_word = word.checked_add(1).ok_or_else(|| invalid("arena extent argument overflows"))?;
                let lengths = machine
                    .memory
                    .iter()
                    .filter_map(|(slot, origin)| (*origin == Origin::Incoming(length_word)).then_some(*slot + machine.frame))
                    .collect::<Vec<_>>();
                let [length] = lengths.as_slice() else {
                    return Err(invalid("arena helper needs one unambiguous incoming extent spill"));
                };
                if *length < scalar_end || *length + 8 > machine.frame.saturating_sub(16) {
                    return Err(invalid("arena extent spill overlaps scalars or frame linkage"));
                }
                slots.extend([(pointer, pointer + 8), (*length, *length + 8)]);
                spans.insert(word, local.source_id);
            }
            word = word.checked_add(width).ok_or_else(|| invalid("arena helper argument expansion overflows"))?;
        }
        let mut seed = seeds.remove(&typed.id).unwrap_or_else(|| lifetime::Seed {
            machine: machine.clone(),
            roots: BTreeSet::new(),
            spans: BTreeMap::new(),
            view_slots: Vec::new(),
            allocated_slots: Vec::new(),
            allocated_spans: BTreeMap::new(),
        });
        if slots.iter().any(|&(start, end)| seed.view_slots.iter().any(|&(a, b)| start < b && a < end)) {
            return Err(invalid("arena parameter aliases a transaction coordinate"));
        }
        seed.machine = machine;
        seed.allocated_slots = slots;
        seed.allocated_spans = spans;
        seeds.insert(typed.id.clone(), seed);
    }
    Ok(())
}
