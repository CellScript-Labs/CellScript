//! Independent typed-CFG owner availability. This does not bind allocator
//! machine instructions; the composing checker also requires allocation machine
//! contracts and memory-effect lifetime checks.
use super::{Operation, TypedSemanticOperationDetail};
use crate::{CheckerError, CheckerRejectionCode, TypedSemanticEntry, TypedSemanticOperation, TypedSemanticRecord};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Handle {
    root: u32,
    borrowed: bool,
    loans: BTreeSet<u32>,
}

#[derive(Clone, Default, PartialEq, Eq)]
struct State {
    available: BTreeMap<u32, Handle>,
    // Availability is a must property; active lexical loans are a may property.
    // A loan on either predecessor must still prohibit mutation at a join.
    loans: BTreeSet<(u32, u32)>,
}

fn invalid(message: impl Into<String>) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2419TypedSemanticsInvalid, format!("allocated owner lifetime: {}", message.into()))
}

impl State {
    fn cells(&self) -> usize {
        self.loans.len() + self.available.values().map(|handle| 1 + handle.loans.len()).sum::<usize>()
    }

    fn get(&self, local: u32) -> Result<Handle, CheckerError> {
        self.available.get(&local).cloned().ok_or_else(|| invalid(format!("local {local} is uninitialized, moved, dropped or stale")))
    }

    fn mutate(&mut self, handle: &Handle, retire: bool) -> Result<(), CheckerError> {
        if handle.borrowed || self.loans.iter().any(|(_, root)| *root == handle.root) {
            return Err(invalid("mutation, move or drop while borrowed"));
        }
        self.available.retain(|_, value| value.root != handle.root || (!retire && !value.borrowed));
        Ok(())
    }

    fn merge(&mut self, other: &Self) -> bool {
        let before = self.clone();
        self.available.retain(|local, handle| other.available.get(local) == Some(handle));
        self.loans.extend(&other.loans);
        *self != before
    }
}

struct Context<'a> {
    entry: &'a TypedSemanticEntry,
    owner_types: &'a BTreeSet<&'a str>,
    locals: BTreeMap<u32, &'a str>,
    starts: BTreeMap<(u32, u32), Vec<(u32, u32)>>,
    ends: BTreeMap<(u32, u32), Vec<u32>>,
}

impl Context<'_> {
    fn is_owner(&self, ty: &str) -> bool {
        self.owner_types.contains(ty.strip_prefix('&').unwrap_or(ty))
    }

    fn operation(&self, state: &mut State, op: &TypedSemanticOperation) -> Result<(), CheckerError> {
        let owner_args = op.operands.iter().filter(|arg| self.is_owner(&arg.ty)).collect::<Vec<_>>();
        let owner_dests =
            op.destinations.iter().filter(|id| self.locals.get(id).is_some_and(|ty| self.is_owner(ty))).collect::<Vec<_>>();
        if owner_args.is_empty() && owner_dests.is_empty() {
            return Ok(());
        }
        // A typed local reference is required even when an outer type/hash is
        // rebound to make a constant or unrelated value look like an owner.
        let handles = owner_args
            .iter()
            .map(|arg| state.get(arg.local.ok_or_else(|| invalid("owner operand is not a local"))?))
            .collect::<Result<Vec<_>, _>>()?;
        if let TypedSemanticOperationDetail::AllocatedValue { operation, .. } = &op.detail {
            if *operation == Operation::New {
                let dest = *op.destinations.first().ok_or_else(|| invalid("constructor has no destination"))?;
                if state.available.get(&dest).is_some_and(|old| old.root != dest) {
                    return Err(invalid("constructor overwrites another owner's handle"));
                }
                // The same constructor may execute on each loop iteration.
                // Retire every previous handle to its private storage first.
                let handle = Handle { root: dest, borrowed: false, loans: BTreeSet::new() };
                state.mutate(&handle, true)?;
                state.available.insert(dest, handle);
                return Ok(());
            }
            let handle = handles.first().ok_or_else(|| invalid("collection operation has no live owner"))?;
            if !matches!(
                operation,
                Operation::Len
                    | Operation::Capacity
                    | Operation::IsEmpty
                    | Operation::Get
                    | Operation::GetOr
                    | Operation::Contains
                    | Operation::EntryKey
                    | Operation::EntryValue
            ) {
                state.mutate(handle, *operation == Operation::Drop)?;
            }
            return Ok(());
        }
        if op.opcode == "move" && handles.len() == 1 && owner_dests.len() == 1 {
            let handle = handles[0].clone();
            let dest = *owner_dests[0];
            if state.available.get(&dest).is_some_and(|old| !old.borrowed) {
                return Err(invalid("move overwrites a live owner"));
            }
            if !handle.borrowed {
                if self.locals[&dest].starts_with('&') {
                    return Err(invalid("an owner move cannot fabricate a borrowed handle"));
                }
                state.mutate(&handle, true)?;
            }
            state.available.insert(dest, handle);
            return Ok(());
        }
        if op.opcode == "unary"
            && handles.len() == 1
            && owner_dests.len() == 1
            && matches!(&op.detail, TypedSemanticOperationDetail::UnaryOperator { operator } if operator == "ref")
        {
            if state.available.get(owner_dests[0]).is_some_and(|old| !old.borrowed) {
                return Err(invalid("reference overwrites a live owner"));
            }
            let mut handle = handles[0].clone();
            handle.borrowed = true;
            state.available.insert(*owner_dests[0], handle);
            return Ok(());
        }
        if op.opcode == "call" && owner_dests.is_empty() {
            let call = op.call.as_ref().ok_or_else(|| invalid("owner call has no contract"))?;
            if call.contract != "typed-local" {
                return Err(invalid("owner reference escapes to an external call"));
            }
            for (arg, expected) in op.operands.iter().zip(&call.params) {
                if self.is_owner(&arg.ty) {
                    let handle = state.get(arg.local.ok_or_else(|| invalid("call owner is not a local"))?)?;
                    if !handle.borrowed || expected != &format!("&{}", arg.ty.strip_prefix('&').unwrap_or(&arg.ty)) {
                        return Err(invalid("helpers accept only checked read-only owner references"));
                    }
                }
            }
            return Ok(());
        }
        Err(invalid(format!("owner escapes or is fabricated by '{}' in {}", op.opcode, self.entry.id)))
    }

    fn events(&self, state: &mut State, block: u32, position: u32) -> Result<(), CheckerError> {
        if let Some(bindings) = self.ends.get(&(block, position)) {
            for binding in bindings {
                state.available.retain(|_, handle| !handle.loans.contains(binding));
                state.loans.retain(|(loan, _)| loan != binding);
            }
        }
        if let Some(bindings) = self.starts.get(&(block, position)) {
            for (binding, root) in bindings {
                let mut handle = state.get(*root)?;
                handle.borrowed = true;
                // Regions canonicalize nested roots to the original owner.
                // Preserve enclosing loans even when the immediate alias no
                // longer appears as an operation operand in the typed record.
                handle.loans.extend(state.loans.iter().filter(|(_, root)| *root == handle.root).map(|(loan, _)| *loan));
                handle.loans.insert(*binding);
                state.loans.insert((*binding, handle.root));
                state.available.insert(*binding, handle);
            }
        }
        Ok(())
    }
}

