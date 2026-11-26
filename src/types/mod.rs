use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use crate::resolve::{FunctionDef, ModuleResolver, TypeDef};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

mod assignments;
mod call_arguments;
mod cell_types;
mod collection_calls;
mod enum_values;
mod expressions;
mod initializers;
mod numeric_types;
mod patterns;
mod return_flow;
mod stdlib_calls;
mod tail_values;

mod actions;
mod allocated;
mod callable_checks;
mod calls;
mod cell_selection;
mod data_declarations;
mod flow_states;
mod interface_roles;
mod nested;
mod parameters;
mod projections;
mod references;
mod spawn_ipc;
mod statement_checks;
mod transaction_views;
mod type_containment;
mod type_validation;
pub(crate) use interface_roles::validate_source_open_handle_roles;
use interface_roles::OpenHandleRoleContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallableKind {
    Action,
    Function,
    Invariant,
    Lock,
}

#[derive(Debug, Clone)]
struct EffectRequirement {
    target: String,
    empty: bool,
    span: Span,
    reason: &'static str,
}

#[derive(Debug, Clone)]
struct FunctionSignature {
    params: Vec<Type>,
    return_type: Option<Type>,
    kind: CallableKind,
}

#[derive(Debug, Clone)]
struct MatchPayloadBindings {
    bindings: Vec<(String, Type)>,
}

#[derive(Debug, Clone)]
struct MatchPatternAnalysis {
    bindings: Vec<(String, Type)>,
    covered_variants: Vec<String>,
    irrefutable: bool,
}

#[derive(Debug, Clone)]
struct MatchConstructor {
    name: String,
    fields: Vec<(Option<String>, Type)>,
}

const MATCH_EXHAUSTIVENESS_BUDGET: usize = 4096;

#[derive(Debug, Clone)]
struct FlowSpec {
    type_name: String,
    field_name: String,
    field_enum_type: Option<String>,
    states: Vec<String>,
    transitions: Vec<StateTransition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct OutputFieldObligation {
    output: String,
    field: String,
}

#[derive(Debug, Clone)]
struct ActionOutputBinding {
    type_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CellTypeKind {
    Resource,
    Shared,
    Receipt,
}

pub struct TypeEnv {
    vars: HashMap<String, Type>,
    mutability: HashMap<String, bool>,
    linear_states: HashMap<String, LinearState>,
    affine_owners: HashSet<String>,
    parent: Option<Box<TypeEnv>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinearState {
    Available,
    Consumed,
    Transferred,
    Destroyed,
    /// Every accepting alternative has discharged ownership, through different
    /// operations. The individual business dispositions remain in source/IR.
    Disposed,
}

impl LinearState {
    fn join(self, other: Self) -> Option<Self> {
        if self == other {
            Some(self)
        } else if self != Self::Available && other != Self::Available {
            Some(Self::Disposed)
        } else {
            None
        }
    }
}

impl Default for TypeEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeEnv {
    pub fn new() -> Self {
        Self {
            vars: HashMap::new(),
            mutability: HashMap::new(),
            linear_states: HashMap::new(),
            affine_owners: HashSet::new(),
            parent: None,
        }
    }

