//! Independent bounded-role instruction validation. This recognizes every word
//! and branch inside a typed verification, including terminal failures. Full
//! source equivalence, enclosing memory effects and builder admission remain
//! separate obligations; the experimental blocker is retained.
use super::layout::{self, Layout};
use super::{Contract, Correspondence, Role};
use crate::{CheckerError, CheckerRejectionCode, ParsedElf, TypedSemanticOperationDetail, VerifiedLoweringRecord};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) mod cursor;
pub(crate) mod data;
pub(crate) mod flow;
mod program;
mod reads;

const PREFIX: &str = ".Lexact_role_check_";
const USE_PREFIX: &str = ".Lexact_role_use_";
const DATA_PREFIX: &str = ".Lexact_role_data_";
const FRAME_BYTES: u32 = layout::FIXED_FRAME_BYTES;

fn invalid(message: impl Into<String>) -> CheckerError {
    CheckerError::new(
        CheckerRejectionCode::V2420TypedMachineBindingInvalid,
        format!("exact-role machine contract: {}", message.into()),
    )
}

/// Effects are produced only after recognizing the complete instruction body
/// and its CFG obligations. Addresses use the shared relaxed-branch logical view.
pub(crate) struct Effect {
    pub end: u64,
    pub offset: u32,
    pub bytes: u32,
    /// The optional scalar pointer materialization is part of the checked body.
    pub destination: Option<u32>,
}

/// Only independently recognized reads and aliases may consume a retained table.
pub(crate) struct Use {
    pub end: u64,
    pub source: u32,
    pub destination: u32,
    pub alias: bool,
    pub coordinate: bool,
}

#[derive(Default)]
pub(crate) struct Checked {
    pub sites: BTreeMap<String, BTreeMap<u64, Effect>>,
    pub uses: BTreeMap<String, BTreeMap<u64, Use>>,
    pub data: BTreeMap<String, BTreeMap<u64, data::Effect>>,
    /// Complete independently recognized bodies for composition with other
    /// typed operations in the same block; never populated from markers alone.
    pub flow_sites: BTreeMap<(String, u32, u32), (u64, u64, u32)>,
}

