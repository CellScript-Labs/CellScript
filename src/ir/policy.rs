//! Fixed-role Type Group binding for an explicitly selected artifact policy.
//!
//! This pass does not change ordinary entry compilation. It validates the
//! retained callable graph before committing any export binding changes. A
//! called action is not an entry wrapper, and an ordinary helper does not carry
//! a physical Cell-origin ABI merely because it has a type. Supported scalar
//! verification failures terminate execution rather than become return values.

use super::*;
use crate::artifact::{ArtifactContext, ArtifactDeclaration, ArtifactDispatch};

const MAX_COMMON_CALL_DEPTH: usize = 256;
const MAX_COMMON_CALLEE_BLOCKS: usize = 262_144;

pub(super) fn validate_artifact_selection(module: &IrModule, declaration: &ArtifactDeclaration) -> Result<()> {
    if declaration.canonicalized()? != *declaration {
        return Err(policy_error("selected artifact declaration is not numerically canonical"));
    }
    for name in declaration.actions.iter().map(|export| &export.action).chain(&declaration.common_checks) {
        let count = module.items.iter().filter(|item| matches!(item, IrItem::Action(action) if action.name == *name)).count();
        if count != 1 {
            return Err(policy_error(format!("selected artifact action '{name}' must identify exactly one retained action")));
        }
    }
    // Revalidation never mutates the module and never invokes entry selection
    // recursively. A forged or stale artifact choice must not silently repair
    // an absolute Cell plan during metadata or machine-code generation.
    let mut validated = module.clone();
    bind_artifact_policy(&mut validated, declaration)?;
    for (original, rebound) in module.items.iter().zip(&validated.items) {
        if let (IrItem::Action(original), IrItem::Action(rebound)) = (original, rebound)
            && (original.body.cell_bindings != rebound.body.cell_bindings || original.entry_trigger != rebound.entry_trigger)
        {
            return Err(policy_error(format!(
                "selected artifact action '{}' has a stale physical binding plan or trigger",
                original.name
            )));
        }
    }
    Ok(())
}