    pub fn child(&self) -> Self {
        Self {
            vars: HashMap::new(),
            mutability: HashMap::new(),
            linear_states: HashMap::new(),
            affine_owners: HashSet::new(),
            parent: Some(Box::new(self.clone())),
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&Type> {
        self.vars.get(name).or_else(|| self.parent.as_ref().and_then(|p| p.lookup(name)))
    }

    pub fn is_mutable(&self, name: &str) -> bool {
        self.mutability.get(name).copied().or_else(|| self.parent.as_ref().map(|p| p.is_mutable(name))).unwrap_or(false)
    }

    pub fn insert(&mut self, name: String, ty: Type, is_linear: bool, is_mut: bool) {
        self.affine_owners.remove(&name);
        self.vars.insert(name.clone(), ty);
        self.mutability.insert(name.clone(), is_mut);
        if is_linear {
            self.linear_states.insert(name, LinearState::Available);
        } else {
            self.linear_states.remove(&name);
        }
    }

    fn bind_new(&mut self, name: String, ty: Type, is_linear: bool, is_mut: bool, span: Span) -> Result<()> {
        if self.lookup(&name).is_some() {
            return Err(CompileError::new(format!("binding '{}' already exists in this scope or an outer scope", name), span));
        }
        self.insert(name, ty, is_linear, is_mut);
        Ok(())
    }

    fn update_type(&mut self, name: &str, ty: Type) -> bool {
        if self.vars.contains_key(name) {
            self.vars.insert(name.to_string(), ty);
            true
        } else {
            self.parent.as_mut().map(|parent| parent.update_type(name, ty)).unwrap_or(false)
        }
    }

    fn merge_existing_type_refinements_from(&mut self, other: &TypeEnv) {
        let names = self.vars.keys().cloned().collect::<Vec<_>>();
        for name in names {
            if let Some(ty) = other.lookup(&name).cloned() {
                self.vars.insert(name, ty);
            }
        }
        if let Some(parent) = self.parent.as_mut() {
            parent.merge_existing_type_refinements_from(other);
        }
    }

    fn merge_existing_linear_states_from(&mut self, other: &TypeEnv) {
        for name in self.linear_names() {
            if let Some(state) = other.linear_state(&name) {
                self.set_existing_linear_state(&name, state);
            }
        }
    }

    pub fn consume(&mut self, name: &str) -> Result<()> {
        self.set_linear_state(name, LinearState::Consumed)
    }

    pub fn transfer(&mut self, name: &str) -> Result<()> {
        self.set_linear_state(name, LinearState::Transferred)
    }

    pub fn destroy(&mut self, name: &str) -> Result<()> {
        self.set_linear_state(name, LinearState::Destroyed)
    }

    fn set_linear_state(&mut self, name: &str, next: LinearState) -> Result<()> {
        match self.linear_states.get_mut(name) {
            Some(state) => {
                if *state != LinearState::Available {
                    return Err(CompileError::new(format!("resource '{}' already {:?}", name, state), Span::default()));
                }
                *state = next;
                Ok(())
            }
            None => {
                if let Some(ref mut parent) = self.parent {
                    parent.set_linear_state(name, next)
                } else {
                    Err(CompileError::new(format!("unknown resource '{}'", name), Span::default()))
                }
            }
        }
    }

    fn linear_state(&self, name: &str) -> Option<LinearState> {
        self.linear_states.get(name).copied().or_else(|| self.parent.as_ref().and_then(|parent| parent.linear_state(name)))
    }

    fn linear_names(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut names = Vec::new();
        self.collect_linear_names(&mut seen, &mut names);
        names.sort();
        names
    }

    fn collect_linear_names(&self, seen: &mut HashSet<String>, names: &mut Vec<String>) {
        if let Some(parent) = &self.parent {
            parent.collect_linear_names(seen, names);
        }
        for name in self.linear_states.keys() {
            if seen.insert(name.clone()) {
                names.push(name.clone());
            }
        }
    }

    fn set_existing_linear_state(&mut self, name: &str, next: LinearState) {
        if let Some(state) = self.linear_states.get_mut(name) {
            *state = next;
        } else if let Some(parent) = self.parent.as_mut() {
            parent.set_existing_linear_state(name, next);
        }
    }

    fn is_affine_owner(&self, name: &str) -> bool {
        if self.vars.contains_key(name) {
            self.affine_owners.contains(name)
        } else {
            self.parent.as_ref().is_some_and(|parent| parent.is_affine_owner(name))
        }
    }

    fn merge_branch_linear_states(
        &mut self,
        then_env: &TypeEnv,
        then_returns: bool,
        else_env: Option<&TypeEnv>,
        else_returns: bool,
        span: Span,
    ) -> Result<()> {
        for name in self.linear_names() {
            let before = self.linear_state(&name).unwrap_or(LinearState::Available);
            let then_state = then_env.linear_state(&name).unwrap_or(before);
            let else_state = else_env.and_then(|env| env.linear_state(&name)).unwrap_or(before);

            let merged = match (then_returns, else_env.is_some(), else_returns) {
                // A returning branch releases invocation-local owner storage.
                // Only the implicit fallthrough path reaches subsequent code;
                // Cell resources retain their explicit lifecycle obligations.
                (true, false, _) if self.is_affine_owner(&name) => before,
                (true, _, true) | (false, true, false) if then_state.join(else_state).is_some() => {
                    then_state.join(else_state).expect("checked ownership join")
                }
                (true, true, false) => else_state,
                (false, true, true) => then_state,
                (false, false, _) if then_state == before => before,
                _ => {
                    return Err(CompileError::new(
                        format!("linear resource '{}' has inconsistent ownership state across if branches", name),
                        span,
                    ));
                }
            };

            self.set_existing_linear_state(&name, merged);
        }
        Ok(())
    }

    fn merge_match_linear_states(&mut self, arm_envs: &[TypeEnv], span: Span) -> Result<()> {
        let Some(first_env) = arm_envs.first() else {
            return Ok(());
        };

        for name in self.linear_names() {
            let before = self.linear_state(&name).unwrap_or(LinearState::Available);
            let mut joined = first_env.linear_state(&name).unwrap_or(before);
            for env in arm_envs.iter().skip(1) {
                joined = joined.join(env.linear_state(&name).unwrap_or(before)).ok_or_else(|| {
                    CompileError::new(format!("linear resource '{}' has inconsistent ownership state across match arms", name), span)
                })?;
            }
            self.set_existing_linear_state(&name, joined);
        }
        Ok(())
    }

    fn reject_loop_linear_state_changes(&self, loop_env: &TypeEnv, span: Span) -> Result<()> {
        for name in self.linear_names() {
            let before = self.linear_state(&name).unwrap_or(LinearState::Available);
            let after = loop_env.linear_state(&name).unwrap_or(before);
            if after != before {
                return Err(CompileError::new(
                    format!("linear resource '{}' cannot change ownership state inside a loop body", name),
                    span,
                ));
            }
        }
        Ok(())
    }

    pub fn check_linear_complete(&self) -> Result<()> {
        let mut states = self.linear_states.iter().collect::<Vec<_>>();
        states.sort_by_key(|(name, _)| *name);
        for (name, state) in states {
            if *state == LinearState::Available && !self.affine_owners.contains(name) {
                return Err(CompileError::new(
                    format!("linear resource '{}' was not consumed, transferred, or destroyed", name),
                    Span::default(),
                ));
            }
        }
        Ok(())
    }
}

impl Clone for TypeEnv {
    fn clone(&self) -> Self {
        Self {
            vars: self.vars.clone(),
            mutability: self.mutability.clone(),
            linear_states: self.linear_states.clone(),
            affine_owners: self.affine_owners.clone(),
            parent: self.parent.as_ref().map(|p| Box::new((**p).clone())),
        }
    }
}

pub struct TypeChecker<'a> {
    env: TypeEnv,
    type_fields: HashMap<String, HashMap<String, Type>>,
    nested_record_fields: HashMap<String, Vec<(String, Type)>>,
    nested_descriptions: crate::nested_values::Descriptions,
    cell_role_catalogs: HashMap<String, CellRolesDef>,
    collection_pools: HashMap<String, CollectionsDef>,
    collection_constraint: bool,
    enum_variants: HashMap<String, Vec<String>>,
    enum_payload_variants: HashMap<String, HashSet<String>>,
    enum_variant_fields: HashMap<String, HashMap<String, Vec<Type>>>,
    functions: HashMap<String, FunctionSignature>,
    inferred_effects: crate::effects::InferredEffects,
    effect_requirements: RefCell<Vec<EffectRequirement>>,
    linear_types: HashSet<String>,
    cell_type_kinds: HashMap<String, CellTypeKind>,
    type_capabilities: HashMap<String, HashSet<Capability>>,
    type_identities: HashMap<String, IdentityPolicy>,
    nominal_identities: std::collections::BTreeMap<String, Option<crate::resolve::ResolvedTypeIdentity>>,
    receipt_claim_outputs: HashMap<String, Option<Type>>,
    flow_states: HashMap<String, Vec<String>>,
    flow_state_fields: HashMap<String, String>,
    flows: HashMap<String, FlowSpec>,
    constants: HashMap<String, ConstDef>,
    resolver: Option<&'a ModuleResolver>,
    current_module: Option<String>,
    current_callable: Option<CallableKind>,
    current_return_type: Option<Option<Type>>,
    current_validity: bool,
    /// Domain contracts from explicit group-bound fresh/pool lock clauses,
    /// keyed by the generated creation's source span and target binding.
    native_lock_domains: HashMap<(usize, usize, String), LockTargetDomain>,
    active_borrows: Vec<ActiveBorrow>,
    loop_labels: Vec<Option<String>>,
}

#[derive(Debug, Clone, Copy)]
enum LockTargetDomain {
    Address,
    ScriptHash,
}

#[derive(Debug, Clone)]
struct ActiveBorrow {
    root: String,
    binding: String,
    root_ty: Type,
}

#[derive(Debug, Clone, Default)]
struct SpawnIpcFdState {
    aliases: HashMap<String, String>,
    closed: HashSet<String>,
    pipe_tuples: HashMap<String, (String, String)>,
}

fn native_lock_domains(action: &ActionDef) -> HashMap<(usize, usize, String), LockTargetDomain> {
    fn domain(treatment: &ReplaceLockTreatment) -> Option<LockTargetDomain> {
        match treatment {
            ReplaceLockTreatment::Exact(_) => Some(LockTargetDomain::Address),
            ReplaceLockTreatment::ExactHash(_) => Some(LockTargetDomain::ScriptHash),
            ReplaceLockTreatment::Same => None,
        }
    }
    let mut domains = HashMap::new();
    if let Some(surface) = &action.next_surface {
        for disposition in &surface.dispositions {
            match disposition.resolve(&action.body).expect("validated lifecycle occurrence") {
                ResolvedDisposition::Pool(pool) => {
                    for (output, treatment) in &pool.output_locks {
                        if let Some(domain) = domain(treatment) {
                            domains.insert((pool.span.start, pool.span.end, output.clone()), domain);
                        }
                    }
                }
                ResolvedDisposition::Fresh(fresh) => {
                    if let Some(domain) = domain(&fresh.lock) {
                        domains.insert((fresh.span.start, fresh.span.end, fresh.output.clone()), domain);
                    }
                }
                ResolvedDisposition::Replace(_) | ResolvedDisposition::Retire(_) => {}
            }
        }
    }
    domains
}

fn function_def_kind(function: &FunctionDef) -> CallableKind {
    match function {
        FunctionDef::Action(_) => CallableKind::Action,
        FunctionDef::Function(_) => CallableKind::Function,
        FunctionDef::Lock(_) => CallableKind::Lock,
    }
}

fn function_def_param_types(function: &FunctionDef) -> Vec<Type> {
    match function {
        FunctionDef::Action(action) => action.params.iter().map(|param| param.ty.clone()).collect(),
        FunctionDef::Function(function) => function.params.iter().map(|param| param.ty.clone()).collect(),
        FunctionDef::Lock(lock) => lock.params.iter().map(|param| param.ty.clone()).collect(),
    }
}

fn type_repr(ty: &Type) -> String {
    ty.source_text()
}

const CKB_LOCK_SCRIPT_REF_TYPE: &str = "__ckb_lock_script_ref";
const CKB_TYPE_SCRIPT_REF_TYPE: &str = "__ckb_type_script_ref";
const CKB_SCRIPT_ARGS_TYPE: &str = "ScriptArgs";
const CKB_SCRIPT_VALUE_TYPE: &str = "Script";
const CKB_INPUT_VIEW_TYPE: &str = "InputView";
const CKB_OUTPUT_VIEW_TYPE: &str = "OutputView";
const CKB_CELL_DEP_VIEW_TYPE: &str = "CellDepView";
const CKB_HEADER_DEP_VIEW_TYPE: &str = "HeaderDepView";
const CKB_WITNESS_ARGS_VIEW_TYPE: &str = "WitnessArgsView";
const CKB_WITNESS_BYTES_VIEW_TYPE: &str = "WitnessBytesView";
const CKB_WITNESS_BYTES_MAX: u64 = crate::ast::MAX_WITNESS_BYTES_VIEW;
const CKB_OUT_POINT_TYPE: &str = "OutPoint";
const CKB_SCRIPT_VIEW_TYPE: &str = "ScriptView";
const CKB_SCRIPT_HASH_TYPE: &str = "ScriptHash";
const CKB_EXACT_SCRIPT_HANDLE_TYPE: &str = crate::script_handle_contract::EXACT_SCRIPT_HANDLE_TYPE;
const CKB_DEPLOYMENT_LINE_HANDLE_TYPE: &str = crate::script_handle_contract::DEPLOYMENT_LINE_HANDLE_TYPE;
const CKB_SIGHASH_ALL_DIGEST_TYPE: &str = "SighashAllDigest";
const CKB_EPOCH_NUMBER_TYPE: &str = "EpochNumber";
const CKB_EPOCH_DURATION_TYPE: &str = "EpochDuration";
const CKB_BLOCK_NUMBER_TYPE: &str = "BlockNumber";
const CKB_EPOCH_LENGTH_TYPE: &str = "EpochLength";
const CKB_TIMESTAMP_MILLIS_TYPE: &str = "TimestampMillis";
const CKB_ENCODED_SINCE_TYPE: &str = "EncodedSince";
const CKB_DECODED_SINCE_TYPE: &str = "DecodedSince";
const CKB_ABSOLUTE_BLOCK_SINCE_TYPE: &str = "AbsoluteBlockSince";
const CKB_ABSOLUTE_EPOCH_SINCE_TYPE: &str = "AbsoluteEpochSince";
const CKB_ABSOLUTE_TIMESTAMP_SINCE_TYPE: &str = "AbsoluteTimestampSince";
const CKB_RELATIVE_BLOCK_SINCE_TYPE: &str = "RelativeBlockSince";
const CKB_RELATIVE_EPOCH_SINCE_TYPE: &str = "RelativeEpochSince";
const CKB_RELATIVE_TIMESTAMP_SINCE_TYPE: &str = "RelativeTimestampSince";

fn param_source_repr(source: ParamSource) -> &'static str {
    match source {
        ParamSource::Default => "default",
        ParamSource::Input => "input",
        ParamSource::Output => "output",
        ParamSource::Protected => "protected",
        ParamSource::Witness => "witness",
        ParamSource::LockArgs => "lock_args",
    }
}

fn type_def_type_id(type_def: &TypeDef) -> Option<&TypeIdentity> {
    match type_def {
        TypeDef::Resource(resource) => resource.type_id.as_ref(),
        TypeDef::Shared(shared) => shared.type_id.as_ref(),
        TypeDef::Receipt(receipt) => receipt.type_id.as_ref(),
        TypeDef::Struct(struct_def) => struct_def.type_id.as_ref(),
        TypeDef::Enum(_) => None,
    }
}

fn register_type_id_value(seen: &mut HashMap<String, Span>, type_name: &str, value: &str, span: Span) -> Result<()> {
    if seen.insert(value.to_string(), span).is_some() {
        return Err(CompileError::new(format!("duplicate type_id '{}' on type '{}'", value, type_name), span));
    }
    Ok(())
}

fn register_type_id(seen: &mut HashMap<String, Span>, type_name: &str, type_id: Option<&TypeIdentity>) -> Result<()> {
    let Some(type_id) = type_id else {
        return Ok(());
    };
    register_type_id_value(seen, type_name, &type_id.value, type_id.span)
}

impl Default for TypeChecker<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> TypeChecker<'a> {
    pub fn new() -> Self {
        Self {
            env: TypeEnv::new(),
            type_fields: HashMap::new(),
            nested_record_fields: HashMap::new(),
            nested_descriptions: Default::default(),
            cell_role_catalogs: HashMap::new(),
            collection_pools: HashMap::new(),
            collection_constraint: false,
            enum_variants: HashMap::new(),
            enum_payload_variants: HashMap::new(),
            enum_variant_fields: HashMap::new(),
            functions: HashMap::new(),
            inferred_effects: Default::default(),
            effect_requirements: RefCell::new(Vec::new()),
            linear_types: HashSet::new(),
            cell_type_kinds: HashMap::new(),
            type_capabilities: HashMap::new(),
            type_identities: HashMap::new(),
            nominal_identities: Default::default(),
            receipt_claim_outputs: HashMap::new(),
            flow_states: HashMap::new(),
            flow_state_fields: HashMap::new(),
            flows: HashMap::new(),
            constants: HashMap::new(),
            resolver: None,
            current_module: None,
            current_callable: None,
            current_return_type: None,
            current_validity: false,
            native_lock_domains: HashMap::new(),
            active_borrows: Vec::new(),
            loop_labels: Vec::new(),
        }
    }

