//! Bind indexed inputs and copied outputs to typed slots and adjacent
//! checked primitives. This proves local transport, not upstream arithmetic,
//! key/value argument copies or full source CFG equality.
use super::*;
use crate::allocated_values::{key_width, MemberKind};
use crate::{TypedSemanticConstant, TypedSemanticLocal, TypedSemanticOperand};
use std::collections::VecDeque;

const INDEX: &str = ".Lallocated_index_";
const RESULT: &str = ".Lallocated_result_";

pub(super) fn is_marker(label: &str) -> bool {
    label.starts_with(INDEX) || label.starts_with(RESULT)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Direction {
    Index,
    Result,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Coordinate {
    direction: Direction,
    source: u64,
    coordinate: u32,
    width: u32,
    output: Option<u32>,
}

struct Stage<'a> {
    owner: &'a str,
    block: u32,
    coordinate: Coordinate,
    start: u64,
    end: u64,
}

struct Expected<'a> {
    contract: &'a Contract,
    member: u32,
    offset: u32,
    pointer: Option<u32>,
    kind: Primitive,
    input: Option<&'a TypedSemanticOperand>,
    output: Option<&'a TypedSemanticLocal>,
    skip: u32,
    width: u32,
}

fn slot(source: u64) -> Result<u32, CheckerError> {
    source.checked_mul(8).and_then(|offset| u32::try_from(offset).ok()).ok_or_else(|| invalid("transport slot overflows"))
}

fn scalar_width(ty: &str) -> Option<u32> {
    match ty {
        "bool" | "u8" => Some(1),
        "u16" => Some(2),
        "u32" | "i32" => Some(4),
        "u64" => Some(8),
        _ => None,
    }
}

fn stages(record: &VerifiedLoweringRecord) -> Result<Vec<Stage<'_>>, CheckerError> {
    let mut boundaries = BTreeMap::new();
    let mut current = BTreeMap::new();
    let mut blocks = record.blocks.iter().collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    for block in blocks {
        if let Some(id) = block.lowering_block_id {
            current.insert(block.owner_entry.as_str(), id);
        }
        for label in &block.allocated_lifecycle_labels {
            let (direction, suffix) = if let Some(suffix) = label.strip_prefix(INDEX) {
                (Direction::Index, suffix)
            } else if let Some(suffix) = label.strip_prefix(RESULT) {
                (Direction::Result, suffix)
            } else {
                continue;
            };
            let parts = suffix.split('_').collect::<Vec<_>>();
            let size = parts.len();
            if !(if direction == Direction::Index { size == 4 } else { matches!(size, 5 | 6) })
                || !matches!(parts[size - 1], "begin" | "end")
            {
                return Err(invalid("invalid collection transport marker"));
            }
            let number = |index: usize| parts[index].parse::<u64>().map_err(|_| invalid("invalid transport coordinate"));
            let small = |index| u32::try_from(number(index)?).map_err(|_| invalid("transport coordinate exceeds frame profile"));
            let coordinate = Coordinate {
                direction,
                source: number(0)?,
                coordinate: small(1)?,
                width: if direction == Direction::Result { small(2)? } else { 0 },
                output: if size == 6 { Some(small(3)?) } else { None },
            };
            let owner = block.owner_entry.as_str();
            let typed = *current.get(owner).ok_or_else(|| invalid("transport outside typed body"))?;
            let key = (owner, coordinate, number(size - 2)?, parts[size - 1]);
            if boundaries.insert(key, (block.range.start, typed)).is_some() {
                return Err(invalid("duplicate collection transport boundary"));
            }
        }
    }
    let mut result = Vec::new();
    for (&(owner, coordinate, id, edge), &(start, block)) in &boundaries {
        if edge != "begin" {
            continue;
        }
        let &(end, end_block) =
            boundaries.get(&(owner, coordinate, id, "end")).ok_or_else(|| invalid("collection transport end missing"))?;
        let limit = if coordinate.output.is_some() { 4096 } else { 1024 };
        if start > end || end - start > limit || block != end_block || (start == end && coordinate.direction == Direction::Result) {
            return Err(invalid("invalid collection transport range"));
        }
        result.push(Stage { owner, block, coordinate, start, end });
    }
    if result.len() * 2 != boundaries.len() {
        return Err(invalid("orphan collection transport end"));
    }
    result.sort_by_key(|stage| (stage.start, stage.end));
    if result.windows(2).any(|pair| pair[0].end > pair[1].start) {
        return Err(invalid("overlapping collection transport stages"));
    }
    Ok(result)
}