pub(crate) fn bind_artifact_policy(module: &mut IrModule, declaration: &ArtifactDeclaration) -> Result<()> {
    declaration.validate()?;
    let ArtifactContext::TypeGroup { resource } = &declaration.context;
    let ArtifactDispatch::PolicyWitnessV1 = declaration.dispatch;
    let schemas = module
        .items
        .iter()
        .filter_map(|item| match item {
            IrItem::TypeDef(schema) => Some(schema),
            _ => None,
        })
        .chain(module.external_type_defs.iter())
        .collect::<Vec<_>>();
    let matching = schemas.iter().filter(|schema| schema.name == *resource).collect::<Vec<_>>();
    if matching.len() != 1 || !cell_backed_kind(matching[0].kind) {
        return Err(policy_error(format!("type-group context '{resource}' must identify exactly one concrete Cell-backed schema")));
    }
    let cell_types = schemas.iter().filter(|schema| cell_backed_kind(schema.kind)).map(|schema| schema.name.as_str()).collect();
    let actions = module
        .items
        .iter()
        .filter_map(|item| match item {
            IrItem::Action(action) => Some((action.name.as_str(), action)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let helpers = module
        .items
        .iter()
        .filter_map(|item| match item {
            IrItem::Function(helper) => Some((helper.name.as_str(), helper)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut exports = BTreeSet::new();
    let mut tags = BTreeSet::new();
    let mut staged = BTreeMap::new();
    for export in &declaration.actions {
        if !exports.insert(export.action.as_str()) || !tags.insert(export.tag) {
            return Err(policy_error("export action names and dispatch tags must be unique"));
        }
        let action = actions
            .get(export.action.as_str())
            .ok_or_else(|| policy_error(format!("export '{}' must name a retained action", export.action)))?;
        validate_fixed_action(action, resource, &cell_types)?;
        let mut bindings = action.body.cell_bindings.clone();
        for binding in &mut bindings {
            match binding.source {
                IrCellSource::Input | IrCellSource::GroupInput => binding.source = IrCellSource::GroupInput,
                IrCellSource::Output | IrCellSource::GroupOutput => binding.source = IrCellSource::GroupOutput,
                IrCellSource::CellDep => continue,
            }
            binding.membership = IrCellMembership::CurrentTypeGroup;
        }
        staged.insert(export.action.clone(), bindings);
    }
    if exports.is_empty() {
        return Err(policy_error("a policy artifact requires at least one exported action"));
    }
    let mut common_checks = BTreeSet::new();
    let mut common_suffix_depths = BTreeMap::new();
    for name in &declaration.common_checks {
        if exports.contains(name.as_str()) || !common_checks.insert(name.as_str()) {
            return Err(policy_error(format!("common check '{name}' must be unique and must not also be exported")));
        }
        let action = actions.get(name.as_str()).ok_or_else(|| policy_error(format!("common check '{name}' must name an action")))?;
        validate_common_check(action)?;
        // Run before any recursive callable-ABI traversal. This is a common
        // policy restriction, not a new limit on ordinary/scoped exports.
        validate_common_call_depth(&action.body, &action.name, &actions, &helpers, &mut BTreeSet::new(), &mut common_suffix_depths)?;
    }
    let mut visited = BTreeSet::new();
    let mut active = BTreeSet::new();
    for (name, allow_unretained) in
        exports.into_iter().map(|name| (name, true)).chain(common_checks.into_iter().map(|name| (name, false)))
    {
        validate_calls(
            &actions[name].body,
            name,
            allow_unretained,
            &actions,
            &helpers,
            &cell_types,
            module,
            &mut visited,
            &mut active,
        )?;
    }
    // No partial rebinding on error: metadata and the emitter must observe the
    // same complete, validated plan.
    for item in &mut module.items {
        if let IrItem::Action(action) = item
            && let Some(bindings) = staged.remove(&action.name)
        {
            action.body.cell_bindings = bindings;
            action.entry_trigger = Some(format!("type-group<{resource}>"));
        }
    }
    Ok(())
}

fn policy_error(message: impl Into<String>) -> CompileError {
    CompileError::without_span(format!("policy-witness-v1 fixed Type Group boundary: {}", message.into())).with_code("E2101")
}

fn cell_backed_kind(kind: IrTypeKind) -> bool {
    matches!(kind, IrTypeKind::Resource | IrTypeKind::Shared | IrTypeKind::Receipt)
}

fn named_schema(ty: &IrType) -> Option<&str> {
    match ty {
        IrType::Named(name) => Some(name),
        IrType::Ref(inner) | IrType::MutRef(inner) => named_schema(inner),
        _ => None,
    }
}

fn cell_parameter(param: &IrParam, cell_types: &BTreeSet<&str>) -> bool {
    named_schema(&param.ty).is_some_and(|name| cell_types.contains(name))
        || matches!(param.ty, IrType::Ref(_) | IrType::MutRef(_))
        || param.is_ref
        || param.is_read_ref
}

fn has_lifecycle(body: &IrBody) -> bool {
    !body.consume_set.is_empty() || !body.create_set.is_empty() || !body.mutate_set.is_empty() || !body.write_intents.is_empty()
}

fn lifecycle_instruction(instruction: &IrInstruction) -> bool {
    matches!(
        instruction,
        IrInstruction::Consume { .. }
            | IrInstruction::Create { .. }
            | IrInstruction::Transfer { .. }
            | IrInstruction::Destroy { .. }
            | IrInstruction::Claim { .. }
            | IrInstruction::Settle { .. }
            | IrInstruction::CreateUnique { .. }
            | IrInstruction::ReplaceUnique { .. }
    )
}

fn validate_fixed_action(action: &IrAction, resource: &str, cell_types: &BTreeSet<&str>) -> Result<()> {
    if action.return_type.as_ref().is_some_and(|ty| !matches!(ty, IrType::Unit)) {
        return Err(policy_error(format!("export action '{}' must use the Unit action status contract", action.name)));
    }
    if !action.body.bounded_collection_ops.is_empty() {
        return Err(policy_error(format!("action '{}' mixes a fixed group policy with bounded Cell operations", action.name)));
    }
    let mut inputs = BTreeMap::new();
    let mut outputs = BTreeMap::new();
    let mut local_locations = BTreeMap::new();
    for binding in &action.body.cell_bindings {
        let source_kind = match binding.source {
            IrCellSource::Input | IrCellSource::GroupInput => 0,
            IrCellSource::Output | IrCellSource::GroupOutput => 1,
            IrCellSource::CellDep => 2,
        };
        if let Some(local_id) = binding.local_id
            && let Some(previous) = local_locations.insert(local_id, (source_kind, binding.ordinal))
            && previous != (source_kind, binding.ordinal)
        {
            return Err(policy_error(format!("action '{}' reuses local {local_id} for distinct physical Cells", action.name)));
        }
        if binding.source == IrCellSource::CellDep {
            if binding.role != IrCellBindingRole::ReadOnly || binding.membership != IrCellMembership::Unproven {
                return Err(policy_error(format!("action '{}' has a non-read-only or implicitly authenticated CellDep", action.name)));
            }
            continue;
        }
        if binding.ty != resource {
            return Err(policy_error(format!(
                "action '{}' cannot rebind '{}' of schema '{}' into type-group<{resource}>",
                action.name, binding.binding, binding.ty
            )));
        }
        let is_output = match binding.source {
            IrCellSource::Input | IrCellSource::GroupInput if binding.role != IrCellBindingRole::Output => false,
            IrCellSource::Output | IrCellSource::GroupOutput if binding.role == IrCellBindingRole::Output => true,
            _ => {
                return Err(policy_error(format!(
                    "action '{}' has an inconsistent source/role for '{}'",
                    action.name, binding.binding
                )))
            }
        };
        let slots = if is_output { &mut outputs } else { &mut inputs };
        if slots.insert(binding.ordinal, binding).is_some() {
            return Err(policy_error(format!("action '{}' aliases distinct roles at group ordinal {}", action.name, binding.ordinal)));
        }
    }
    if inputs.is_empty() && outputs.is_empty() {
        return Err(policy_error(format!(
            "action '{}' has a 0-to-0 group and cannot establish current Script membership",
            action.name
        )));
    }
    for (side, slots) in [("input", inputs), ("output", outputs)] {
        if slots.keys().copied().ne(0..slots.len()) {
            return Err(policy_error(format!("action '{}' requires dense, explicitly accounted group-{side} roles", action.name)));
        }
    }
    for param in &action.params {
        if param.source != ParamSource::LockArgs
            && cell_parameter(param, cell_types)
            && action.body.cell_binding_for_local(param.binding.id).is_none()
        {
            return Err(policy_error(format!(
                "action '{}' Cell parameter '{}' has no resolved physical binding",
                action.name, param.name
            )));
        }
    }
    validate_fixed_role_paths(&action.name, &action.body)
}

fn successors(block: &IrBlock) -> Vec<BlockId> {
    match block.terminator {
        IrTerminator::Return(_) => Vec::new(),
        IrTerminator::Jump(target) => vec![target],
        IrTerminator::Branch { then_block, else_block, .. } => vec![then_block, else_block],
    }
}

fn reachable_blocks(body: &IrBody, start: BlockId, excluded: Option<BlockId>) -> BTreeSet<usize> {
    let mut visited = BTreeSet::new();
    let mut pending = vec![start];
    while let Some(id) = pending.pop() {
        if Some(id) == excluded || !visited.insert(id.0) {
            continue;
        }
        if let Some(block) = body.blocks.iter().find(|block| block.id == id) {
            pending.extend(successors(block));
        }
    }
    visited
}

fn validate_fixed_role_paths(name: &str, body: &IrBody) -> Result<()> {
    let Some(entry) = body.blocks.first() else { return Err(policy_error(format!("action '{name}' has no entry block"))) };
    let reachable = reachable_blocks(body, entry.id, None);
    let accepting = body
        .blocks
        .iter()
        .filter(|block| {
            reachable.contains(&block.id.0) && block.runtime_error.is_none() && matches!(block.terminator, IrTerminator::Return(_))
        })
        .map(|block| block.id.0)
        .collect::<BTreeSet<_>>();
    for block in body.blocks.iter().filter(|block| block.instructions.iter().any(lifecycle_instruction)) {
        let bypass = reachable_blocks(body, entry.id, Some(block.id));
        let repeated = successors(block).iter().any(|next| reachable_blocks(body, *next, None).contains(&block.id.0));
        if !reachable.contains(&block.id.0) || !accepting.is_disjoint(&bypass) || repeated {
            return Err(policy_error(format!(
                "action '{name}' has branch-varying or repeated Cell lifecycle roles; path-sensitive group plans are not supported yet"
            )));
        }
    }
    Ok(())
}

fn validate_common_check(action: &IrAction) -> Result<()> {
    if action.body.blocks.len() > MAX_COMMON_CALLEE_BLOCKS {
        return Err(policy_error("policy common check has excessive blocks"));
    }
    if !action.params.is_empty()
        || action.return_type.as_ref().is_some_and(|ty| !matches!(ty, IrType::Unit))
        || has_lifecycle(&action.body)
        || !action.body.bounded_collection_ops.is_empty()
        || action.body.cell_bindings.iter().any(|binding| {
            binding.source != IrCellSource::CellDep
                || binding.role != IrCellBindingRole::ReadOnly
                || binding.membership != IrCellMembership::Unproven
        })
        || action.body.blocks.iter().flat_map(|block| &block.instructions).any(lifecycle_instruction)
    {
        return Err(policy_error(format!(
            "common check '{}' must be a zero-parameter Unit action with no lifecycle or input/output Cells; explicit read_ref dependencies and retained bounded scalar calls are allowed",
            action.name
        )));
    }
    Ok(())
}

fn validate_common_call_depth<'a>(
    body: &'a IrBody,
    owner: &'a str,
    actions: &BTreeMap<&str, &'a IrAction>,
    helpers: &BTreeMap<&str, &'a IrFunction>,
    active: &mut BTreeSet<&'a str>,
    suffix_depths: &mut BTreeMap<&'a str, usize>,
) -> Result<usize> {
    if active.contains(owner) {
        return Err(policy_error(format!("recursive policy call '{owner}' has no bounded failure-propagation contract")));
    }
    if let Some(depth) = suffix_depths.get(owner).copied() {
        if active.len() + depth > MAX_COMMON_CALL_DEPTH {
            return Err(policy_error("policy common call graph exceeds the call-depth bound of 256, including the common action"));
        }
        return Ok(depth);
    }
    if active.len() >= MAX_COMMON_CALL_DEPTH {
        return Err(policy_error("policy common call graph exceeds the call-depth bound of 256, including the common action"));
    }
    if body.blocks.len() > MAX_COMMON_CALLEE_BLOCKS {
        return Err(policy_error("policy common callee has excessive blocks"));
    }
    active.insert(owner);
    let mut longest_suffix = 1;
    for instruction in body.blocks.iter().flat_map(|block| &block.instructions) {
        let IrInstruction::Call { func, .. } = instruction else { continue };
        let callee =
            actions.get(func.as_str()).map(|action| &action.body).or_else(|| helpers.get(func.as_str()).map(|helper| &helper.body));
        let Some(callee) = callee else {
            // The retained-call ABI validator rejects these separately. This
            // preflight must not invent an external body's depth or contract.
            continue;
        };
        let depth = validate_common_call_depth(callee, func, actions, helpers, active, suffix_depths)?;
        longest_suffix = longest_suffix.max(1 + depth);
    }
    active.remove(owner);
    suffix_depths.insert(owner, longest_suffix);
    Ok(longest_suffix)
}

#[allow(clippy::too_many_arguments)]
fn validate_calls<'a>(
    body: &IrBody,
    owner: &str,
    allow_unretained: bool,
    actions: &BTreeMap<&'a str, &'a IrAction>,
    helpers: &BTreeMap<&'a str, &'a IrFunction>,
    cell_types: &BTreeSet<&str>,
    module: &IrModule,
    visited: &mut BTreeSet<String>,
    active: &mut BTreeSet<String>,
) -> Result<()> {
    for instruction in body.blocks.iter().flat_map(|block| &block.instructions) {
        let IrInstruction::Call { func, .. } = instruction else { continue };
        if let Some(callee) = actions.get(func.as_str()) {
            if has_lifecycle(&callee.body)
                || !callee.body.cell_bindings.is_empty()
                || !callee.body.bounded_collection_ops.is_empty()
                || callee.params.iter().any(|param| cell_parameter(param, cell_types))
            {
                return Err(policy_error(format!(
                    "'{owner}' calls Cell-bearing/stateful action '{func}'; entry bindings cannot be reused as a caller-bound Cell ABI"
                )));
            }
            validate_bounded_signature(func, &callee.params, callee.return_type.as_ref())?;
            validate_bounded_body(func, &callee.body, helpers, actions)?;
        } else if let Some(callee) = helpers.get(func.as_str()) {
            validate_bounded_signature(func, &callee.params, callee.return_type.as_ref())?;
            validate_bounded_body(func, &callee.body, helpers, actions)?;
        } else {
            if !allow_unretained || module.external_callable_abis.iter().any(|callee| callee.name == *func) {
                return Err(policy_error(format!("'{owner}' calls external '{func}' without a retained, failure-auditable body")));
            }
            // Export-owned builtins remain subject to the existing executable
            // surface classifier. They do not introduce another entry wrapper.
            continue;
        }
        if active.contains(func) {
            return Err(policy_error(format!("recursive policy call '{func}' has no bounded failure-propagation contract")));
        }
        if visited.contains(func) {
            continue;
        }
        active.insert(func.clone());
        let callee_body = helpers.get(func.as_str()).map(|callee| &callee.body).unwrap_or_else(|| &actions[func.as_str()].body);
        validate_calls(callee_body, func, false, actions, helpers, cell_types, module, visited, active)?;
        active.remove(func);
        visited.insert(func.clone());
    }
    Ok(())
}

fn bounded_value_type(ty: &IrType) -> bool {
    matches!(ty, IrType::U8 | IrType::U16 | IrType::U32 | IrType::I32 | IrType::U64 | IrType::Bool | IrType::Unit)
        || matches!(ty, IrType::Ref(inner) if matches!(inner.as_ref(), IrType::Named(_)))
        || matches!(ty, IrType::Tuple(items) if items.is_empty())
}

fn bounded_value(operand: &IrOperand) -> bool {
    match operand {
        IrOperand::Var(var) => bounded_value_type(&var.ty),
        IrOperand::Const(IrConst::Unit) => true,
        _ => bounded_scalar(operand),
    }
}

fn validate_bounded_signature(name: &str, params: &[IrParam], return_type: Option<&IrType>) -> Result<()> {
    if params.iter().any(|param| param.is_mut || !bounded_value_type(&param.ty))
        // Immutable data-view parameters use the existing caller-bound ABI.
        // Returning a view would additionally need a length/origin channel;
        // public source already prohibits references escaping a callable.
        || return_type.is_some_and(|ty| matches!(ty, IrType::Ref(_)) || !bounded_value_type(ty))
    {
        return Err(policy_error(format!(
            "callee '{name}' has an aggregate, mutable, wide-value, or reference-return ABI outside the bounded scalar/data-view-parameter policy call contract"
        )));
    }
    Ok(())
}

fn validate_bounded_body(
    name: &str,
    body: &IrBody,
    helpers: &BTreeMap<&str, &IrFunction>,
    actions: &BTreeMap<&str, &IrAction>,
) -> Result<()> {
    if has_lifecycle(body) || !body.cell_bindings.is_empty() || !body.bounded_collection_ops.is_empty() {
        return Err(policy_error(format!(
            "callee '{name}' owns physical Cell operations; policy calls currently support caller-bound values only"
        )));
    }
    for block in &body.blocks {
        if successors(block).iter().any(|next| reachable_blocks(body, *next, None).contains(&block.id.0)) {
            return Err(policy_error(format!("callee '{name}' may repeat; policy calls require an acyclic bounded body")));
        }
        if let Some(error) = block.runtime_error
            && (!matches!(
                error,
                CellScriptRuntimeError::AssertionFailed
                    | CellScriptRuntimeError::NumericOrDiscriminantInvalid
                    | CellScriptRuntimeError::ShiftAmountInvalid
            ) || !matches!(block.terminator, IrTerminator::Return(_)))
        {
            return Err(policy_error(format!("callee '{name}' has a runtime failure outside the bounded scalar verifier contract")));
        }
        if let IrTerminator::Return(Some(value)) = &block.terminator
            && !bounded_value(value)
        {
            return Err(policy_error(format!("callee '{name}' returns a value outside the bounded policy call ABI")));
        }
        for instruction in &block.instructions {
            let safe = match instruction {
                IrInstruction::LoadConst { dest, value } => {
                    bounded_value_type(&dest.ty) && bounded_value(&IrOperand::Const(value.clone()))
                }
                IrInstruction::LoadVar { dest, .. } => bounded_value_type(&dest.ty),
                IrInstruction::StoreVar { src, .. } => bounded_value(src),
                IrInstruction::Move { dest, src } => bounded_value_type(&dest.ty) && bounded_value(src),
                IrInstruction::Tuple { dest, fields } => {
                    fields.is_empty() && (dest.ty == IrType::Unit || matches!(&dest.ty, IrType::Tuple(items) if items.is_empty()))
                }
                IrInstruction::Binary { dest, left, right, .. } => {
                    bounded_scalar(&IrOperand::Var(dest.clone())) && bounded_scalar(left) && bounded_scalar(right)
                }
                IrInstruction::Unary { op: UnaryOp::Not, dest, operand } => bounded_value_type(&dest.ty) && bounded_scalar(operand),
                IrInstruction::Call { dest, func, args } => {
                    (helpers.contains_key(func.as_str()) || actions.contains_key(func.as_str()))
                        && dest.as_ref().is_none_or(|dest| bounded_value_type(&dest.ty))
                        && args.iter().all(bounded_value)
                }
                _ => false,
            };
            if !safe {
                return Err(policy_error(format!(
                    "callee '{name}' requires physical Cell metadata, field/index checks, or an unsupported runtime operation; keep it export-local until that bounded caller ABI is supported"
                )));
            }
        }
    }
    Ok(())
}

fn bounded_scalar(operand: &IrOperand) -> bool {
    match operand {
        IrOperand::Var(var) => matches!(var.ty, IrType::U8 | IrType::U16 | IrType::U32 | IrType::I32 | IrType::U64 | IrType::Bool),
        IrOperand::Const(constant) => {
            matches!(constant, IrConst::U8(_) | IrConst::U16(_) | IrConst::U32(_) | IrConst::U64(_) | IrConst::Bool(_))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::ArtifactAction;

    fn declaration(names: &[&str]) -> ArtifactDeclaration {
        ArtifactDeclaration {
            name: "TokenPolicy".to_string(),
            context: ArtifactContext::TypeGroup { resource: "Token".to_string() },
            dispatch: ArtifactDispatch::PolicyWitnessV1,
            actions: names
                .iter()
                .enumerate()
                .map(|(tag, name)| ArtifactAction { tag: tag as u32, action: (*name).to_string() })
                .collect(),
            common_checks: Vec::new(),
        }
    }

    fn module() -> IrModule {
        let source = r#"
module policy_binding
resource Token { amount: u64 }
resource Config { limit: u64 }
action main() { verification }
action auxiliary() { verification }
fn safe(value: u64) -> bool { return value > 0 }
fn checked(value: u64) -> u64 { return value + 1 }
fn divide(value: u64) -> u64 { return 8 / value }
fn narrow(value: u64) -> u8 { return value as u8 }
fn shift(value: u64) -> u64 { return 1 << value }
fn fields(value: &Token) -> u64 { return value.amount }
fn view(value: &Token) -> &Token { return value }
fn view_scalar(value: &Token) -> u64 { return 7 }
fn wide(value: Hash) -> Hash { return value }
fn empty(value: ()) -> bool { return true }
fn unit() -> () { return () }
action checked_action(value: u64) {
    verification
    require value > 0
    let _ = divide(value)
    let _ = narrow(value)
}
action scalar_action(value: u64) -> u64 {
    verification
    require value > 0
    return checked(value)
}
action common_graph() {
    verification
    checked_action(1)
    let _ = scalar_action(1)
}
"#;
        let ast = crate::frontend::parse(source, crate::CellScriptEdition::Edition2026).unwrap();
        crate::ir::generate(&ast).unwrap()
    }

    fn action_mut<'a>(module: &'a mut IrModule, name: &str) -> &'a mut IrAction {
        module
            .items
            .iter_mut()
            .find_map(|item| match item {
                IrItem::Action(action) if action.name == name => Some(action),
                _ => None,
            })
            .unwrap()
    }

    fn role(binding: &str, source: IrCellSource, ordinal: usize, local_id: usize) -> IrCellBinding {
        IrCellBinding {
            binding: binding.to_string(),
            role: match source {
                IrCellSource::Input | IrCellSource::GroupInput => IrCellBindingRole::Input,
                IrCellSource::Output | IrCellSource::GroupOutput => IrCellBindingRole::Output,
                IrCellSource::CellDep => IrCellBindingRole::ReadOnly,
            },
            local_id: Some(local_id),
            ty: if source == IrCellSource::CellDep { "Config" } else { "Token" }.to_string(),
            source,
            ordinal,
            membership: IrCellMembership::Unproven,
        }
    }

    fn set_roles(module: &mut IrModule, name: &str, inputs: usize, outputs: usize) {
        action_mut(module, name).body.cell_bindings = (0..inputs)
            .map(|ordinal| role(&format!("before{ordinal}"), IrCellSource::Input, ordinal, ordinal))
            .chain((0..outputs).map(|ordinal| role(&format!("after{ordinal}"), IrCellSource::Output, ordinal, inputs + ordinal)))
            .collect();
    }

    #[test]
    fn fixed_mint_transfer_merge_and_burn_keep_independent_zero_sides() {
        for (inputs, outputs) in [(0, 1), (1, 1), (2, 1), (1, 0)] {
            let mut module = module();
            set_roles(&mut module, "main", inputs, outputs);
            let dependency = role("config", IrCellSource::CellDep, 2, inputs + outputs);
            action_mut(&mut module, "main").body.cell_bindings.push(dependency.clone());
            bind_artifact_policy(&mut module, &declaration(&["main"])).unwrap();
            let action = action_mut(&mut module, "main");
            assert_eq!(action.entry_trigger.as_deref(), Some("type-group<Token>"));
            assert_eq!(action.body.cell_bindings.iter().filter(|binding| binding.source == IrCellSource::GroupInput).count(), inputs);
            assert_eq!(
                action.body.cell_bindings.iter().filter(|binding| binding.source == IrCellSource::GroupOutput).count(),
                outputs
            );
            assert_eq!(action.body.cell_bindings.last(), Some(&dependency));
            assert!(action
                .body
                .cell_bindings
                .iter()
                .filter(|binding| binding.source != IrCellSource::CellDep)
                .all(|binding| binding.membership == IrCellMembership::CurrentTypeGroup));
        }
    }

    #[test]
    fn invalid_roles_reject_without_partially_rebinding_an_earlier_export() {
        for invalid in ["cross-schema", "gap", "alias", "duplicate", "reused-local", "dep-local", "empty"] {
            let mut module = module();
            set_roles(&mut module, "main", 1, 1);
            set_roles(&mut module, "auxiliary", 1, 1);
            let invalid_action = action_mut(&mut module, "auxiliary");
            match invalid {
                "cross-schema" => invalid_action.body.cell_bindings[0].ty = "Config".to_string(),
                "gap" => invalid_action.body.cell_bindings[0].ordinal = 1,
                "alias" => invalid_action.body.cell_bindings.push(role("other", IrCellSource::Input, 0, 5)),
                "duplicate" => invalid_action.body.cell_bindings.push(invalid_action.body.cell_bindings[0].clone()),
                "reused-local" => invalid_action.body.cell_bindings[1].local_id = Some(0),
                "dep-local" => invalid_action.body.cell_bindings.push(role("config", IrCellSource::CellDep, 0, 0)),
                "empty" => invalid_action.body.cell_bindings.clear(),
                _ => unreachable!(),
            }
            assert!(bind_artifact_policy(&mut module, &declaration(&["main", "auxiliary"])).is_err(), "{invalid}");
            let unchanged = action_mut(&mut module, "main");
            assert!(unchanged.entry_trigger.is_none(), "{invalid}");
            assert_eq!(unchanged.body.cell_bindings[0].source, IrCellSource::Input, "{invalid}");
            assert_eq!(unchanged.body.cell_bindings[0].membership, IrCellMembership::Unproven, "{invalid}");
        }
    }

    #[test]
    fn called_exports_cannot_rebind_the_callers_cells() {
        let mut module = module();
        set_roles(&mut module, "main", 1, 1);
        set_roles(&mut module, "auxiliary", 1, 1);
        action_mut(&mut module, "main").body.blocks[0].instructions.push(IrInstruction::Call {
            dest: None,
            func: "auxiliary".to_string(),
            args: Vec::new(),
        });
        let error = bind_artifact_policy(&mut module, &declaration(&["main", "auxiliary"])).unwrap_err();
        assert!(error.to_string().contains("caller-bound Cell ABI"));
    }

    #[test]
    fn bounded_scalar_failures_remain_legal_in_retained_policy_callees() {
        for callee in ["checked", "divide", "narrow", "shift", "checked_action", "scalar_action", "unit"] {
            let mut module = module();
            set_roles(&mut module, "main", 0, 1);
            action_mut(&mut module, "main").body.blocks[0].instructions.push(IrInstruction::Call {
                dest: None,
                func: callee.to_string(),
                args: if callee == "unit" { Vec::new() } else { vec![IrOperand::Const(IrConst::U64(1))] },
            });
            bind_artifact_policy(&mut module, &declaration(&["main"])).unwrap_or_else(|error| panic!("{callee}: {error}"));
            let body = module
                .items
                .iter()
                .find_map(|item| match item {
                    IrItem::Function(helper) if helper.name == callee => Some(&helper.body),
                    IrItem::Action(action) if action.name == callee => Some(&action.body),
                    _ => None,
                })
                .unwrap();
            assert!(body.cell_bindings.is_empty());
            if callee == "narrow" {
                assert!(body
                    .blocks
                    .iter()
                    .any(|block| block.runtime_error == Some(CellScriptRuntimeError::NumericOrDiscriminantInvalid)));
                assert!(body
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .any(|instruction| matches!(instruction, IrInstruction::Move { dest, .. } if dest.ty == IrType::U8)));
            }
            if callee.ends_with("action") {
                assert!(body.blocks.iter().any(|block| block.runtime_error == Some(CellScriptRuntimeError::AssertionFailed)));
            }
        }
    }

    #[test]
    fn common_checks_validate_the_complete_retained_call_graph_before_binding() {
        let mut module = module();
        set_roles(&mut module, "main", 0, 1);
        let mut selected = declaration(&["main"]);
        selected.common_checks.push("common_graph".to_string());
        bind_artifact_policy(&mut module, &selected).unwrap();
        assert!(action_mut(&mut module, "common_graph").entry_trigger.is_none());
        set_roles(&mut module, "auxiliary", 1, 1);
        action_mut(&mut module, "checked_action").body.blocks[0].instructions.push(IrInstruction::Call {
            dest: None,
            func: "auxiliary".to_string(),
            args: Vec::new(),
        });
        assert!(bind_artifact_policy(&mut module, &selected).unwrap_err().to_string().contains("caller-bound Cell ABI"));
    }

    #[test]
    fn common_graphs_reject_recursion_and_unretained_calls() {
        for case in ["recursive", "unknown", "external"] {
            let mut module = module();
            set_roles(&mut module, "main", 0, 1);
            let mut selected = declaration(&["main"]);
            selected.common_checks.push("common_graph".to_string());
            let (owner, target) =
                if case == "recursive" { ("checked_action", "checked_action") } else { ("common_graph", "unretained") };
            action_mut(&mut module, owner).body.blocks[0].instructions.push(IrInstruction::Call {
                dest: None,
                func: target.to_string(),
                args: vec![IrOperand::Const(IrConst::U64(1))],
            });
            if case == "external" {
                module.external_callable_abis.push(IrCallableAbi {
                    name: target.to_string(),
                    params: Vec::new(),
                    type_hash_param_indices: BTreeSet::new(),
                });
            }
            let error = bind_artifact_policy(&mut module, &selected).unwrap_err().to_string();
            assert!(
                error.contains(if case == "recursive" { "recursive policy call" } else { "without a retained" }),
                "{case}: {error}"
            );
            let export = action_mut(&mut module, "main");
            assert!(export.entry_trigger.is_none(), "failed common graph must not partially bind exports");
            assert_eq!(export.body.cell_bindings[0].source, IrCellSource::Output);
        }
    }

    fn add_unit_chain(module: &mut IrModule, prefix: &str, length: usize, tail: Option<&str>) -> String {
        let template = module
            .items
            .iter()
            .find_map(|item| match item {
                IrItem::Function(helper) if helper.name == "unit" => Some(helper.clone()),
                _ => None,
            })
            .unwrap();
        for index in 1..=length {
            let mut helper = template.clone();
            helper.name = format!("{prefix}{index}");
            let next = if index < length { Some(format!("{prefix}{}", index + 1)) } else { tail.map(str::to_string) };
            if let Some(next) = next {
                helper.body.blocks[0].instructions.push(IrInstruction::Call { dest: None, func: next, args: Vec::new() });
            }
            module.items.push(IrItem::Function(helper));
        }
        format!("{prefix}1")
    }

    #[test]
    fn common_call_depth_is_root_inclusive_and_counts_shared_suffixes_in_either_order() {
        for head_length in [200, 55, 56] {
            for tail_first in [true, false] {
                let mut module = module();
                set_roles(&mut module, "main", 0, 1);
                let tail = add_unit_chain(&mut module, "tail", 200, None);
                let head = add_unit_chain(&mut module, "head", head_length, Some(&tail));
                let calls = if tail_first { [tail, head] } else { [head, tail] };
                action_mut(&mut module, "auxiliary").body.blocks[0]
                    .instructions
                    .extend(calls.into_iter().map(|func| IrInstruction::Call { dest: None, func, args: Vec::new() }));
                let mut selected = declaration(&["main"]);
                selected.common_checks.push("auxiliary".to_string());
                let result = bind_artifact_policy(&mut module, &selected);
                let actual_path = 1 + head_length + 200;
                if actual_path <= 256 {
                    result.unwrap();
                } else {
                    assert!(
                        result.unwrap_err().message.contains("call-depth bound"),
                        "actual path {actual_path}, tail_first={tail_first}"
                    );
                    assert!(
                        action_mut(&mut module, "main").entry_trigger.is_none(),
                        "depth rejection must not partially bind exports"
                    );
                }
            }
        }
    }

    #[test]
    fn common_depth_limit_does_not_add_an_export_only_policy_restriction() {
        let mut module = module();
        set_roles(&mut module, "main", 0, 1);
        let head = add_unit_chain(&mut module, "chain", 256, None);
        action_mut(&mut module, "main").body.blocks[0].instructions.push(IrInstruction::Call {
            dest: None,
            func: head,
            args: Vec::new(),
        });
        bind_artifact_policy(&mut module, &declaration(&["main"])).unwrap();
    }

    #[test]
    fn bounded_callees_keep_mutable_repetition_and_unsupported_failure_boundaries() {
        for case in ["mutable", "repeat", "unsupported-error"] {
            let mut module = module();
            set_roles(&mut module, "main", 0, 1);
            action_mut(&mut module, "main").body.blocks[0].instructions.push(IrInstruction::Call {
                dest: None,
                func: "checked".to_string(),
                args: vec![IrOperand::Const(IrConst::U64(1))],
            });
            let helper = module
                .items
                .iter_mut()
                .find_map(|item| match item {
                    IrItem::Function(helper) if helper.name == "checked" => Some(helper),
                    _ => None,
                })
                .unwrap();
            let expected = match case {
                "mutable" => {
                    helper.params[0].is_mut = true;
                    "mutable"
                }
                "repeat" => {
                    helper.body.blocks[0].terminator = IrTerminator::Jump(helper.body.blocks[0].id);
                    "may repeat"
                }
                "unsupported-error" => {
                    helper.body.blocks[0].runtime_error = Some(CellScriptRuntimeError::SighashAllUnsupported);
                    "outside the bounded scalar verifier contract"
                }
                _ => unreachable!(),
            };
            let error = bind_artifact_policy(&mut module, &declaration(&["main"])).unwrap_err().to_string();
            assert!(error.contains(expected), "{case}: {error}");
        }
    }

    #[test]
    fn common_checks_use_unit_action_status_and_cannot_hide_nested_failures() {
        let mut module = module();
        set_roles(&mut module, "main", 0, 1);
        let dependency = role("config", IrCellSource::CellDep, 0, 0);
        action_mut(&mut module, "auxiliary").body.cell_bindings.push(dependency.clone());
        let mut declaration = declaration(&["main"]);
        declaration.common_checks.push("auxiliary".to_string());
        bind_artifact_policy(&mut module, &declaration).unwrap();
        assert_eq!(action_mut(&mut module, "auxiliary").body.cell_bindings, vec![dependency]);
        assert!(action_mut(&mut module, "auxiliary").entry_trigger.is_none());
        action_mut(&mut module, "auxiliary").body.blocks[0].instructions.push(IrInstruction::Call {
            dest: None,
            func: "safe".to_string(),
            args: vec![IrOperand::Const(IrConst::U64(1))],
        });
        bind_artifact_policy(&mut module, &declaration).unwrap();
    }

    #[test]
    fn bounded_scalar_helpers_exclude_field_and_wide_operations() {
        for (helper, accepted) in [
            ("safe", true),
            ("view", false),
            ("view_scalar", true),
            ("empty", true),
            ("checked", true),
            ("fields", false),
            ("wide", false),
        ] {
            let mut module = module();
            set_roles(&mut module, "main", 0, 1);
            action_mut(&mut module, "main").body.blocks[0].instructions.push(IrInstruction::Call {
                dest: None,
                func: helper.to_string(),
                args: vec![IrOperand::Const(IrConst::U64(1))],
            });
            assert_eq!(bind_artifact_policy(&mut module, &declaration(&["main"])).is_ok(), accepted, "{helper}");
        }
        // The raw IR fixture deliberately bypasses the source return-type
        // validator. Closing its unsupported return ABI loses no public
        // Edition 2026/2027 feature and does not ban immutable view parameters.
        let source = "module reference_boundary\nresource Token { amount: u64 }\nfn view(value: &Token) -> &Token { return value }\n";
        for edition in [crate::CellScriptEdition::Edition2026, crate::CellScriptEdition::Edition2027] {
            let module = crate::frontend::parse(source, edition).unwrap();
            assert!(crate::types::check(&module).unwrap_err().message.contains("references cannot escape callable boundaries"));
            let scalar = source.replace("-> &Token { return value }", "-> u64 { return 7 }");
            crate::types::check(&crate::frontend::parse(&scalar, edition).unwrap()).unwrap();
        }
    }

    #[test]
    fn explicit_artifact_selection_never_falls_back_or_repairs_stale_bindings() {
        let mut module = module();
        set_roles(&mut module, "main", 1, 1);
        let selected = declaration(&["main"]);
        module.entry_selection = IrEntrySelection::Artifact(selected.clone());
        assert!(module.resolved_entry().is_none());
        assert!(module.validate_entry_selection().unwrap_err().to_string().contains("stale"));
        assert_eq!(action_mut(&mut module, "main").body.cell_bindings[0].source, IrCellSource::Input);
        bind_artifact_policy(&mut module, &selected).unwrap();
        module.validate_entry_selection().unwrap();
        action_mut(&mut module, "main").entry_trigger = None;
        assert!(module.validate_entry_selection().unwrap_err().to_string().contains("stale"));
        module.entry_selection = IrEntrySelection::Artifact(declaration(&["missing"]));
        assert!(module.validate_entry_selection().unwrap_err().to_string().contains("exactly one"));
    }

    #[test]
    fn policy_entry_reference_parameters_require_an_actual_physical_source() {
        for inner in [IrType::U64, IrType::Named("UnboundView".to_string())] {
            let mut module = module();
            set_roles(&mut module, "main", 0, 1);
            let ty = IrType::Ref(Box::new(inner));
            action_mut(&mut module, "main").params.push(IrParam {
                name: "view".to_string(),
                ty: ty.clone(),
                is_mut: false,
                is_ref: false,
                is_read_ref: false,
                source: ParamSource::Default,
                binding: IrVar { id: 55, name: "view".to_string(), ty },
            });
            assert!(bind_artifact_policy(&mut module, &declaration(&["main"]))
                .unwrap_err()
                .to_string()
                .contains("no resolved physical binding"));
        }
    }

    #[test]
    fn lifecycle_roles_must_dominate_normal_returns_and_not_repeat() {
        let mut module = module();
        set_roles(&mut module, "main", 1, 0);
        let action = action_mut(&mut module, "main");
        let consume = IrInstruction::Consume {
            operand: IrOperand::Var(IrVar { id: 0, name: "before0".to_string(), ty: IrType::Named("Token".to_string()) }),
        };
        action.body.blocks = vec![
            IrBlock {
                id: BlockId(0),
                instructions: Vec::new(),
                terminator: IrTerminator::Branch {
                    cond: IrOperand::Const(IrConst::Bool(true)),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
                runtime_error: None,
            },
            IrBlock { id: BlockId(1), instructions: vec![consume], terminator: IrTerminator::Return(None), runtime_error: None },
            IrBlock { id: BlockId(2), instructions: Vec::new(), terminator: IrTerminator::Return(None), runtime_error: None },
        ];
        assert!(bind_artifact_policy(&mut module, &declaration(&["main"])).unwrap_err().to_string().contains("branch-varying"));
        action_mut(&mut module, "main").body.blocks[2].runtime_error = Some(CellScriptRuntimeError::AssertionFailed);
        bind_artifact_policy(&mut module, &declaration(&["main"])).unwrap();
        action_mut(&mut module, "main").body.blocks[1].terminator = IrTerminator::Jump(BlockId(1));
        assert!(bind_artifact_policy(&mut module, &declaration(&["main"])).unwrap_err().to_string().contains("repeated"));
    }
}