    pub fn with_resolver(resolver: &'a ModuleResolver, current_module: impl Into<String>) -> Self {
        let mut checker = Self::new();
        checker.resolver = Some(resolver);
        checker.current_module = Some(current_module.into());
        checker
    }

    pub fn check_module(&mut self, module: &Module) -> Result<()> {
        let diagnostics = self.check_module_diagnostics(module);
        match diagnostics.into_iter().next() {
            Some(error) => Err(error),
            _ => Ok(()),
        }
    }

    pub fn check_module_diagnostics(&mut self, module: &Module) -> Vec<CompileError> {
        let mut diagnostics = Vec::new();
        // Reject ambiguous representation labels before collecting fields or
        // inferring builtin properties, including unused generic templates.
        for item in module.items.iter().chain(&module.interface_templates) {
            let binding = match item {
                Item::Resource(def) => Some((def.name.as_str(), def.span)),
                Item::Shared(def) => Some((def.name.as_str(), def.span)),
                Item::Receipt(def) => Some((def.name.as_str(), def.span)),
                Item::Struct(def) => Some((def.name.as_str(), def.span)),
                Item::Enum(def) => Some((def.name.as_str(), def.span)),
                Item::DataShape(def) => Some((def.name.as_str(), def.span)),
                Item::Collections(def) => Some((def.name.as_str(), def.span)),
                _ => None,
            };
            if let Some((name, span)) = binding
                && let Err(error) = crate::resolve::validate_builtin_type_binding(name, span)
            {
                diagnostics.push(error);
            }
        }
        if !diagnostics.is_empty() {
            return diagnostics;
        }
        self.nominal_identities = crate::resolve::nominal_identities(module, self.resolver);
        self.nested_descriptions = Default::default();
        if self.current_module.is_none() {
            self.current_module = Some(module.name.clone());
        }
        self.effect_requirements.borrow_mut().clear();
        let mut seen_symbols = HashSet::new();
        let mut seen_type_ids = HashMap::new();
        for item in &module.items {
            if let Some((symbol, span)) = item_symbol_name_and_span(item)
                && !seen_symbols.insert(symbol.to_string())
            {
                diagnostics.push(CompileError::new(format!("duplicate symbol '{}'", symbol), span));
            }
            match item {
                Item::Const(const_def) => {
                    if let Err(error) = self.validate_type(&const_def.ty) {
                        diagnostics.push(error);
                    }
                    self.env.insert(const_def.name.clone(), const_def.ty.clone(), false, false);
                    self.constants.insert(const_def.name.clone(), const_def.clone());
                }
                Item::Resource(resource) => {
                    if let Err(error) = register_type_id(&mut seen_type_ids, &resource.name, resource.type_id.as_ref()) {
                        diagnostics.push(error);
                    }
                    self.linear_types.insert(resource.name.clone());
                    self.cell_type_kinds.insert(resource.name.clone(), CellTypeKind::Resource);
                    self.type_capabilities.insert(resource.name.clone(), resource.capabilities.iter().copied().collect());
                    self.type_identities.insert(resource.name.clone(), resource.identity.clone());
                    self.type_fields.insert(
                        resource.name.clone(),
                        resource.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect(),
                    );
                }
                Item::Shared(shared) => {
                    if let Err(error) = register_type_id(&mut seen_type_ids, &shared.name, shared.type_id.as_ref()) {
                        diagnostics.push(error);
                    }
                    self.linear_types.insert(shared.name.clone());
                    self.cell_type_kinds.insert(shared.name.clone(), CellTypeKind::Shared);
                    self.type_capabilities.insert(shared.name.clone(), shared.capabilities.iter().copied().collect());
                    self.type_identities.insert(shared.name.clone(), shared.identity.clone());
                    self.type_fields.insert(
                        shared.name.clone(),
                        shared.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect(),
                    );
                }
                Item::Receipt(receipt) => {
                    if let Err(error) = register_type_id(&mut seen_type_ids, &receipt.name, receipt.type_id.as_ref()) {
                        diagnostics.push(error);
                    }
                    self.linear_types.insert(receipt.name.clone());
                    self.cell_type_kinds.insert(receipt.name.clone(), CellTypeKind::Receipt);
                    self.type_capabilities.insert(receipt.name.clone(), receipt.capabilities.iter().copied().collect());
                    self.type_identities.insert(receipt.name.clone(), receipt.identity.clone());
                    self.receipt_claim_outputs.insert(receipt.name.clone(), receipt.claim_output.clone());
                    self.type_fields.insert(
                        receipt.name.clone(),
                        receipt.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect(),
                    );
                }
                Item::Struct(struct_def) => {
                    self.nested_record_fields.insert(
                        struct_def.name.clone(),
                        struct_def.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect(),
                    );
                    if let Err(error) = register_type_id(&mut seen_type_ids, &struct_def.name, struct_def.type_id.as_ref()) {
                        diagnostics.push(error);
                    }
                    self.type_fields.insert(
                        struct_def.name.clone(),
                        struct_def.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect(),
                    );
                }
                Item::BoundPolicy(_) | Item::DataShape(_) => {}
                Item::Collections(def) => {
                    self.collection_pools.insert(def.name.clone(), def.clone());
                }
                Item::CellRoles(def) => {
                    self.cell_role_catalogs.insert(def.name.clone(), def.clone());
                }
                Item::Invariant(_) => {}
                Item::Enum(enum_def) => {
                    self.enum_variants
                        .insert(enum_def.name.clone(), enum_def.variants.iter().map(|variant| variant.name.clone()).collect());
                    self.enum_payload_variants.insert(
                        enum_def.name.clone(),
                        enum_def
                            .variants
                            .iter()
                            .filter(|variant| !variant.fields.is_empty())
                            .map(|variant| variant.name.clone())
                            .collect(),
                    );
                    self.enum_variant_fields.insert(
                        enum_def.name.clone(),
                        enum_def.variants.iter().map(|variant| (variant.name.clone(), variant.fields.clone())).collect(),
                    );
                }
                Item::Action(action) => {
                    self.functions.insert(
                        action.name.clone(),
                        FunctionSignature {
                            params: action.params.iter().map(|param| param.ty.clone()).collect(),
                            return_type: action.return_type.clone(),
                            kind: CallableKind::Action,
                        },
                    );
                }
                Item::Function(function) => {
                    self.functions.insert(
                        function.name.clone(),
                        FunctionSignature {
                            params: function.params.iter().map(|param| param.ty.clone()).collect(),
                            return_type: function.return_type.clone(),
                            kind: CallableKind::Function,
                        },
                    );
                }
                Item::Lock(lock) => {
                    self.functions.insert(
                        lock.name.clone(),
                        FunctionSignature {
                            params: lock.params.iter().map(|param| param.ty.clone()).collect(),
                            return_type: Some(Type::Bool),
                            kind: CallableKind::Lock,
                        },
                    );
                }
                Item::Flow(_) => {}
                Item::Use(_) => {}
            }
        }

        if let Err(error) = self.register_imported_type_ids(&mut seen_type_ids) {
            diagnostics.push(error);
        }
        if let Err(error) = self.validate_source_interface_roles(module) {
            diagnostics.push(error);
        }
        // Phantom parameters need no field storage, but their module-interface
        // arguments must still be valid before metadata-only compilation.
        for origin in module.specializations.values() {
            for argument in &origin.arguments {
                if let Err(error) = self.validate_interface_argument_roles(argument, 0, OpenHandleRoleContext::Retained) {
                    diagnostics.push(error);
                }
            }
        }
        if let Err(error) = self.register_flows(module) {
            diagnostics.push(error);
        }
        if let Err(error) = self.validate_flow_action_edges(module) {
            diagnostics.push(error);
        }

        for item in &module.items {
            diagnostics.extend(self.check_item_diagnostics(item));
        }
        // Effect constraints are checked only after ordinary typing and binding
        // succeeded. Lowering exposes injected reads and lifecycle operations.
        if diagnostics.is_empty() {
            match crate::effects::infer_module_diagnostics(module, self.resolver) {
                Ok(facts) => {
                    for requirement in self.effect_requirements.borrow().iter() {
                        match facts.get(&requirement.target) {
                            Some(effect) if if requirement.empty { effect.is_empty() } else { effect.allows_observation() } => {}
                            Some(effect) => diagnostics.push(CompileError::new(
                                format!("{} '{}': {}", requirement.reason, requirement.target, effect),
                                requirement.span,
                            )),
                            None => diagnostics.push(CompileError::new(
                                format!("effect evidence is unavailable for '{}'", requirement.target),
                                requirement.span,
                            )),
                        }
                    }
                    self.inferred_effects = facts;
                }
                Err(errors) => diagnostics.extend(errors),
            }
        }
        // Resolve types and validate the old ownership/reference/ABI boundary
        // before suggesting a source. In particular an invalid owned Lock Cell
        // must not receive an invented input, protected or CellDep authority.
        if diagnostics.is_empty() {
            diagnostics.extend(self.entry_source_diagnostics(module));
        }
        diagnostics
    }

