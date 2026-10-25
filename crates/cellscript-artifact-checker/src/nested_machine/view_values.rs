//! Typed coordinate constructors and aliases. Marker names locate candidates;
//! decoded instructions bind source families, index operands and destination
//! slots. The enclosing CFG proof tracks their availability at reads and calls.
use super::*;
use crate::cell_selection::machine::{cursor::Cursor, flow};
use crate::{TypedSemanticConstant, TypedSemanticEntry, TypedSemanticOperation};

pub(super) fn is_coordinate(ty: &str) -> bool {
    ["InputView<", "OutputView<", "CellDataView<"].iter().any(|prefix| ty.starts_with(prefix)) && ty.ends_with('>')
}

pub(super) struct ValueSite {
    pub end: u64,
    pub source: Option<i64>,
    pub destination: i64,
    pub call: bool,
}

pub(super) type Checked = BTreeMap<String, BTreeMap<u64, ValueSite>>;

fn slot(entry: &TypedSemanticEntry, local: u32, frame: u32) -> Result<u32, CheckerError> {
    entry
        .locals
        .iter()
        .find(|value| value.id == local)
        .and_then(|value| value.source_id.checked_mul(8))
        .and_then(|offset| u32::try_from(offset).ok())
        .filter(|offset| offset.checked_add(8).is_some_and(|end| end <= frame.saturating_sub(16)))
        .ok_or_else(|| invalid("Cell view value slot exceeds its private frame"))
}

fn value_operation(entry: &TypedSemanticEntry, op: &TypedSemanticOperation) -> bool {
    matches!(op.opcode.as_str(), "call" | "move")
        && op
            .destinations
            .first()
            .and_then(|id| entry.locals.iter().find(|local| local.id == *id))
            .is_some_and(|local| is_coordinate(&local.ty))
}

fn family(name: &str) -> Option<(u64, &'static str)> {
    Some(match name {
        "__ckb_source_input" => (1, "InputView<"),
        "__ckb_source_output" => (2, "OutputView<"),
        "__ckb_source_group_input" => (5, "InputView<"),
        "__ckb_source_group_output" => (6, "OutputView<"),
        _ => return None,
    })
}

fn helper(record: &VerifiedLoweringRecord, elf: &ParsedElf, name: &str, tag: u64, abort: u64) -> Result<u64, CheckerError> {
    let owner = format!("runtime:{name}");
    let lowering =
        record.entries.iter().find(|entry| entry.id == owner).ok_or_else(|| invalid("Cell view constructor has no runtime helper"))?;
    let start = record
        .blocks
        .iter()
        .find(|block| block.id == lowering.entry_block)
        .ok_or_else(|| invalid("Cell view helper has no entry"))?
        .range
        .start;
    let end = record
        .blocks
        .iter()
        .filter(|block| block.owner_entry == owner)
        .map(|block| block.range.end)
        .max()
        .ok_or_else(|| invalid("Cell view helper has no extent"))?;
    if end - start > 128
        || elf.control_flow.iter().any(|edge| start < edge.target && edge.target < end && !(start..end).contains(&edge.address))
    {
        return Err(invalid("Cell view constructor has an excessive extent or partial entry"));
    }
    let words = elf.instructions.iter().map(|i| (i.address, i.word)).collect();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect();
    let mut c = Cursor::new(start, end, &words, &targets);
    if end - start == 8 {
        // A target that disables CKB runtime support must terminate, never
        // return an unvalidated coordinate through a success-shaped stub.
        c.constant(10, 3)?;
    } else {
        let bad = c.label();
        c.immediate(0x13, 5, 6, 10, 32)?;
        c.branch(1, 6, 0, bad)?;
        c.constant(5, tag)?;
        c.immediate(0x13, 1, 5, 5, 32)?;
        c.add(10, 10, 5)?;
        c.exact(0x00008067)?;
        c.bind(bad)?;
        c.constant(10, 44)?;
    }
    if c.jump_target()? != abort {
        return Err(invalid("Cell view constructor failure does not terminate"));
    }
    c.finish()?;
    Ok(start)
}