pub(super) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    machine: &Machine<'_>,
    constructors: &BTreeMap<String, BTreeMap<u64, Constructor>>,
    sites: &[Site<'_>],
) -> Result<(), CheckerError> {
    let mut expected = BTreeMap::<(&str, u32, Direction, u64, u32), VecDeque<Expected<'_>>>::new();
    let owner_types = crate::allocated_values::owner_types(&record.typed_semantics);
    let types = record.typed_semantics.types.iter().map(|ty| (ty.name.as_str(), ty)).collect();
    let mut storage_bounds = BTreeMap::new();
    let mut work = 0usize;
    for entry in &record.typed_semantics.entries {
        let locals = entry.locals.iter().map(|local| (local.id, local)).collect::<BTreeMap<_, _>>();
        let local = |id| locals.get(&id).copied().ok_or_else(|| invalid("transport typed local missing"));
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
                let shape = &contract.members.get(*member as usize).ok_or_else(|| invalid("transport member missing"))?.shape;
                let vector = matches!(shape, MemberKind::Vec { .. });
                let index_kind = match operation {
                    Operation::Reserve => Some(Primitive::Reserve),
                    Operation::EntryKey | Operation::EntryValue => Some(Primitive::Read),
                    Operation::Get if vector => Some(Primitive::Read),
                    Operation::Remove if vector => Some(Primitive::Take),
                    Operation::Insert if vector => Some(Primitive::Insert),
                    Operation::Set if vector => Some(Primitive::Set),
                    _ => None,
                };
                let result_kind = match operation {
                    Operation::Get if !vector => Some(Primitive::Required),
                    Operation::GetOr => Some(Primitive::Lookup),
                    Operation::Get | Operation::EntryKey | Operation::EntryValue => Some(Primitive::Read),
                    Operation::Remove if vector => Some(Primitive::Take),
                    Operation::Pop => Some(Primitive::Pop),
                    _ => None,
                };
                let output = if result_kind.is_some() {
                    Some(local(*op.destinations.first().ok_or_else(|| invalid("collection result missing"))?)?)
                } else {
                    None
                };
                if index_kind.is_none() && output.is_none() {
                    continue;
                }
                let owner = op.operands.first().and_then(|arg| arg.local).ok_or_else(|| invalid("transport owner missing"))?;
                let mut root = owner;
                let mut visited = BTreeSet::new();
                while !owned.contains_key(&root) && !params.contains_key(&root) {
                    work += 1;
                    if work > 4_000_000 || !visited.insert(root) {
                        return Err(invalid("cyclic or excessive transport aliases"));
                    }
                    root = *aliases
                        .get(&root)
                        .filter(|sources| sources.len() == 1)
                        .and_then(|sources| sources.first())
                        .ok_or_else(|| invalid("ambiguous collection transport owner"))?;
                }
                let (offset, pointer) = if let Some(owned) = owned.get(&root) {
                    if owned.owner_type != contract.owner {
                        return Err(invalid("transport constructor type differs"));
                    }
                    (u32::try_from(owned.offset).map_err(|_| invalid("transport constructor offset invalid"))?, None)
                } else {
                    if entry.kind != "helper"
                        || !params
                            .get(&root)
                            .is_some_and(|param| !param.mutable && param.ty.strip_prefix('&') == Some(contract.owner.as_str()))
                    {
                        return Err(invalid("transport has no readonly parameter root"));
                    }
                    (0, Some(slot(local(owner)?.source_id)?))
                };
                if let Some(kind) = index_kind {
                    let input = op.operands.get(1).ok_or_else(|| invalid("typed index missing"))?;
                    expected
                        .entry((entry.id.as_str(), block.id, Direction::Index, local(owner)?.source_id, *member))
                        .or_default()
                        .push_back(Expected {
                            contract,
                            member: *member,
                            offset,
                            pointer,
                            kind,
                            input: Some(input),
                            output: None,
                            skip: 0,
                            width: 0,
                        });
                }
                if let (Some(kind), Some(output)) = (result_kind, output) {
                    let width = crate::allocated_values::value_width(&output.ty, &types, &mut BTreeSet::new())
                        .ok_or_else(|| invalid("collection result has no checked fixed layout"))?;
                    if scalar_width(&output.ty).is_none() && !storage_bounds.contains_key(entry.id.as_str()) {
                        let lowering = record
                            .entries
                            .iter()
                            .find(|lowering| lowering.id == entry.id)
                            .ok_or_else(|| invalid("result has no owned callable frame"))?;
                        let frame = crate::nested_machine::owned_frame(record, elf, lowering)?;
                        let scalar_end = entry
                            .locals
                            .iter()
                            .map(|local| local.source_id)
                            .max()
                            .unwrap_or(0)
                            .checked_add(1)
                            .and_then(|count| count.checked_mul(8))
                            .and_then(|bytes| u32::try_from(bytes).ok())
                            .ok_or_else(|| invalid("result scalar extent overflows"))?;
                        storage_bounds.insert(entry.id.as_str(), (scalar_end, frame.saturating_sub(16)));
                    }
                    let skip = match shape {
                        MemberKind::OrderedMap { key, .. } if *operation != Operation::EntryKey => {
                            key_width(key).ok_or_else(|| invalid("result has invalid map key width"))?
                        }
                        _ => 0,
                    };
                    let queue = expected.entry((entry.id.as_str(), block.id, Direction::Result, output.source_id, 0)).or_default();
                    if !queue.is_empty() {
                        return Err(invalid("ambiguous collection result destination"));
                    }
                    queue.push_back(Expected {
                        contract,
                        member: *member,
                        offset,
                        pointer,
                        kind,
                        input: None,
                        output: Some(output),
                        skip,
                        width,
                    });
                }
            }
        }
    }
    let mut starts = BTreeMap::new();
    let mut ends = BTreeMap::new();
    let mut protected = BTreeMap::<&str, BTreeSet<(u32, u32)>>::new();
    for (owner, constructors) in constructors {
        for constructor in constructors.values() {
            let start = u32::try_from(constructor.offset).map_err(|_| invalid("result owner offset invalid"))?;
            let end = start
                .checked_add(constructor.extent)
                .and_then(|end| end.checked_add(128))
                .ok_or_else(|| invalid("result owner extent overflows"))?;
            protected.entry(owner.as_str()).or_default().insert((start, end));
        }
    }
    for site in sites {
        if let Some(borrow) = site.borrowed.filter(|_| site.kind != Primitive::Extent) {
            protected.entry(site.owner).or_default().insert((borrow.scratch, borrow.scratch + 128));
        }
        if starts.insert((site.owner, site.kind, site.start), site).is_some()
            || ends.insert((site.owner, site.kind, site.end), site).is_some()
        {
            return Err(invalid("ambiguous primitive transport boundary"));
        }
    }
    let locals = record
        .typed_semantics
        .entries
        .iter()
        .map(|entry| (entry.id.as_str(), entry.locals.iter().map(|local| (local.id, local.source_id)).collect::<BTreeMap<_, _>>()))
        .collect::<BTreeMap<_, _>>();
    let mut incoming = BTreeMap::<u64, Vec<u64>>::new();
    for edge in &elf.control_flow {
        incoming.entry(edge.target).or_default().push(edge.address);
    }
    let mut blocks = record.blocks.iter().collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    let mut typed_blocks = BTreeMap::<&str, BTreeMap<u64, u32>>::new();
    for block in &blocks {
        if let Some(id) = block.lowering_block_id {
            typed_blocks.entry(block.owner_entry.as_str()).or_default().insert(block.range.start, id);
        }
    }
    let mut result_regions = BTreeMap::<(&str, u64), (u32, u32)>::new();
    for stage in stages(record)? {
        let coordinate = stage.coordinate;
        let input = coordinate.direction == Direction::Index;
        let key = (stage.owner, stage.block, coordinate.direction, coordinate.source, if input { coordinate.coordinate } else { 0 });
        let expected =
            expected.get_mut(&key).and_then(VecDeque::pop_front).ok_or_else(|| invalid("extra or unbound collection transport"))?;
        let site = if input {
            starts.get(&(stage.owner, expected.kind, stage.end))
        } else {
            ends.get(&(stage.owner, expected.kind, stage.start))
        }
        .copied()
        .ok_or_else(|| invalid("transport is detached from its checked primitive"))?;
        if site.offset != expected.offset
            || site.contract != expected.contract
            || site.borrowed.map(|borrow| borrow.pointer) != expected.pointer
            || site.descriptor != Some(expected.offset + 32 + expected.contract.bytes + expected.member * 32)
        {
            return Err(invalid("transport uses a different owner or member"));
        }
        let mut c = Cursor::new(stage.start, stage.end, machine.words, machine.targets);
        if typed_blocks.get(stage.owner).and_then(|blocks| blocks.range(..=site.start).next_back()).map(|(_, id)| *id)
            != Some(stage.block)
        {
            return Err(invalid("transport producer belongs to another typed block"));
        }
        if let Some(operand) = expected.input {
            if let Some(local) = operand.local {
                let source = *locals[stage.owner].get(&local).ok_or_else(|| invalid("index source missing"))?;
                c.stack_load(11, slot(source)?)?;
            } else {
                let value = match &operand.constant {
                    Some(
                        TypedSemanticConstant::U8(value)
                        | TypedSemanticConstant::U16(value)
                        | TypedSemanticConstant::U32(value)
                        | TypedSemanticConstant::U64(value),
                    ) => value,
                    _ => return Err(invalid("index has no supported scalar origin")),
                };
                c.constant(11, value.parse().map_err(|_| invalid("index constant invalid"))?)?;
            }
        } else if let Some(output) = expected.output {
            let offset = site.scratch() + 64 + expected.skip;
            if coordinate.coordinate != offset || coordinate.width != expected.width {
                return Err(invalid("collection result has wrong copy layout"));
            }
            if scalar_width(&output.ty).is_some() {
                if coordinate.output.is_some() {
                    return Err(invalid("scalar result cannot publish a buffer pointer"));
                }
                c.stack_address(29, offset)?;
                c.constant(5, 0)?;
                for byte in 0..expected.width {
                    c.load(7, 29, byte, true)?;
                    if byte != 0 {
                        c.immediate(0x13, 1, 7, 7, (byte * 8) as i32)?;
                    }
                    c.bit_or(5, 5, 7)?;
                }
                if output.ty == "i32" {
                    c.immediate(0x13, 1, 5, 5, 32)?;
                    c.immediate(0x13, 5, 5, 5, 0x400 + 32)?;
                }
            } else {
                let destination = coordinate.output.ok_or_else(|| invalid("fixed result has no buffer destination"))?;
                let end = destination.checked_add(expected.width).ok_or_else(|| invalid("result buffer overflows"))?;
                let (scalar_end, frame_end) = storage_bounds[stage.owner];
                if destination < scalar_end || end > frame_end {
                    return Err(invalid("result buffer overlaps scalar slots or leaves its private frame"));
                }
                for &(start, limit) in protected.get(stage.owner).into_iter().flatten() {
                    work += 1;
                    if work > 4_000_000 || (destination < limit && start < end) {
                        return Err(invalid(format!("result buffer {destination}..{end} overlaps arena/scratch {start}..{limit} in {} or exceeds its proof budget", stage.owner)));
                    }
                }
                for (&(owner, source), &(start, limit)) in &result_regions {
                    work += 1;
                    if work > 4_000_000 || (owner == stage.owner && source != output.source_id && destination < limit && start < end) {
                        return Err(invalid("distinct copied results overlap or exceed the proof budget"));
                    }
                }
                if result_regions
                    .insert((stage.owner, output.source_id), (destination, end))
                    .is_some_and(|old| old != (destination, end))
                {
                    return Err(invalid("fixed result changes its buffer between publications"));
                }
                for byte in 0..expected.width {
                    c.stack_byte(5, offset + byte, false)?;
                    c.stack_byte(5, destination + byte, true)?;
                }
                c.stack_address(5, destination)?;
            }
            c.stack_store(5, slot(output.source_id)?)?;
        }
        c.finish()?;
        let (start, end) = if input { (stage.start, site.end) } else { (site.start, stage.end) };
        let guarded_start = stage.start + if input { 1 } else { 0 };
        for sources in incoming.range(guarded_start..end).map(|(_, sources)| sources) {
            for source in sources {
                work += 1;
                if work > 4_000_000 || !(start..end).contains(source) {
                    return Err(invalid("control flow bypasses collection transport or exceeds its proof budget"));
                }
            }
        }
        let first = blocks.partition_point(|block| block.range.end <= start);
        for block in blocks[first..].iter().take_while(|block| block.range.start < end) {
            work += 1;
            if work > 4_000_000 || block.owner_entry != stage.owner || block.lowering_block_id.is_some_and(|id| id != stage.block) {
                return Err(invalid("collection transport leaves its entry or exceeds its proof budget"));
            }
        }
    }
    if expected.values().any(|queue| !queue.is_empty()) {
        return Err(invalid("missing typed collection transport"));
    }
    Ok(())
}