    fn register_imported_type_ids(&self, seen_type_ids: &mut HashMap<String, Span>) -> Result<()> {
        let (Some(resolver), Some(module_name)) = (self.resolver, self.current_module.as_deref()) else {
            return Ok(());
        };

        for import in resolver.imports_for_module(module_name) {
            let local_name = import.alias.as_deref().unwrap_or(&import.name);
            let Some(type_def) = resolver.resolve_type(module_name, local_name) else {
                continue;
            };
            crate::resolve::validate_builtin_type_binding(local_name, import.span)?;
            if let Some(type_id) = type_def_type_id(&type_def) {
                register_type_id_value(seen_type_ids, local_name, &type_id.value, import.span)?;
            }
        }

        Ok(())
    }

    fn check_item(&mut self, item: &Item) -> Result<()> {
        match item {
            Item::BoundPolicy(def) => {
                Err(CompileError::new("named policy bounds must resolve before concrete type admission", def.span()))
            }
            Item::Collections(def) => {
                crate::allocated_values::validate_resolved_declaration(def, |ty| self.allocated_value_width(ty, &mut HashSet::new()))
            }
            Item::DataShape(def) => self
                .validate_nested_type(&crate::nested_values::NestedType::Values(crate::nested_values::declared_values(def)?))
                .map_err(|mut error| {
                    error.span = def.span;
                    error
                }),
            Item::CellRoles(def) => self.validate_cell_role_contract(&crate::cell_selection::named_contract(def)?, def.span),
            Item::Resource(r) => self.check_resource(r),
            Item::Shared(s) => self.check_shared(s),
            Item::Receipt(r) => self.check_receipt(r),
            Item::Struct(s) => self.check_struct(s),
            Item::Flow(_) => Ok(()),
            Item::Invariant(i) => self.check_invariant(i),
            Item::Const(c) => self.check_const(c),
            Item::Enum(e) => self.check_enum(e),
            Item::Action(a) => self.check_action(a),
            Item::Function(f) => self.check_function(f),
            Item::Lock(l) => self.check_lock(l),
            Item::Use(_) => Ok(()),
        }
    }

    fn check_item_diagnostics(&mut self, item: &Item) -> Vec<CompileError> {
        match item {
            Item::Action(action) => self.check_action_diagnostics(action),
            Item::Function(function) => self.check_function_diagnostics(function),
            Item::Lock(lock) => self.check_lock_diagnostics(lock),
            _ => self.check_item(item).err().into_iter().collect(),
        }
    }

    fn validate_expr_allowed_in_current_callable(&self, expr: &Expr) -> Result<()> {
        if matches!(expr, Expr::Require(_) | Expr::RequireBlock(_))
            && !matches!(self.current_callable, Some(CallableKind::Action | CallableKind::Invariant | CallableKind::Lock))
        {
            return Err(CompileError::new(
                "require/preserve is verifier-boundary syntax for actions and locks; use ordinary boolean expressions inside functions",
                expr_span(expr),
            ));
        }
        if matches!(expr, Expr::Preserve(_)) && !matches!(self.current_callable, Some(CallableKind::Action | CallableKind::Lock)) {
            return Err(CompileError::new("preserve is verifier-boundary syntax for actions and locks", expr_span(expr)));
        }
        if matches!(expr, Expr::ReplaceRelation(_))
            && !matches!(self.current_callable, Some(CallableKind::Action | CallableKind::Lock))
        {
            return Err(CompileError::new("replace relations are verifier-boundary syntax for actions and locks", expr_span(expr)));
        }

        let operation = match expr {
            Expr::Create(_) => Some("create"),
            Expr::Consume(_) => Some("consume"),
            Expr::Destroy(_) => Some("destroy"),
            Expr::ReadRef(_) => Some("read_ref"),
            Expr::StdlibCall(call) => {
                let qualified = format!("std::{}::{}", call.namespace, call.name);
                match qualified.as_str() {
                    "std::lifecycle::transfer" | "std::receipt::claim" | "std::lifecycle::settle" => Some("consume"),
                    _ => None,
                }
            }
            Expr::Claim(_) => Some("claim"),
            Expr::Settle(_) => Some("settle"),
            Expr::CreateUnique(_) => Some("create_unique"),
            Expr::ReplaceUnique(_) => Some("replace_unique"),
            _ => None,
        };

        match (self.current_callable, operation) {
            (Some(CallableKind::Invariant), Some(operation)) => {
                return Err(CompileError::new(format!("pure function cannot contain '{}'", operation), expr_span(expr)));
            }
            (Some(CallableKind::Lock), Some(operation)) if operation != "read_ref" => {
                return Err(CompileError::new(
                    format!("lock cannot contain '{}' Cell state transition; move state transition logic into an action", operation),
                    expr_span(expr),
                ));
            }
            _ => {}
        }

        Ok(())
    }

    fn validate_runtime_call_allowed_in_current_callable(&self, call: &CallExpr) -> Result<()> {
        let _ = call;
        Ok(())
    }

