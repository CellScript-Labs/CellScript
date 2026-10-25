//! Exact typed Cell load and complete-table codec, with checked summaries for
//! the separate CFG lifetime pass. This does not grant Cell authority.
use super::*;
use crate::checker::{canonical_abi_type, nested_value_contract};
use crate::{CellBindingRole, CellBindingSource, TypedSemanticType};

const PREFIX: &str = ".Lnested_cell_codec_";

#[derive(Clone, Copy)]
pub(super) struct Buffer {
    pub size: u32,
    pub start: u32,
    pub end: u32,
}

pub(super) type Buffers = BTreeMap<(String, u64), Buffer>;

pub(super) struct Load {
    pub end: u64,
    pub source: u64,
    pub buffer: Buffer,
    pub initializes_pointer: bool,
}

pub(super) struct Checked {
    pub buffers: Buffers,
    pub loads: BTreeMap<String, BTreeMap<u64, Load>>,
}

pub(crate) enum Field {
    Fixed(u32),
    Values(NestedValueContract),
}

pub(crate) fn layout(ty: &TypedSemanticType, types: &BTreeMap<&str, &TypedSemanticType>) -> Result<(Vec<Field>, u32), CheckerError> {
    if !matches!(ty.kind.as_str(), "resource" | "shared" | "receipt") {
        return Err(invalid("nested Cell codec has no Cell type"));
    }
    let mut capacity = u32::try_from(ty.fields.len())
        .ok()
        .and_then(|count| count.checked_add(1))
        .and_then(|count| count.checked_mul(4))
        .ok_or_else(|| invalid("Cell header overflows"))?;
    let mut fields = Vec::new();
    let mut nested = false;
    let mut ordered = ty.fields.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|field| field.molecule_field_index);
    for (index, field) in ordered.into_iter().enumerate() {
        if field.molecule_field_index != Some(index as u32) {
            return Err(invalid("Cell codec has no unique declared field order"));
        }
        let (field, width) = if let Some(contract) = nested_value_contract(&field.ty, types) {
            nested = true;
            let width = contract.max_total_bytes;
            (Field::Values(contract), width)
        } else {
            let ty = canonical_abi_type(&field.ty);
            let width = match ty.as_str() {
                "u8" => 1,
                "u16" => 2,
                "u32" => 4,
                "u64" => 8,
                "u128" => 16,
                "hash" | "address" => 32,
                _ => ty
                    .strip_prefix("[u8;")
                    .and_then(|ty| ty.strip_suffix(']'))
                    .and_then(|n| n.parse::<u32>().ok())
                    .filter(|width| (1..=256).contains(width))
                    .ok_or_else(|| invalid("unsupported nested Cell field"))?,
            };
            (Field::Fixed(width), width)
        };
        capacity = capacity.checked_add(width).ok_or_else(|| invalid("Cell capacity overflows"))?;
        fields.push(field);
    }
    if !nested {
        return Err(invalid("Cell codec annotation has no nested field"));
    }
    Ok((fields, capacity.max(512)))
}

