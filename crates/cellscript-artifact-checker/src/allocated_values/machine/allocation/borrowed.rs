//! Read-only caller-relative descriptors, searches and copied elements. Typed
//! inventories survive erased labels; the shared CFG proof checks each site's
//! actual incoming pointer/extent facts. The transport module binds indices and
//! copied scalar destinations; key inputs and wide publication remain separate.
use super::*;
use crate::allocated_values::MemberKind;

pub(super) const PREFIX: &str = ".Lallocated_borrowed_";

pub(super) struct Checked<'a> {
    pub reads: BTreeMap<String, BTreeMap<u64, BorrowedRead>>,
    pub sites: Vec<Site<'a>>,
}

pub(super) fn validate<'a>(
    record: &'a VerifiedLoweringRecord,
    elf: &ParsedElf,
    machine: &Machine<'_>,
    constructors: &BTreeMap<String, BTreeMap<u64, Constructor>>,
) -> Result<Checked<'a>, CheckerError> {
    let owner_types = crate::allocated_values::owner_types(&record.typed_semantics);
    let mut expected = BTreeMap::new();
    let mut frames = BTreeMap::new();
    let mut work = 0usize;
    for entry in &record.typed_semantics.entries {
        let aliases = super::super::owner_aliases(entry, &owner_types)?;
        let params = entry.params.iter().map(|param| (param.binding_id, param)).collect::<BTreeMap<_, _>>();
        let owned =
            constructors.get(&entry.id).into_iter().flat_map(|sites| sites.values()).map(|site| site.local).collect::<BTreeSet<_>>();
        for block in &entry.blocks {
            for op in &block.operations {
                let TypedSemanticOperationDetail::AllocatedValue { operation, contract, member } = &op.detail else { continue };
                if *operation == Operation::New {
                    continue;
                }
                let local = op.operands.first().and_then(|arg| arg.local).ok_or_else(|| invalid("borrowed operation has no local"))?;
                let mut root = local;
                let mut visited = BTreeSet::new();
                while !owned.contains(&root) && !params.contains_key(&root) {
                    work += 1;
                    if work > 4_000_000 || !visited.insert(root) {
                        return Err(invalid("cyclic or excessive borrowed owner aliases"));
                    }
                    root = *aliases
                        .get(&root)
                        .filter(|sources| sources.len() == 1)
                        .and_then(|sources| sources.first())
                        .ok_or_else(|| invalid("borrowed operation has ambiguous owner"))?;
                }
                if owned.contains(&root) {
                    continue;
                }
                if entry.kind != "helper"
                    || !params
                        .get(&root)
                        .is_some_and(|param| !param.mutable && param.ty.strip_prefix('&') == Some(contract.owner.as_str()))
                {
                    return Err(invalid("borrowed read has no readonly parameter"));
                }
                let source = |id| {
                    entry
                        .locals
                        .iter()
                        .find(|value| value.id == id)
                        .map(|value| value.source_id)
                        .ok_or_else(|| invalid("borrowed read source local missing"))
                };
                let pointer = u32::try_from(source(local)?.checked_mul(8).ok_or_else(|| invalid("borrowed pointer slot overflows"))?)
                    .map_err(|_| invalid("borrowed pointer slot exceeds frame profile"))?;
                let root_source = source(root)?;
                let index = member.ok_or_else(|| invalid("borrowed read has no member"))?;
                let member = contract.members.get(index as usize).ok_or_else(|| invalid("borrowed member missing"))?;
                let descriptor = 32 + contract.bytes + index * 32;
                let ordered = !matches!(member.shape, MemberKind::Vec { .. });
                let kinds = match operation {
                    Operation::Len => vec![Primitive::Len],
                    Operation::Capacity => vec![Primitive::Capacity],
                    Operation::IsEmpty => vec![Primitive::Empty],
                    Operation::Contains if ordered => vec![Primitive::Find],
                    Operation::Get if ordered => vec![Primitive::Find, Primitive::Read, Primitive::Lookup, Primitive::Required],
                    Operation::GetOr if ordered => vec![Primitive::Find, Primitive::Read, Primitive::Lookup],
                    Operation::Get | Operation::EntryKey | Operation::EntryValue => vec![Primitive::Read],
                    _ => return Err(invalid("unsupported borrowed collection operation")),
                };
                for kind in std::iter::once(Primitive::Extent).chain(kinds) {
                    let coordinate = (kind != Primitive::Extent).then_some(descriptor);
                    let value = expected.entry((entry.id.as_str(), block.id, pointer, coordinate, kind)).or_insert((
                        0usize,
                        contract,
                        root_source,
                    ));
                    if value.1 != contract || value.2 != root_source {
                        return Err(invalid("borrowed slot has conflicting roots or layouts"));
                    }
                    value.0 += 1;
                }
            }
        }
        if expected.keys().any(|(owner, ..)| *owner == entry.id) {
            let lowering = record
                .entries
                .iter()
                .find(|lowering| lowering.id == entry.id)
                .ok_or_else(|| invalid("borrowed helper has no lowering"))?;
            let frame = crate::nested_machine::owned_frame(record, elf, lowering)?;
            let scalar_end = entry
                .locals
                .iter()
                .map(|local| local.source_id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .and_then(|count| count.checked_mul(8))
                .ok_or_else(|| invalid("borrowed scalar slots overflow"))?;
            frames.insert(entry.id.as_str(), (scalar_end, frame));
        }
    }
    let mut boundaries = BTreeMap::new();
    let mut current = BTreeMap::new();
    let mut blocks = record.blocks.iter().collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    for block in blocks {
        if let Some(id) = block.lowering_block_id {
            current.insert(block.owner_entry.as_str(), id);
        }
        for label in &block.allocated_lifecycle_labels {
            let Some(suffix) = label.strip_prefix(PREFIX) else { continue };
            let parts = suffix.split('_').collect::<Vec<_>>();
            if parts.len() != 6 || !matches!(parts[5], "begin" | "end") {
                return Err(invalid("invalid borrowed read marker"));
            }
            let kind = match parts[0] {
                "extent" => Primitive::Extent,
                "len" => Primitive::Len,
                "capacity" => Primitive::Capacity,
                "empty" => Primitive::Empty,
                "find" => Primitive::Find,
                "read" => Primitive::Read,
                "lookup" => Primitive::Lookup,
                "required" => Primitive::Required,
                _ => return Err(invalid("unknown borrowed read primitive")),
            };
            let number = |index: usize| parts[index].parse::<u32>().map_err(|_| invalid("invalid borrowed marker coordinate"));
            let (pointer, coordinate, scratch) = (number(1)?, number(2)?, number(3)?);
            let id = parts[4].parse::<u64>().map_err(|_| invalid("invalid borrowed marker identity"))?;
            let owner = block.owner_entry.as_str();
            let typed = *current.get(owner).ok_or_else(|| invalid("borrowed read outside typed body"))?;
            if boundaries.insert((owner, pointer, coordinate, scratch, kind, id, parts[5]), (block.range.start, typed)).is_some() {
                return Err(invalid("duplicate borrowed read boundary"));
            }
        }
    }
    let mut checked = BTreeSet::new();
    let mut scratch_by_owner = BTreeMap::new();
    let mut result = BTreeMap::<String, BTreeMap<u64, BorrowedRead>>::new();
    let mut sites = Vec::new();
    for (&(owner, pointer, coordinate, scratch, kind, id, edge), &(start, typed)) in &boundaries {
        if edge != "begin" {
            continue;
        }
        let end_key = (owner, pointer, coordinate, scratch, kind, id, "end");
        let &(end, end_typed) = boundaries.get(&end_key).ok_or_else(|| invalid("borrowed read end missing"))?;
        if end <= start || end_typed != typed || end - start > 131_072 {
            return Err(invalid("invalid borrowed read range"));
        }
        let descriptor = (kind != Primitive::Extent).then_some(coordinate);
        let (remaining, contract, source) = expected
            .get_mut(&(owner, typed, pointer, descriptor, kind))
            .filter(|(count, ..)| *count > 0)
            .ok_or_else(|| invalid("extra or unbound borrowed read"))?;
        *remaining -= 1;
        if record.blocks.iter().any(|block| block.range.start < end && start < block.range.end && block.owner_entry != owner) {
            return Err(invalid("borrowed read leaves its helper"));
        }
        let (scalar_end, frame) = frames[owner];
        if kind == Primitive::Extent {
            if scratch != 0 || u64::from(coordinate) + 8 > u64::from(frame).saturating_sub(16) {
                return Err(invalid("invalid borrowed extent slot"));
            }
            let mut c = Cursor::new(start, end, machine.words, machine.targets);
            c.stack_load(5, coordinate)?;
            c.constant(6, u64::from(32 + contract.bytes + contract.members.len() as u32 * 32))?;
            let done = c.target(end);
            c.branch(0, 5, 6, done)?;
            machine.failure(c.jump_target()?)?;
            c.finish()?;
        } else {
            if u64::from(scratch) < scalar_end
                || !scratch.is_multiple_of(16)
                || u64::from(scratch) + 128 > u64::from(frame).saturating_sub(16)
                || constructors.get(owner).into_iter().flat_map(|sites| sites.values()).any(|site| {
                    i64::from(scratch) < site.offset + i64::from(site.extent) + 128 && site.offset < i64::from(scratch) + 128
                })
                || scratch_by_owner.insert(owner, scratch).is_some_and(|previous| previous != scratch)
            {
                return Err(invalid("borrowed copy scratch is not one disjoint private buffer"));
            }
        }
        let read = BorrowedRead { pointer, source: *source, extent_slot: (kind == Primitive::Extent).then_some(coordinate) };
        if let Some(previous) = result.entry(owner.to_string()).or_default().insert(start, read)
            && (previous.pointer != pointer || previous.source != *source || previous.extent_slot.is_some())
        {
            return Err(invalid("borrowed read boundaries disagree on their source"));
        }
        sites.push(Site { start, end, owner, offset: 0, descriptor, kind, contract, borrowed: Some(Borrowed { pointer, scratch }) });
        checked.insert((owner, pointer, coordinate, scratch, kind, id, edge));
        checked.insert(end_key);
    }
    if checked.len() != boundaries.len() || expected.values().any(|(count, ..)| *count != 0) {
        return Err(invalid("missing or orphan borrowed read"));
    }
    let types = record.typed_semantics.types.iter().map(|ty| (ty.name.as_str(), ty)).collect();
    for site in sites.iter().filter(|site| site.kind.tier() == 3) {
        match site.kind {
            Primitive::Read => machine.elements(site, &BTreeMap::new(), &mut BTreeSet::new(), &types)?,
            Primitive::Find => machine.find(site, &types)?,
            Primitive::Len | Primitive::Capacity | Primitive::Empty => {
                let (descriptor, width, limit) = descriptor::layout(site, &types)?;
                let mut c = Cursor::new(site.start, site.end, machine.words, machine.targets);
                machine.check_descriptor(&mut c, site, descriptor, width, limit)?;
                match site.kind {
                    Primitive::Len => c.addi(10, 6, 0)?,
                    Primitive::Capacity => c.addi(10, 7, 0)?,
                    _ => c.immediate(0x13, 3, 10, 6, 1)?,
                }
                c.finish()?;
            }
            _ => return Err(invalid("unknown borrowed read body")),
        }
    }
    let children = sites.iter().filter(|site| site.kind.tier() == 3).map(|site| (site.start, site)).collect();
    let mut consumed = BTreeSet::new();
    for site in sites.iter().filter(|site| site.kind == Primitive::Lookup) {
        machine.read_operation(site, &children, &mut consumed, &types)?;
    }
    let children = sites.iter().filter(|site| site.kind == Primitive::Lookup).map(|site| (site.start, site)).collect();
    let mut consumed = BTreeSet::new();
    for site in sites.iter().filter(|site| site.kind == Primitive::Required) {
        machine.read_operation(site, &children, &mut consumed, &types)?;
    }
    for tier in [0, 3, 4, 5] {
        let mut ranges = sites.iter().filter(|site| site.kind.tier() == tier).map(|site| (site.start, site.end)).collect::<Vec<_>>();
        ranges.sort_unstable();
        if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            return Err(invalid("overlapping borrowed read bodies"));
        }
        for edge in &elf.control_flow {
            let index = ranges.partition_point(|(start, _)| *start < edge.target);
            if let Some(&(start, end)) = index.checked_sub(1).and_then(|index| ranges.get(index))
                && edge.target < end
                && !(start..end).contains(&edge.address)
            {
                return Err(invalid("control flow enters a partial borrowed read"));
            }
        }
    }
    Ok(Checked { reads: result, sites })
}