    fn initializer_types_equal(&self, actual: &Type, expected: &Type) -> bool {
        self.types_equal(actual, expected)
            || (matches!(actual, Type::Named(name) if name == "Vec") && expected.vector_element().is_some())
    }
}

fn stmt_span(stmt: &Stmt) -> Span {
    match stmt {
        Stmt::Lifecycle(value) => value.span(),
        Stmt::Let(let_stmt) => let_stmt.span,
        Stmt::Expr(expr) | Stmt::Discard(expr) => expr_span(expr),
        Stmt::Return(ReturnStmt { value: Some(expr), span }) => non_default_span(expr_span(expr), *span),
        Stmt::Return(ReturnStmt { value: None, span }) => *span,
        Stmt::If(if_stmt) => if_stmt.span,
        Stmt::For(for_stmt) => for_stmt.span,
        Stmt::While(while_stmt) => while_stmt.span,
        Stmt::Break(control) | Stmt::Continue(control) => control.span,
        Stmt::Borrow(borrow_stmt) => borrow_stmt.span,
    }
}

fn non_default_span(preferred: Span, fallback: Span) -> Span {
    if preferred.line == 0 || preferred.column == 0 {
        fallback
    } else {
        preferred
    }
}

fn push_diagnostic(diagnostics: &mut Vec<CompileError>, result: Result<()>) {
    if let Err(error) = result {
        diagnostics.push(error);
    }
}

fn note_replace_field(schema_fields: &[String], covered: &mut Vec<String>, field: &str, span: crate::error::Span) -> Result<()> {
    if !schema_fields.iter().any(|candidate| candidate == field) {
        return Err(CompileError::new(format!("replace relation field '{field}' does not exist on the relation's resource"), span)
            .with_code(crate::error::DiagnosticCode::E1002));
    }
    if covered.iter().any(|candidate| candidate == field) {
        return Err(CompileError::new(format!("replace relation states field '{field}' more than once"), span));
    }
    covered.push(field.to_string());
    Ok(())
}

fn check_replace_assigned_field(
    checker: &mut TypeChecker,
    env: &mut TypeEnv,
    resource_ty: &Type,
    field: &str,
    value: &Expr,
    span: crate::error::Span,
) -> Result<()> {
    let expected = checker.lookup_field_type(resource_ty, field, span)?;
    let actual = checker.infer_expr(env, value)?;
    if !checker.types_equal(&expected, &actual) {
        return Err(CompileError::new(
            format!("replace relation field '{field}' expects {}, found {}", type_repr(&expected), type_repr(&actual)),
            span,
        )
        .with_code(crate::error::DiagnosticCode::E1004));
    }
    Ok(())
}

fn expr_span(expr: &Expr) -> Span {
    match expr {
        Expr::Integer(_) | Expr::Bool(_) | Expr::String(_) | Expr::ByteString(_) | Expr::Identifier(_) => Span::default(),
        Expr::StdlibCall(call) => call.span,
        Expr::Assign(assign) => assign.span,
        Expr::Binary(binary) => binary.span,
        Expr::Unary(unary) => unary.span,
        Expr::Call(call) => call.span,
        Expr::FieldAccess(field) => field.span,
        Expr::Index(index) => index.span,
        Expr::Create(create) => create.span,
        Expr::Consume(consume) => consume.span,
        Expr::Destroy(destroy) => destroy.span,
        Expr::ReadRef(read_ref) => read_ref.span,
        Expr::Claim(claim) => claim.span,
        Expr::Settle(settle) => settle.span,
        Expr::CreateUnique(cu) => cu.span,
        Expr::ReplaceUnique(ru) => ru.span,
        Expr::Assert(assert_expr) => assert_expr.span,
        Expr::Require(require_expr) => require_expr.span,
        Expr::Block(stmts) => stmts.last().map(stmt_span).unwrap_or_default(),
        Expr::Tuple(_) | Expr::Array(_) => Span::default(),
        Expr::If(if_expr) => if_expr.span,
        Expr::Cast(cast) => cast.span,
        Expr::Range(range) => range.span,
        Expr::StructInit(init) => init.span,
        Expr::Match(match_expr) => match_expr.span,
        Expr::RequireBlock(require_block) => require_block.span,
        Expr::Preserve(preserve) => preserve.span,
        Expr::ReplaceRelation(relation) => relation.span,
    }
}

fn expr_uses_identifier(expr: &Expr, expected: &str) -> bool {
    match expr {
        Expr::Identifier(name) => name == expected,
        Expr::Assign(assign) => expr_uses_identifier(&assign.target, expected) || expr_uses_identifier(&assign.value, expected),
        Expr::Binary(binary) => expr_uses_identifier(&binary.left, expected) || expr_uses_identifier(&binary.right, expected),
        Expr::Unary(unary) => expr_uses_identifier(&unary.expr, expected),
        Expr::Call(call) => {
            expr_uses_identifier(&call.func, expected) || call.args.iter().any(|arg| expr_uses_identifier(arg, expected))
        }
        Expr::FieldAccess(field) => expr_uses_identifier(&field.expr, expected),
        Expr::Index(index) => expr_uses_identifier(&index.expr, expected) || expr_uses_identifier(&index.index, expected),
        Expr::Create(create) => {
            create.fields.iter().any(|(_, value)| expr_uses_identifier(value, expected))
                || create.lock.as_ref().is_some_and(|lock| expr_uses_identifier(lock, expected))
        }
        Expr::Consume(consume) => expr_uses_identifier(&consume.expr, expected),
        Expr::Destroy(destroy) => expr_uses_identifier(&destroy.expr, expected),
        Expr::ReadRef(_) => false,
        Expr::Claim(claim) => expr_uses_identifier(&claim.receipt, expected),
        Expr::Settle(settle) => expr_uses_identifier(&settle.expr, expected),
        Expr::CreateUnique(create) => {
            create.fields.iter().any(|(_, value)| expr_uses_identifier(value, expected))
                || create.lock.as_ref().is_some_and(|lock| expr_uses_identifier(lock, expected))
        }
        Expr::ReplaceUnique(replace) => {
            expr_uses_identifier(&replace.expr, expected)
                || replace.fields.iter().any(|(_, value)| expr_uses_identifier(value, expected))
        }
        Expr::Assert(assert_expr) => {
            expr_uses_identifier(&assert_expr.condition, expected) || expr_uses_identifier(&assert_expr.message, expected)
        }
        Expr::Require(require_expr) => {
            expr_uses_identifier(&require_expr.condition, expected)
                || require_expr.message.as_ref().is_some_and(|message| expr_uses_identifier(message, expected))
        }
        Expr::Block(stmts) => stmts.iter().any(|stmt| stmt_uses_identifier(stmt, expected)),
        Expr::Tuple(items) | Expr::Array(items) => items.iter().any(|item| expr_uses_identifier(item, expected)),
        Expr::If(if_expr) => {
            expr_uses_identifier(&if_expr.condition, expected)
                || expr_uses_identifier(&if_expr.then_branch, expected)
                || expr_uses_identifier(&if_expr.else_branch, expected)
        }
        Expr::Cast(cast) => expr_uses_identifier(&cast.expr, expected),
        Expr::Range(range) => expr_uses_identifier(&range.start, expected) || expr_uses_identifier(&range.end, expected),
        Expr::StructInit(init) => init.fields.iter().any(|(_, value)| expr_uses_identifier(value, expected)),
        Expr::Match(match_expr) => {
            expr_uses_identifier(&match_expr.expr, expected)
                || match_expr.arms.iter().any(|arm| expr_uses_identifier(&arm.value, expected))
        }
        Expr::RequireBlock(require_block) => require_block.expressions.iter().any(|expr| expr_uses_identifier(expr, expected)),
        Expr::Preserve(preserve) => preserve.output_name == expected || preserve.input_name == expected,
        Expr::ReplaceRelation(relation) => {
            relation.before == expected
                || relation.after == expected
                || match &relation.lock {
                    ReplaceLockTreatment::Exact(lock) | ReplaceLockTreatment::ExactHash(lock) => expr_uses_identifier(lock, expected),
                    ReplaceLockTreatment::Same => false,
                }
                || match &relation.data {
                    ReplaceDataTreatment::Fields(treatments) => treatments.iter().any(|treatment| match treatment {
                        ReplaceFieldTreatment::Same(_) => false,
                        ReplaceFieldTreatment::Assign(_, value) => expr_uses_identifier(value, expected),
                    }),
                    ReplaceDataTreatment::SameExcept(assigned) => {
                        assigned.iter().any(|(_, value)| expr_uses_identifier(value, expected))
                    }
                }
        }
        Expr::StdlibCall(call) => call.args.iter().any(|arg| expr_uses_identifier(arg, expected)),
        Expr::Integer(_) | Expr::Bool(_) | Expr::String(_) | Expr::ByteString(_) => false,
    }
}

fn expr_contains_borrow_marker(expr: &Expr, binding: &str) -> bool {
    match expr {
        Expr::Identifier(name) => name == binding,
        Expr::ReplaceRelation(relation) => relation.value_exprs().iter().any(|value| expr_contains_borrow_marker(value, binding)),
        Expr::Tuple(items) | Expr::Array(items) => items.iter().any(|item| expr_contains_borrow_marker(item, binding)),
        Expr::StructInit(init) => init.fields.iter().any(|(_, value)| expr_contains_borrow_marker(value, binding)),
        Expr::Cast(cast) => expr_contains_borrow_marker(&cast.expr, binding),
        Expr::If(if_expr) => {
            expr_contains_borrow_marker(&if_expr.then_branch, binding) || expr_contains_borrow_marker(&if_expr.else_branch, binding)
        }
        Expr::Match(match_expr) => match_expr.arms.iter().any(|arm| expr_contains_borrow_marker(&arm.value, binding)),
        Expr::Block(stmts) => stmts.last().is_some_and(|stmt| match stmt {
            Stmt::Expr(expr) | Stmt::Discard(expr) | Stmt::Return(ReturnStmt { value: Some(expr), .. }) => {
                expr_contains_borrow_marker(expr, binding)
            }
            _ => false,
        }),
        Expr::Unary(unary) if unary.op == UnaryOp::Deref => false,
        Expr::Unary(unary) => expr_contains_borrow_marker(&unary.expr, binding),
        Expr::Assign(assign) => expr_contains_borrow_marker(&assign.value, binding),
        Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::String(_)
        | Expr::ByteString(_)
        | Expr::Call(_)
        | Expr::FieldAccess(_)
        | Expr::Create(_)
        | Expr::Consume(_)
        | Expr::Destroy(_)
        | Expr::ReadRef(_)
        | Expr::Claim(_)
        | Expr::Settle(_)
        | Expr::CreateUnique(_)
        | Expr::ReplaceUnique(_)
        | Expr::Assert(_)
        | Expr::Require(_)
        | Expr::Range(_)
        | Expr::RequireBlock(_)
        | Expr::Preserve(_)
        | Expr::StdlibCall(_) => false,
        Expr::Binary(binary) => {
            expr_contains_borrow_marker(&binary.left, binding) || expr_contains_borrow_marker(&binary.right, binding)
        }
        Expr::Index(index) => expr_contains_borrow_marker(&index.expr, binding),
    }
}

fn stmt_uses_identifier(stmt: &Stmt, expected: &str) -> bool {
    match stmt {
        Stmt::Lifecycle(value) => {
            value.input_bindings().iter().chain(value.output_bindings()).any(|name| name == expected)
                || value.value_exprs().iter().any(|value| expr_uses_identifier(value, expected))
        }
        Stmt::Let(let_stmt) => expr_uses_identifier(&let_stmt.value, expected),
        Stmt::Expr(expr) | Stmt::Discard(expr) => expr_uses_identifier(expr, expected),
        Stmt::Return(return_stmt) => return_stmt.value.as_ref().is_some_and(|value| expr_uses_identifier(value, expected)),
        Stmt::Break(_) | Stmt::Continue(_) => false,
        Stmt::If(if_stmt) => {
            expr_uses_identifier(&if_stmt.condition, expected)
                || if_stmt.then_branch.iter().any(|stmt| stmt_uses_identifier(stmt, expected))
                || if_stmt.else_branch.as_ref().is_some_and(|branch| branch.iter().any(|stmt| stmt_uses_identifier(stmt, expected)))
        }
        Stmt::For(for_stmt) => {
            expr_uses_identifier(&for_stmt.iterable, expected) || for_stmt.body.iter().any(|stmt| stmt_uses_identifier(stmt, expected))
        }
        Stmt::While(while_stmt) => {
            expr_uses_identifier(&while_stmt.condition, expected)
                || while_stmt.body.iter().any(|stmt| stmt_uses_identifier(stmt, expected))
        }
        Stmt::Borrow(borrow_stmt) => borrow_stmt.body.iter().any(|stmt| stmt_uses_identifier(stmt, expected)),
    }
}

fn collect_required_output_fields(
    expr: &Expr,
    outputs: &HashMap<String, ActionOutputBinding>,
    fields: &mut HashSet<OutputFieldObligation>,
) {
    if let Some(field) = output_field_constraint(expr, outputs) {
        fields.insert(field);
    }

    match expr {
        Expr::Assign(assign) => {
            collect_required_output_fields(&assign.target, outputs, fields);
            collect_required_output_fields(&assign.value, outputs, fields);
        }
        Expr::Binary(binary) => {
            collect_required_output_fields(&binary.left, outputs, fields);
            collect_required_output_fields(&binary.right, outputs, fields);
        }
        Expr::Unary(unary) => collect_required_output_fields(&unary.expr, outputs, fields),
        Expr::Call(call) => {
            collect_required_output_fields(&call.func, outputs, fields);
            for arg in &call.args {
                collect_required_output_fields(arg, outputs, fields);
            }
        }
        Expr::FieldAccess(field) => collect_required_output_fields(&field.expr, outputs, fields),
        Expr::Index(index) => {
            collect_required_output_fields(&index.expr, outputs, fields);
            collect_required_output_fields(&index.index, outputs, fields);
        }
        Expr::Create(create) => {
            for (_, value) in &create.fields {
                collect_required_output_fields(value, outputs, fields);
            }
            if let Some(lock) = &create.lock {
                collect_required_output_fields(lock, outputs, fields);
            }
        }
        Expr::Consume(consume) => collect_required_output_fields(&consume.expr, outputs, fields),
        Expr::Destroy(destroy) => collect_required_output_fields(&destroy.expr, outputs, fields),
        Expr::Claim(claim) => collect_required_output_fields(&claim.receipt, outputs, fields),
        Expr::Settle(settle) => collect_required_output_fields(&settle.expr, outputs, fields),
        Expr::CreateUnique(create) => {
            for (_, value) in &create.fields {
                collect_required_output_fields(value, outputs, fields);
            }
            if let Some(lock) = &create.lock {
                collect_required_output_fields(lock, outputs, fields);
            }
        }
        Expr::ReplaceUnique(replace) => {
            collect_required_output_fields(&replace.expr, outputs, fields);
            for (_, value) in &replace.fields {
                collect_required_output_fields(value, outputs, fields);
            }
        }
        Expr::Assert(assert_expr) => {
            collect_required_output_fields(&assert_expr.condition, outputs, fields);
            collect_required_output_fields(&assert_expr.message, outputs, fields);
        }
        Expr::Require(require_expr) => {
            collect_required_output_fields(&require_expr.condition, outputs, fields);
            if let Some(message) = &require_expr.message {
                collect_required_output_fields(message, outputs, fields);
            }
        }
        Expr::Block(stmts) => {
            for stmt in stmts {
                collect_required_output_fields_from_stmt(stmt, outputs, fields);
            }
        }
        Expr::Tuple(elems) | Expr::Array(elems) => {
            for elem in elems {
                collect_required_output_fields(elem, outputs, fields);
            }
        }
        Expr::If(if_expr) => {
            collect_required_output_fields(&if_expr.condition, outputs, fields);
            collect_required_output_fields(&if_expr.then_branch, outputs, fields);
            collect_required_output_fields(&if_expr.else_branch, outputs, fields);
        }
        Expr::Cast(cast) => collect_required_output_fields(&cast.expr, outputs, fields),
        Expr::Range(range) => {
            collect_required_output_fields(&range.start, outputs, fields);
            collect_required_output_fields(&range.end, outputs, fields);
        }
        Expr::StructInit(init) => {
            for (_, value) in &init.fields {
                collect_required_output_fields(value, outputs, fields);
            }
        }
        Expr::Match(match_expr) => {
            collect_required_output_fields(&match_expr.expr, outputs, fields);
            for arm in &match_expr.arms {
                collect_required_output_fields(&arm.value, outputs, fields);
            }
        }
        Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::String(_)
        | Expr::ByteString(_)
        | Expr::Identifier(_)
        | Expr::ReadRef(_)
        | Expr::StdlibCall(_) => {}
        Expr::RequireBlock(require_block) => {
            for expr in &require_block.expressions {
                collect_required_output_fields(expr, outputs, fields);
            }
        }
        Expr::Preserve(preserve) => {
            for field in &preserve.fields {
                if outputs.contains_key(&preserve.output_name) {
                    fields.insert(OutputFieldObligation { output: preserve.output_name.clone(), field: field.clone() });
                }
            }
        }
        Expr::ReplaceRelation(relation) => {
            let relation_fields: Vec<String> = match &relation.data {
                ReplaceDataTreatment::Fields(treatments) => treatments
                    .iter()
                    .map(|treatment| match treatment {
                        ReplaceFieldTreatment::Same(field) => field.clone(),
                        ReplaceFieldTreatment::Assign(field, _) => field.clone(),
                    })
                    .collect(),
                ReplaceDataTreatment::SameExcept(_) => Vec::new(),
            };
            for field in relation_fields {
                if outputs.contains_key(&relation.after) {
                    fields.insert(OutputFieldObligation { output: relation.after.clone(), field });
                }
            }
        }
    }
}

fn collect_required_output_fields_from_stmt(
    stmt: &Stmt,
    outputs: &HashMap<String, ActionOutputBinding>,
    fields: &mut HashSet<OutputFieldObligation>,
) {
    match stmt {
        Stmt::Lifecycle(value) if matches!(value.as_ref(), LifecycleStmt::Fresh(_)) => {
            for expression in value.value_exprs() {
                collect_required_output_fields(expression, outputs, fields);
            }
        }
        Stmt::Lifecycle(value) => {
            for statement in value.kernel(None).expect("validated lifecycle policy") {
                collect_required_output_fields_from_stmt(&statement, outputs, fields);
            }
        }
        Stmt::Let(let_stmt) => collect_required_output_fields(&let_stmt.value, outputs, fields),
        Stmt::Expr(expr) | Stmt::Discard(expr) | Stmt::Return(ReturnStmt { value: Some(expr), .. }) => {
            collect_required_output_fields(expr, outputs, fields)
        }
        Stmt::Return(ReturnStmt { value: None, .. }) => {}
        Stmt::Break(_) | Stmt::Continue(_) => {}
        Stmt::If(if_stmt) => {
            collect_required_output_fields(&if_stmt.condition, outputs, fields);
            for stmt in &if_stmt.then_branch {
                collect_required_output_fields_from_stmt(stmt, outputs, fields);
            }
            if let Some(else_branch) = &if_stmt.else_branch {
                for stmt in else_branch {
                    collect_required_output_fields_from_stmt(stmt, outputs, fields);
                }
            }
        }
        Stmt::For(for_stmt) => {
            collect_required_output_fields(&for_stmt.iterable, outputs, fields);
            for stmt in &for_stmt.body {
                collect_required_output_fields_from_stmt(stmt, outputs, fields);
            }
        }
        Stmt::While(while_stmt) => {
            collect_required_output_fields(&while_stmt.condition, outputs, fields);
            for stmt in &while_stmt.body {
                collect_required_output_fields_from_stmt(stmt, outputs, fields);
            }
        }
        Stmt::Borrow(borrow_stmt) => {
            for stmt in &borrow_stmt.body {
                collect_required_output_fields_from_stmt(stmt, outputs, fields);
            }
        }
    }
}

fn output_field_constraint(expr: &Expr, outputs: &HashMap<String, ActionOutputBinding>) -> Option<OutputFieldObligation> {
    let Expr::FieldAccess(field) = expr else {
        return None;
    };
    let Expr::Identifier(base) = field.expr.as_ref() else {
        return None;
    };
    if outputs.contains_key(base) {
        Some(OutputFieldObligation { output: base.clone(), field: field.field.clone() })
    } else {
        None
    }
}

fn forbidden_consensus_call_name(expr: &Expr) -> Option<&'static str> {
    match expr {
        Expr::Identifier(name) => forbidden_consensus_terminal(name),
        Expr::FieldAccess(field) => forbidden_consensus_terminal(&field.field),
        _ => None,
    }
}

fn forbidden_consensus_terminal(name: &str) -> Option<&'static str> {
    match name.rsplit("::").next().unwrap_or(name) {
        "unwrap" => Some("unwrap"),
        "expect" => Some("expect"),
        "unwrap_or" => Some("unwrap_or"),
        _ => None,
    }
}

