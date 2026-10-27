//! Bind scalar and pointer-backed fixed-value arguments to their consuming
//! primitive. Short arrays use the representation derived from their producers;
//! sorted inputs bind their evaluated typed slot to the checked input pointer.
use super::*;
use crate::allocated_values::MemberKind;
use crate::{TypedSemanticConstant, TypedSemanticOperand, TypedSemanticOperation};
use std::collections::VecDeque;

pub(super) const PREFIX: &str = ".Lallocated_args_";

fn scalar_width(ty: &str) -> Option<u32> {
    match ty {
        "bool" | "u8" => Some(1),
        "u16" => Some(2),
        "u32" | "i32" => Some(4),
        "u64" => Some(8),
        _ => None,
    }
}

fn argument_width(ty: &str, types: &BTreeMap<&str, &crate::TypedSemanticType>) -> Option<u32> {
    crate::allocated_values::value_width(ty, types, &mut BTreeSet::new())
}

fn short_width(ty: &str) -> Option<u32> {
    ty.strip_prefix("[u8; ")?.strip_suffix(']')?.parse::<u32>().ok().filter(|width| (1..=8).contains(width))
}

fn sorted_input(
    shape: &MemberKind,
    input: &TypedSemanticOperand,
    types: &BTreeMap<&str, &crate::TypedSemanticType>,
) -> Result<(u32, u32), CheckerError> {
    let (element, width) = match shape {
        MemberKind::OrderedSet { key } => (key.clone(), argument_width(key, types)),
        MemberKind::OrderedMap { key, value } => (
            format!("({key}, {value})"),
            argument_width(key, types).and_then(|key| argument_width(value, types).and_then(|value| key.checked_add(value))),
        ),
        _ => return Err(invalid("sorted input targets an unordered member")),
    };
    let (item, count) = input
        .ty
        .strip_prefix('[')
        .and_then(|ty| ty.strip_suffix(']'))
        .and_then(|ty| ty.rsplit_once("; "))
        .ok_or_else(|| invalid("sorted input has no fixed array type"))?;
    let count =
        count.parse::<u32>().ok().filter(|count| *count <= 1024).ok_or_else(|| invalid("sorted input count exceeds its bound"))?;
    if item != element || input.local.is_none() {
        return Err(invalid("sorted input differs from its member or typed local"));
    }
    let bytes = width.and_then(|width| width.checked_mul(count)).ok_or_else(|| invalid("sorted input width is invalid"))?;
    Ok((count, bytes))
}

fn short_pointer_locals(entry: &crate::TypedSemanticEntry) -> Result<BTreeSet<u32>, CheckerError> {
    let operations = entry.blocks.iter().flat_map(|block| &block.operations).collect::<Vec<_>>();
    let locals = entry.locals.iter().map(|local| (local.id, local.ty.as_str())).collect::<BTreeMap<_, _>>();
    let mut pointers = operations
        .iter()
        .filter(|op| matches!(op.opcode.as_str(), "field-access" | "index" | "allocated-value"))
        .flat_map(|op| op.destinations.iter().copied())
        .collect::<BTreeSet<_>>();
    // Reference conversions and non-byte aggregate aliases preserve storage.
    // Short byte-array Moves pack a word instead of retaining a source pointer.
    let mut work = 0usize;
    for _ in 0..=operations.len() {
        let mut changed = false;
        for op in &operations {
            work += 1;
            if work > 4_000_000 {
                return Err(invalid("short argument aliases exceed the proof budget"));
            }
            let alias = matches!(&op.detail, TypedSemanticOperationDetail::UnaryOperator { operator } if matches!(operator.as_str(), "ref" | "deref"))
                || (op.opcode == "move"
                    && op.destinations.iter().all(|dest| locals.get(dest).is_some_and(|ty| short_width(ty).is_none())));
            if alias && op.operands.first().and_then(|operand| operand.local).is_some_and(|id| pointers.contains(&id)) {
                for &dest in &op.destinations {
                    changed |= pointers.insert(dest);
                }
            }
        }
        if !changed {
            break;
        }
    }
    Ok(pointers)
}

fn slot(source: u64) -> Result<u32, CheckerError> {
    source.checked_mul(8).and_then(|offset| u32::try_from(offset).ok()).ok_or_else(|| invalid("argument slot overflows"))
}

struct Expected<'a> {
    op: &'a TypedSemanticOperation,
    contract: &'a Contract,
    offset: u32,
    pointer: Option<u32>,
    kind: Primitive,
    // (operand ordinal, byte offset relative to the checked private scratch).
    copies: Vec<(usize, u32)>,
    index: bool,
    sorted_bytes: Option<u32>,
}