pub(crate) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    terminal_sink: Option<&str>,
) -> Result<Checked, CheckerError> {
    let present = record
        .typed_semantics
        .entries
        .iter()
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .any(|op| matches!(op.opcode.as_str(), "verify-exact-cell-roles" | "read-exact-cell-role"));
    let data_present =
        record.typed_semantics.entries.iter().flat_map(|entry| &entry.locals).any(|local| local.ty.starts_with("CellDataView<"));
    if !present && !data_present && record.blocks.iter().all(|block| block.exact_role_labels.is_empty()) {
        return Ok(Checked::default());
    }
    let normalized = crate::policy_machine::normalize_relaxed_branches(record, elf).map_err(invalid)?;
    let (record, elf) = normalized.as_ref().map_or((record, elf), |(record, elf)| (record, elf));
    let abort = record
        .blocks
        .iter()
        .find(|block| Some(block.id.as_str()) == terminal_sink)
        .map(|block| block.range.start)
        .ok_or_else(|| invalid("missing independently checked fatal sink"))?;
    let words = elf.instructions.iter().map(|i| (i.address, i.word)).collect::<BTreeMap<_, _>>();
    let targets = elf.control_flow.iter().map(|flow| (flow.address, flow.target)).collect::<BTreeMap<_, _>>();
    let blocks = record.blocks.iter().map(|block| (block.range.start, block)).collect::<BTreeMap<_, _>>();
    let mut labels = BTreeMap::new();
    let mut typed_owners = BTreeMap::new();
    let mut current = BTreeMap::new();
    for (address, block) in &blocks {
        if let Some(id) = block.lowering_block_id {
            current.insert(block.owner_entry.as_str(), id);
        }
        typed_owners.insert(*address, (block.owner_entry.as_str(), current.get(block.owner_entry.as_str()).copied()));
        for label in &block.exact_role_labels {
            if labels.insert((block.owner_entry.as_str(), label.as_str()), block.range.start).is_some() {
                return Err(invalid("duplicate verification marker"));
            }
        }
    }
    let mut markers = BTreeMap::new();
    let mut use_keys = BTreeSet::new();
    let mut data_keys = BTreeSet::new();
    let mut data_storage = BTreeMap::new();
    let mut ranges = Vec::new();
    for ((owner, label), start) in &labels {
        if label.ends_with("_done") {
            continue;
        }
        let parts = label
            .strip_prefix(PREFIX)
            .or_else(|| label.strip_prefix(USE_PREFIX))
            .or_else(|| label.strip_prefix(DATA_PREFIX))
            .ok_or_else(|| invalid("invalid marker prefix"))?
            .split('_')
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| invalid("invalid marker coordinates"))?;
        let (block, operation, offset) = match parts.as_slice() {
            [block, operation, offset, _ordinal] => (*block, *operation, *offset),
            [block, operation, offset, size, length, _ordinal] if label.starts_with(DATA_PREFIX) => {
                data_storage.insert((*owner, *block, *operation), data::Storage { size: *size, length: *length });
                (*block, *operation, *offset)
            }
            _ => return Err(invalid("invalid marker shape")),
        };
        let end = *labels.get(&(*owner, format!("{label}_done").as_str())).ok_or_else(|| invalid("missing completion marker"))?;
        if end <= *start || markers.insert((*owner, block, operation), (*start, end, offset)).is_some() {
            return Err(invalid("duplicate typed operation or invalid verification range"));
        }
        if typed_owners.get(start) != Some(&(*owner, Some(block))) {
            return Err(invalid("verification marker does not belong to its typed block"));
        }
        if label.starts_with(USE_PREFIX) {
            if offset != 0 {
                return Err(invalid("read markers cannot supply a frame offset"));
            }
            use_keys.insert((*owner, block, operation));
        }
        if label.starts_with(DATA_PREFIX) {
            data_keys.insert((*owner, block, operation));
        }
        ranges.push((*start, end));
    }
    if labels.len() != markers.len() * 2 {
        return Err(invalid("orphan completion marker"));
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(invalid("overlapping verification ranges"));
    }
    let mut checked = BTreeSet::new();
    let mut regions = BTreeMap::<&str, Vec<(u32, u32, bool)>>::new();
    let mut frame_sizes = BTreeMap::new();
    let mut destinations = BTreeMap::new();
    let mut uses = BTreeMap::new();
    let mut data_effects = BTreeMap::new();
    for entry in &record.typed_semantics.entries {
        for block in &entry.blocks {
            for operation in &block.operations {
                let key = (entry.id.as_str(), block.id, operation.index);
                if data::is_operation(operation) {
                    let &(start, end, output) =
                        markers.get(&key).ok_or_else(|| invalid("typed Cell data read has no machine body"))?;
                    if !data_keys.contains(&key) {
                        return Err(invalid("Cell data read has the wrong marker"));
                    }
                    let effect = data::validate(
                        record,
                        elf,
                        entry,
                        operation,
                        cursor::Cursor::new(start, end, &words, &targets),
                        output,
                        abort,
                        data_storage.get(&key).copied(),
                    )?;
                    data_effects.entry(entry.id.clone()).or_insert_with(BTreeMap::new).insert(start, effect);
                    checked.insert(key);
                    continue;
                }
                let handle_move = matches!(operation.opcode.as_str(), "move" | "field-access")
                    && operation.operands.first().is_some_and(|operand| operand.ty.starts_with("ExactCellSelection<"));
                if operation.opcode == "read-exact-cell-role" || handle_move {
                    let &(start, end, _) = markers.get(&key).ok_or_else(|| invalid("typed selection use has no machine body"))?;
                    if !use_keys.contains(&key) {
                        return Err(invalid("selection use has a verification marker"));
                    }
                    cursor::Cursor::new(start, end, &words, &targets).selected_use(entry, operation, abort)?;
                    let source = operation.operands[0].local.ok_or_else(|| invalid("selection use has no local source"))?;
                    uses.entry(entry.id.clone()).or_insert_with(BTreeMap::new).insert(
                        start,
                        Use {
                            end,
                            source: reads::slot(entry, source)?,
                            destination: reads::slot(entry, operation.destinations[0])?,
                            alias: handle_move,
                            coordinate: matches!(&operation.detail, TypedSemanticOperationDetail::ExactRoleRead { read, .. } if read.indexed()),
                        },
                    );
                    checked.insert(key);
                    continue;
                }
                if operation.opcode != "verify-exact-cell-roles" {
                    continue;
                }
                let TypedSemanticOperationDetail::ExactRoleSelection { contract } = &operation.detail else {
                    return Err(invalid("missing typed role contract"));
                };
                contract.validate().map_err(invalid)?;
                let types = record.typed_semantics.types.iter().map(|ty| (ty.name.as_str(), ty)).collect();
                let layouts = layout::derive(contract, &types).map_err(invalid)?;
                let bytes = layout::frame_bytes(&layouts);
                frame_sizes.insert(key, bytes);
                let &(start, end, offset) = markers.get(&key).ok_or_else(|| invalid("typed verification has no machine body"))?;
                if use_keys.contains(&key) {
                    return Err(invalid("selection verification has a read marker"));
                }
                checked.insert(key);
                let lowering =
                    record.entries.iter().find(|lowering| lowering.id == entry.id).ok_or_else(|| invalid("missing owning entry"))?;
                let prologue = record
                    .blocks
                    .iter()
                    .find(|block| block.id == lowering.entry_block)
                    .ok_or_else(|| invalid("missing allocating prologue"))?;
                let owned = elf
                    .stack_adjustments
                    .iter()
                    .find(|adjustment| prologue.range.contains(adjustment.address))
                    .filter(|adjustment| adjustment.delta < 0)
                    .and_then(|adjustment| u32::try_from(adjustment.delta.unsigned_abs()).ok())
                    .ok_or_else(|| invalid("verification has no owned frame"))?;
                let scalar_end = entry
                    .locals
                    .iter()
                    .map(|local| local.source_id.checked_add(1).and_then(|id| id.checked_mul(8)))
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| invalid("scalar frame overflow"))?
                    .into_iter()
                    .max()
                    .unwrap_or(0);
                if !offset.is_multiple_of(16)
                    || u64::from(offset) < scalar_end
                    || owned > lowering.frame_size_bytes
                    || offset.checked_add(bytes).is_none_or(|end| end > owned.saturating_sub(16))
                {
                    return Err(invalid("selection scratch overlaps scalar locals, saved registers or the frame boundary"));
                }
                let persistent = !operation.destinations.is_empty();
                let prior = regions.entry(entry.id.as_str()).or_default();
                if prior
                    .iter()
                    .any(|&(other, length, retained)| (persistent || retained) && offset < other + length && other < offset + bytes)
                {
                    return Err(invalid("a selection handle's retained table overlaps another selection"));
                }
                prior.push((offset, bytes, persistent));
                let mut machine = cursor::Cursor::new(start, end, &words, &targets);
                machine.selection(contract, &layouts, offset, abort, record)?;
                if let Some(destination) = operation.destinations.first() {
                    destinations.insert(key, reads::slot(entry, *destination)?);
                    machine.stack_address(5, offset)?;
                    machine.stack_store(5, reads::slot(entry, *destination)?)?;
                }
                machine.finish()?;
            }
        }
    }
    for (&(owner, _, _), &(start, end, _)) in &markers {
        for (address, _) in words.range(start..end) {
            if !blocks
                .range(..=*address)
                .next_back()
                .is_some_and(|(_, block)| block.owner_entry == owner && block.range.contains(*address))
            {
                return Err(invalid("verification crosses its owning entry"));
            }
        }
        if elf.control_flow.iter().any(|flow| start < flow.target && flow.target < end && !(start..end).contains(&flow.address)) {
            return Err(invalid("external control flow enters a partial verification"));
        }
    }
    if checked.len() != markers.len() {
        return Err(invalid("machine verification lacks a typed obligation"));
    }
    data::validate_result_storage(&data_effects)?;
    flow::validate(record, elf, &markers, abort)?;
    let mut effects = Checked { uses, data: data_effects, ..Default::default() };
    for (key @ (owner, _, _), (start, end, offset)) in markers {
        effects.flow_sites.insert((owner.to_string(), key.1, key.2), (start, end, offset));
        if use_keys.contains(&key) || data_keys.contains(&key) {
            continue;
        }
        effects
            .sites
            .entry(owner.to_owned())
            .or_default()
            .insert(start, Effect { end, offset, bytes: frame_sizes[&key], destination: destinations.get(&key).copied() });
    }
    Ok(effects)
}