fn match_pattern_variant<'a>(enum_name: &str, pattern: &'a str) -> Option<&'a str> {
    if let Some((qualifier, variant)) = pattern.rsplit_once("::") {
        let qualifier_terminal = qualifier.rsplit("::").next().unwrap_or(qualifier);
        if qualifier == enum_name || qualifier_terminal == enum_name {
            Some(variant)
        } else {
            None
        }
    } else {
        Some(pattern)
    }
}

fn lock_args_static_type_len(ty: &Type) -> Option<usize> {
    match ty {
        Type::Bool | Type::U8 => Some(1),
        Type::U16 => Some(2),
        Type::U32 | Type::I32 => Some(4),
        Type::U64 => Some(8),
        Type::U128 => Some(16),
        Type::Address | Type::Hash => Some(32),
        Type::Array(inner, len) => lock_args_static_type_len(inner).and_then(|inner_len| inner_len.checked_mul(*len)),
        Type::Tuple(items) => {
            items.iter().try_fold(0usize, |acc, item| lock_args_static_type_len(item).and_then(|len| acc.checked_add(len)))
        }
        Type::Unit => Some(0),
        Type::Named(_) | Type::Applied(_) | Type::ArrayBound(_, _) | Type::Selection(_) | Type::Ref(_) | Type::MutRef(_) => None,
    }
}