struct Stage<'a> {
    owner: &'a str,
    block: u32,
    source: u64,
    member: u32,
    start: u64,
    end: u64,
}

fn stages(record: &VerifiedLoweringRecord) -> Result<Vec<Stage<'_>>, CheckerError> {
    let mut boundaries = BTreeMap::new();
    let mut current = BTreeMap::new();
    let mut blocks = record.blocks.iter().collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    for block in blocks {
        let owner = block.owner_entry.as_str();
        if let Some(id) = block.lowering_block_id {
            current.insert(owner, id);
        }
        for label in &block.allocated_lifecycle_labels {
            let Some(suffix) = label.strip_prefix(PREFIX) else { continue };
            let parts = suffix.split('_').collect::<Vec<_>>();
            if parts.len() != 4 || !matches!(parts[3], "begin" | "end") {
                return Err(invalid("invalid collection argument marker"));
            }
            let source = parts[0].parse::<u64>().map_err(|_| invalid("invalid argument owner coordinate"))?;
            let member = parts[1].parse::<u32>().map_err(|_| invalid("invalid argument member coordinate"))?;
            let id = parts[2].parse::<u64>().map_err(|_| invalid("invalid argument site identity"))?;
            let typed = *current.get(owner).ok_or_else(|| invalid("arguments outside typed body"))?;
            if boundaries.insert((owner, source, member, id, parts[3]), (block.range.start, typed)).is_some() {
                return Err(invalid("duplicate collection argument boundary"));
            }
        }
    }
    let mut result = Vec::new();
    for (&(owner, source, member, id, edge), &(start, block)) in &boundaries {
        if edge != "begin" {
            continue;
        }
        let &(end, end_block) =
            boundaries.get(&(owner, source, member, id, "end")).ok_or_else(|| invalid("collection argument end missing"))?;
        if start >= end || end - start > 4096 || block != end_block {
            return Err(invalid("invalid collection argument range"));
        }
        result.push(Stage { owner, block, source, member, start, end });
    }
    if result.len() * 2 != boundaries.len() {
        return Err(invalid("orphan collection argument end"));
    }
    result.sort_by_key(|stage| stage.start);
    if result.windows(2).any(|pair| pair[0].end > pair[1].start) {
        return Err(invalid("overlapping collection argument stages"));
    }
    Ok(result)
}

fn scalar(c: &mut Cursor<'_>, locals: &BTreeMap<u32, u64>, operand: &TypedSemanticOperand, register: u32) -> Result<(), CheckerError> {
    if let Some(id) = operand.local {
        let source = *locals.get(&id).ok_or_else(|| invalid("collection argument local missing"))?;
        c.stack_load(register, slot(source)?)
    } else {
        let value = match &operand.constant {
            Some(TypedSemanticConstant::Bool(value)) => u64::from(*value),
            Some(
                TypedSemanticConstant::U8(value)
                | TypedSemanticConstant::U16(value)
                | TypedSemanticConstant::U32(value)
                | TypedSemanticConstant::U64(value),
            ) => value.parse().map_err(|_| invalid("invalid collection argument constant"))?,
            Some(TypedSemanticConstant::Array(items)) if short_width(&operand.ty) == Some(items.len() as u32) => {
                items.iter().enumerate().try_fold(0u64, |value, (index, item)| {
                    let TypedSemanticConstant::U8(byte) = item else {
                        return Err(invalid("non-byte short argument literal"));
                    };
                    let byte = byte.parse::<u8>().map_err(|_| invalid("invalid short argument byte"))?;
                    Ok(value | (u64::from(byte) << (index * 8)))
                })?
            }
            _ => return Err(invalid("collection argument has no scalar source")),
        };
        c.constant(register, value)
    }
}

