//! Typed, bounded instruction contracts for nested reads and pointer moves.
//! Witness lifetime/effects use these checked summaries. Cell-loader provenance
//! is separate; this module does not grant stable nested interface admission.
use super::*;
use crate::checker::{canonical_abi_type, committed_state_type_width, is_nested_source_type, nested_source_parts};
use crate::{TypedSemanticEntry, TypedSemanticOperand, TypedSemanticOperationDetail};

pub(super) const PREFIX: &str = ".Lnested_use_";

pub(super) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    roots: &BTreeMap<(String, u64), u32>,
    seeds: &BTreeMap<String, lifetime::Seed>,
    effects: lifetime::CellEffects<'_>,
    terminal_sink: Option<u64>,
) -> Result<BTreeMap<u64, u64>, CheckerError> {
    let cells = effects.cells;
    let cell_buffers = &cells.buffers;
    let words = elf.instructions.iter().map(|instruction| (instruction.address, instruction.word)).collect::<BTreeMap<_, _>>();
    let targets = elf.control_flow.iter().map(|flow| (flow.address, flow.target)).collect::<BTreeMap<_, _>>();
    let mut starts = BTreeMap::new();
    let mut ends = BTreeMap::new();
    let mut blocks_by_address = BTreeMap::new();
    for block in &record.blocks {
        blocks_by_address.insert(block.range.start, block);
        for label in &block.nested_read_labels {
            if label.ends_with("_done") {
                if ends.insert((block.owner_entry.clone(), label.clone()), block).is_some() {
                    return Err(invalid("duplicate nested read completion"));
                }
            } else {
                let (prefix, ordinal) = label.rsplit_once('_').ok_or_else(|| invalid("invalid nested read label"))?;
                ordinal.parse::<u64>().map_err(|_| invalid("invalid nested read ordinal"))?;
                if starts.insert((block.owner_entry.clone(), format!("{prefix}_")), (block, label)).is_some() {
                    return Err(invalid("duplicate nested read operation"));
                }
            }
        }
    }
    let mut typed_owner_at = BTreeMap::new();
    let mut current = BTreeMap::new();
    for (address, block) in &blocks_by_address {
        if let Some(id) = block.lowering_block_id {
            current.insert(block.owner_entry.as_str(), id);
        }
        typed_owner_at.insert(*address, (block.owner_entry.as_str(), current.get(block.owner_entry.as_str()).copied()));
    }
    let mut incoming = BTreeMap::<u64, Vec<u64>>::new();
    for flow in &elf.control_flow {
        incoming.entry(flow.target).or_default().push(flow.address);
    }
    let mut checked = BTreeSet::new();
    let mut lifetime_sizes = BTreeMap::new();
    let mut lifetime_uses = BTreeMap::<String, BTreeMap<u64, lifetime::Use>>::new();
    for entry in &record.typed_semantics.entries {
        if !seeds.contains_key(&entry.id)
            && !roots.keys().any(|(owner, _)| owner == &entry.id)
            && !entry.blocks.iter().flat_map(|block| &block.operations).any(|operation| {
                operation.operands.first().is_some_and(|operand| is_nested_source_type(&operand.ty))
                    || cell_field(record, operation).is_some()
            })
        {
            continue;
        }
        let mut sizes = BTreeMap::<u64, u32>::new();
        for ((owner, source), offset) in roots {
            if owner == &entry.id {
                sizes.insert(*source, *offset);
            }
        }
        let mut aliases = Vec::new();
        let frame = super::owned_frame(
            record,
            elf,
            record.entries.iter().find(|lowering| lowering.id == entry.id).ok_or_else(|| invalid("missing use owner"))?,
        )?;
        let scalar_end = entry
            .locals
            .iter()
            .map(|local| local.source_id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .and_then(|count| count.checked_mul(8))
            .ok_or_else(|| invalid("nested scalar frame overflows"))?;
        let mut physical_locals = BTreeMap::new();
        for local in &entry.locals {
            let nested = is_nested_source_type(&local.ty);
            if physical_locals.insert(local.source_id, nested).is_some_and(|previous| previous || nested) {
                return Err(invalid("nested pointer slot aliases another typed local"));
            }
        }
        for (source, offset) in sizes.clone() {
            bind_size(&mut sizes, source, offset, scalar_end, frame)?;
        }
        let source_id = |local: u32| {
            entry
                .locals
                .iter()
                .find(|candidate| candidate.id == local)
                .map(|local| local.source_id)
                .ok_or_else(|| invalid("nested operation has no source local"))
        };
        for block in &entry.blocks {
            for operation in &block.operations {
                let Some(source) = operation.operands.first() else { continue };
                let cell_field = cell_field(record, operation);
                if !is_nested_source_type(&source.ty) && cell_field.is_none() {
                    continue;
                }
                let kind = if cell_field.is_some() {
                    "cellfield"
                } else {
                    match operation.opcode.as_str() {
                        "index" => "index",
                        "length" => "length",
                        "field-access" => "field",
                        "move" => "move",
                        _ => continue,
                    }
                };
                let [destination] = operation.destinations.as_slice() else { return Err(invalid("nested use has no unique result")) };
                let destination_id = source_id(*destination)?;
                let source = source.local.ok_or_else(|| invalid("nested use has no local pointer"))?;
                let source_id_value = source_id(source)?;
                let source_type = canonical_abi_type(&operation.operands[0].ty);
                let marker_prefix = format!("{PREFIX}{kind}_{destination_id}_");
                let (marker, marker_name) =
                    starts.get(&(entry.id.clone(), marker_prefix.clone())).ok_or_else(|| invalid("nested use marker missing"))?;
                if !checked.insert((entry.id.clone(), marker_prefix.clone())) {
                    return Err(invalid("multiple typed operations claim one nested use"));
                }
                let done_name = format!("{marker_name}_done");
                let end = ends
                    .get(&(entry.id.clone(), done_name.clone()))
                    .ok_or_else(|| invalid("nested use completion missing"))?
                    .range
                    .start;
                checked.insert((entry.id.clone(), done_name));
                let start = marker.range.start;
                if end <= start
                    || incoming.range(start + 4..end).any(|(_, sources)| sources.iter().any(|address| !(start..end).contains(address)))
                {
                    return Err(invalid("control flow enters a nested read after its input or bounds check"));
                }
                if let Some((address, (owner, id))) =
                    typed_owner_at.range(start..end).find(|(_, (owner, id))| *owner != entry.id || *id != Some(block.id))
                {
                    return Err(invalid(format!(
                        "nested read crosses its owning typed block: {} block {} operation {} at {address:#x} belongs to {owner} {id:?}",
                        entry.id, block.id, operation.index
                    )));
                }
                let mut c = Decoder { pc: start, words: &words, targets: &targets, labels: BTreeMap::new(), branches: Vec::new() };
                let pointer_slot = slot(source_id_value)?;
                let mut stores = vec![slot(destination_id)?];
                let mut final_store = true;
                let mut cell_source_size = None;
                match kind {
                    "cellfield" => {
                        let (ty, index) = cell_field.ok_or_else(|| invalid("missing typed Cell field"))?;
                        expect_stack(&mut c, 29, pointer_slot, false, frame)?;
                        let size = stack(&mut c, 10, false, frame)?;
                        cell_source_size = Some(size);
                        if let Some(buffer) = cell_buffers.get(&(entry.id.clone(), source_id_value))
                            && size != buffer.size
                        {
                            return Err(invalid("Cell field length differs from its checked load"));
                        }
                        c.constant(6, (ty.fields.len() as u64 + 1) * 4)?;
                        let invalid_target = branch(&mut c, 6, 10, 6)?;
                        c.u32_le(5, 29, 0, 7)?;
                        let mut failures = vec![branch(&mut c, 1, 5, 10)?];
                        c.u32_le(30, 29, (index as i32 + 1) * 4, 7)
                            .map_err(|error| invalid(format!("Cell field {} at Molecule index {index}: {error}", ty.name)))?;
                        if index + 1 < ty.fields.len() {
                            c.u32_le(31, 29, (index as i32 + 2) * 4, 7)?;
                        } else {
                            c.addi(31, 10, 0)?;
                        }
                        failures.extend([branch(&mut c, 6, 30, 6)?, branch(&mut c, 6, 31, 30)?, branch(&mut c, 6, 10, 31)?]);
                        c.add(5, 29, 30)?;
                        c.sub(6, 31, 30)?;
                        expect_stack(&mut c, 5, slot(destination_id)?, true, frame)?;
                        let result_size = stack(&mut c, 6, true, frame)?;
                        bind_size(&mut sizes, destination_id, result_size, scalar_end, frame)?;
                        if result_size == size {
                            return Err(invalid("Cell field view overwrites its source length"));
                        }
                        if cell_buffers.iter().any(|((owner, _), buffer)| {
                            owner == &entry.id
                                && (result_size == buffer.size || result_size < buffer.end && result_size + 8 > buffer.start)
                        }) {
                            return Err(invalid("Cell field view length aliases loaded Cell storage"));
                        }
                        stores.push(result_size);
                        if c.jump()? != end || c.pc != invalid_target || failures.iter().any(|target| *target != invalid_target) {
                            return Err(invalid("Cell field span check can be bypassed"));
                        }
                        let failure = c.jump()?;
                        if !record.verifier_failure_exits.iter().any(|exit| exit.address == failure && exit.code == 2) {
                            return Err(invalid("Cell field span lost its checked fatal exit"));
                        }
                        final_store = false;
                    }
                    "move" => {
                        expect_stack(&mut c, 5, pointer_slot, false, frame)?;
                        aliases.push((destination_id, source_id_value));
                    }
                    "length" | "index" => {
                        if kind == "index" {
                            let index = operation.operands.get(1).ok_or_else(|| invalid("nested index has no operand"))?;
                            operand(&mut c, entry, index, 13, frame)?;
                        }
                        expect_stack(&mut c, 10, pointer_slot, false, frame)?;
                        let size = stack(&mut c, 11, false, frame)?;
                        bind_size(&mut sizes, source_id_value, size, scalar_end, frame)?;
                        let values = nested_source_parts(&source_type, "NestedValues", 4).is_some();
                        count(&mut c, values)?;
                        if kind == "index" {
                            let in_range = branch(&mut c, 6, 13, 5)?;
                            let failure = c.jump()?;
                            if in_range != c.pc
                                || !record.verifier_failure_exits.iter().any(|exit| exit.address == failure && exit.code == 21)
                            {
                                return Err(invalid("nested index does not enforce its fatal range boundary"));
                            }
                            if values {
                                c.immediate(0x13, 1, 6, 13, 2)?;
                                c.add(6, 10, 6)?;
                                c.u32_le(7, 6, 4, 28)?;
                                c.addi(13, 13, 1)?;
                                c.addi(29, 11, 0)?;
                                let last = branch(&mut c, 0, 13, 5)?;
                                c.u32_le(29, 6, 8, 28)?;
                                if last != c.pc {
                                    return Err(invalid("nested final element does not use the complete slice end"));
                                }
                                c.sub(29, 29, 7)?;
                                c.add(5, 10, 7)?;
                                let size = stack(&mut c, 29, true, frame)?;
                                bind_size(&mut sizes, destination_id, size, scalar_end, frame)?;
                                stores.push(size);
                            } else {
                                c.add(6, 10, 13)?;
                                c.immediate(3, 4, 5, 6, 4)?;
                            }
                        }
                    }
                    "field" => {
                        let parts = nested_source_parts(&source_type, "NestedRecordView", 2)
                            .ok_or_else(|| invalid("nested field lacks its record type"))?;
                        let ty = record
                            .typed_semantics
                            .types
                            .iter()
                            .find(|ty| canonical_abi_type(&ty.name) == parts[0])
                            .ok_or_else(|| invalid("missing nested record declaration"))?;
                        let TypedSemanticOperationDetail::Field { name } = &operation.detail else {
                            return Err(invalid("nested field has no typed name"));
                        };
                        let index =
                            ty.fields.iter().position(|field| &field.name == name).ok_or_else(|| invalid("unknown nested field"))?;
                        let mut offset = (ty.fields.len() as u32 + 1) * 4;
                        for field in &ty.fields[..index] {
                            let width = committed_state_type_width(&canonical_abi_type(&field.ty), &record.typed_semantics.types)
                                .and_then(|width| u32::try_from(width).ok())
                                .ok_or_else(|| invalid("nested fixed prefix has no width"))?;
                            offset = offset.checked_add(width).ok_or_else(|| invalid("nested field offset overflows"))?;
                        }
                        expect_stack(&mut c, 10, pointer_slot, false, frame)?;
                        c.li(6, offset)?;
                        c.add(5, 10, 6)?;
                        if index + 1 == ty.fields.len() {
                            let size = stack(&mut c, 11, false, frame)?;
                            bind_size(&mut sizes, source_id_value, size, scalar_end, frame)?;
                            c.sub(11, 11, 6)?;
                            let size = stack(&mut c, 11, true, frame)?;
                            bind_size(&mut sizes, destination_id, size, scalar_end, frame)?;
                            stores.push(size);
                        } else {
                            let width =
                                committed_state_type_width(&canonical_abi_type(&ty.fields[index].ty), &record.typed_semantics.types)
                                    .ok_or_else(|| invalid("nested scalar field has no width"))?;
                            let scalar = matches!(canonical_abi_type(&ty.fields[index].ty).as_str(), "u8" | "u16" | "u32" | "u64");
                            if scalar {
                                c.addi(10, 5, 0)?;
                                c.li(5, 0)?;
                                for byte in 0..width {
                                    c.immediate(3, 4, 7, 10, byte as i32)?;
                                    if byte != 0 {
                                        c.immediate(0x13, 1, 7, 7, byte as i32 * 8)?;
                                    }
                                    c.register(6, 0, 5, 5, 7)?;
                                }
                            }
                        }
                    }
                    _ => unreachable!(),
                }
                if final_store {
                    expect_stack(&mut c, 5, slot(destination_id)?, true, frame)?;
                }
                if c.pc != end {
                    return Err(invalid(format!("unchecked nested read tail at {:#x}, end {end:#x}", c.pc)));
                }
                let nested_result =
                    entry.locals.iter().find(|local| local.id == *destination).is_some_and(|local| is_nested_source_type(&local.ty));
                if lifetime_uses
                    .entry(entry.id.clone())
                    .or_default()
                    .insert(
                        start,
                        lifetime::Use {
                            end,
                            source: source_id_value,
                            destination: destination_id,
                            nested_result,
                            alias: kind == "move",
                            stores,
                            cell_source_size,
                        },
                    )
                    .is_some()
                {
                    return Err(invalid("multiple nested reads claim one machine range"));
                }
            }
        }
        // Moves preserve the encoded length without allocating a second size
        // slot. Bind all alias constraints, including aliases used before the
        // producer in physical block order (e.g. loop bodies).
        for _ in 0..=aliases.len() {
            let mut changed = false;
            for &(destination, source) in &aliases {
                match (sizes.get(&destination).copied(), sizes.get(&source).copied()) {
                    (Some(a), Some(b)) if a != b => return Err(invalid("nested pointer alias changed its length slot")),
                    (Some(offset), None) => {
                        sizes.insert(source, offset);
                        changed = true;
                    }
                    (None, Some(offset)) => {
                        sizes.insert(destination, offset);
                        changed = true;
                    }
                    _ => {}
                }
            }
            if !changed {
                break;
            }
        }
        let alias_sources = aliases.iter().copied().collect::<BTreeMap<_, _>>();
        let mut occupied_sizes = BTreeMap::new();
        for (source, offset) in &sizes {
            if cell_buffers.iter().any(|((owner, _), buffer)| {
                owner == &entry.id && (*offset == buffer.size || *offset < buffer.end && *offset + 8 > buffer.start)
            }) {
                return Err(invalid("nested view length aliases loaded Cell storage"));
            }
            let mut root = *source;
            let mut visited = BTreeSet::new();
            while let Some(parent) = alias_sources.get(&root) {
                if !visited.insert(root) {
                    return Err(invalid("cyclic nested pointer aliases"));
                }
                root = *parent;
            }
            if occupied_sizes.insert(*offset, root).is_some_and(|previous| previous != root) {
                return Err(invalid("independent nested values share an encoded length slot"));
            }
        }
        lifetime_sizes.insert(entry.id.clone(), sizes);
    }
    for block in &record.blocks {
        for label in &block.nested_read_labels {
            let key = if label.ends_with("_done") {
                label.clone()
            } else {
                format!("{}_", label.rsplit_once('_').ok_or_else(|| invalid("invalid nested read ordinal"))?.0)
            };
            if !checked.contains(&(block.owner_entry.clone(), key)) {
                return Err(invalid("nested read marker has no typed operation"));
            }
        }
    }
    lifetime::validate(record, elf, seeds, &lifetime_sizes, &lifetime_uses, effects, terminal_sink)
}

fn slot(source: u64) -> Result<u32, CheckerError> {
    source.checked_mul(8).and_then(|slot| u32::try_from(slot).ok()).ok_or_else(|| invalid("nested scalar slot overflows"))
}

fn cell_field<'a>(
    record: &'a VerifiedLoweringRecord,
    operation: &crate::TypedSemanticOperation,
) -> Option<(&'a crate::TypedSemanticType, usize)> {
    if operation.opcode != "field-access" {
        return None;
    }
    let TypedSemanticOperationDetail::Field { name } = &operation.detail else { return None };
    let source = canonical_abi_type(&operation.operands.first()?.ty);
    let source = source.strip_prefix("&mut").or_else(|| source.strip_prefix('&')).unwrap_or(&source);
    let ty = record.typed_semantics.types.iter().find(|ty| canonical_abi_type(&ty.name) == source)?;
    if !matches!(ty.kind.as_str(), "resource" | "shared" | "receipt") {
        return None;
    }
    let field = ty.fields.iter().find(|field| &field.name == name)?;
    nested_source_parts(&field.ty, "NestedValues", 4)?;
    Some((ty, field.molecule_field_index? as usize))
}
fn bind_size(sizes: &mut BTreeMap<u64, u32>, source: u64, offset: u32, scalar_end: u64, frame: u32) -> Result<(), CheckerError> {
    if u64::from(offset) < scalar_end || offset.checked_add(8).is_none_or(|end| end > frame.saturating_sub(16)) {
        return Err(invalid("nested length slot overlaps scalar values or frame bookkeeping"));
    }
    if sizes.insert(source, offset).is_some_and(|previous| previous != offset) {
        return Err(invalid("nested value changed its encoded length slot"));
    }
    Ok(())
}
fn stack(c: &mut Decoder<'_>, register: u32, store: bool, frame: u32) -> Result<u32, CheckerError> {
    let mut word = *c.words.get(&c.pc).ok_or_else(|| invalid("missing nested stack access"))?;
    let mut offset = 0i64;
    let base = if word & 0x7f == 0x37 {
        // t6 is scratch, never a live operand/result in these read contracts.
        if word >> 7 & 31 != 31 {
            return Err(invalid("nested stack helper clobbers a live register"));
        }
        offset = i64::from((c.word()? & 0xfffff000) as i32);
        word = *c.words.get(&c.pc).ok_or_else(|| invalid("missing stack address continuation"))?;
        if word & 0x707f == 0x13 && word >> 7 & 31 == 31 && word >> 15 & 31 == 31 {
            offset += i64::from((c.word()? as i32) >> 20);
        }
        c.add(31, 2, 31)?;
        word = *c.words.get(&c.pc).ok_or_else(|| invalid("missing indirect stack access"))?;
        31
    } else {
        2
    };
    let immediate = if store { ((((word >> 7) & 31) | ((word >> 25) << 5)) as i32) << 20 >> 20 } else { (word as i32) >> 20 };
    if base == 31 && immediate != 0 {
        return Err(invalid("nested indirect access has an unchecked displacement"));
    }
    offset += i64::from(immediate);
    if offset < 0 || offset % 8 != 0 || offset + 8 > i64::from(frame.saturating_sub(16)) {
        return Err(invalid("nested access leaves its owned frame"));
    }
    let expected = if store {
        ((immediate as u32 & 0xfe0) << 20) | (register << 20) | (base << 15) | (3 << 12) | ((immediate as u32 & 31) << 7) | 0x23
    } else {
        ((immediate as u32 & 0xfff) << 20) | (base << 15) | (3 << 12) | (register << 7) | 3
    };
    c.exact(expected)?;
    Ok(offset as u32)
}
pub(super) fn expect_stack(c: &mut Decoder<'_>, register: u32, offset: u32, store: bool, frame: u32) -> Result<(), CheckerError> {
    if stack(c, register, store, frame)? != offset {
        return Err(invalid("nested access uses another source or result slot"));
    }
    Ok(())
}
fn branch(c: &mut Decoder<'_>, kind: u32, left: u32, right: u32) -> Result<u64, CheckerError> {
    let pc = c.pc;
    let word = c.word()?;
    if word & 0x01fff07f != ((right << 20) | (left << 15) | (kind << 12) | 0x63) {
        return Err(invalid("nested read condition differs"));
    }
    c.targets.get(&pc).copied().ok_or_else(|| invalid("nested read branch lacks its machine target"))
}
fn count(c: &mut Decoder<'_>, values: bool) -> Result<(), CheckerError> {
    if values {
        c.li(5, 0)?;
        c.li(6, 4)?;
        let empty = branch(c, 0, 11, 6)?;
        c.u32_le(5, 10, 4, 6)?;
        c.immediate(0x13, 5, 5, 5, 2)?;
        c.addi(5, 5, -1)?;
        if empty != c.pc {
            return Err(invalid("nested empty count branch differs"));
        }
    } else {
        c.u32_le(5, 10, 0, 6)?;
    }
    Ok(())
}
fn operand(
    c: &mut Decoder<'_>,
    entry: &TypedSemanticEntry,
    operand: &TypedSemanticOperand,
    register: u32,
    frame: u32,
) -> Result<(), CheckerError> {
    if let Some(local) = operand.local {
        let local =
            entry.locals.iter().find(|candidate| candidate.id == local).ok_or_else(|| invalid("nested index local is absent"))?;
        expect_stack(c, register, slot(local.source_id)?, false, frame)
    } else {
        use crate::TypedSemanticConstant::*;
        let value = match operand.constant.as_ref() {
            Some(U8(value) | U16(value) | U32(value) | U64(value)) => {
                value.parse::<u64>().map_err(|_| invalid("invalid unsigned index"))?
            }
            _ => return Err(invalid("nested index is not an unsigned scalar")),
        };
        // Decode the finite RISC-V constant sequence independently; high index
        // constants still have to reach the checked runtime range guard.
        c.constant(register, value)
    }
}
