//! Compiler-owned IR dataflow. The checker reconstructs these facts using its
//! own typed-record analysis; producer contracts never seed either fixed point.
use super::resolved::{self, ArgumentSources, CallableId, InferenceBudgets, ResolvedCall, ResolvedCallable, ResolvedFactGraph};
use crate::error::{CompileError, Result, SourceDiagnosticCode, Span};
use crate::ir::*;
use cellscript_artifact_checker::effect_facts::runtime::{ByteSourceKind, RuntimeArgumentFacts, RuntimeEffectContract};
use cellscript_artifact_checker::effect_facts::{
    CanonicalSet, ContextAccess as A, ContextSource as S, EffectFacts, LifecycleOperation as L,
};
use cellscript_artifact_checker::runtime_contract::RuntimeHelper;
use std::collections::{BTreeMap, BTreeSet};

const MAX_STEPS: usize = 1_000_000;
fn invalid(message: impl std::fmt::Display) -> CompileError {
    SourceDiagnosticCode::UnresolvedEffect.error(message.to_string(), Span::default())
}
#[derive(Clone, Default, PartialEq, Eq)]
struct Origin {
    sources: CanonicalSet<S>,
    kinds: CanonicalSet<S>,
    values: BTreeSet<u64>,
    ordinary: bool,
}
impl Origin {
    fn value() -> Self {
        Self { ordinary: true, ..Self::default() }
    }
    fn source(source: S) -> Result<Self> {
        let mut result = Self::default();
        result.sources.insert(source).map_err(invalid)?;
        if !matches!(source, S::Parameter { .. }) {
            result.kinds.insert(source).map_err(invalid)?;
        }
        Ok(result)
    }
    fn number(value: u64) -> Self {
        Self { values: BTreeSet::from([value]), ordinary: true, ..Self::default() }
    }
    fn merge(&mut self, other: &Self) -> Result<()> {
        for source in other.sources.iter() {
            self.sources.insert(*source).map_err(invalid)?;
        }
        for kind in other.kinds.iter() {
            self.kinds.insert(*kind).map_err(invalid)?;
        }
        self.values.extend(other.values.iter().copied());
        if self.values.len() > 4096 {
            return Err(invalid("effect constant alternatives exceed their bound"));
        }
        self.ordinary |= other.ordinary;
        Ok(())
    }
    fn returns(&self) -> bool {
        self.ordinary || !self.sources.is_empty() || !self.values.is_empty()
    }
    fn argument(&self) -> ArgumentSources {
        if self.ordinary || self.sources.is_empty() {
            ArgumentSources::Value
        } else {
            ArgumentSources::Sources(self.sources.clone())
        }
    }
    fn instantiate(&self, arguments: &[Self], strict: bool) -> Result<Self> {
        let mut result = Self { values: self.values.clone(), ordinary: self.ordinary, kinds: self.kinds.clone(), ..Self::default() };
        for source in self.sources.iter() {
            if let S::Parameter { index } = source {
                let argument =
                    arguments.get(usize::from(*index)).ok_or_else(|| invalid("source result references missing parameter"))?;
                if strict && (argument.ordinary || argument.sources.is_empty()) {
                    return Err(invalid("source result has no resolved argument source"));
                }
                result.merge(argument)?;
            } else {
                result.sources.insert(*source).map_err(invalid)?;
            }
        }
        Ok(result)
    }
}
#[derive(Clone, Default, PartialEq, Eq)]
struct State {
    locals: BTreeMap<usize, Origin>,
    names: BTreeMap<String, Origin>,
}
impl State {
    fn merge(&mut self, other: &Self) -> Result<()> {
        for (id, origin) in &other.locals {
            self.locals.entry(*id).or_default().merge(origin)?;
        }
        for (name, origin) in &other.names {
            self.names.entry(name.clone()).or_default().merge(origin)?;
        }
        Ok(())
    }
    fn bind(&mut self, dest: &IrVar, origin: Origin) {
        self.names.insert(dest.name.clone(), origin.clone());
        self.locals.insert(dest.id, origin);
    }
    fn operand(&self, operand: &IrOperand) -> Origin {
        match operand {
            IrOperand::Var(var) => self.locals.get(&var.id).cloned().unwrap_or_default(),
            IrOperand::Const(value) => constant(value),
        }
    }
}
fn constant(value: &IrConst) -> Origin {
    match value {
        IrConst::U8(x) => Origin::number(u64::from(*x)),
        IrConst::U16(x) => Origin::number(u64::from(*x)),
        IrConst::U32(x) => Origin::number(u64::from(*x)),
        IrConst::U64(x) => Origin::number(*x),
        _ => Origin::value(),
    }
}
struct Callable<'a> {
    name: &'a str,
    params: &'a [IrParam],
    body: &'a IrBody,
    action: bool,
}
fn callables(module: &IrModule) -> Vec<Callable<'_>> {
    module
        .items
        .iter()
        .filter_map(|item| match item {
            IrItem::Action(x) => Some(Callable { name: &x.name, params: &x.params, body: &x.body, action: true }),
            IrItem::Function(x) => Some(Callable { name: &x.name, params: &x.params, body: &x.body, action: false }),
            IrItem::Lock(x) => Some(Callable { name: &x.name, params: &x.params, body: &x.body, action: false }),
            _ => None,
        })
        .collect()
}
fn carries_source(ty: &IrType, module: &IrModule) -> bool {
    match ty {
        IrType::Ref(inner) | IrType::MutRef(inner) => carries_source(inner, module),
        IrType::Named(name) => {
            matches!(
                name.split('<').next(),
                Some("InputView" | "OutputView" | "CellDepView" | "HeaderDepView" | "CellDataView" | "WitnessArgsView" | "ScriptView")
            ) || module
                .external_type_defs
                .iter()
                .chain(module.items.iter().filter_map(|x| if let IrItem::TypeDef(t) = x { Some(t) } else { None }))
                .any(|definition| definition.name == *name && definition.kind != IrTypeKind::Struct)
        }
        _ => false,
    }
}
fn source(source: IrCellSource) -> S {
    match source {
        IrCellSource::Input => S::Input,
        IrCellSource::Output => S::Output,
        IrCellSource::GroupInput => S::GroupInput,
        IrCellSource::GroupOutput => S::GroupOutput,
        IrCellSource::CellDep => S::CellDep,
    }
}
fn read(facts: &mut EffectFacts, origin: &Origin, access: A, strict: bool) -> Result<()> {
    if strict && (origin.sources.is_empty() || origin.ordinary) {
        return Err(invalid("context observation lacks resolved source evidence"));
    }
    for source in origin.sources.iter() {
        facts.include_read(*source, access).map_err(invalid)?;
    }
    Ok(())
}
fn fixed(facts: &mut EffectFacts, source: S, access: A) -> Result<()> {
    facts.include_read(source, access).map_err(invalid)
}
fn life(facts: &mut EffectFacts, operation: L) -> Result<()> {
    facts.include_lifecycle(operation).map_err(invalid)
}
fn selection(facts: &mut EffectFacts, contract: &crate::cell_selection::Contract) -> Result<()> {
    contract.validate().map_err(invalid)?;
    for source in [S::Input, S::Output] {
        for access in [A::CellData, A::TypeScript, A::LockScript, A::RoleSelection, A::Cardinality] {
            fixed(facts, source, access)?;
        }
    }
    if contract.roles.iter().any(|role| role.local_type) {
        fixed(facts, S::CurrentScript, A::Script)?;
    }
    Ok(())
}
struct Analysis {
    direct: EffectFacts,
    calls: Vec<ResolvedCall>,
    returns: Origin,
}
fn analyze(
    module: &IrModule,
    callable: &Callable<'_>,
    returns: &BTreeMap<String, Origin>,
    strict: bool,
    steps: &mut usize,
) -> Result<Analysis> {
    let mut facts = EffectFacts::empty();
    let mut initial = State::default();
    for (index, param) in callable.params.iter().enumerate() {
        let mut origin = if carries_source(&param.ty, module) {
            Origin::source(S::Parameter { index: u16::try_from(index).map_err(invalid)? })?
        } else {
            Origin::value()
        };
        if let IrType::Named(name) = &param.ty {
            let kinds: &[S] = match name.split('<').next() {
                Some("InputView") => &[S::Input, S::GroupInput],
                Some("OutputView") => &[S::Output, S::GroupOutput],
                Some("CellDepView") => &[S::CellDep],
                Some("HeaderDepView") => &[S::HeaderDep],
                _ => &[],
            };
            for kind in kinds {
                origin.kinds.insert(*kind).map_err(invalid)?;
            }
        }
        initial.bind(&param.binding, origin);
    }
    for binding in &callable.body.cell_bindings {
        let origin = Origin::source(source(binding.source))?;
        initial.names.insert(binding.binding.clone(), origin.clone());
        if let Some(id) = binding.local_id {
            initial.locals.insert(id, origin);
        }
        fixed(&mut facts, source(binding.source), A::CellData)?;
        if binding.membership != IrCellMembership::Unproven {
            fixed(&mut facts, source(binding.source), A::TypeScript)?;
            fixed(&mut facts, source(binding.source), A::LockScript)?;
            fixed(&mut facts, S::CurrentScript, A::Script)?;
        }
        if callable.action {
            match binding.role {
                IrCellBindingRole::Input => life(&mut facts, L::Consume)?,
                IrCellBindingRole::Output => life(&mut facts, L::Create)?,
                IrCellBindingRole::ReadOnly => {}
            }
        }
    }
    if !callable.body.mutate_set.is_empty() {
        life(&mut facts, L::Replace)?;
    }
    let mut states = BTreeMap::new();
    let mut pending = BTreeSet::new();
    if let Some(block) = callable.body.blocks.first() {
        states.insert(block.id.0, initial);
        pending.insert(block.id.0);
    }
    let blocks = callable.body.blocks.iter().map(|block| (block.id.0, block)).collect::<BTreeMap<_, _>>();
    let mut calls = BTreeMap::new();
    let mut result = Origin::default();
    while let Some(block_id) = pending.pop_first() {
        *steps += 1;
        if *steps > MAX_STEPS {
            return Err(SourceDiagnosticCode::EffectBudget.error("IR effect inference exceeded its work budget", Span::default()));
        }
        let block = blocks.get(&block_id).ok_or_else(|| invalid("effect CFG references absent block"))?;
        let mut state = states[&block_id].clone();
        let mut terminated = false;
        for (position, instruction) in block.instructions.iter().enumerate() {
            match instruction {
                IrInstruction::LoadConst { dest, value } => state.bind(dest, constant(value)),
                IrInstruction::LoadVar { dest, name } => state.bind(dest, state.names.get(name).cloned().unwrap_or_default()),
                IrInstruction::StoreVar { name, src } => {
                    state.names.insert(name.clone(), state.operand(src));
                }
                IrInstruction::Move { dest, src } => state.bind(dest, state.operand(src)),
                IrInstruction::Unary { dest, operand, .. } => state.bind(dest, state.operand(operand)),
                IrInstruction::FieldAccess { dest, obj, .. } => {
                    let origin = state.operand(obj);
                    if !origin.sources.is_empty() {
                        read(&mut facts, &origin, A::CellData, strict)?;
                    }
                    state.bind(dest, if carries_source(&dest.ty, module) { origin } else { Origin::value() });
                }
                IrInstruction::Index { dest, arr, .. } => {
                    let origin = state.operand(arr);
                    if !origin.sources.is_empty() {
                        read(&mut facts, &origin, A::CellData, strict)?;
                    }
                    state.bind(dest, if carries_source(&dest.ty, module) { origin } else { Origin::value() });
                }
                IrInstruction::TypeHash { dest, operand } => {
                    if let IrOperand::Var(var) = operand
                        && carries_source(&var.ty, module)
                    {
                        read(&mut facts, &state.operand(operand), A::TypeScript, strict)?;
                    }
                    state.bind(dest, Origin::value());
                }
                IrInstruction::CellMetadataEquality { left, right, field } => {
                    let access = match field {
                        CellMetadataField::LockHash => A::LockScript,
                        CellMetadataField::Capacity => A::Capacity,
                    };
                    read(&mut facts, &state.operand(left), access, strict)?;
                    read(&mut facts, &state.operand(right), access, strict)?;
                }
                IrInstruction::ReadRef { dest, .. } => {
                    fixed(&mut facts, S::CellDep, A::CellData)?;
                    state.bind(dest, Origin::source(S::CellDep)?);
                }
                IrInstruction::Call { dest, func, args } => {
                    let arguments = args.iter().map(|arg| state.operand(arg)).collect::<Vec<_>>();
                    if let Some(return_origin) = returns.get(func) {
                        calls.insert(
                            (block_id, position),
                            ResolvedCall {
                                target: CallableId { owner: module.name.clone(), name: func.clone() },
                                arguments: arguments.iter().map(Origin::argument).collect(),
                            },
                        );
                        if !return_origin.returns() {
                            terminated = true;
                            break;
                        }
                        if let Some(dest) = dest {
                            state.bind(dest, return_origin.instantiate(&arguments, strict)?);
                        }
                    } else {
                        let helper =
                            RuntimeHelper::from_symbol(func).ok_or_else(|| invalid(format!("unresolved effect target '{func}'")))?;
                        match helper.fact_contract() {
                            RuntimeEffectContract::Available(template) => {
                                let mut bound = RuntimeArgumentFacts::default();
                                for (index, argument) in arguments.iter().enumerate() {
                                    let index = u16::try_from(index).map_err(invalid)?;
                                    if !argument.ordinary && !argument.sources.is_empty() {
                                        bound.sources.insert(index, argument.sources.clone());
                                    }
                                    bound.source_kinds.insert(index, argument.kinds.clone());
                                    if argument.values.len() == 1
                                        && let Some(value) = argument.values.first()
                                        && let Ok(kind) = ByteSourceKind::from_wire(*value)
                                    {
                                        bound.byte_kinds.insert(index, kind);
                                    }
                                }
                                match template.instantiate(&bound) {
                                    Ok(effect) => facts = facts.union(&effect).map_err(invalid)?,
                                    Err(error) if strict => return Err(invalid(format!("{func}: {error}"))),
                                    Err(_) => {}
                                }
                                if helper.return_abi() == cellscript_artifact_checker::runtime_contract::RuntimeReturnAbi::NoReturn {
                                    terminated = true;
                                    break;
                                }
                                if let Some(dest) = dest {
                                    let origin = match template.result_sources(&bound) {
                                        Ok(Some(sources)) => {
                                            let mut origin = Origin { sources, ..Origin::default() };
                                            for source in origin.sources.iter() {
                                                if !matches!(source, S::Parameter { .. }) {
                                                    origin.kinds.insert(*source).map_err(invalid)?;
                                                }
                                            }
                                            if let cellscript_artifact_checker::effect_facts::runtime::SourceResult::Argument(index) =
                                                template.result
                                            {
                                                origin.kinds = arguments[usize::from(index)].kinds.clone();
                                            }
                                            origin
                                        }
                                        Ok(None) => Origin::value(),
                                        Err(error) if strict => return Err(invalid(format!("{func}: {error}"))),
                                        Err(_) => Origin::default(),
                                    };
                                    state.bind(dest, origin);
                                }
                            }
                            RuntimeEffectContract::MachineOnly(helper) => {
                                return Err(invalid(format!(
                                    "private helper {} cannot bypass its enclosing typed operation",
                                    helper.symbol()
                                )))
                            }
                            RuntimeEffectContract::Failure(failure) => {
                                if helper.return_abi() != cellscript_artifact_checker::runtime_contract::RuntimeReturnAbi::NoReturn
                                    || helper.failure() != Some(failure.failure())
                                {
                                    return Err(invalid("non-returning helper has no exact failure contract"));
                                }
                                terminated = true;
                                break;
                            }
                            RuntimeEffectContract::Deferred(feature) => {
                                if helper.return_abi() != cellscript_artifact_checker::runtime_contract::RuntimeReturnAbi::NoReturn
                                    || helper.deferred() != Some(feature)
                                {
                                    return Err(invalid("deferred helper has no exact non-returning failure contract"));
                                }
                                terminated = true;
                                break;
                            }
                        }
                    }
                }
                IrInstruction::VerifyExactCellRoles { dest, contract } => {
                    selection(&mut facts, contract)?;
                    if let Some(dest) = dest {
                        state.bind(dest, Origin::value());
                    }
                }
                IrInstruction::ReadExactCellRole { dest, read: which, .. } => {
                    use cellscript_artifact_checker::cell_selection::Read;
                    let src = match which {
                        Read::Input | Read::InputCount => S::Input,
                        Read::Output | Read::OutputCount | Read::Successor => S::Output,
                    };
                    fixed(&mut facts, src, if which.indexed() { A::RoleSelection } else { A::Cardinality })?;
                    state.bind(dest, if which.indexed() { Origin::source(src)? } else { Origin::value() });
                }
                IrInstruction::BoundedCellLoad { dest, found, .. } => {
                    fixed(&mut facts, S::GroupInput, A::CellData)?;
                    fixed(&mut facts, S::GroupInput, A::Cardinality)?;
                    if callable.action && callable.params.iter().any(|param| param.source == crate::ast::ParamSource::Input) {
                        life(&mut facts, L::Consume)?;
                    }
                    state.bind(dest, Origin::source(S::GroupInput)?);
                    state.bind(found, Origin::value());
                }
                IrInstruction::BoundedOutputVerify { .. } => {
                    life(&mut facts, L::Create)?;
                    for access in [A::CellData, A::LockScript, A::Capacity] {
                        fixed(&mut facts, S::GroupOutput, access)?;
                    }
                }
                IrInstruction::BoundedOutputEnd { .. } => fixed(&mut facts, S::GroupOutput, A::Cardinality)?,
                IrInstruction::Consume { .. } => life(&mut facts, L::Consume)?,
                IrInstruction::Destroy { .. } => life(&mut facts, L::Destroy)?,
                IrInstruction::Create { dest, .. } | IrInstruction::CreateUnique { dest, .. } => {
                    life(&mut facts, L::Create)?;
                    state.bind(dest, Origin::source(S::Output)?);
                }
                IrInstruction::ReplaceUnique { dest, .. } => {
                    life(&mut facts, L::Replace)?;
                    state.bind(dest, Origin::source(S::Output)?);
                }
                IrInstruction::Transfer { dest, .. } => {
                    life(&mut facts, L::Transfer)?;
                    state.bind(dest, Origin::source(S::Output)?);
                }
                IrInstruction::Claim { dest, .. } => {
                    life(&mut facts, L::Claim)?;
                    state.bind(dest, Origin::source(S::Output)?);
                }
                IrInstruction::Settle { dest, .. } => {
                    life(&mut facts, L::Settle)?;
                    state.bind(dest, Origin::source(S::Output)?);
                }
                IrInstruction::Binary { dest, .. }
                | IrInstruction::Length { dest, .. }
                | IrInstruction::CollectionNew { dest, .. }
                | IrInstruction::CollectionCapacity { dest, .. }
                | IrInstruction::CollectionContains { dest, .. }
                | IrInstruction::CollectionRemove { dest, .. }
                | IrInstruction::CollectionPop { dest, .. }
                | IrInstruction::Tuple { dest, .. }
                | IrInstruction::EnumConstruct { dest, .. }
                | IrInstruction::EnumTag { dest, .. }
                | IrInstruction::EnumPayload { dest, .. } => state.bind(dest, Origin::value()),
                IrInstruction::BoundedPlanLoad { dest, found, .. } => {
                    state.bind(dest, Origin::value());
                    state.bind(found, Origin::value());
                }
                IrInstruction::AllocatedValue(operation) => {
                    // Arena owners and their copied fixed values are ordinary
                    // values. Leaving a result unbound makes its enclosing
                    // helper appear non-returning and drops later call facts.
                    if let Some(dest) = &operation.dest {
                        state.bind(dest, Origin::value());
                    }
                }
                IrInstruction::CollectionPush { .. }
                | IrInstruction::CollectionExtend { .. }
                | IrInstruction::CollectionClear { .. }
                | IrInstruction::CollectionInsert { .. }
                | IrInstruction::CollectionSet { .. }
                | IrInstruction::CollectionReverse { .. }
                | IrInstruction::CollectionTruncate { .. }
                | IrInstruction::CollectionSwap { .. } => {}
            }
        }
        if terminated || block.runtime_error.is_some() {
            continue;
        }
        let successors = match &block.terminator {
            IrTerminator::Return(value) => {
                if let Some(value) = value {
                    result.merge(&state.operand(value))?;
                } else {
                    result.merge(&Origin::value())?;
                }
                vec![]
            }
            IrTerminator::Jump(target) => vec![target.0],
            IrTerminator::Branch { then_block, else_block, .. } => vec![then_block.0, else_block.0],
        };
        for target in successors {
            if let Some(previous) = states.get_mut(&target) {
                let before = previous.clone();
                previous.merge(&state)?;
                if *previous != before {
                    pending.insert(target);
                }
            } else {
                states.insert(target, state.clone());
                pending.insert(target);
            }
        }
    }
    Ok(Analysis { direct: facts, calls: calls.into_values().collect(), returns: result })
}
pub(super) fn infer(module: &IrModule) -> Result<BTreeMap<String, EffectFacts>> {
    let callables = callables(module);
    if callables.len() > 4096 {
        return Err(invalid("effect callable budget exceeded"));
    }
    let mut returns = callables.iter().map(|c| (c.name.to_string(), Origin::default())).collect::<BTreeMap<_, _>>();
    if returns.len() != callables.len() {
        return Err(invalid("duplicate lowered effect callable"));
    }
    let mut steps = 0;
    loop {
        let mut changed = false;
        for callable in &callables {
            let analyzed = analyze(module, callable, &returns, false, &mut steps)?;
            let current = returns.get_mut(callable.name).ok_or_else(|| invalid("missing return-source accumulator"))?;
            let before = current.clone();
            current.merge(&analyzed.returns)?;
            changed |= *current != before;
        }
        if !changed {
            break;
        }
    }
    let mut graph = ResolvedFactGraph::default();
    for callable in &callables {
        let analyzed = analyze(module, callable, &returns, true, &mut steps)?;
        graph
            .insert(ResolvedCallable {
                id: CallableId { owner: module.name.clone(), name: callable.name.into() },
                parameter_count: u16::try_from(callable.params.len()).map_err(invalid)?,
                direct: analyzed.direct,
                calls: analyzed.calls,
            })
            .map_err(invalid)?;
    }
    let inferred = resolved::infer(&graph, &InferenceBudgets::default()).map_err(invalid)?;
    callables
        .into_iter()
        .map(|callable| {
            let id = CallableId { owner: module.name.clone(), name: callable.name.into() };
            Ok((callable.name.into(), inferred.get(&id).ok_or_else(|| invalid("missing inferred callable"))?.clone()))
        })
        .collect()
}
