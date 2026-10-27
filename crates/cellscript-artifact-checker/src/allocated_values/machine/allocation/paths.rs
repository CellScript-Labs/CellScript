//! Bind top-level checked primitives to source operations. These events join
//! scalar/Cell events in the shared successful-path proof after all independent
//! instruction and lifetime checks finish. Nested primitive children are not
//! independently skippable source operations.
use super::*;
use crate::cell_selection::machine::flow::operation_position;
use std::collections::VecDeque;

type Key<'a> = (&'a str, u32, Primitive, u32, Option<u32>, Option<u32>);

fn kind(operation: Operation, ordered: bool, op: &crate::TypedSemanticOperation) -> Result<Primitive, CheckerError> {
    Ok(match operation {
        Operation::Len => Primitive::Len,
        Operation::Capacity => Primitive::Capacity,
        Operation::IsEmpty => Primitive::Empty,
        Operation::Reserve => Primitive::Reserve,
        Operation::Push => Primitive::Push,
        Operation::Insert if ordered => Primitive::Put,
        Operation::Insert => Primitive::Insert,
        Operation::Set => Primitive::Set,
        Operation::Remove if ordered => Primitive::Erase,
        Operation::Remove => Primitive::Take,
        Operation::Pop => Primitive::Pop,
        Operation::Contains => Primitive::Find,
        Operation::Get if ordered => Primitive::Required,
        Operation::Get | Operation::EntryKey | Operation::EntryValue => Primitive::Read,
        Operation::GetOr => Primitive::Lookup,
        Operation::LoadSorted => Primitive::Sorted(
            op.operands
                .get(1)
                .and_then(|arg| arg.ty.strip_suffix(']'))
                .and_then(|ty| ty.rsplit_once("; "))
                .and_then(|(_, count)| count.parse().ok())
                .filter(|count| *count <= 1024)
                .ok_or_else(|| invalid("source sorted operation has no fixed count"))?,
        ),
        Operation::New | Operation::Drop => return Err(invalid("owner lifecycle is not a member operation")),
    })
}

pub(super) fn sites(
    record: &VerifiedLoweringRecord,
    constructors: &BTreeMap<String, BTreeMap<u64, Constructor>>,
    sites: &[Site<'_>],
) -> Result<FlowSites, CheckerError> {
    let mut sorted = sites.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|site| (site.owner, site.start, std::cmp::Reverse(site.end)));
    let mut roots = BTreeMap::<Key<'_>, VecDeque<&Site<'_>>>::new();
    let mut enclosing: Option<&Site<'_>> = None;
    for site in sorted {
        if let Some(parent) = enclosing.filter(|parent| parent.owner == site.owner && site.start < parent.end) {
            if site.end > parent.end || site.block != parent.block {
                return Err(invalid("source operation has partially overlapping primitive children"));
            }
            continue;
        }
        enclosing = Some(site);
        roots
            .entry((site.owner, site.block, site.kind, site.offset, site.descriptor, site.borrowed.map(|borrow| borrow.pointer)))
            .or_default()
            .push_back(site);
    }
    let mut result = FlowSites::new();
    let owner_types = crate::allocated_values::owner_types(&record.typed_semantics);
    let mut work = 0usize;
    for entry in &record.typed_semantics.entries {
        let aliases = super::super::owner_aliases(entry, &owner_types)?;
        let locals = entry.locals.iter().map(|local| (local.id, local)).collect::<BTreeMap<_, _>>();
        let params = entry.params.iter().map(|param| (param.binding_id, param)).collect::<BTreeMap<_, _>>();
        let owned = constructors
            .get(&entry.id)
            .into_iter()
            .flat_map(|sites| sites.iter())
            .map(|(&start, site)| (site.local, (start, site)))
            .collect::<BTreeMap<_, _>>();
        for block in &entry.blocks {
            for op in &block.operations {
                let TypedSemanticOperationDetail::AllocatedValue { operation, contract, member } = &op.detail else { continue };
                if *operation == Operation::New {
                    let (start, site) = op
                        .destinations
                        .first()
                        .and_then(|id| owned.get(id))
                        .ok_or_else(|| invalid("source constructor has no checked initializer"))?;
                    result.insert((entry.id.clone(), block.id, operation_position(op.index, 0)?), (*start, site.end, 0));
                    continue;
                }
                let local = op.operands.first().and_then(|arg| arg.local).ok_or_else(|| invalid("source operation has no owner"))?;
                let mut root = local;
                let mut visited = BTreeSet::new();
                while !owned.contains_key(&root) && !params.contains_key(&root) {
                    work += 1;
                    if work > 4_000_000 || !visited.insert(root) {
                        return Err(invalid("cyclic or excessive source owner aliases"));
                    }
                    root = *aliases
                        .get(&root)
                        .filter(|sources| sources.len() == 1)
                        .and_then(|sources| sources.first())
                        .ok_or_else(|| invalid("source operation has ambiguous owner aliases"))?;
                }
                let (offset, pointer) = if let Some((_, site)) = owned.get(&root) {
                    (u32::try_from(site.offset).map_err(|_| invalid("source owner offset overflows"))?, None)
                } else {
                    if entry.kind != "helper"
                        || !params
                            .get(&root)
                            .is_some_and(|param| !param.mutable && param.ty.strip_prefix('&') == Some(contract.owner.as_str()))
                    {
                        return Err(invalid("source operation lacks a readonly parameter root"));
                    }
                    let slot = locals
                        .get(&local)
                        .and_then(|local| local.source_id.checked_mul(8))
                        .and_then(|offset| u32::try_from(offset).ok())
                        .ok_or_else(|| invalid("source pointer slot overflows"))?;
                    (0, Some(slot))
                };
                let mut events = Vec::new();
                if pointer.is_some() {
                    events.push((Primitive::Extent, None));
                }
                if *operation == Operation::Drop {
                    events.extend((0..contract.members.len() as u32).map(|member| (Primitive::Drop, Some(member))));
                } else {
                    let member = member
                        .filter(|member| (*member as usize) < contract.members.len())
                        .ok_or_else(|| invalid("source member operation has no declared member"))?;
                    let ordered = !matches!(contract.members[member as usize].shape, crate::allocated_values::MemberKind::Vec { .. });
                    events.push((kind(*operation, ordered, op)?, Some(member)));
                }
                for (part, (kind, member)) in events.into_iter().enumerate() {
                    let descriptor = member
                        .map(|member| {
                            offset
                                .checked_add(32)
                                .and_then(|value| value.checked_add(contract.bytes))
                                .and_then(|value| member.checked_mul(32).and_then(|member| value.checked_add(member)))
                                .ok_or_else(|| invalid("source operation descriptor overflows"))
                        })
                        .transpose()?;
                    let site = roots
                        .get_mut(&(entry.id.as_str(), block.id, kind, offset, descriptor, pointer))
                        .and_then(VecDeque::pop_front)
                        .filter(|site| site.contract == contract)
                        .ok_or_else(|| invalid("source operation has no complete checked primitive"))?;
                    result.insert((entry.id.clone(), block.id, operation_position(op.index, part as u32)?), (site.start, site.end, 0));
                }
            }
        }
    }
    if roots.values().any(|queue| !queue.is_empty()) {
        return Err(invalid("checked primitive has no source operation"));
    }
    Ok(result)
}