pub(crate) fn owner_types(typed: &TypedSemanticRecord) -> BTreeSet<&str> {
    typed
        .entries
        .iter()
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .filter_map(|op| match &op.detail {
            TypedSemanticOperationDetail::AllocatedValue { contract, .. } => Some(contract.owner.as_str()),
            _ => None,
        })
        .collect()
}

pub(crate) fn validate(entry: &TypedSemanticEntry, owner_types: &BTreeSet<&str>) -> Result<(), CheckerError> {
    let mut context = Context {
        entry,
        owner_types,
        locals: entry.locals.iter().map(|local| (local.id, local.ty.as_str())).collect(),
        starts: BTreeMap::new(),
        ends: BTreeMap::new(),
    };
    if !context.locals.values().any(|ty| context.is_owner(ty)) {
        return Ok(());
    }
    if context.is_owner(&entry.return_type) {
        return Err(invalid("owner or borrowed owner return escapes its invocation"));
    }
    let mut initial = State::default();
    for param in &entry.params {
        if context.is_owner(&param.ty) {
            if entry.kind != "helper" || !param.ty.starts_with('&') || param.mutable {
                return Err(invalid("owner parameters must be read-only helper references"));
            }
            initial.available.insert(param.binding_id, Handle { root: param.binding_id, borrowed: true, loans: BTreeSet::new() });
        }
    }
    for borrow in &entry.borrows {
        if !context.is_owner(&borrow.root_type) {
            continue;
        }
        let local = |name: &str, ty: &str| {
            entry
                .locals
                .iter()
                .find(|local| local.name == name && local.ty.trim_start_matches('&') == ty.trim_start_matches('&'))
                .map(|local| local.id)
                .ok_or_else(|| invalid("lexical loan has no matching local"))
        };
        let root = local(&borrow.root, &borrow.root_type)?;
        let binding = local(&borrow.binding, &borrow.view_type)?;
        context.starts.entry((borrow.start_block, borrow.start_operation)).or_default().push((binding, root));
        if let (Some(block), Some(position)) = (borrow.end_block, borrow.end_operation) {
            context.ends.entry((block, position)).or_default().push(binding);
        }
    }
    let blocks = entry.blocks.iter().map(|block| (block.id, block)).collect::<BTreeMap<_, _>>();
    let mut resident = initial.cells();
    let mut incoming = BTreeMap::from([(entry.entry_block, initial)]);
    let mut pending = BTreeSet::from([entry.entry_block]);
    // Bound both aggregate work and resident data; never accept by abandoning
    // a difficult path. Existing typed structural checks precede this analysis.
    let mut work = 0usize;
    while let Some(id) = pending.pop_first() {
        let block = blocks.get(&id).ok_or_else(|| invalid("missing CFG block"))?;
        let mut state = incoming[&id].clone();
        for position in 0..=block.operations.len() {
            work = work.saturating_add(1 + state.cells());
            if work > 4_000_000 {
                return Err(invalid("typed lifetime analysis work budget exceeded"));
            }
            context.events(&mut state, id, position as u32)?;
            if let Some(op) = block.operations.get(position) {
                context.operation(&mut state, op)?;
            }
        }
        for next in &block.successors {
            if let Some(previous) = incoming.get_mut(next) {
                resident -= previous.cells();
                if previous.merge(&state) {
                    pending.insert(*next);
                }
                resident += previous.cells();
            } else {
                resident += state.cells();
                incoming.insert(*next, state.clone());
                pending.insert(*next);
            }
            if resident > 1_000_000 {
                return Err(invalid("typed lifetime resident state budget exceeded"));
            }
        }
    }
    Ok(())
}
