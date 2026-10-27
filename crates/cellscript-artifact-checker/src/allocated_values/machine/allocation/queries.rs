//! Descriptor queries and publication of their scalar results. A query's
//! producer is independently recognized before its adjacent store is trusted.
use super::*;
use crate::TypedSemanticType;
use std::collections::VecDeque;

impl Machine<'_> {
    pub(super) fn query(&self, site: &Site<'_>, types: &BTreeMap<&str, &TypedSemanticType>) -> Result<(), CheckerError> {
        let (descriptor, width, limit) = descriptor::layout(site, types)?;
        let mut cursor = Cursor::new(site.start, site.end, self.words, self.targets);
        self.check_descriptor(&mut cursor, site, descriptor, width, limit)?;
        match site.kind {
            Primitive::Len => cursor.addi(10, 6, 0)?,
            Primitive::Capacity => cursor.addi(10, 7, 0)?,
            Primitive::Empty => cursor.immediate(0x13, 3, 10, 6, 1)?,
            _ => return Err(invalid("unknown descriptor query")),
        }
        cursor.finish()
    }
}

fn slot(source: u64) -> Result<u32, CheckerError> {
    source.checked_mul(8).and_then(|value| u32::try_from(value).ok()).ok_or_else(|| invalid("query slot overflows"))
}

pub(super) fn validate(
    record: &VerifiedLoweringRecord,
    elf: &ParsedElf,
    machine: &Machine<'_>,
    constructors: &BTreeMap<String, BTreeMap<u64, Constructor>>,
    sites: &[Site<'_>],
) -> Result<(), CheckerError> {
    // A find nested inside put/lookup/erase is not a source contains result.
    // Only independently checked top-level query producers publish here.
    let mut roots = sites.iter().collect::<Vec<_>>();
    roots.sort_by_key(|site| (site.owner, site.start, std::cmp::Reverse(site.end)));
    let mut queries = BTreeMap::new();
    let mut enclosing: Option<(&str, u64)> = None;
    for site in roots {
        if enclosing.is_some_and(|(owner, end)| owner == site.owner && site.start < end) {
            continue;
        }
        enclosing = Some((site.owner, site.end));
        if matches!(site.kind, Primitive::Len | Primitive::Capacity | Primitive::Empty | Primitive::Find) {
            queries
                .entry((site.owner, site.block, site.kind, site.offset, site.descriptor, site.borrowed.map(|borrow| borrow.pointer)))
                .or_insert_with(VecDeque::new)
                .push_back(site);
        }
    }
    let owner_types = crate::allocated_values::owner_types(&record.typed_semantics);
    let mut blocks = record.blocks.iter().collect::<Vec<_>>();
    blocks.sort_by_key(|block| block.range.start);
    let mut current = BTreeMap::new();
    let mut locations = BTreeMap::new();
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
    let mut work = 0usize;
    for entry in &record.typed_semantics.entries {
        let mut frame = None;
        let aliases = super::super::owner_aliases(entry, &owner_types)?;
        let locals = entry.locals.iter().map(|local| (local.id, local)).collect::<BTreeMap<_, _>>();
        let parameters = entry.params.iter().map(|param| (param.binding_id, param)).collect::<BTreeMap<_, _>>();
        let owned = constructors
            .get(&entry.id)
            .into_iter()
            .flat_map(|sites| sites.values())
            .map(|site| (site.local, site))
            .collect::<BTreeMap<_, _>>();
        for block in &entry.blocks {
            for op in &block.operations {
                let TypedSemanticOperationDetail::AllocatedValue { operation, contract, member: Some(member) } = &op.detail else {
                    continue;
                };
                let kind = match operation {
                    Operation::Len => Primitive::Len,
                    Operation::Capacity => Primitive::Capacity,
                    Operation::IsEmpty => Primitive::Empty,
                    Operation::Contains => Primitive::Find,
                    _ => continue,
                };
                let local = op.operands.first().and_then(|operand| operand.local).ok_or_else(|| invalid("query owner missing"))?;
                let mut root = local;
                let mut visited = BTreeSet::new();
                while !owned.contains_key(&root) && !parameters.contains_key(&root) {
                    work += 1;
                    if work > 4_000_000 || !visited.insert(root) {
                        return Err(invalid("cyclic or excessive query owner aliases"));
                    }
                    root = *aliases
                        .get(&root)
                        .filter(|sources| sources.len() == 1)
                        .and_then(|sources| sources.first())
                        .ok_or_else(|| invalid("ambiguous query owner"))?;
                }
                let (offset, pointer) = if let Some(constructor) = owned.get(&root) {
                    (u32::try_from(constructor.offset).map_err(|_| invalid("query owner offset overflows"))?, None)
                } else {
                    if entry.kind != "helper"
                        || !parameters
                            .get(&root)
                            .is_some_and(|param| !param.mutable && param.ty.strip_prefix('&') == Some(contract.owner.as_str()))
                    {
                        return Err(invalid("query lacks a readonly parameter root"));
                    }
                    (0, Some(slot(locals.get(&local).ok_or_else(|| invalid("query local missing"))?.source_id)?))
                };
                let descriptor = offset
                    .checked_add(32)
                    .and_then(|value| value.checked_add(contract.bytes))
                    .and_then(|value| member.checked_mul(32).and_then(|member| value.checked_add(member)))
                    .ok_or_else(|| invalid("query descriptor overflows"))?;
                let site = queries
                    .get_mut(&(entry.id.as_str(), block.id, kind, offset, Some(descriptor), pointer))
                    .and_then(VecDeque::pop_front)
                    .filter(|site| site.contract == contract)
                    .ok_or_else(|| invalid("typed query has no checked producer"))?;
                let output = op.destinations.first().and_then(|id| locals.get(id)).ok_or_else(|| invalid("query result missing"))?;
                let expected_type = if matches!(kind, Primitive::Len | Primitive::Capacity) { "u64" } else { "bool" };
                if output.ty != expected_type {
                    return Err(invalid("query result type differs"));
                }
                let frame = if let Some(frame) = frame {
                    frame
                } else {
                    let lowering = record
                        .entries
                        .iter()
                        .find(|candidate| candidate.id == entry.id)
                        .ok_or_else(|| invalid("query caller frame missing"))?;
                    let bytes = crate::nested_machine::owned_frame(record, elf, lowering)?;
                    frame = Some(bytes);
                    bytes
                };
                let destination = slot(output.source_id)?;
                if destination.checked_add(8).is_none_or(|end| end > frame.saturating_sub(16)) {
                    return Err(invalid("query result leaves the private frame"));
                }
                let mut cursor = Cursor::new(site.end, site.end + 256, machine.words, machine.targets);
                cursor.stack_store(10, destination)?;
                for address in (site.end..cursor.pc).step_by(4) {
                    work += 1;
                    if work > 4_000_000
                        || locations.range(..=address).next_back().map(|(_, location)| *location)
                            != Some((entry.id.as_str(), Some(block.id)))
                    {
                        return Err(invalid("query result leaves its owner or proof budget"));
                    }
                }
                for (_, sources) in incoming.range(site.end..cursor.pc) {
                    work += sources.len();
                    if work > 4_000_000 || sources.iter().any(|source| !(site.start..cursor.pc).contains(source)) {
                        return Err(invalid("control flow bypasses the query result producer"));
                    }
                }
            }
        }
    }
    if queries.values().any(|queue| !queue.is_empty()) {
        return Err(invalid("orphan checked query producer"));
    }
    Ok(())
}