pub(super) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    abort: Option<u64>,
    selections: &crate::cell_selection::machine::Checked,
) -> Result<Checked, CheckerError> {
    let present = record
        .typed_semantics
        .entries
        .iter()
        .any(|entry| entry.blocks.iter().flat_map(|block| &block.operations).any(|op| value_operation(entry, op)));
    if !present && record.blocks.iter().all(|block| block.cell_view_value_labels.is_empty()) {
        return Ok(BTreeMap::new());
    }
    let abort = abort.ok_or_else(|| invalid("Cell view values have no checked fatal sink"))?;
    let words = elf.instructions.iter().map(|i| (i.address, i.word)).collect();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect();
    let mut markers = BTreeMap::new();
    for block in &record.blocks {
        for label in &block.cell_view_value_labels {
            let suffix = label.strip_prefix(".Lcell_view_value_").ok_or_else(|| invalid("unknown Cell view value marker"))?;
            let (suffix, done) = suffix.strip_suffix("_done").map_or((suffix, false), |suffix| (suffix, true));
            let parts = suffix.split('_').collect::<Vec<_>>();
            if parts.len() != 3 || parts[2].parse::<u64>().is_err() {
                return Err(invalid("invalid Cell view value marker"));
            }
            let block_id = parts[0].parse::<u32>().map_err(|_| invalid("invalid Cell view block"))?;
            let op = parts[1].parse::<u32>().map_err(|_| invalid("invalid Cell view operation"))?;
            if markers.insert((block.owner_entry.as_str(), block_id, op, done), block.range.start).is_some() {
                return Err(invalid("duplicate Cell view value boundary"));
            }
        }
    }
    let mut checked = BTreeSet::new();
    let mut helpers = BTreeMap::new();
    let mut sites = Checked::new();
    let mut flow_sites = flow::Markers::new();
    for entry in &record.typed_semantics.entries {
        if !entry.blocks.iter().flat_map(|block| &block.operations).any(|op| value_operation(entry, op)) {
            continue;
        }
        let lowering = record
            .entries
            .iter()
            .find(|lowering| lowering.id == entry.id)
            .ok_or_else(|| invalid("Cell view owner has no machine entry"))?;
        let frame = owned_frame(record, elf, lowering)?;
        for block in &entry.blocks {
            for op in block.operations.iter().filter(|op| value_operation(entry, op)) {
                let key = (entry.id.as_str(), block.id, op.index);
                let begin = (key.0, key.1, key.2, false);
                let done = (key.0, key.1, key.2, true);
                let start = *markers.get(&begin).ok_or_else(|| invalid("Cell view value lacks a machine start"))?;
                let end = *markers.get(&done).ok_or_else(|| invalid("Cell view value lacks a machine end"))?;
                if start >= end || end - start > 256 {
                    return Err(invalid("invalid Cell view value extent"));
                }
                let destination = slot(entry, op.destinations[0], frame)?;
                let ty = &entry
                    .locals
                    .iter()
                    .find(|local| local.id == op.destinations[0])
                    .ok_or_else(|| invalid("Cell view destination missing"))?
                    .ty;
                if op.destinations.len() != 1 || op.operands.len() != 1 {
                    return Err(invalid("Cell view value has an invalid signature"));
                }
                let operand = &op.operands[0];
                let mut c = Cursor::new(start, end, &words, &targets);
                let source = if op.opcode == "move" {
                    if operand.ty != *ty {
                        return Err(invalid("Cell view alias changes its source family or schema"));
                    }
                    let source = slot(entry, operand.local.ok_or_else(|| invalid("Cell view alias has no local root"))?, frame)?;
                    c.stack_load(5, source)?;
                    c.stack_store(5, destination)?;
                    Some(i64::from(source))
                } else {
                    let call = op.call.as_ref().ok_or_else(|| invalid("Cell view constructor has no call contract"))?;
                    let (tag, prefix) = family(&call.target).ok_or_else(|| invalid("Cell view value has an unproved producer"))?;
                    if !ty.starts_with(prefix)
                        || operand.ty != "u64"
                        || call.params != ["u64"]
                        || call.return_type != *ty
                        || call.contract != "versioned-runtime-helper"
                    {
                        return Err(invalid("Cell view constructor changes its index, source family or nominal type"));
                    }
                    if let Some(local) = operand.local {
                        c.stack_load(10, slot(entry, local, frame)?)?;
                    } else if let Some(TypedSemanticConstant::U64(value)) = &operand.constant {
                        c.constant(10, value.parse().map_err(|_| invalid("invalid Cell view index"))?)?;
                    } else {
                        return Err(invalid("Cell view index has no typed unsigned origin"));
                    }
                    let target = if let Some(target) = helpers.get(&call.target) {
                        *target
                    } else {
                        let target = helper(record, elf, &call.target, tag, abort)?;
                        helpers.insert(call.target.clone(), target);
                        target
                    };
                    c.call(target)?;
                    c.stack_store(10, destination)?;
                    None
                };
                c.finish()?;
                if elf.control_flow.iter().any(|edge| start < edge.target && edge.target < end) {
                    return Err(invalid("control flow enters a partial Cell view value"));
                }
                checked.insert(begin);
                checked.insert(done);
                flow_sites.insert(key, (start, end, destination));
                sites
                    .entry(entry.id.clone())
                    .or_default()
                    .insert(start, ValueSite { end, source, destination: i64::from(destination), call: op.opcode == "call" });
            }
        }
    }
    if checked.len() != markers.len() {
        return Err(invalid("Cell view machine value has no typed operation"));
    }
    // Compose complete checked data/selection bodies, including their terminal
    // failures. Walking an unrelated syscall from metadata alone cannot prove
    // whether that body returns to the next coordinate constructor.
    for ((owner, block, op), range) in &selections.flow_sites {
        if flow_sites.insert((owner.as_str(), *block, *op), *range).is_some() {
            return Err(invalid("Cell view value overlaps another typed machine obligation"));
        }
    }
    flow::validate(record, elf, &flow_sites, abort).map_err(|error| invalid(format!("Cell view value flow: {}", error.message)))?;
    Ok(sites)
}