fn item_symbol_name_and_span(item: &Item) -> Option<(&str, Span)> {
    match item {
        Item::BoundPolicy(def) => Some((def.name(), def.span())),
        Item::DataShape(def) => Some((&def.name, def.span)),
        Item::Collections(def) => Some((&def.name, def.span)),
        Item::CellRoles(def) => Some((&def.name, def.span)),
        Item::Resource(def) => Some((&def.name, def.span)),
        Item::Shared(def) => Some((&def.name, def.span)),
        Item::Receipt(def) => Some((&def.name, def.span)),
        Item::Struct(def) => Some((&def.name, def.span)),
        Item::Flow(def) => def.name.as_deref().map(|name| (name, def.span)),
        Item::Invariant(def) => Some((&def.name, def.span)),
        Item::Enum(def) => Some((&def.name, def.span)),
        Item::Const(def) => Some((&def.name, def.span)),
        Item::Action(def) => Some((&def.name, def.span)),
        Item::Function(def) => Some((&def.name, def.span)),
        Item::Lock(def) => Some((&def.name, def.span)),
        Item::Use(_) => None,
    }
}

fn aggregate_field_type_is_supported(ty: &Type) -> bool {
    match ty {
        Type::U8 | Type::U16 | Type::U32 | Type::I32 | Type::U64 | Type::U128 | Type::Address | Type::Hash => true,
        Type::Array(inner, _) => matches!(inner.as_ref(), Type::U8),
        _ => false,
    }
}

fn assignment_root_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Identifier(name) => Some(name.as_str()),
        Expr::FieldAccess(field) => assignment_root_name(&field.expr),
        Expr::Index(index) => assignment_root_name(&index.expr),
        _ => None,
    }
}

fn action_param_owned_named_type<'a>(action: &'a ActionDef, name: &str) -> Option<&'a str> {
    action.params.iter().find(|param| param.name == name).and_then(|param| match &param.ty {
        Type::Named(type_name) if matches!(param.source, ParamSource::Default | ParamSource::Input) && !param.is_read_ref => {
            Some(type_name.as_str())
        }
        _ => None,
    })
}

fn action_param_output_named_type<'a>(action: &'a ActionDef, name: &str) -> Option<&'a str> {
    if let Some(output) = action.outputs.iter().find(|output| output.name == name)
        && let Type::Named(type_name) = &output.ty
    {
        return Some(type_name.as_str());
    }
    None
}

fn action_output_binding_names(action: &ActionDef) -> HashMap<String, ActionOutputBinding> {
    let mut bindings = HashMap::new();
    for output in &action.outputs {
        if let Type::Named(type_name) = &output.ty {
            bindings.insert(output.name.clone(), ActionOutputBinding { type_name: type_name.as_str().to_string() });
        }
    }
    bindings
}

fn action_core_evidence_binding_names(action: &ActionDef) -> HashSet<String> {
    let mut bindings = action_output_binding_names(action).keys().cloned().collect::<HashSet<_>>();
    for input in action_inferred_lineage_bindings(action).keys() {
        bindings.insert(input.clone());
    }
    bindings
}

fn action_inferred_lineage_bindings(action: &ActionDef) -> HashMap<String, String> {
    let mut bindings = HashMap::new();
    let consumed = action_consumed_bindings(action);
    for state_edge in &action.state_edges {
        if consumed.contains(&state_edge.path.base) {
            continue;
        }
        bindings.entry(state_edge.path.base.clone()).or_insert_with(|| state_edge.to_path.base.clone());
    }

    let mut outputs_by_type: HashMap<String, Vec<String>> = HashMap::new();
    for (name, binding) in action_output_binding_names(action) {
        if bindings.values().any(|bound_output| bound_output == &name) {
            continue;
        }
        outputs_by_type.entry(binding.type_name).or_default().push(name);
    }

    let mut inputs_by_type: HashMap<String, Vec<String>> = HashMap::new();
    for param in &action.params {
        if consumed.contains(&param.name) || bindings.contains_key(&param.name) {
            continue;
        }
        let Some(type_name) = action_param_owned_named_type(action, &param.name) else {
            continue;
        };
        inputs_by_type.entry(type_name.to_string()).or_default().push(param.name.clone());
    }

    for (type_name, inputs) in inputs_by_type {
        let Some(outputs) = outputs_by_type.get(&type_name) else {
            continue;
        };
        if inputs.len() == 1 && outputs.len() == 1 {
            bindings.insert(inputs[0].clone(), outputs[0].clone());
        }
    }

    bindings
}

fn action_consumed_bindings(action: &ActionDef) -> HashSet<String> {
    let mut bindings = HashSet::new();
    collect_consumed_bindings_from_stmts(&action.body, &mut bindings);
    bindings
}

fn collect_consumed_bindings_from_stmts(stmts: &[Stmt], bindings: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Lifecycle(value) => {
                bindings.extend(value.input_bindings().iter().cloned());
                for expression in value.value_exprs() {
                    collect_consumed_bindings_from_expr(expression, bindings);
                }
            }
            Stmt::Let(let_stmt) => collect_consumed_bindings_from_expr(&let_stmt.value, bindings),
            Stmt::Expr(expr) | Stmt::Discard(expr) | Stmt::Return(ReturnStmt { value: Some(expr), .. }) => {
                collect_consumed_bindings_from_expr(expr, bindings)
            }
            Stmt::Return(ReturnStmt { value: None, .. }) => {}
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::If(if_stmt) => {
                collect_consumed_bindings_from_expr(&if_stmt.condition, bindings);
                collect_consumed_bindings_from_stmts(&if_stmt.then_branch, bindings);
                if let Some(else_branch) = &if_stmt.else_branch {
                    collect_consumed_bindings_from_stmts(else_branch, bindings);
                }
            }
            Stmt::For(for_stmt) => {
                collect_consumed_bindings_from_expr(&for_stmt.iterable, bindings);
                collect_consumed_bindings_from_stmts(&for_stmt.body, bindings);
            }
            Stmt::While(while_stmt) => {
                collect_consumed_bindings_from_expr(&while_stmt.condition, bindings);
                collect_consumed_bindings_from_stmts(&while_stmt.body, bindings);
            }
            Stmt::Borrow(borrow_stmt) => collect_consumed_bindings_from_stmts(&borrow_stmt.body, bindings),
        }
    }
}