pub(super) fn validate(record: &VerifiedLoweringRecord, elf: &ParsedElf) -> Result<Checked, CheckerError> {
    let words = elf.instructions.iter().map(|i| (i.address, i.word)).collect::<BTreeMap<_, _>>();
    let targets = elf.control_flow.iter().map(|flow| (flow.address, flow.target)).collect();
    let types = record.typed_semantics.types.iter().map(|ty| (ty.name.as_str(), ty)).collect::<BTreeMap<_, _>>();
    let mut labels = BTreeMap::new();
    for block in &record.blocks {
        for label in &block.nested_cell_labels {
            if labels.insert((block.owner_entry.as_str(), label.as_str()), block.range.start).is_some() {
                return Err(invalid("duplicate Cell codec marker"));
            }
        }
    }
    // Partition the declared byte ranges before decoding. Each instruction is
    // visited by at most one Cell codec, even with malicious co-located labels.
    let mut ranges = Vec::new();
    for ((owner, label), start) in &labels {
        if label.ends_with("_done") || label.ends_with("_validate") {
            continue;
        }
        let done = *labels.get(&(*owner, format!("{label}_done").as_str())).ok_or_else(|| invalid("missing Cell codec completion"))?;
        if done <= *start {
            return Err(invalid("invalid Cell codec extent"));
        }
        ranges.push((*start, done.checked_add(8).ok_or_else(|| invalid("Cell status range overflows"))?));
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(invalid("overlapping Cell codec ranges"));
    }
    let mut incoming = BTreeMap::<u64, Vec<u64>>::new();
    for flow in &elf.control_flow {
        incoming.entry(flow.target).or_default().push(flow.address);
    }
    let owners = record.blocks.iter().map(|block| (block.range.start, block.owner_entry.as_str())).collect::<BTreeMap<_, _>>();
    let mut checked = BTreeSet::new();
    let mut covered = BTreeSet::new();
    let mut buffers = BTreeMap::new();
    let mut loads = BTreeMap::<String, BTreeMap<u64, Load>>::new();
    for ((owner, label), start) in &labels {
        if label.ends_with("_done") || label.ends_with("_validate") {
            continue;
        }
        let fields = label
            .strip_prefix(PREFIX)
            .ok_or_else(|| invalid("invalid Cell codec marker"))?
            .split('_')
            .map(|value| value.parse::<u32>().map_err(|_| invalid("invalid Cell codec coordinates")))
            .collect::<Result<Vec<_>, _>>()?;
        let [source, size, buffer, _ordinal] = fields.as_slice() else { return Err(invalid("invalid Cell codec coordinates")) };
        let entry = record
            .typed_semantics
            .entries
            .iter()
            .find(|entry| entry.id == *owner)
            .ok_or_else(|| invalid("missing Cell codec owner"))?;
        let local = entry
            .locals
            .iter()
            .find(|local| local.source_id == u64::from(*source))
            .ok_or_else(|| invalid("Cell codec has no typed local"))?;
        let binding = entry
            .cell_bindings
            .iter()
            .find(|binding| binding.local_id == Some(local.id))
            .ok_or_else(|| invalid(format!("Cell codec {owner}:{source} ({}) has no typed binding", local.ty)))?;
        let ty = types.get(binding.ty.as_str()).ok_or_else(|| invalid("Cell codec has no declared layout"))?;
        let (fields, capacity) = layout(ty, &types)?;
        let lowering = record.entries.iter().find(|entry| entry.id == *owner).ok_or_else(|| invalid("missing Cell frame"))?;
        let frame = owned_frame(record, elf, lowering)?;
        let scalar_end = entry
            .locals
            .iter()
            .map(|local| local.source_id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .and_then(|count| count.checked_mul(8))
            .ok_or_else(|| invalid("Cell scalar frame overflows"))?;
        let end = buffer.checked_add(capacity).ok_or_else(|| invalid("Cell buffer overflows"))?;
        if u64::from(*buffer) < scalar_end
            || u64::from(*size) < scalar_end
            || end > frame.saturating_sub(16)
            || size % 8 != 0
            || size.checked_add(8).is_none_or(|end| end > frame.saturating_sub(16))
            || (*size < end && *size + 8 > *buffer)
        {
            return Err(invalid("Cell codec storage overlaps its frame or size slot"));
        }
        if let Some(previous) = buffers.insert((*owner, *source), (*size, *buffer, end))
            && previous != (*size, *buffer, end)
        {
            return Err(invalid("Cell reload changed its owned buffer"));
        }
        let validate_label = format!("{label}_validate");
        let done_label = format!("{label}_done");
        let validation = *labels.get(&(*owner, validate_label.as_str())).ok_or_else(|| invalid("missing Cell validation boundary"))?;
        let done = *labels.get(&(*owner, done_label.as_str())).ok_or_else(|| invalid("missing Cell codec completion"))?;
        checked.extend(
            [(*owner, *label), (*owner, validate_label.as_str()), (*owner, done_label.as_str())]
                .map(|(owner, label)| (owner.to_string(), label.to_string())),
        );
        covered.insert((*owner, local.id));
        let codec_words = words.range(*start..done).map(|(address, word)| (*address, *word)).collect();
        let mut c = Decoder { pc: *start, words: &codec_words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
        constant(&mut c, 5, u64::from(capacity))?;
        uses::expect_stack(&mut c, 5, *size, true, frame)?;
        pointer(&mut c, 10, *buffer)?;
        pointer(&mut c, 11, *size)?;
        constant(&mut c, 12, 0)?;
        constant(&mut c, 13, u64::from(binding.ordinal))?;
        let domain = match binding.source {
            CellBindingSource::Input => 1,
            CellBindingSource::Output => 2,
            CellBindingSource::CellDep => 3,
            CellBindingSource::GroupInput => (1 << 56) | 1,
            CellBindingSource::GroupOutput => (1 << 56) | 2,
        };
        constant(&mut c, 14, domain)?;
        constant(&mut c, 17, 2092)?;
        c.exact(0x73)?;
        if c.pc != validation {
            return Err(invalid("Cell decoder is detached from its load"));
        }
        let actual_done = check_table(&mut c, record, &fields, capacity, *size, *buffer, frame)?;
        if actual_done != done {
            return Err(invalid("Cell load failure continuation changed"));
        }
        // A failed load preserves a0 above, and must now enter the caller's
        // checked syscall-failure exit rather than expose an uninitialized view.
        let mut continuation = Decoder { pc: done, words: &words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
        let ready = branch(&mut continuation, 0, 10, 0)?;
        failure(&mut continuation, record, 1)?;
        if continuation.pc != ready {
            return Err(invalid("Cell load status check has an unchecked continuation"));
        }
        if binding.role != CellBindingRole::Output {
            pointer(&mut continuation, 5, *buffer)?;
            uses::expect_stack(
                &mut continuation,
                5,
                source.checked_mul(8).ok_or_else(|| invalid("Cell pointer slot overflows"))?,
                true,
                frame,
            )?;
        }
        let complete = continuation.pc;
        if incoming.range(start + 4..complete).any(|(_, sources)| sources.iter().any(|source| *source < *start || *source >= complete))
        {
            return Err(invalid("external control flow bypasses Cell load or decoding"));
        }
        if owners.range(*start..complete).any(|(_, actual_owner)| actual_owner != owner) {
            return Err(invalid("Cell codec crosses an entry boundary"));
        }
        loads.entry(owner.to_string()).or_default().insert(
            *start,
            Load {
                end: complete,
                source: u64::from(*source),
                buffer: Buffer { size: *size, start: *buffer, end },
                initializes_pointer: binding.role != CellBindingRole::Output,
            },
        );
    }
    if checked.len() != labels.len() {
        return Err(invalid("unbound Cell codec annotation"));
    }
    for entry in &record.typed_semantics.entries {
        for binding in &entry.cell_bindings {
            if types.get(binding.ty.as_str()).is_some_and(|ty| ty.fields.iter().any(|field| field.ty.starts_with("NestedValues<")))
                && binding.local_id.is_some_and(|local| !covered.contains(&(entry.id.as_str(), local)))
            {
                return Err(invalid("typed nested Cell binding lacks a complete load codec"));
            }
        }
    }
    let mut ranges = Vec::with_capacity(buffers.len() * 2);
    for ((owner, _), (size, start, end)) in &buffers {
        ranges.extend([(*owner, *size, size + 8), (*owner, *start, *end)]);
    }
    ranges.sort_unstable();
    for pair in ranges.windows(2) {
        if pair[0].0 == pair[1].0 && pair[0].2 > pair[1].1 {
            return Err(invalid("typed Cell buffers or lengths alias"));
        }
    }
    let buffers = buffers
        .into_iter()
        .map(|((owner, source), (size, start, end))| ((owner.to_string(), u64::from(source)), Buffer { size, start, end }))
        .collect();
    Ok(Checked { buffers, loads })
}

pub(super) fn pointer(c: &mut Decoder<'_>, register: u32, offset: u32) -> Result<(), CheckerError> {
    if offset <= 2047 {
        c.addi(register, 2, offset as i32)
    } else {
        constant(c, register, u64::from(offset))?;
        c.add(register, 2, register)
    }
}

fn constant(c: &mut Decoder<'_>, register: u32, expected: u64) -> Result<(), CheckerError> {
    c.constant(register, expected)
}

pub(super) fn branch(c: &mut Decoder<'_>, kind: u32, left: u32, right: u32) -> Result<u64, CheckerError> {
    let pc = c.pc;
    let word = c.word()?;
    if word & 0x7f != 0x63 || word >> 12 & 7 != kind || word >> 15 & 31 != left || word >> 20 & 31 != right {
        return Err(invalid(format!("Cell condition changed at {pc:#x}")));
    }
    c.targets.get(&pc).copied().ok_or_else(|| invalid("missing Cell branch target"))
}

fn expect_branch(c: &mut Decoder<'_>, kind: u32, left: u32, right: u32, target: u64) -> Result<(), CheckerError> {
    if branch(c, kind, left, right)? != target {
        return Err(invalid("Cell branch target changed"));
    }
    Ok(())
}

pub(super) fn failure(c: &mut Decoder<'_>, record: &VerifiedLoweringRecord, code: i32) -> Result<(), CheckerError> {
    let target = c.jump()?;
    if !record.verifier_failure_exits.iter().any(|exit| exit.address == target && exit.code == code) {
        return Err(invalid("Cell codec lost its checked fatal exit"));
    }
    Ok(())
}

// The table schema and owned memory coordinates are separate proof inputs.
#[allow(clippy::too_many_arguments)]
fn check_table(
    c: &mut Decoder<'_>,
    record: &VerifiedLoweringRecord,
    fields: &[Field],
    capacity: u32,
    size: u32,
    buffer: u32,
    frame: u32,
) -> Result<u64, CheckerError> {
    let done = branch(c, 1, 10, 0)?;
    uses::expect_stack(c, 5, size, false, frame)?;
    constant(c, 6, u64::from(capacity))?;
    let invalid_target = branch(c, 6, 6, 5)?;
    constant(c, 6, 4 * (fields.len() as u64 + 1))?;
    expect_branch(c, 6, 5, 6, invalid_target)?;
    pointer(c, 10, buffer)?;
    c.u32_le(5, 10, 4, 7)?;
    expect_branch(c, 1, 5, 6, invalid_target)?;
    c.u32_le(5, 10, 0, 7)?;
    uses::expect_stack(c, 6, size, false, frame)?;
    let bounds_target = branch(c, 1, 5, 6)?;
    for (index, field) in fields.iter().enumerate() {
        pointer(c, 29, buffer)?;
        uses::expect_stack(c, 11, size, false, frame)?;
        c.u32_le(30, 29, 4 * (index as i32 + 1), 7)?;
        if index + 1 < fields.len() {
            c.u32_le(31, 29, 4 * (index as i32 + 2), 7)?;
        } else {
            c.addi(31, 11, 0)?;
        }
        expect_branch(c, 6, 31, 30, bounds_target)?;
        expect_branch(c, 6, 11, 31, bounds_target)?;
        c.sub(11, 31, 30)?;
        match field {
            Field::Fixed(width) => {
                constant(c, 5, u64::from(*width))?;
                expect_branch(c, 1, 11, 5, invalid_target)?;
            }
            Field::Values(contract) => {
                c.add(10, 29, 30)?;
                c.labels.clear();
                c.branches.clear();
                c.body(contract)?;
                c.labels.insert(Label::Invalid, invalid_target);
                if c.branches.iter().any(|(label, target)| c.labels.get(label) != Some(target)) {
                    return Err(invalid("Cell nested decoder branch or loop changed"));
                }
            }
        }
    }
    c.li(10, 0)?;
    if c.jump()? != done || c.pc != invalid_target {
        return Err(invalid("Cell codec success continuation changed"));
    }
    failure(c, record, 16)?;
    if c.pc != bounds_target {
        return Err(invalid("Cell bounds failure moved"));
    }
    failure(c, record, 2)?;
    if c.pc != done {
        return Err(invalid("Cell codec range has unchecked instructions"));
    }
    Ok(done)
}

/// Check the same complete table codec inside a selection's privately owned
/// scratch region. The selector independently binds the preceding syscall.
// Keep the decoder's immutable evidence and scratch ownership explicit.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_selected_table(
    record: &VerifiedLoweringRecord,
    pc: u64,
    words: &BTreeMap<u64, u32>,
    targets: &BTreeMap<u64, u64>,
    fields: &[Field],
    capacity: u32,
    size: u32,
    buffer: u32,
    frame: u32,
) -> Result<u64, CheckerError> {
    let mut c = Decoder { pc, words, targets, labels: BTreeMap::new(), branches: Vec::new() };
    check_table(&mut c, record, fields, capacity, size, buffer, frame)
}