pub(super) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    machine: &Machine<'_>,
    constructors: &BTreeMap<String, BTreeMap<u64, Constructor>>,
    sites: &[Site<'_>],
) -> Result<(), CheckerError> {
    let types = record.typed_semantics.types.iter().map(|ty| (ty.name.as_str(), ty)).collect::<BTreeMap<_, _>>();
    let short_pointers = record
        .typed_semantics
        .entries
        .iter()
        .map(|entry| short_pointer_locals(entry).map(|locals| (entry.id.as_str(), locals)))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let mut expected = BTreeMap::<(&str, u32, u64, u32), VecDeque<Expected<'_>>>::new();
    let owner_types = crate::allocated_values::owner_types(&record.typed_semantics);
    let mut work = 0usize;
    for entry in &record.typed_semantics.entries {
        let locals = entry.locals.iter().map(|local| (local.id, local)).collect::<BTreeMap<_, _>>();
        let owned = constructors
            .get(&entry.id)
            .into_iter()
            .flat_map(|sites| sites.values())
            .map(|site| (site.local, site))
            .collect::<BTreeMap<_, _>>();
        let params = entry.params.iter().map(|param| (param.binding_id, param)).collect::<BTreeMap<_, _>>();
        let aliases = super::super::owner_aliases(entry, &owner_types)?;
        for block in &entry.blocks {
            for op in &block.operations {
                let TypedSemanticOperationDetail::AllocatedValue { operation, contract, member: Some(member) } = &op.detail else {
                    continue;
                };
                if op.operands.len() < 2
                    || (*operation != Operation::LoadSorted
                        && op.operands[1..].iter().any(|operand| argument_width(&operand.ty, &types).is_none()))
                {
                    continue;
                }
                let shape = &contract.members.get(*member as usize).ok_or_else(|| invalid("argument member missing"))?.shape;
                let sorted = if *operation == Operation::LoadSorted {
                    let input = sorted_input(shape, &op.operands[1], &types)?;
                    // The checked zero-count primitive reads no source bytes.
                    if input.0 == 0 {
                        continue;
                    }
                    Some(input)
                } else {
                    None
                };
                let (kind, copies, index) = match (shape, operation) {
                    (_, Operation::LoadSorted) => (Primitive::Sorted(sorted.unwrap().0), vec![], false),
                    (MemberKind::Vec { .. }, Operation::Push) => (Primitive::Push, vec![(1, 0)], false),
                    (MemberKind::Vec { .. }, Operation::Insert) => (Primitive::Insert, vec![(2, 0)], true),
                    (MemberKind::Vec { .. }, Operation::Set) => (Primitive::Set, vec![(2, 0)], true),
                    (MemberKind::Vec { .. }, _) => continue,
                    (MemberKind::OrderedMap { key, .. }, Operation::Insert) => (
                        Primitive::Put,
                        vec![(1, 0), (2, argument_width(key, &types).ok_or_else(|| invalid("unsupported map argument key"))?)],
                        false,
                    ),
                    (MemberKind::OrderedSet { .. }, Operation::Insert) => (Primitive::Put, vec![(1, 0)], false),
                    (_, Operation::Remove) => (Primitive::Erase, vec![(1, 0)], false),
                    (_, Operation::Contains) => (Primitive::Find, vec![(1, 0)], false),
                    (_, Operation::Get) => (Primitive::Required, vec![(1, 0)], false),
                    (MemberKind::OrderedMap { key, .. }, Operation::GetOr) => (
                        Primitive::Lookup,
                        vec![(1, 0), (2, 64 + argument_width(key, &types).ok_or_else(|| invalid("unsupported map argument key"))?)],
                        false,
                    ),
                    _ => continue,
                };
                let owner = op.operands[0].local.ok_or_else(|| invalid("collection argument owner missing"))?;
                let source = locals.get(&owner).ok_or_else(|| invalid("collection argument owner local missing"))?.source_id;
                let mut root = owner;
                let mut visited = BTreeSet::new();
                while !owned.contains_key(&root) && !params.contains_key(&root) {
                    work += 1;
                    if work > 4_000_000 || !visited.insert(root) {
                        return Err(invalid("cyclic or excessive argument owner aliases"));
                    }
                    root = *aliases
                        .get(&root)
                        .filter(|sources| sources.len() == 1)
                        .and_then(|sources| sources.first())
                        .ok_or_else(|| invalid("ambiguous collection argument owner"))?;
                }
                let (offset, pointer) = if let Some(owned) = owned.get(&root) {
                    if owned.owner_type != contract.owner {
                        return Err(invalid("argument constructor type differs"));
                    }
                    (u32::try_from(owned.offset).map_err(|_| invalid("argument constructor offset invalid"))?, None)
                } else {
                    if entry.kind != "helper"
                        || !params
                            .get(&root)
                            .is_some_and(|param| !param.mutable && param.ty.strip_prefix('&') == Some(contract.owner.as_str()))
                    {
                        return Err(invalid("collection argument has no readonly parameter root"));
                    }
                    (0, Some(slot(source)?))
                };
                expected.entry((entry.id.as_str(), block.id, source, *member)).or_default().push_back(Expected {
                    op,
                    contract,
                    offset,
                    pointer,
                    kind,
                    copies,
                    index,
                    sorted_bytes: sorted.map(|(_, bytes)| bytes),
                });
            }
        }
    }
    let local_sources = record
        .typed_semantics
        .entries
        .iter()
        .map(|entry| (entry.id.as_str(), entry.locals.iter().map(|local| (local.id, local.source_id)).collect::<BTreeMap<_, _>>()))
        .collect::<BTreeMap<_, _>>();
    let mut starts = BTreeMap::new();
    for site in sites {
        if starts.insert((site.owner, site.kind, site.start), site).is_some() {
            return Err(invalid("ambiguous collection argument consumer"));
        }
    }
    let mut incoming = BTreeMap::<u64, Vec<u64>>::new();
    for edge in &elf.control_flow {
        incoming.entry(edge.target).or_default().push(edge.address);
    }
    let mut blocks = record.blocks.iter().collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    for stage in stages(record)? {
        let expected = expected
            .get_mut(&(stage.owner, stage.block, stage.source, stage.member))
            .and_then(VecDeque::pop_front)
            .ok_or_else(|| invalid("extra or unbound collection argument stage"))?;
        let site = starts
            .get(&(stage.owner, expected.kind, stage.end))
            .ok_or_else(|| invalid("collection arguments detached from checked primitive"))?;
        if site.offset != expected.offset
            || site.contract != expected.contract
            || site.borrowed.map(|borrow| borrow.pointer) != expected.pointer
            || site.descriptor != Some(expected.offset + 32 + expected.contract.bytes + stage.member * 32)
        {
            return Err(invalid("collection arguments use a different owner or member"));
        }
        let mut c = Cursor::new(stage.start, stage.end, machine.words, machine.targets);
        if let Some(bytes) = expected.sorted_bytes {
            let input = &expected.op.operands[1];
            let local = input.local.ok_or_else(|| invalid("sorted input local missing"))?;
            let source = *local_sources[stage.owner].get(&local).ok_or_else(|| invalid("sorted input slot missing"))?;
            if bytes <= 8 && !short_pointers[stage.owner].contains(&local) {
                c.stack_address(14, slot(source)?)?;
            } else {
                c.stack_load(14, slot(source)?)?;
            }
            c.stack_store(14, site.scratch())?;
        }
        for (ordinal, offset) in expected.copies {
            let operand = expected.op.operands.get(ordinal).ok_or_else(|| invalid("collection argument absent"))?;
            let bytes = argument_width(&operand.ty, &types).ok_or_else(|| invalid("unsupported fixed-value argument"))?;
            let packed_short =
                short_width(&operand.ty).is_some() && !operand.local.is_some_and(|id| short_pointers[stage.owner].contains(&id));
            if scalar_width(&operand.ty).is_some() || packed_short {
                scalar(&mut c, &local_sources[stage.owner], operand, 6)?;
                for byte in 0..bytes {
                    c.stack_byte(6, site.scratch() + offset + byte, true)?;
                    if byte + 1 < bytes {
                        c.immediate(0x13, 5, 6, 6, 8)?;
                    }
                }
            } else {
                let source = operand
                    .local
                    .and_then(|id| local_sources[stage.owner].get(&id))
                    .ok_or_else(|| invalid("fixed-value argument lacks its typed pointer local"))?;
                c.stack_load(29, slot(*source)?)?;
                for byte in 0..bytes {
                    c.load(6, 29, byte, true)?;
                    c.stack_byte(6, site.scratch() + offset + byte, true)?;
                }
            }
        }
        if expected.index {
            scalar(&mut c, &local_sources[stage.owner], &expected.op.operands[1], 11)?;
        }
        c.finish()?;
        for sources in incoming.range(stage.start + 1..site.end).map(|(_, sources)| sources) {
            for source in sources {
                work += 1;
                if work > 4_000_000 || !(stage.start..site.end).contains(source) {
                    return Err(invalid("control flow bypasses collection arguments or exceeds the proof budget"));
                }
            }
        }
        let first = blocks.partition_point(|block| block.range.end <= stage.start);
        for block in blocks[first..].iter().take_while(|block| block.range.start < site.end) {
            work += 1;
            if work > 4_000_000 || block.owner_entry != stage.owner || block.lowering_block_id.is_some_and(|id| id != stage.block) {
                return Err(invalid("collection arguments leave their typed block or exceed the proof budget"));
            }
        }
    }
    if expected.values().any(|queue| !queue.is_empty()) {
        return Err(invalid("missing typed collection argument preparation"));
    }
    Ok(())
}
