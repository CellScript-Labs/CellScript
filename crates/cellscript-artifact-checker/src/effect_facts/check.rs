//! Independent reconstruction from checked typed operations. Neither the
//! producer's entry facts nor its call summaries participate in this analysis.
use super::runtime::{ByteSourceKind, RuntimeArgumentFacts, RuntimeEffectContract};
use super::{CanonicalSet, ContextAccess as Access, ContextSource as Source, EffectFacts, LifecycleOperation as Life};
use crate::runtime_contract::RuntimeHelper;
use crate::{
    CellBindingMembership, CellBindingRole, CellBindingSource, CheckerError, CheckerRejectionCode, TypedSemanticConstant,
    TypedSemanticEntry, TypedSemanticOperand, TypedSemanticOperationDetail as Detail, TypedSemanticRecord,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

fn reject(message: impl std::fmt::Display) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2419TypedSemanticsInvalid, message.to_string())
}
#[derive(Clone, Default, PartialEq, Eq)]
struct Value {
    sources: BTreeSet<Source>,
    kinds: BTreeSet<Source>,
    integers: BTreeSet<u64>,
    plain: bool,
}
impl Value {
    fn plain() -> Self {
        Self { plain: true, ..Self::default() }
    }
    fn source(source: Source) -> Self {
        Self {
            sources: BTreeSet::from([source]),
            kinds: if matches!(source, Source::Parameter { .. }) { BTreeSet::new() } else { BTreeSet::from([source]) },
            ..Self::default()
        }
    }
    fn constant(constant: &TypedSemanticConstant) -> Self {
        let integer = match constant {
            TypedSemanticConstant::U8(x)
            | TypedSemanticConstant::U16(x)
            | TypedSemanticConstant::U32(x)
            | TypedSemanticConstant::U64(x) => x.parse().ok(),
            _ => None,
        };
        Self { integers: integer.into_iter().collect(), plain: true, ..Self::default() }
    }
    fn join(&mut self, other: &Self) -> Result<(), CheckerError> {
        self.sources.extend(&other.sources);
        self.kinds.extend(&other.kinds);
        self.integers.extend(&other.integers);
        self.plain |= other.plain;
        if self.sources.len() > 4096 || self.integers.len() > 4096 {
            return Err(reject("effect provenance alternatives exceed their bound"));
        }
        Ok(())
    }
    fn returns(&self) -> bool {
        self.plain || !self.sources.is_empty() || !self.integers.is_empty()
    }
    fn at_call(&self, args: &[Value], strict: bool) -> Result<Self, CheckerError> {
        let mut result = Self { integers: self.integers.clone(), plain: self.plain, kinds: self.kinds.clone(), ..Self::default() };
        for source in &self.sources {
            match source {
                Source::Parameter { index } => {
                    let value = args.get(usize::from(*index)).ok_or_else(|| reject("effect return parameter is unbound"))?;
                    if strict && (value.sources.is_empty() || value.plain) {
                        return Err(reject("effect return has no source argument evidence"));
                    }
                    result.join(value)?;
                }
                source => {
                    result.sources.insert(*source);
                }
            }
        }
        Ok(result)
    }
}
#[derive(Clone, Default, PartialEq, Eq)]
struct Environment {
    values: BTreeMap<u32, Value>,
    bindings: BTreeMap<String, Value>,
}
impl Environment {
    fn join(&mut self, incoming: &Self) -> Result<bool, CheckerError> {
        let before = self.clone();
        for (id, value) in &incoming.values {
            self.values.entry(*id).or_default().join(value)?;
        }
        for (name, value) in &incoming.bindings {
            self.bindings.entry(name.clone()).or_default().join(value)?;
        }
        Ok(*self != before)
    }
    fn operand(&self, operand: &TypedSemanticOperand) -> Value {
        if let Some(local) = operand.local {
            self.values.get(&local).cloned().unwrap_or_default()
        } else if let Some(value) = &operand.constant {
            Value::constant(value)
        } else {
            Value::default()
        }
    }
}
#[derive(Clone, Default, PartialEq, Eq)]
struct Contract {
    facts: EffectFacts,
    returned: Value,
}
fn source(source: CellBindingSource) -> Source {
    match source {
        CellBindingSource::Input => Source::Input,
        CellBindingSource::Output => Source::Output,
        CellBindingSource::GroupInput => Source::GroupInput,
        CellBindingSource::GroupOutput => Source::GroupOutput,
        CellBindingSource::CellDep => Source::CellDep,
    }
}
fn carries(ty: &str, record: &TypedSemanticRecord) -> bool {
    let ty = ty.trim_start_matches('&').trim_start_matches("mut ");
    matches!(
        ty.split('<').next(),
        Some("InputView" | "OutputView" | "CellDepView" | "HeaderDepView" | "CellDataView" | "WitnessArgsView" | "ScriptView")
    ) || record
        .types
        .iter()
        .any(|definition| definition.name == ty && matches!(definition.kind.as_str(), "resource" | "shared" | "receipt"))
}
fn read(facts: &mut EffectFacts, value: &Value, access: Access, strict: bool) -> Result<(), CheckerError> {
    if strict && (value.sources.is_empty() || value.plain) {
        return Err(reject("context observation has missing or ordinary source evidence"));
    }
    for source in &value.sources {
        facts.include_read(*source, access).map_err(reject)?;
    }
    Ok(())
}
fn fixed(facts: &mut EffectFacts, source: Source, access: Access) -> Result<(), CheckerError> {
    facts.include_read(source, access).map_err(reject)
}
fn life(facts: &mut EffectFacts, operation: Life) -> Result<(), CheckerError> {
    facts.include_lifecycle(operation).map_err(reject)
}
fn include_call(facts: &mut EffectFacts, callee: &EffectFacts, args: &[Value], strict: bool) -> Result<(), CheckerError> {
    for operation in callee.lifecycle_ops().iter() {
        life(facts, *operation)?;
    }
    for operation in callee.process_ops().iter() {
        facts.include_process(*operation).map_err(reject)?;
    }
    for observation in callee.context_reads().iter() {
        if let Source::Parameter { index } = observation.source {
            let arg = args.get(usize::from(index)).ok_or_else(|| reject("effect call parameter is out of bounds"))?;
            read(facts, arg, observation.access, strict)?;
        } else {
            fixed(facts, observation.source, observation.access)?;
        }
    }
    Ok(())
}
fn evaluate(
    record: &TypedSemanticRecord,
    entry: &TypedSemanticEntry,
    contracts: &BTreeMap<&str, Contract>,
    strict: bool,
    work: &mut usize,
) -> Result<Contract, CheckerError> {
    let mut contract = Contract::default();
    let mut seed = Environment::default();
    let names = entry.locals.iter().map(|local| (local.id, local.name.as_str())).collect::<BTreeMap<_, _>>();
    let types = entry.locals.iter().map(|local| (local.id, local.ty.as_str())).collect::<BTreeMap<_, _>>();
    let mut named_locals = BTreeMap::<(&str, &str), Vec<u32>>::new();
    for local in &entry.locals {
        named_locals.entry((&local.name, &local.ty)).or_default().push(local.id);
    }
    let mut borrow_starts = BTreeMap::<(u32, u32), Vec<_>>::new();
    for borrow in &entry.borrows {
        borrow_starts.entry((borrow.start_block, borrow.start_operation)).or_default().push(borrow);
    }
    for param in &entry.params {
        let mut value = if carries(&param.ty, record) {
            Value::source(Source::Parameter { index: u16::try_from(param.index).map_err(reject)? })
        } else {
            Value::plain()
        };
        let kinds: &[Source] = match param.ty.split('<').next() {
            Some("InputView") => &[Source::Input, Source::GroupInput],
            Some("OutputView") => &[Source::Output, Source::GroupOutput],
            Some("CellDepView") => &[Source::CellDep],
            Some("HeaderDepView") => &[Source::HeaderDep],
            _ => &[],
        };
        value.kinds.extend(kinds);
        seed.values.insert(param.binding_id, value.clone());
        seed.bindings.insert(param.name.clone(), value);
    }
    for binding in &entry.cell_bindings {
        let value = Value::source(source(binding.source));
        seed.bindings.insert(binding.binding.clone(), value.clone());
        if let Some(id) = binding.local_id {
            seed.values.insert(id, value);
        }
        fixed(&mut contract.facts, source(binding.source), Access::CellData)?;
        if binding.membership != CellBindingMembership::Unproven {
            fixed(&mut contract.facts, source(binding.source), Access::TypeScript)?;
            fixed(&mut contract.facts, source(binding.source), Access::LockScript)?;
            fixed(&mut contract.facts, Source::CurrentScript, Access::Script)?;
        }
        if entry.kind == "action" {
            match binding.role {
                CellBindingRole::Input => life(&mut contract.facts, Life::Consume)?,
                CellBindingRole::Output => life(&mut contract.facts, Life::Create)?,
                CellBindingRole::ReadOnly => {}
            }
        }
    }
    if entry.ownership.iter().any(|item| item.operation == "mutate") {
        life(&mut contract.facts, Life::Replace)?;
    }
    let blocks = entry.blocks.iter().map(|block| (block.id, block)).collect::<BTreeMap<_, _>>();
    let mut incoming = BTreeMap::from([(entry.entry_block, seed)]);
    let mut queue = VecDeque::from([entry.entry_block]);
    let mut queued = BTreeSet::from([entry.entry_block]);
    while let Some(id) = queue.pop_front() {
        queued.remove(&id);
        *work += 1;
        if *work > 1_000_000 {
            return Err(reject("effect reconstruction exceeds its work bound"));
        }
        let block = blocks.get(&id).ok_or_else(|| reject("effect CFG block is absent"))?;
        let mut env = incoming[&id].clone();
        let mut terminated = false;
        for operation in &block.operations {
            *work += 1;
            if *work > 1_000_000 {
                return Err(reject("effect reconstruction exceeds its work bound"));
            }
            // Borrow views have distinct typed local IDs but are projections
            // of an existing checked binding. Establish their provenance at
            // the region's start, after any preceding writes to the root.
            // Do not seed a view from its asserted type or guessed source ID.
            for borrow in borrow_starts.get(&(id, operation.index)).into_iter().flatten() {
                *work += 1;
                if *work > 1_000_000 {
                    return Err(reject("effect reconstruction exceeds its work bound"));
                }
                let value = if carries(&borrow.view_type, record) {
                    env.bindings.get(&borrow.root).cloned().unwrap_or_default()
                } else {
                    Value::plain()
                };
                for local in named_locals.get(&(borrow.binding.as_str(), borrow.view_type.as_str())).into_iter().flatten() {
                    env.values.insert(*local, value.clone());
                }
                env.bindings.insert(borrow.binding.clone(), value);
            }
            let args = operation.operands.iter().map(|arg| env.operand(arg)).collect::<Vec<_>>();
            let arg = |index: usize| args.get(index).cloned().ok_or_else(|| reject("effect operation operand is absent"));
            let mut output = Value::plain();
            match operation.opcode.as_str() {
                "load-const" => {
                    let Detail::Constant { value } = &operation.detail else {
                        return Err(reject("constant effect detail is absent"));
                    };
                    output = Value::constant(value);
                }
                "load-var" => {
                    let Detail::Binding { name } = &operation.detail else {
                        return Err(reject("load binding is absent"));
                    };
                    output = env.bindings.get(name).cloned().unwrap_or_default();
                }
                "store-var" => {
                    let Detail::Binding { name } = &operation.detail else {
                        return Err(reject("store binding is absent"));
                    };
                    env.bindings.insert(name.clone(), arg(0)?);
                }
                "move" | "unary" => output = arg(0)?,
                "field-access" | "index" => {
                    let value = arg(0)?;
                    if !value.sources.is_empty() {
                        read(&mut contract.facts, &value, Access::CellData, strict)?;
                    }
                    if operation.destinations.first().and_then(|id| types.get(id)).is_some_and(|ty| carries(ty, record)) {
                        output = value;
                    }
                }
                "type-hash" => {
                    if operation.operands.first().is_some_and(|operand| carries(&operand.ty, record)) {
                        read(&mut contract.facts, &arg(0)?, Access::TypeScript, strict)?;
                    }
                }
                "cell-metadata-equality" => {
                    let Detail::CellMetadata { field } = &operation.detail else {
                        return Err(reject("cell metadata effect detail is absent"));
                    };
                    let access = match field.as_str() {
                        "lock-hash" => Access::LockScript,
                        "capacity" => Access::Capacity,
                        _ => return Err(reject("unknown cell metadata effect field")),
                    };
                    read(&mut contract.facts, &arg(0)?, access, strict)?;
                    read(&mut contract.facts, &arg(1)?, access, strict)?;
                }
                "read-ref" => {
                    fixed(&mut contract.facts, Source::CellDep, Access::CellData)?;
                    output = Value::source(Source::CellDep);
                }
                "call" => {
                    let call = operation.call.as_ref().ok_or_else(|| reject("call effect evidence is absent"))?;
                    match call.contract.as_str() {
                        "typed-local" => {
                            if call.runtime_failure.is_some() {
                                return Err(reject("ordinary call has a fabricated deferred failure"));
                            }
                            let callee = contracts
                                .get(call.target.as_str())
                                .ok_or_else(|| reject(format!("effect callee '{}' is absent", call.target)))?;
                            include_call(&mut contract.facts, &callee.facts, &args, strict)?;
                            if !callee.returned.returns() {
                                terminated = true;
                                break;
                            }
                            output = callee.returned.at_call(&args, strict)?;
                        }
                        "versioned-runtime-helper" => {
                            let helper = RuntimeHelper::from_symbol(&call.target)
                                .ok_or_else(|| reject("unregistered runtime effect helper"))?;
                            if call.runtime_failure != helper.failure() {
                                return Err(reject("runtime deferred failure was forged"));
                            }
                            match helper.fact_contract() {
                                RuntimeEffectContract::Available(template) => {
                                    let mut bound = RuntimeArgumentFacts::default();
                                    for (index, value) in args.iter().enumerate() {
                                        let index = u16::try_from(index).map_err(reject)?;
                                        if !value.plain && !value.sources.is_empty() {
                                            let mut sources = CanonicalSet::default();
                                            for source in &value.sources {
                                                sources.insert(*source).map_err(reject)?;
                                            }
                                            bound.sources.insert(index, sources);
                                        }
                                        let mut kinds = CanonicalSet::default();
                                        for kind in &value.kinds {
                                            kinds.insert(*kind).map_err(reject)?;
                                        }
                                        bound.source_kinds.insert(index, kinds);
                                        if value.integers.len() == 1
                                            && let Some(value) = value.integers.first()
                                            && let Ok(kind) = ByteSourceKind::from_wire(*value)
                                        {
                                            bound.byte_kinds.insert(index, kind);
                                        }
                                    }
                                    match template.instantiate(&bound) {
                                        Ok(facts) => contract.facts = contract.facts.union(&facts).map_err(reject)?,
                                        Err(error) if strict => return Err(reject(format!("{}: {error}", call.target))),
                                        Err(_) => {}
                                    }
                                    if helper.return_abi() == crate::runtime_contract::RuntimeReturnAbi::NoReturn {
                                        terminated = true;
                                        break;
                                    }
                                    output = match template.result_sources(&bound) {
                                        Ok(Some(sources)) => {
                                            let mut value = Value { sources: sources.iter().copied().collect(), ..Value::default() };
                                            for source in &value.sources {
                                                if !matches!(source, Source::Parameter { .. }) {
                                                    value.kinds.insert(*source);
                                                }
                                            }
                                            if let super::runtime::SourceResult::Argument(index) = template.result {
                                                value.kinds = args[usize::from(index)].kinds.clone();
                                            }
                                            value
                                        }
                                        Ok(None) => Value::plain(),
                                        Err(error) if strict => return Err(reject(error)),
                                        Err(_) => Value::default(),
                                    };
                                }
                                RuntimeEffectContract::MachineOnly(helper) => {
                                    return Err(reject(format!(
                                        "private helper {} has no enclosing typed-operation evidence",
                                        helper.symbol()
                                    )))
                                }
                                RuntimeEffectContract::Failure(failure) => {
                                    if helper.return_abi() != crate::runtime_contract::RuntimeReturnAbi::NoReturn
                                        || helper.failure() != Some(failure.failure())
                                    {
                                        return Err(reject("non-returning helper has no exact failure contract"));
                                    }
                                    terminated = true;
                                    break;
                                }
                                RuntimeEffectContract::Deferred(feature) => {
                                    if helper.return_abi() != crate::runtime_contract::RuntimeReturnAbi::NoReturn
                                        || helper.deferred() != Some(feature)
                                    {
                                        return Err(reject("deferred operation lacks its exact non-returning failure"));
                                    }
                                    terminated = true;
                                    break;
                                }
                            }
                        }
                        _ => return Err(reject("unknown call authority in effect reconstruction")),
                    }
                }
                "verify-exact-cell-roles" => {
                    let Detail::ExactRoleSelection { contract: selection } = &operation.detail else {
                        return Err(reject("selection effect detail is absent"));
                    };
                    selection.validate().map_err(reject)?;
                    for source in [Source::Input, Source::Output] {
                        for access in
                            [Access::CellData, Access::TypeScript, Access::LockScript, Access::RoleSelection, Access::Cardinality]
                        {
                            fixed(&mut contract.facts, source, access)?;
                        }
                    }
                    if selection.roles.iter().any(|role| role.local_type) {
                        fixed(&mut contract.facts, Source::CurrentScript, Access::Script)?;
                    }
                }
                "read-exact-cell-role" => {
                    let Detail::ExactRoleRead { read: kind, .. } = &operation.detail else {
                        return Err(reject("role read effect detail is absent"));
                    };
                    use crate::cell_selection::Read;
                    let source = match kind {
                        Read::Input | Read::InputCount => Source::Input,
                        Read::Output | Read::OutputCount | Read::Successor => Source::Output,
                    };
                    fixed(&mut contract.facts, source, if kind.indexed() { Access::RoleSelection } else { Access::Cardinality })?;
                    if kind.indexed() {
                        output = Value::source(source);
                    }
                }
                "bounded-cell-load" => {
                    fixed(&mut contract.facts, Source::GroupInput, Access::CellData)?;
                    fixed(&mut contract.facts, Source::GroupInput, Access::Cardinality)?;
                    if entry.kind == "action" && entry.params.iter().any(|param| param.source == "input") {
                        life(&mut contract.facts, Life::Consume)?;
                    }
                    output = Value::source(Source::GroupInput);
                }
                "bounded-output-verify" => {
                    life(&mut contract.facts, Life::Create)?;
                    for access in [Access::CellData, Access::LockScript, Access::Capacity] {
                        fixed(&mut contract.facts, Source::GroupOutput, access)?;
                    }
                }
                "bounded-output-end" => fixed(&mut contract.facts, Source::GroupOutput, Access::Cardinality)?,
                "consume" => life(&mut contract.facts, Life::Consume)?,
                "destroy" => life(&mut contract.facts, Life::Destroy)?,
                "create" | "create-unique" => {
                    life(&mut contract.facts, Life::Create)?;
                    output = Value::source(Source::Output);
                }
                "replace-unique" => {
                    life(&mut contract.facts, Life::Replace)?;
                    output = Value::source(Source::Output);
                }
                "transfer" => {
                    life(&mut contract.facts, Life::Transfer)?;
                    output = Value::source(Source::Output);
                }
                "claim" => {
                    life(&mut contract.facts, Life::Claim)?;
                    output = Value::source(Source::Output);
                }
                "settle" => {
                    life(&mut contract.facts, Life::Settle)?;
                    output = Value::source(Source::Output);
                }
                "return" => {
                    contract.returned.join(&args.first().cloned().unwrap_or_else(Value::plain))?;
                }
                "allocated-value"
                | "binary"
                | "length"
                | "collection-new"
                | "collection-capacity"
                | "collection-push"
                | "collection-extend"
                | "collection-clear"
                | "collection-contains"
                | "collection-remove"
                | "collection-insert"
                | "collection-set"
                | "collection-pop"
                | "collection-reverse"
                | "collection-truncate"
                | "collection-swap"
                | "bounded-plan-load"
                | "tuple"
                | "enum-construct"
                | "enum-tag"
                | "enum-payload"
                | "branch-condition"
                | "verifier-failure" => {}
                opcode => return Err(reject(format!("opcode '{opcode}' has no explicit effect semantics"))),
            }
            for (index, dest) in operation.destinations.iter().enumerate() {
                let value = if operation.opcode == "bounded-cell-load" && index != 0 { Value::plain() } else { output.clone() };
                env.values.insert(*dest, value.clone());
                if let Some(name) = names.get(dest) {
                    env.bindings.insert((*name).into(), value);
                }
            }
        }
        if terminated || block.runtime_error.is_some() {
            continue;
        }
        for successor in &block.successors {
            let changed = if let Some(previous) = incoming.get_mut(successor) {
                previous.join(&env)?
            } else {
                incoming.insert(*successor, env.clone());
                true
            };
            if changed && queued.insert(*successor) {
                queue.push_back(*successor);
            }
        }
    }
    Ok(contract)
}
pub(crate) fn infer(record: &TypedSemanticRecord) -> Result<BTreeMap<&str, EffectFacts>, CheckerError> {
    if record.entries.len() > 4096 {
        return Err(reject("effect callable count exceeds its bound"));
    }
    let mut contracts = record.entries.iter().map(|entry| (entry.name.as_str(), Contract::default())).collect::<BTreeMap<_, _>>();
    if contracts.len() != record.entries.len() {
        return Err(reject("duplicate effect callable"));
    }
    let mut work = 0;
    loop {
        let mut changed = false;
        for entry in &record.entries {
            let next = evaluate(record, entry, &contracts, false, &mut work)?;
            let previous = contracts.get_mut(entry.name.as_str()).ok_or_else(|| reject("missing effect accumulator"))?;
            let facts = previous.facts.union(&next.facts).map_err(reject)?;
            let before = previous.clone();
            previous.facts = facts;
            previous.returned.join(&next.returned)?;
            changed |= *previous != before;
        }
        if !changed {
            break;
        }
    }
    for entry in &record.entries {
        let reconstructed = evaluate(record, entry, &contracts, true, &mut work)?;
        if reconstructed != contracts[entry.name.as_str()] {
            return Err(reject("effect reconstruction did not converge exactly"));
        }
    }
    Ok(contracts.into_iter().map(|(name, contract)| (name, contract.facts)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        TypedSemanticBlock as Block, TypedSemanticCall as Call, TypedSemanticLocal as Local, TypedSemanticOperation as Operation,
        TypedSemanticParam as Param,
    };
    fn operand(local: u32) -> TypedSemanticOperand {
        TypedSemanticOperand { local: Some(local), ty: "u64".into(), constant: None }
    }
    fn integer(value: u64) -> TypedSemanticOperand {
        TypedSemanticOperand { local: None, ty: "u64".into(), constant: Some(TypedSemanticConstant::U64(value.to_string())) }
    }
    fn call(index: u32, dest: Option<u32>, target: &str, args: Vec<TypedSemanticOperand>, local: bool) -> Operation {
        Operation {
            index,
            opcode: "call".into(),
            destinations: dest.into_iter().collect(),
            operands: args,
            call: Some(Call {
                target: target.into(),
                contract: if local { "typed-local" } else { "versioned-runtime-helper" }.into(),
                return_type: if dest.is_some() { "u64" } else { "unit" }.into(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
    fn entry(name: &str, operations: Vec<Operation>) -> TypedSemanticEntry {
        TypedSemanticEntry {
            name: name.into(),
            kind: "helper".into(),
            blocks: vec![Block { id: 0, operations, terminator: "return".into(), ..Default::default() }],
            ..Default::default()
        }
    }
    fn returned(index: u32, value: TypedSemanticOperand) -> Operation {
        Operation { index, opcode: "return".into(), operands: vec![value], ..Default::default() }
    }
    #[test]
    fn independently_substitutes_bound_call_sources() {
        let mut capacity =
            entry("capacity", vec![call(0, Some(1), "__ckb_cell_capacity", vec![operand(0)], false), returned(1, operand(1))]);
        capacity.params.push(Param {
            index: 0,
            binding_id: 0,
            name: "view".into(),
            ty: "InputView<Token>".into(),
            ..Default::default()
        });
        let inspect = entry(
            "inspect",
            vec![
                call(0, Some(2), "__ckb_source_group_input", vec![integer(0)], false),
                call(1, Some(3), "capacity", vec![operand(2)], true),
                returned(2, operand(3)),
            ],
        );
        let record = TypedSemanticRecord { entries: vec![capacity, inspect], ..Default::default() };
        let result = infer(&record).unwrap();
        assert_eq!(result["capacity"], EffectFacts::read(Source::Parameter { index: 0 }, Access::Capacity));
        assert_eq!(result["inspect"], EffectFacts::read(Source::GroupInput, Access::Capacity));
    }
    #[test]
    fn producer_facts_never_seed_reconstruction() {
        let mut local = entry("local", vec![returned(0, integer(1))]);
        local.effects = EffectFacts::process(super::super::ProcessOperation::Exec);
        let record = TypedSemanticRecord { entries: vec![local], ..Default::default() };
        assert!(infer(&record).unwrap()["local"].is_empty());
    }
    #[test]
    fn borrowed_view_keeps_checked_root_provenance_at_region_start() {
        let mut inspect =
            entry("inspect", vec![call(0, Some(2), "__ckb_cell_capacity", vec![operand(1)], false), returned(1, operand(2))]);
        inspect.params.push(Param {
            index: 0,
            binding_id: 0,
            name: "root".into(),
            ty: "InputView<Token>".into(),
            ..Default::default()
        });
        inspect.locals.push(Local { id: 1, name: "alias".into(), ty: "&InputView<Token>".into(), ..Default::default() });
        inspect.borrows.push(crate::TypedSemanticBorrow {
            root: "root".into(),
            binding: "alias".into(),
            view_type: "&InputView<Token>".into(),
            start_block: 0,
            start_operation: 0,
            ..Default::default()
        });
        let mut record = TypedSemanticRecord { entries: vec![inspect], ..Default::default() };
        assert_eq!(infer(&record).unwrap()["inspect"], EffectFacts::read(Source::Parameter { index: 0 }, Access::Capacity));
        // Neither a claimed view type nor a producer fact can create source
        // authority for an ordinary or absent root.
        record.entries[0].params[0].ty = "u64".into();
        assert!(infer(&record).is_err());
        record.entries[0].params[0].ty = "InputView<Token>".into();
        record.entries[0].borrows[0].root = "absent".into();
        assert!(infer(&record).is_err());
        record.entries[0].borrows[0].root = "root".into();
        record.entries[0].borrows[0].start_operation = 1;
        assert!(infer(&record).is_err());
    }
    #[test]
    fn ordinary_scalars_cannot_fabricate_a_context_source() {
        let record = TypedSemanticRecord {
            entries: vec![entry(
                "invalid",
                vec![call(0, Some(0), "__ckb_cell_capacity", vec![integer(1)], false), returned(1, operand(0))],
            )],
            ..Default::default()
        };
        assert!(infer(&record).unwrap_err().message.contains("no resolved source evidence"));
    }
    #[test]
    fn recursive_call_graph_retains_process_operations() {
        let a = entry("a", vec![call(0, Some(0), "b", vec![], true), returned(1, operand(0))]);
        let b = entry(
            "b",
            vec![
                call(0, Some(0), "__ckb_pipe_write", vec![integer(1), integer(7)], false),
                call(1, Some(1), "a", vec![], true),
                returned(2, operand(1)),
            ],
        );
        let record = TypedSemanticRecord { entries: vec![a, b], ..Default::default() };
        let result = infer(&record).unwrap();
        assert_eq!(result["a"], EffectFacts::process(super::super::ProcessOperation::PipeWrite));
        assert_eq!(result["a"], result["b"]);
    }
    #[test]
    fn cfg_join_preserves_all_observed_sources() {
        let mut inspect = entry("inspect", vec![]);
        inspect.locals = vec![Local { id: 0, name: "view".into(), ty: "InputView<Token>".into(), ..Default::default() }];
        inspect.blocks = vec![
            Block { id: 0, successors: vec![1, 2], terminator: "branch".into(), ..Default::default() },
            Block {
                id: 1,
                operations: vec![call(0, Some(0), "__ckb_source_input", vec![integer(0)], false)],
                successors: vec![3],
                terminator: "jump".into(),
                ..Default::default()
            },
            Block {
                id: 2,
                operations: vec![call(0, Some(0), "__ckb_source_group_input", vec![integer(0)], false)],
                successors: vec![3],
                terminator: "jump".into(),
                ..Default::default()
            },
            Block {
                id: 3,
                operations: vec![call(0, Some(1), "__ckb_cell_capacity", vec![operand(0)], false), returned(1, operand(1))],
                terminator: "return".into(),
                ..Default::default()
            },
        ];
        let record = TypedSemanticRecord { entries: vec![inspect], ..Default::default() };
        let result = infer(&record).unwrap();
        assert_eq!(
            result["inspect"],
            EffectFacts::read(Source::Input, Access::Capacity)
                .union(&EffectFacts::read(Source::GroupInput, Access::Capacity))
                .unwrap()
        );
    }
    #[test]
    fn a_bound_abort_does_not_turn_unavailable_semantics_into_success() {
        let mut abort = call(0, None, "__ckb_require_time", vec![integer(100)], false);
        abort.call.as_mut().unwrap().runtime_failure = Some(crate::runtime_contract::DeferredFeature::CkbSinceConstraints.failure());
        let record = TypedSemanticRecord {
            entries: vec![entry(
                "reject",
                vec![abort, call(1, Some(0), "__ckb_pipe_write", vec![integer(1), integer(7)], false), returned(2, integer(0))],
            )],
            ..Default::default()
        };
        assert!(infer(&record).unwrap()["reject"].is_empty());
        let mut forged = record;
        forged.entries[0].blocks[0].operations[0].call.as_mut().unwrap().runtime_failure = None;
        assert!(infer(&forged).is_err());
    }
    #[test]
    fn unknown_operations_and_helpers_reject() {
        for operation in
            [Operation { opcode: "future-operation".into(), ..Default::default() }, call(0, None, "__future_helper", vec![], false)]
        {
            assert!(infer(&TypedSemanticRecord { entries: vec![entry("reject", vec![operation])], ..Default::default() }).is_err());
        }
    }
}