fn collect_consumed_bindings_from_expr(expr: &Expr, bindings: &mut HashSet<String>) {
    match expr {
        Expr::Consume(consume) => {
            if let Expr::Identifier(name) = consume.expr.as_ref() {
                bindings.insert(name.clone());
            }
            collect_consumed_bindings_from_expr(&consume.expr, bindings);
        }
        Expr::Assign(assign) => {
            collect_consumed_bindings_from_expr(&assign.target, bindings);
            collect_consumed_bindings_from_expr(&assign.value, bindings);
        }
        Expr::Binary(binary) => {
            collect_consumed_bindings_from_expr(&binary.left, bindings);
            collect_consumed_bindings_from_expr(&binary.right, bindings);
        }
        Expr::Unary(unary) => collect_consumed_bindings_from_expr(&unary.expr, bindings),
        Expr::Call(call) => {
            collect_consumed_bindings_from_expr(&call.func, bindings);
            for arg in &call.args {
                collect_consumed_bindings_from_expr(arg, bindings);
            }
        }
        Expr::FieldAccess(field) => collect_consumed_bindings_from_expr(&field.expr, bindings),
        Expr::Index(index) => {
            collect_consumed_bindings_from_expr(&index.expr, bindings);
            collect_consumed_bindings_from_expr(&index.index, bindings);
        }
        Expr::Create(create) => {
            for (_, value) in &create.fields {
                collect_consumed_bindings_from_expr(value, bindings);
            }
            if let Some(lock) = &create.lock {
                collect_consumed_bindings_from_expr(lock, bindings);
            }
        }
        Expr::Destroy(destroy) => {
            if let Expr::Identifier(name) = destroy.expr.as_ref() {
                bindings.insert(name.clone());
            }
            collect_consumed_bindings_from_expr(&destroy.expr, bindings);
        }
        Expr::Claim(claim) => {
            if let Expr::Identifier(name) = claim.receipt.as_ref() {
                bindings.insert(name.clone());
            }
            collect_consumed_bindings_from_expr(&claim.receipt, bindings);
        }
        Expr::Settle(settle) => {
            if let Expr::Identifier(name) = settle.expr.as_ref() {
                bindings.insert(name.clone());
            }
            collect_consumed_bindings_from_expr(&settle.expr, bindings);
        }
        Expr::CreateUnique(create) => {
            for (_, value) in &create.fields {
                collect_consumed_bindings_from_expr(value, bindings);
            }
            if let Some(lock) = &create.lock {
                collect_consumed_bindings_from_expr(lock, bindings);
            }
        }
        Expr::ReplaceUnique(replace) => {
            if let Expr::Identifier(name) = replace.expr.as_ref() {
                bindings.insert(name.clone());
            }
            collect_consumed_bindings_from_expr(&replace.expr, bindings);
            for (_, value) in &replace.fields {
                collect_consumed_bindings_from_expr(value, bindings);
            }
        }
        Expr::Assert(assert_expr) => {
            collect_consumed_bindings_from_expr(&assert_expr.condition, bindings);
            collect_consumed_bindings_from_expr(&assert_expr.message, bindings);
        }
        Expr::Require(require_expr) => {
            collect_consumed_bindings_from_expr(&require_expr.condition, bindings);
            if let Some(message) = &require_expr.message {
                collect_consumed_bindings_from_expr(message, bindings);
            }
        }
        Expr::Block(stmts) => collect_consumed_bindings_from_stmts(stmts, bindings),
        Expr::Tuple(items) | Expr::Array(items) => {
            for item in items {
                collect_consumed_bindings_from_expr(item, bindings);
            }
        }
        Expr::If(if_expr) => {
            collect_consumed_bindings_from_expr(&if_expr.condition, bindings);
            collect_consumed_bindings_from_expr(&if_expr.then_branch, bindings);
            collect_consumed_bindings_from_expr(&if_expr.else_branch, bindings);
        }
        Expr::Cast(cast) => collect_consumed_bindings_from_expr(&cast.expr, bindings),
        Expr::Range(range) => {
            collect_consumed_bindings_from_expr(&range.start, bindings);
            collect_consumed_bindings_from_expr(&range.end, bindings);
        }
        Expr::StructInit(init) => {
            for (_, value) in &init.fields {
                collect_consumed_bindings_from_expr(value, bindings);
            }
        }
        Expr::Match(match_expr) => {
            collect_consumed_bindings_from_expr(&match_expr.expr, bindings);
            for arm in &match_expr.arms {
                collect_consumed_bindings_from_expr(&arm.value, bindings);
            }
        }
        Expr::Integer(_) | Expr::Bool(_) | Expr::String(_) | Expr::ByteString(_) | Expr::Identifier(_) | Expr::ReadRef(_) => {}
        Expr::StdlibCall(call) => {
            let qualified = format!("std::{}::{}", call.namespace, call.name);
            match qualified.as_str() {
                "std::lifecycle::transfer" | "std::receipt::claim" | "std::lifecycle::settle" => {
                    if !call.args.is_empty()
                        && let Expr::Identifier(name) = &call.args[0]
                    {
                        bindings.insert(name.clone());
                    }
                }
                _ => {}
            }
        }
        Expr::RequireBlock(require_block) => {
            for expr in &require_block.expressions {
                collect_consumed_bindings_from_expr(expr, bindings);
            }
        }
        Expr::Preserve(_) => {}
        Expr::ReplaceRelation(relation) => {
            bindings.insert(relation.before.clone());
        }
    }
}

fn direct_call_name(call: &CallExpr) -> Option<&str> {
    match call.func.as_ref() {
        Expr::Identifier(name) => Some(name.as_str()),
        _ => None,
    }
}

fn is_direct_call(expr: &Expr, expected: &str) -> bool {
    matches!(expr, Expr::Call(call) if direct_call_name(call) == Some(expected))
}

fn binding_pattern_name(pattern: &BindingPattern) -> Option<&str> {
    match pattern {
        BindingPattern::Name(name) => Some(name.as_str()),
        BindingPattern::Tuple(_) | BindingPattern::Wildcard => None,
    }
}

fn mutable_reference_root_names(expr: &Expr) -> Vec<&str> {
    let mut roots = Vec::new();
    collect_mutable_reference_root_names(expr, &mut roots);
    roots
}

fn collect_mutable_reference_root_names<'a>(expr: &'a Expr, roots: &mut Vec<&'a str>) {
    match expr {
        Expr::Identifier(name) => push_unique_root(roots, name.as_str()),
        Expr::FieldAccess(field) => collect_mutable_reference_root_names(&field.expr, roots),
        Expr::Index(index) => collect_mutable_reference_root_names(&index.expr, roots),
        Expr::Cast(cast) => collect_mutable_reference_root_names(&cast.expr, roots),
        Expr::If(if_expr) => {
            collect_mutable_reference_root_names(&if_expr.then_branch, roots);
            collect_mutable_reference_root_names(&if_expr.else_branch, roots);
        }
        Expr::Match(match_expr) => {
            for arm in &match_expr.arms {
                collect_mutable_reference_root_names(&arm.value, roots);
            }
        }
        Expr::Block(stmts) => collect_mutable_reference_root_names_from_tail_stmts(stmts, roots),
        _ => {}
    }
}

fn collect_mutable_reference_root_names_from_tail_stmts<'a>(stmts: &'a [Stmt], roots: &mut Vec<&'a str>) {
    let Some(last) = stmts.last() else {
        return;
    };
    match last {
        Stmt::Expr(expr) | Stmt::Discard(expr) | Stmt::Return(ReturnStmt { value: Some(expr), .. }) => {
            collect_mutable_reference_root_names(expr, roots)
        }
        Stmt::If(if_stmt) => {
            collect_mutable_reference_root_names_from_tail_stmts(&if_stmt.then_branch, roots);
            if let Some(else_branch) = &if_stmt.else_branch {
                collect_mutable_reference_root_names_from_tail_stmts(else_branch, roots);
            }
        }
        _ => {}
    }
}

fn push_unique_root<'a>(roots: &mut Vec<&'a str>, root: &'a str) {
    if !roots.contains(&root) {
        roots.push(root);
    }
}

fn identity_condition(identity: &IdentityPolicy) -> String {
    match identity {
        IdentityPolicy::None => "identity(none)".to_string(),
        IdentityPolicy::CkbTypeId => "identity(ckb_type_id)".to_string(),
        IdentityPolicy::Field(field) => format!("identity(field({field}))"),
        IdentityPolicy::ScriptArgs => "identity(script_args)".to_string(),
        IdentityPolicy::SingletonType => "identity(singleton_type)".to_string(),
    }
}

fn is_state_storage_type(ty: &Type) -> bool {
    matches!(ty, Type::U8 | Type::U16 | Type::U32 | Type::U64)
}

pub fn check(module: &Module) -> Result<()> {
    let mut checker = TypeChecker::new();
    checker.check_module(module)
}

pub fn diagnostics(module: &Module) -> Vec<CompileError> {
    let mut checker = TypeChecker::new();
    checker.check_module_diagnostics(module)
}

pub fn check_with_resolver(module: &Module, resolver: &ModuleResolver, current_module: &str) -> Result<()> {
    let mut checker = TypeChecker::with_resolver(resolver, current_module);
    checker.check_module(module)
}

pub fn diagnostics_with_resolver(module: &Module, resolver: &ModuleResolver, current_module: &str) -> Vec<CompileError> {
    let mut checker = TypeChecker::with_resolver(resolver, current_module);
    checker.check_module_diagnostics(module)
}

#[cfg(test)]
mod tests;
