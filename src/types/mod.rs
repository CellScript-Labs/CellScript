use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use crate::resolve::{FunctionDef, ModuleResolver, TypeDef};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallableKind {
    Action,
    Function,
    Lock,
}

#[derive(Debug, Clone)]
struct FunctionSignature {
    params: Vec<Type>,
    return_type: Option<Type>,
    kind: CallableKind,
}

#[derive(Debug, Clone)]
struct FlowSpec {
    type_name: String,
    field_name: String,
    field_enum_type: Option<String>,
    states: Vec<String>,
    transitions: Vec<StateTransition>,
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
    parent: Option<Box<TypeEnv>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinearState {
    Available,
    Consumed,
    Destroyed,
}

impl Default for TypeEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeEnv {
    pub fn new() -> Self {
        Self { vars: HashMap::new(), mutability: HashMap::new(), linear_states: HashMap::new(), parent: None }
    }

    pub fn child(&self) -> Self {
        Self { vars: HashMap::new(), mutability: HashMap::new(), linear_states: HashMap::new(), parent: Some(Box::new(self.clone())) }
    }

    pub fn lookup(&self, name: &str) -> Option<&Type> {
        self.vars.get(name).or_else(|| self.parent.as_ref().and_then(|p| p.lookup(name)))
    }

    pub fn is_mutable(&self, name: &str) -> bool {
        self.mutability.get(name).copied().or_else(|| self.parent.as_ref().map(|p| p.is_mutable(name))).unwrap_or(false)
    }

    pub fn insert(&mut self, name: String, ty: Type, is_linear: bool, is_mut: bool) {
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
                (true, _, true) if then_state == else_state => then_state,
                (true, true, false) => else_state,
                (false, true, true) => then_state,
                (false, true, false) if then_state == else_state => then_state,
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
            let first_state = first_env.linear_state(&name).unwrap_or(before);
            if arm_envs.iter().skip(1).any(|env| env.linear_state(&name).unwrap_or(before) != first_state) {
                return Err(CompileError::new(
                    format!("linear resource '{}' has inconsistent ownership state across match arms", name),
                    span,
                ));
            }
            self.set_existing_linear_state(&name, first_state);
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
        for (name, state) in &self.linear_states {
            if *state == LinearState::Available {
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
            parent: self.parent.as_ref().map(|p| Box::new((**p).clone())),
        }
    }
}

pub struct TypeChecker<'a> {
    env: TypeEnv,
    type_fields: HashMap<String, HashMap<String, Type>>,
    enum_variants: HashMap<String, Vec<String>>,
    enum_payload_variants: HashMap<String, HashSet<String>>,
    functions: HashMap<String, FunctionSignature>,
    linear_types: HashSet<String>,
    cell_type_kinds: HashMap<String, CellTypeKind>,
    type_capabilities: HashMap<String, HashSet<Capability>>,
    receipt_claim_outputs: HashMap<String, Option<Type>>,
    flow_states: HashMap<String, Vec<String>>,
    flow_state_fields: HashMap<String, String>,
    flows: HashMap<String, FlowSpec>,
    resolver: Option<&'a ModuleResolver>,
    current_module: Option<String>,
    current_callable: Option<CallableKind>,
    current_return_type: Option<Option<Type>>,
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
    match ty {
        Type::U8 => "u8".to_string(),
        Type::U16 => "u16".to_string(),
        Type::U32 => "u32".to_string(),
        Type::U64 => "u64".to_string(),
        Type::U128 => "u128".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Unit => "()".to_string(),
        Type::Address => "Address".to_string(),
        Type::Hash => "Hash".to_string(),
        Type::Array(inner, size) => format!("[{}; {}]", type_repr(inner), size),
        Type::Tuple(items) => format!("({})", items.iter().map(type_repr).collect::<Vec<_>>().join(", ")),
        Type::Named(name) => name.clone(),
        Type::Ref(inner) => format!("&{}", type_repr(inner)),
        Type::MutRef(inner) => format!("&mut {}", type_repr(inner)),
    }
}

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
            enum_variants: HashMap::new(),
            enum_payload_variants: HashMap::new(),
            functions: HashMap::new(),
            linear_types: HashSet::new(),
            cell_type_kinds: HashMap::new(),
            type_capabilities: HashMap::new(),
            receipt_claim_outputs: HashMap::new(),
            flow_states: HashMap::new(),
            flow_state_fields: HashMap::new(),
            flows: HashMap::new(),
            resolver: None,
            current_module: None,
            current_callable: None,
            current_return_type: None,
        }
    }

    pub fn with_resolver(resolver: &'a ModuleResolver, current_module: impl Into<String>) -> Self {
        let mut checker = Self::new();
        checker.resolver = Some(resolver);
        checker.current_module = Some(current_module.into());
        checker
    }

    pub fn check_module(&mut self, module: &Module) -> Result<()> {
        if self.current_module.is_none() {
            self.current_module = Some(module.name.clone());
        }
        let mut seen_symbols = HashSet::new();
        let mut seen_type_ids = HashMap::new();
        for item in &module.items {
            if let Some((symbol, span)) = item_symbol_name_and_span(item) {
                if !seen_symbols.insert(symbol.to_string()) {
                    return Err(CompileError::new(format!("duplicate symbol '{}'", symbol), span));
                }
            }
            match item {
                Item::Const(const_def) => {
                    self.validate_type(&const_def.ty)?;
                    self.env.insert(const_def.name.clone(), const_def.ty.clone(), false, false);
                }
                Item::Resource(resource) => {
                    register_type_id(&mut seen_type_ids, &resource.name, resource.type_id.as_ref())?;
                    self.linear_types.insert(resource.name.clone());
                    self.cell_type_kinds.insert(resource.name.clone(), CellTypeKind::Resource);
                    self.type_capabilities.insert(resource.name.clone(), resource.capabilities.iter().copied().collect());
                    self.type_fields.insert(
                        resource.name.clone(),
                        resource.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect(),
                    );
                }
                Item::Shared(shared) => {
                    register_type_id(&mut seen_type_ids, &shared.name, shared.type_id.as_ref())?;
                    self.linear_types.insert(shared.name.clone());
                    self.cell_type_kinds.insert(shared.name.clone(), CellTypeKind::Shared);
                    self.type_capabilities.insert(shared.name.clone(), shared.capabilities.iter().copied().collect());
                    self.type_fields.insert(
                        shared.name.clone(),
                        shared.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect(),
                    );
                }
                Item::Receipt(receipt) => {
                    register_type_id(&mut seen_type_ids, &receipt.name, receipt.type_id.as_ref())?;
                    self.linear_types.insert(receipt.name.clone());
                    self.cell_type_kinds.insert(receipt.name.clone(), CellTypeKind::Receipt);
                    self.type_capabilities.insert(receipt.name.clone(), receipt.capabilities.iter().copied().collect());
                    self.receipt_claim_outputs.insert(receipt.name.clone(), receipt.claim_output.clone());
                    self.type_fields.insert(
                        receipt.name.clone(),
                        receipt.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect(),
                    );
                }
                Item::Struct(struct_def) => {
                    register_type_id(&mut seen_type_ids, &struct_def.name, struct_def.type_id.as_ref())?;
                    self.type_fields.insert(
                        struct_def.name.clone(),
                        struct_def.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect(),
                    );
                }
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

        self.register_imported_type_ids(&mut seen_type_ids)?;
        self.register_flows(module)?;
        self.validate_flow_action_edges(module)?;

        for item in &module.items {
            self.check_item(item)?;
        }
        Ok(())
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
            if let Some(type_id) = type_def_type_id(&type_def) {
                register_type_id_value(seen_type_ids, local_name, &type_id.value, import.span)?;
            }
        }

        Ok(())
    }

    fn register_flows(&mut self, module: &Module) -> Result<()> {
        let mut seen_targets = HashSet::new();
        for item in &module.items {
            let Item::Flow(machine) = item else {
                continue;
            };
            if machine.transitions.is_empty() {
                return Err(CompileError::new("flow must declare at least one transition", machine.span));
            }
            let type_name = machine.target.base.clone();
            let field_name = machine.target.field.clone();
            let target_key = format!("{}.{}", type_name, field_name);
            if !seen_targets.insert(target_key.clone()) {
                return Err(CompileError::new(format!("duplicate flow for '{}'", target_key), machine.target.span));
            }
            if self.flow_states.contains_key(&type_name) {
                return Err(CompileError::new(
                    format!(
                        "type '{}' already has flow policy; this release supports one flow-backed state field per Cell type",
                        type_name
                    ),
                    machine.target.span,
                ));
            }
            if self.resolve_cell_type_kind(&type_name).is_none() {
                return Err(CompileError::new(
                    format!("flow target type '{}' must be a resource, shared, or receipt Cell type", type_name),
                    machine.target.span,
                ));
            }

            let fields = self
                .resolve_named_type_fields(&type_name)
                .ok_or_else(|| CompileError::new(format!("flow target type '{}' is not defined", type_name), machine.target.span))?;
            let field_ty = fields.get(&field_name).ok_or_else(|| {
                CompileError::new(format!("flow target field '{}.{}' is not defined", type_name, field_name), machine.target.span)
            })?;

            let (states, field_enum_type) = self.flow_states_for_decl(machine, field_ty)?;
            let mut seen_transitions = HashSet::new();
            let mut normalized_transitions = Vec::new();
            for transition in &machine.transitions {
                let from = self.canonical_state_name_for_flow(
                    &type_name,
                    field_enum_type.as_deref(),
                    &states,
                    &transition.from,
                    transition.span,
                )?;
                let to = self.canonical_state_name_for_flow(
                    &type_name,
                    field_enum_type.as_deref(),
                    &states,
                    &transition.to,
                    transition.span,
                )?;
                if from == to {
                    return Err(CompileError::new(format!("state transition '{} -> {}' is a no-op", from, to), transition.span));
                }
                if !seen_transitions.insert((from.clone(), to.clone())) {
                    return Err(CompileError::new(format!("duplicate state transition '{} -> {}'", from, to), transition.span));
                }
                if let Some(action) = &transition.action {
                    match self.functions.get(action) {
                        Some(signature) if signature.kind == CallableKind::Action => {}
                        Some(_) => {
                            return Err(CompileError::new(
                                format!("state transition action '{}' is not an action", action),
                                transition.span,
                            ))
                        }
                        None => {
                            return Err(CompileError::new(
                                format!("state transition action '{}' is not defined", action),
                                transition.span,
                            ))
                        }
                    }
                }
                normalized_transitions.push(StateTransition { from, to, action: transition.action.clone(), span: transition.span });
            }

            self.flow_states.insert(type_name.clone(), states.clone());
            self.flow_state_fields.insert(type_name.clone(), field_name.clone());
            self.flows.insert(
                type_name.clone(),
                FlowSpec { type_name, field_name, field_enum_type, states, transitions: normalized_transitions },
            );
        }

        Ok(())
    }

    fn validate_flow_action_edges(&self, module: &Module) -> Result<()> {
        let actions = module
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Action(action) => Some((action.name.as_str(), action)),
                _ => None,
            })
            .collect::<HashMap<_, _>>();

        for spec in self.flows.values() {
            for transition in &spec.transitions {
                let Some(action_name) = &transition.action else {
                    continue;
                };
                let Some(action) = actions.get(action_name.as_str()).copied() else {
                    continue;
                };
                let has_exact_move = action.state_edges.iter().any(|state_edge| {
                    let from_path = &state_edge.path;
                    let to_path = &state_edge.to_path;
                    from_path.field == spec.field_name
                        && to_path.field == spec.field_name
                        && action_param_owned_named_type(action, &from_path.base).is_some_and(|ty| ty == spec.type_name)
                        && action_param_output_named_type(action, &to_path.base).is_some_and(|ty| ty == spec.type_name)
                        && self
                            .canonical_state_name_for_flow(
                                &spec.type_name,
                                spec.field_enum_type.as_deref(),
                                &spec.states,
                                &state_edge.from,
                                state_edge.span,
                            )
                            .ok()
                            .is_some_and(|from| from == transition.from)
                        && self
                            .canonical_state_name_for_flow(
                                &spec.type_name,
                                spec.field_enum_type.as_deref(),
                                &spec.states,
                                &state_edge.to,
                                state_edge.span,
                            )
                            .ok()
                            .is_some_and(|to| to == transition.to)
                });
                if !has_exact_move {
                    return Err(CompileError::new(
                        format!(
                            "state transition action '{}' is bound to '{}.{} {} -> {}' and must declare the exact field-to-field move",
                            action.name, spec.type_name, spec.field_name, transition.from, transition.to
                        ),
                        transition.span,
                    ));
                }
            }
        }

        Ok(())
    }

    fn flow_states_for_decl(&self, machine: &FlowDef, field_ty: &Type) -> Result<(Vec<String>, Option<String>)> {
        if let Type::Named(enum_name) = field_ty {
            if let Some(variants) = self.resolve_enum_variants(enum_name) {
                if variants.iter().any(|variant| self.enum_variant_has_payload(enum_name, variant)) {
                    return Err(CompileError::new(
                        format!(
                            "flow field '{}.{}' enum '{}' must not have payload variants",
                            machine.target.base, machine.target.field, enum_name
                        ),
                        machine.target.span,
                    ));
                }
                for transition in &machine.transitions {
                    self.canonical_state_name_for_flow(
                        &machine.target.base,
                        Some(enum_name),
                        &variants,
                        &transition.from,
                        transition.span,
                    )?;
                    self.canonical_state_name_for_flow(
                        &machine.target.base,
                        Some(enum_name),
                        &variants,
                        &transition.to,
                        transition.span,
                    )?;
                }
                return Ok((variants, Some(enum_name.clone())));
            }
        }

        if !is_state_storage_type(field_ty) {
            return Err(CompileError::new(
                format!(
                    "flow field '{}.{}' must be an unsigned integer or no-payload enum",
                    machine.target.base, machine.target.field
                ),
                machine.target.span,
            ));
        }

        let mut states = Vec::new();
        for transition in &machine.transitions {
            for raw in [&transition.from, &transition.to] {
                let state = raw.rsplit_once("::").map_or(raw.as_str(), |(_, state)| state).to_string();
                if !states.iter().any(|existing| existing == &state) {
                    states.push(state);
                }
            }
        }
        if states.len() < 2 {
            return Err(CompileError::new("flow must mention at least two states", machine.span));
        }
        Ok((states, None))
    }

    fn canonical_state_name_for_flow(
        &self,
        type_name: &str,
        enum_name: Option<&str>,
        states: &[String],
        raw: &str,
        span: Span,
    ) -> Result<String> {
        let state = if let Some((qualifier, state)) = raw.rsplit_once("::") {
            if qualifier != type_name && Some(qualifier) != enum_name {
                return Err(CompileError::new(format!("state '{}' does not belong to '{}'", raw, type_name), span));
            }
            state
        } else {
            raw
        };
        if states.iter().any(|candidate| candidate == state) {
            Ok(state.to_string())
        } else {
            Err(CompileError::new(format!("unknown state '{}::{}'", type_name, state), span))
        }
    }

    fn check_item(&mut self, item: &Item) -> Result<()> {
        match item {
            Item::Resource(r) => self.check_resource(r),
            Item::Shared(s) => self.check_shared(s),
            Item::Receipt(r) => self.check_receipt(r),
            Item::Struct(s) => self.check_struct(s),
            Item::Flow(_) => Ok(()),
            Item::Const(c) => self.check_const(c),
            Item::Enum(e) => self.check_enum(e),
            Item::Action(a) => self.check_action(a),
            Item::Function(f) => self.check_function(f),
            Item::Lock(l) => self.check_lock(l),
            Item::Use(_) => Ok(()),
        }
    }

    fn check_resource(&mut self, resource: &ResourceDef) -> Result<()> {
        self.validate_schema_fields(&resource.fields, "resource", &resource.name)
    }

    fn check_shared(&mut self, shared: &SharedDef) -> Result<()> {
        self.validate_schema_fields(&shared.fields, "shared", &shared.name)
    }

    fn check_receipt(&mut self, receipt: &ReceiptDef) -> Result<()> {
        self.validate_schema_fields(&receipt.fields, "receipt", &receipt.name)?;
        if let Some(output) = &receipt.claim_output {
            self.validate_type(output)?;
            self.validate_receipt_claim_output(output, receipt.span)?;
        }
        Ok(())
    }

    fn check_struct(&mut self, struct_def: &StructDef) -> Result<()> {
        self.validate_schema_fields(&struct_def.fields, "struct", &struct_def.name)
    }

    fn validate_schema_fields(&self, fields: &[Field], item_kind: &str, item_name: &str) -> Result<()> {
        let mut seen = HashSet::new();
        for field in fields {
            if field.name == "_" {
                return Err(CompileError::new(
                    format!(
                        "{} '{}' field must have a stable name; '_' is reserved for local wildcard bindings",
                        item_kind, item_name
                    ),
                    field.span,
                ));
            }
            if !seen.insert(field.name.clone()) {
                return Err(CompileError::new(
                    format!("duplicate field '{}' in {} '{}'", field.name, item_kind, item_name),
                    field.span,
                ));
            }
            self.validate_type(&field.ty)?;
            self.validate_stored_type_has_no_references(
                &field.ty,
                &format!("{} '{}' field '{}'", item_kind, item_name, field.name),
                field.span,
            )?;
        }
        Ok(())
    }

    fn check_enum(&mut self, enum_def: &EnumDef) -> Result<()> {
        let mut seen = HashSet::new();
        for variant in &enum_def.variants {
            if !seen.insert(variant.name.clone()) {
                return Err(CompileError::new(format!("duplicate enum variant '{}::{}'", enum_def.name, variant.name), variant.span));
            }
            for field_ty in &variant.fields {
                self.validate_type(field_ty)?;
                self.validate_stored_type_has_no_references(
                    field_ty,
                    &format!("enum variant '{}::{}' payload", enum_def.name, variant.name),
                    variant.span,
                )?;
            }
        }
        Ok(())
    }

    fn check_const(&mut self, const_def: &ConstDef) -> Result<()> {
        let mut env = self.env.clone();
        let value_ty = self.infer_expr(&mut env, &const_def.value)?;
        if !self.types_equal(&value_ty, &const_def.ty) {
            return Err(CompileError::new(
                format!("const '{}' has type mismatch: expected {:?}, found {:?}", const_def.name, const_def.ty, value_ty),
                const_def.span,
            ));
        }
        Ok(())
    }

    fn check_action(&mut self, action: &ActionDef) -> Result<()> {
        let previous_callable = self.current_callable.replace(CallableKind::Action);
        let previous_return_type = self.current_return_type.replace(action.return_type.clone());
        let result = (|| {
            let mut env = self.env.child();
            let core_evidence_bindings = action_core_evidence_binding_names(action);

            self.bind_callable_params_with_non_linear(&mut env, &action.params, "action", &action.name, &core_evidence_bindings)?;
            self.bind_action_outputs(&mut env, action)?;
            self.validate_action_state_edges(action, &env)?;
            self.validate_action_create_targets(action)?;
            self.validate_action_branch_obligations(action)?;
            if let Some(return_type) = &action.return_type {
                self.validate_callable_return_type("action", &action.name, return_type, action.span)?;
            }
            let return_env = env.clone();
            self.check_no_unreachable_stmts(&action.body)?;

            let tail = self.check_body_statements(&mut env, &action.body)?;

            if let Some(return_type) = &action.return_type {
                self.check_body_returns_or_tail_expr("action", &action.name, &action.body, return_type, action.span, &return_env)?;
            }

            if let Some((tail_base, stmt)) = tail {
                self.mark_stmt_as_returned(&mut env, &tail_base, stmt)?;
            }

            env.check_linear_complete()
        })();
        self.current_callable = previous_callable;
        self.current_return_type = previous_return_type;
        result
    }

    fn bind_action_outputs(&self, env: &mut TypeEnv, action: &ActionDef) -> Result<()> {
        let mut seen = action.params.iter().map(|param| param.name.clone()).collect::<HashSet<_>>();
        for output in &action.outputs {
            if output.name == "_" {
                return Err(CompileError::new(
                    format!("action '{}' output binding must have a stable name", action.name),
                    output.span,
                ));
            }
            if !seen.insert(output.name.clone()) {
                return Err(CompileError::new(
                    format!("duplicate action binding '{}' in action '{}'", output.name, action.name),
                    output.span,
                ));
            }
            self.validate_type(&output.ty)?;
            let Some(type_name) = Self::base_type_name(&output.ty) else {
                return Err(CompileError::new(
                    format!("action output '{}' must name a Cell-backed resource, shared cell, or receipt type", output.name),
                    output.span,
                ));
            };
            if self.resolve_cell_type_kind(type_name).is_none() {
                return Err(CompileError::new(
                    format!(
                        "action output '{}' references non-Cell type {}; action outputs are proposed transaction output Cells",
                        output.name,
                        type_repr(&output.ty)
                    ),
                    output.span,
                ));
            }
            env.bind_new(output.name.clone(), output.ty.clone(), false, false, output.span)?;
        }
        Ok(())
    }

    fn validate_action_create_targets(&self, action: &ActionDef) -> Result<()> {
        let outputs = action_output_binding_names(action);
        self.validate_create_targets_in_stmts(&action.body, &outputs)
    }

    fn validate_action_branch_obligations(&self, action: &ActionDef) -> Result<()> {
        let outputs = action_output_binding_names(action).keys().cloned().collect::<HashSet<_>>();
        if outputs.is_empty() {
            return Ok(());
        }

        self.validate_branch_obligations_in_stmts(&action.body, &outputs, HashSet::new())?;
        Ok(())
    }

    fn validate_branch_obligations_in_stmts(
        &self,
        stmts: &[Stmt],
        outputs: &HashSet<String>,
        mut guaranteed: HashSet<String>,
    ) -> Result<HashSet<String>> {
        for stmt in stmts {
            match stmt {
                Stmt::Let(let_stmt) => {
                    guaranteed = self.validate_branch_obligations_in_expr(&let_stmt.value, outputs, guaranteed)?;
                }
                Stmt::Expr(expr) | Stmt::Return(Some(expr)) => {
                    guaranteed = self.validate_branch_obligations_in_expr(expr, outputs, guaranteed)?;
                }
                Stmt::Return(None) => {}
                Stmt::If(if_stmt) => {
                    self.validate_branch_obligations_in_expr(&if_stmt.condition, outputs, guaranteed.clone())?;
                    let then_guaranteed =
                        self.validate_branch_obligations_in_stmts(&if_stmt.then_branch, outputs, guaranteed.clone())?;
                    let else_guaranteed = if let Some(else_branch) = &if_stmt.else_branch {
                        self.validate_branch_obligations_in_stmts(else_branch, outputs, guaranteed.clone())?
                    } else {
                        guaranteed.clone()
                    };
                    self.reject_asymmetric_branch_constraints(
                        &guaranteed,
                        &[then_guaranteed.clone(), else_guaranteed.clone()],
                        if_stmt.span,
                    )?;
                    guaranteed = then_guaranteed.intersection(&else_guaranteed).cloned().collect();
                }
                Stmt::For(for_stmt) => {
                    self.validate_branch_obligations_in_expr(&for_stmt.iterable, outputs, guaranteed.clone())?;
                    self.validate_branch_obligations_in_stmts(&for_stmt.body, outputs, guaranteed.clone())?;
                }
                Stmt::While(while_stmt) => {
                    self.validate_branch_obligations_in_expr(&while_stmt.condition, outputs, guaranteed.clone())?;
                    self.validate_branch_obligations_in_stmts(&while_stmt.body, outputs, guaranteed.clone())?;
                }
            }
        }

        Ok(guaranteed)
    }

    fn validate_branch_obligations_in_expr(
        &self,
        expr: &Expr,
        outputs: &HashSet<String>,
        mut guaranteed: HashSet<String>,
    ) -> Result<HashSet<String>> {
        match expr {
            Expr::Require(require_expr) => {
                collect_required_output_fields(&require_expr.condition, outputs, &mut guaranteed);
                self.validate_branch_obligations_in_expr(&require_expr.condition, outputs, guaranteed.clone())?;
                if let Some(message) = &require_expr.message {
                    self.validate_branch_obligations_in_expr(message, outputs, guaranteed.clone())?;
                }
                Ok(guaranteed)
            }
            Expr::If(if_expr) => {
                self.validate_branch_obligations_in_expr(&if_expr.condition, outputs, guaranteed.clone())?;
                let then_guaranteed = self.validate_branch_obligations_in_expr(&if_expr.then_branch, outputs, guaranteed.clone())?;
                let else_guaranteed = self.validate_branch_obligations_in_expr(&if_expr.else_branch, outputs, guaranteed.clone())?;
                self.reject_asymmetric_branch_constraints(
                    &guaranteed,
                    &[then_guaranteed.clone(), else_guaranteed.clone()],
                    if_expr.span,
                )?;
                Ok(then_guaranteed.intersection(&else_guaranteed).cloned().collect())
            }
            Expr::Match(match_expr) => {
                self.validate_branch_obligations_in_expr(&match_expr.expr, outputs, guaranteed.clone())?;
                let mut arm_sets = Vec::with_capacity(match_expr.arms.len());
                for arm in &match_expr.arms {
                    arm_sets.push(self.validate_branch_obligations_in_expr(&arm.value, outputs, guaranteed.clone())?);
                }
                self.reject_asymmetric_branch_constraints(&guaranteed, &arm_sets, match_expr.span)?;
                let mut iter = arm_sets.into_iter();
                let Some(first) = iter.next() else {
                    return Ok(guaranteed);
                };
                Ok(iter.fold(first, |acc, arm| acc.intersection(&arm).cloned().collect()))
            }
            Expr::Block(stmts) => self.validate_branch_obligations_in_stmts(stmts, outputs, guaranteed),
            Expr::Assign(assign) => {
                self.validate_branch_obligations_in_expr(&assign.target, outputs, guaranteed.clone())?;
                self.validate_branch_obligations_in_expr(&assign.value, outputs, guaranteed)
            }
            Expr::Binary(binary) => {
                self.validate_branch_obligations_in_expr(&binary.left, outputs, guaranteed.clone())?;
                self.validate_branch_obligations_in_expr(&binary.right, outputs, guaranteed)
            }
            Expr::Unary(unary) => self.validate_branch_obligations_in_expr(&unary.expr, outputs, guaranteed),
            Expr::Call(call) => {
                self.validate_branch_obligations_in_expr(&call.func, outputs, guaranteed.clone())?;
                for arg in &call.args {
                    self.validate_branch_obligations_in_expr(arg, outputs, guaranteed.clone())?;
                }
                Ok(guaranteed)
            }
            Expr::FieldAccess(field) => self.validate_branch_obligations_in_expr(&field.expr, outputs, guaranteed),
            Expr::Index(index) => {
                self.validate_branch_obligations_in_expr(&index.expr, outputs, guaranteed.clone())?;
                self.validate_branch_obligations_in_expr(&index.index, outputs, guaranteed)
            }
            Expr::Create(create) => {
                for (_, value) in &create.fields {
                    self.validate_branch_obligations_in_expr(value, outputs, guaranteed.clone())?;
                }
                if let Some(lock) = &create.lock {
                    self.validate_branch_obligations_in_expr(lock, outputs, guaranteed.clone())?;
                }
                Ok(guaranteed)
            }
            Expr::Consume(consume) => self.validate_branch_obligations_in_expr(&consume.expr, outputs, guaranteed),
            Expr::Destroy(destroy) => self.validate_branch_obligations_in_expr(&destroy.expr, outputs, guaranteed),
            Expr::Assert(assert_expr) => {
                self.validate_branch_obligations_in_expr(&assert_expr.condition, outputs, guaranteed.clone())?;
                self.validate_branch_obligations_in_expr(&assert_expr.message, outputs, guaranteed)
            }
            Expr::Tuple(elems) | Expr::Array(elems) => {
                for elem in elems {
                    self.validate_branch_obligations_in_expr(elem, outputs, guaranteed.clone())?;
                }
                Ok(guaranteed)
            }
            Expr::Cast(cast) => self.validate_branch_obligations_in_expr(&cast.expr, outputs, guaranteed),
            Expr::Range(range) => {
                self.validate_branch_obligations_in_expr(&range.start, outputs, guaranteed.clone())?;
                self.validate_branch_obligations_in_expr(&range.end, outputs, guaranteed)
            }
            Expr::StructInit(init) => {
                for (_, value) in &init.fields {
                    self.validate_branch_obligations_in_expr(value, outputs, guaranteed.clone())?;
                }
                Ok(guaranteed)
            }
            Expr::Integer(_)
            | Expr::Bool(_)
            | Expr::String(_)
            | Expr::ByteString(_)
            | Expr::Identifier(_)
            | Expr::ReadRef(_)
            | Expr::StdlibCall(_) => Ok(guaranteed),
            Expr::RequireBlock(require_block) => {
                let mut current = guaranteed;
                for expr in &require_block.expressions {
                    current = self.validate_branch_obligations_in_expr(expr, outputs, current)?;
                }
                Ok(current)
            }
            Expr::Preserve(preserve) => {
                for field in &preserve.fields {
                    guaranteed.insert(field.clone());
                }
                Ok(guaranteed)
            }
        }
    }

    fn reject_asymmetric_branch_constraints(&self, base: &HashSet<String>, branches: &[HashSet<String>], span: Span) -> Result<()> {
        if branches.len() < 2 {
            return Ok(());
        }

        let mut union = HashSet::new();
        for branch in branches {
            for field in branch {
                if !base.contains(field) {
                    union.insert(field.clone());
                }
            }
        }

        let mut asymmetric = union
            .into_iter()
            .filter(|field| {
                branches.iter().any(|branch| branch.contains(field)) && branches.iter().any(|branch| !branch.contains(field))
            })
            .collect::<Vec<_>>();
        asymmetric.sort();

        if let Some(field) = asymmetric.into_iter().next() {
            return Err(CompileError::new(
                format!("incomplete branch constraints: field '{}' is constrained in one branch but not all branches", field),
                span,
            ));
        }

        Ok(())
    }

    fn validate_create_targets_in_stmts(&self, stmts: &[Stmt], outputs: &HashMap<String, ActionOutputBinding>) -> Result<()> {
        for stmt in stmts {
            match stmt {
                Stmt::Let(let_stmt) => self.validate_create_targets_in_expr(&let_stmt.value, outputs)?,
                Stmt::Expr(expr) | Stmt::Return(Some(expr)) => self.validate_create_targets_in_expr(expr, outputs)?,
                Stmt::Return(None) => {}
                Stmt::If(if_stmt) => {
                    self.validate_create_targets_in_expr(&if_stmt.condition, outputs)?;
                    self.validate_create_targets_in_stmts(&if_stmt.then_branch, outputs)?;
                    if let Some(else_branch) = &if_stmt.else_branch {
                        self.validate_create_targets_in_stmts(else_branch, outputs)?;
                    }
                }
                Stmt::For(for_stmt) => {
                    self.validate_create_targets_in_expr(&for_stmt.iterable, outputs)?;
                    self.validate_create_targets_in_stmts(&for_stmt.body, outputs)?;
                }
                Stmt::While(while_stmt) => {
                    self.validate_create_targets_in_expr(&while_stmt.condition, outputs)?;
                    self.validate_create_targets_in_stmts(&while_stmt.body, outputs)?;
                }
            }
        }
        Ok(())
    }

    fn validate_create_targets_in_expr(&self, expr: &Expr, outputs: &HashMap<String, ActionOutputBinding>) -> Result<()> {
        match expr {
            Expr::Create(create) => {
                if let Some(target) = &create.target {
                    let Some(binding) = outputs.get(target) else {
                        return Err(CompileError::new(
                            format!("create target '{}' must be declared as an action output binding", target),
                            create.span,
                        ));
                    };
                    if binding.type_name != create.ty {
                        return Err(CompileError::new(
                            format!(
                                "create target '{}' has type '{}', but initializer constructs '{}'",
                                target, binding.type_name, create.ty
                            ),
                            create.span,
                        ));
                    }
                }
                for (_, value) in &create.fields {
                    self.validate_create_targets_in_expr(value, outputs)?;
                }
                if let Some(lock) = &create.lock {
                    self.validate_create_targets_in_expr(lock, outputs)?;
                }
            }
            Expr::Assign(assign) => {
                self.validate_create_targets_in_expr(&assign.target, outputs)?;
                self.validate_create_targets_in_expr(&assign.value, outputs)?;
            }
            Expr::Binary(binary) => {
                self.validate_create_targets_in_expr(&binary.left, outputs)?;
                self.validate_create_targets_in_expr(&binary.right, outputs)?;
            }
            Expr::Unary(unary) => self.validate_create_targets_in_expr(&unary.expr, outputs)?,
            Expr::Call(call) => {
                self.validate_create_targets_in_expr(&call.func, outputs)?;
                for arg in &call.args {
                    self.validate_create_targets_in_expr(arg, outputs)?;
                }
            }
            Expr::FieldAccess(field) => self.validate_create_targets_in_expr(&field.expr, outputs)?,
            Expr::Index(index) => {
                self.validate_create_targets_in_expr(&index.expr, outputs)?;
                self.validate_create_targets_in_expr(&index.index, outputs)?;
            }
            Expr::Consume(consume) => self.validate_create_targets_in_expr(&consume.expr, outputs)?,
            Expr::Destroy(destroy) => self.validate_create_targets_in_expr(&destroy.expr, outputs)?,
            Expr::Assert(assert_expr) => {
                self.validate_create_targets_in_expr(&assert_expr.condition, outputs)?;
                self.validate_create_targets_in_expr(&assert_expr.message, outputs)?;
            }
            Expr::Require(require_expr) => {
                self.validate_create_targets_in_expr(&require_expr.condition, outputs)?;
                if let Some(message) = &require_expr.message {
                    self.validate_create_targets_in_expr(message, outputs)?;
                }
            }
            Expr::Block(stmts) => self.validate_create_targets_in_stmts(stmts, outputs)?,
            Expr::Tuple(items) | Expr::Array(items) => {
                for item in items {
                    self.validate_create_targets_in_expr(item, outputs)?;
                }
            }
            Expr::If(if_expr) => {
                self.validate_create_targets_in_expr(&if_expr.condition, outputs)?;
                self.validate_create_targets_in_expr(&if_expr.then_branch, outputs)?;
                self.validate_create_targets_in_expr(&if_expr.else_branch, outputs)?;
            }
            Expr::Cast(cast) => self.validate_create_targets_in_expr(&cast.expr, outputs)?,
            Expr::Range(range) => {
                self.validate_create_targets_in_expr(&range.start, outputs)?;
                self.validate_create_targets_in_expr(&range.end, outputs)?;
            }
            Expr::StructInit(init) => {
                for (_, value) in &init.fields {
                    self.validate_create_targets_in_expr(value, outputs)?;
                }
            }
            Expr::Match(match_expr) => {
                self.validate_create_targets_in_expr(&match_expr.expr, outputs)?;
                for arm in &match_expr.arms {
                    self.validate_create_targets_in_expr(&arm.value, outputs)?;
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
                    self.validate_create_targets_in_expr(expr, outputs)?;
                }
            }
            Expr::Preserve(_) => {}
        }
        Ok(())
    }

    fn validate_action_state_edges(&self, action: &ActionDef, env: &TypeEnv) -> Result<()> {
        let output_bindings = action_output_binding_names(action);
        let mut lineage_inputs = HashMap::new();
        let mut lineage_outputs = HashMap::new();
        for state_edge in &action.state_edges {
            let from_path = &state_edge.path;
            let to_path = &state_edge.to_path;
            if let Some(previous_output) = lineage_inputs.insert(from_path.base.clone(), to_path.base.clone()) {
                if previous_output != to_path.base {
                    return Err(CompileError::new(
                        format!(
                            "state move binding '{}' points to both '{}' and '{}'; split/merge lineage is not supported",
                            from_path.base, previous_output, to_path.base
                        ),
                        state_edge.span,
                    ));
                }
            }
            if let Some(previous_input) = lineage_outputs.insert(to_path.base.clone(), from_path.base.clone()) {
                if previous_input != from_path.base {
                    return Err(CompileError::new(
                        format!(
                            "state move output '{}' is reached from both '{}' and '{}'; split/merge lineage is not supported",
                            to_path.base, previous_input, from_path.base
                        ),
                        state_edge.span,
                    ));
                }
            }
        }

        for state_edge in &action.state_edges {
            let path = &state_edge.path;
            let to_path = &state_edge.to_path;
            let ty = env
                .lookup(&path.base)
                .ok_or_else(|| CompileError::new(format!("unknown state move binding '{}'", path.base), path.span))?;
            let Some(param) = action.params.iter().find(|param| param.name == path.base) else {
                return Err(CompileError::new(
                    format!("state move binding '{}' must name an action input parameter", path.base),
                    path.span,
                ));
            };
            if !matches!(param.source, ParamSource::Default | ParamSource::Input)
                || param.is_read_ref
                || !matches!(param.ty, Type::Named(_))
            {
                return Err(CompileError::new(
                    format!(
                        "state move binding '{}' must be an owned Cell input parameter, not a reference, witness, lock_args, protected, output, or read parameter",
                        path.base
                    ),
                    path.span,
                ));
            }
            let type_name = Self::base_type_name(ty)
                .ok_or_else(|| CompileError::new(format!("state move binding '{}' is not a named state type", path.base), path.span))?
                .to_string();
            let Some(output_binding) = output_bindings.get(&to_path.base) else {
                return Err(CompileError::new(
                    format!("state move output binding '{}' must be a named action return", to_path.base),
                    to_path.span,
                ));
            };
            if output_binding.type_name != type_name {
                return Err(CompileError::new(
                    format!(
                        "state move input '{}.{}' has type '{}', but output '{}.{}' has type '{}'",
                        path.base, path.field, type_name, to_path.base, to_path.field, output_binding.type_name
                    ),
                    state_edge.span,
                ));
            }
            let spec = self
                .flows
                .get(&type_name)
                .ok_or_else(|| CompileError::new(format!("type '{}' has no declared flow", type_name), path.span))?;
            if spec.field_name != path.field {
                return Err(CompileError::new(
                    format!(
                        "state move field '{}.{}' does not match declared flow field '{}.{}'",
                        path.base, path.field, type_name, spec.field_name
                    ),
                    path.span,
                ));
            }
            if spec.field_name != to_path.field {
                return Err(CompileError::new(
                    format!(
                        "state move output field '{}.{}' does not match declared flow field '{}.{}'",
                        to_path.base, to_path.field, type_name, spec.field_name
                    ),
                    to_path.span,
                ));
            }

            let states = self
                .flow_states
                .get(&type_name)
                .ok_or_else(|| CompileError::new(format!("type '{}' has no declared flow", type_name), state_edge.span))?;
            let from = self.canonical_state_name_for_flow(
                &type_name,
                spec.field_enum_type.as_deref(),
                states,
                &state_edge.from,
                state_edge.span,
            )?;
            let to = self.canonical_state_name_for_flow(
                &type_name,
                spec.field_enum_type.as_deref(),
                states,
                &state_edge.to,
                state_edge.span,
            )?;
            if !spec.transitions.iter().any(|transition| transition.from == from && transition.to == to) {
                return Err(CompileError::new(
                    format!("state transition '{}.{} {} -> {}' is not declared", type_name, spec.field_name, from, to),
                    state_edge.span,
                ));
            }
        }

        Ok(())
    }

    fn check_function(&mut self, function: &FnDef) -> Result<()> {
        let previous_callable = self.current_callable.replace(CallableKind::Function);
        let previous_return_type = self.current_return_type.replace(function.return_type.clone());
        let result = (|| {
            let mut env = self.env.child();

            self.bind_callable_params(&mut env, &function.params, "function", &function.name)?;
            if let Some(return_type) = &function.return_type {
                self.validate_callable_return_type("function", &function.name, return_type, function.span)?;
            }
            let return_env = env.clone();
            self.check_no_unreachable_stmts(&function.body)?;

            let tail = self.check_body_statements(&mut env, &function.body)?;

            if let Some(return_type) = &function.return_type {
                self.check_body_returns_or_tail_expr(
                    "function",
                    &function.name,
                    &function.body,
                    return_type,
                    function.span,
                    &return_env,
                )?;
            }

            if let Some((tail_base, stmt)) = tail {
                self.mark_stmt_as_returned(&mut env, &tail_base, stmt)?;
            }

            env.check_linear_complete()
        })();
        self.current_callable = previous_callable;
        self.current_return_type = previous_return_type;
        result
    }

    fn check_lock(&mut self, lock: &LockDef) -> Result<()> {
        let previous_callable = self.current_callable.replace(CallableKind::Lock);
        let previous_return_type = self.current_return_type.replace(Some(Type::Bool));
        let result = (|| {
            if lock.return_type != Type::Bool {
                return Err(CompileError::new("lock definitions must return bool", lock.span));
            }

            let mut env = self.env.child();

            self.bind_callable_params(&mut env, &lock.params, "lock", &lock.name)?;
            self.check_no_unreachable_stmts(&lock.body)?;

            let tail = self.check_body_statements(&mut env, &lock.body)?;

            let Some(stmt) = lock.body.last() else {
                return Err(CompileError::new("lock body must return a bool value", lock.span));
            };
            let return_ty = self.infer_lock_terminal_stmt(&mut env, stmt)?;
            if !self.is_bool_type(&return_ty) {
                return Err(CompileError::new("lock body must evaluate to bool", lock.span));
            }
            if let Some((tail_base, stmt)) = tail {
                self.mark_stmt_as_returned(&mut env, &tail_base, stmt)?;
            }

            env.check_linear_complete()
        })();
        self.current_callable = previous_callable;
        self.current_return_type = previous_return_type;
        result
    }

    fn validate_callable_return_type(&self, callable_kind: &str, callable_name: &str, return_type: &Type, span: Span) -> Result<()> {
        self.validate_type(return_type)?;
        if self.type_contains_reference(return_type) {
            return Err(CompileError::new(
                format!(
                    "{} '{}' cannot return reference type {}; references cannot escape callable boundaries",
                    callable_kind,
                    callable_name,
                    type_repr(return_type)
                ),
                span,
            ));
        }
        if callable_kind == "function" && self.type_contains_cell_backed_value(return_type) {
            return Err(CompileError::new(
                format!(
                    "function '{}' cannot return cell-backed type {}; pure helpers must return non-Cell values",
                    callable_name,
                    type_repr(return_type)
                ),
                span,
            ));
        }
        Ok(())
    }

    fn validate_stored_type_has_no_references(&self, ty: &Type, owner: &str, span: Span) -> Result<()> {
        if self.type_contains_reference(ty) {
            return Err(CompileError::new(
                format!("{} cannot use reference type {}; schema storage must use owned serializable values", owner, type_repr(ty)),
                span,
            ));
        }
        Ok(())
    }

    fn type_contains_reference(&self, ty: &Type) -> bool {
        match ty {
            Type::Ref(_) | Type::MutRef(_) => true,
            Type::Array(inner, _) => self.type_contains_reference(inner),
            Type::Tuple(items) => items.iter().any(|item| self.type_contains_reference(item)),
            Type::Named(name) => self.named_type_contains_reference(name),
            _ => false,
        }
    }

    fn type_contains_mutable_reference(ty: &Type) -> bool {
        match ty {
            Type::MutRef(_) => true,
            Type::Array(inner, _) => Self::type_contains_mutable_reference(inner),
            Type::Tuple(items) => items.iter().any(Self::type_contains_mutable_reference),
            Type::Named(name) => name.contains("&mut "),
            _ => false,
        }
    }

    fn type_contains_cell_backed_value(&self, ty: &Type) -> bool {
        match ty {
            Type::Array(inner, _) => self.type_contains_cell_backed_value(inner),
            Type::Tuple(items) => items.iter().any(|item| self.type_contains_cell_backed_value(item)),
            Type::Named(name) => {
                let base_name = name.split('<').next().unwrap_or(name.as_str());
                self.resolve_cell_type_kind(base_name).is_some() || self.named_type_generic_payload_contains_cell_backed_value(name)
            }
            Type::Ref(_) | Type::MutRef(_) => false,
            _ => false,
        }
    }

    fn named_type_contains_reference(&self, name: &str) -> bool {
        name.contains("read_ref ") || name.contains('&')
    }

    fn named_type_generic_payload<'b>(&self, name: &'b str) -> Option<&'b str> {
        let start = name.find('<')?;
        name.ends_with('>').then_some(&name[start + 1..name.len() - 1])
    }

    fn named_type_generic_payload_contains_cell_backed_value(&self, name: &str) -> bool {
        self.named_type_generic_payload(name).is_some_and(|payload| self.type_fragment_contains_cell_backed_name(payload))
    }

    fn type_fragment_contains_cell_backed_name(&self, fragment: &str) -> bool {
        let mut token = String::new();
        for ch in fragment.chars() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' {
                token.push(ch);
            } else if self.type_name_token_is_cell_backed(&token) {
                return true;
            } else {
                token.clear();
            }
        }
        self.type_name_token_is_cell_backed(&token)
    }

    fn type_name_token_is_cell_backed(&self, token: &str) -> bool {
        match token {
            "" | "u8" | "u16" | "u32" | "u64" | "u128" | "bool" | "Address" | "Hash" | "String" | "Range" | "Vec" | "usize"
            | "isize" | "read_ref" | "mut" => false,
            name => self.resolve_cell_type_kind(name).is_some(),
        }
    }

    fn reference_target_is_cell_backed_aggregate(&self, ty: &Type) -> bool {
        match ty {
            Type::Array(_, _) | Type::Tuple(_) => self.type_contains_cell_backed_value(ty),
            _ => false,
        }
    }

    fn bind_callable_params(&self, env: &mut TypeEnv, params: &[Param], callable_kind: &str, callable_name: &str) -> Result<()> {
        self.bind_callable_params_with_non_linear(env, params, callable_kind, callable_name, &HashSet::new())
    }

    fn bind_callable_params_with_non_linear(
        &self,
        env: &mut TypeEnv,
        params: &[Param],
        callable_kind: &str,
        callable_name: &str,
        non_linear_params: &HashSet<String>,
    ) -> Result<()> {
        let mut seen = HashSet::new();
        for param in params {
            if param.name == "_" {
                return Err(CompileError::new(
                    format!(
                        "{} '{}' parameter must have a stable name; '_' is reserved for local wildcard bindings",
                        callable_kind, callable_name
                    ),
                    param.span,
                ));
            }
            if !seen.insert(param.name.clone()) {
                return Err(CompileError::new(
                    format!("duplicate parameter '{}' in {} '{}'", param.name, callable_kind, callable_name),
                    param.span,
                ));
            }
            self.validate_type(&param.ty)?;
            self.validate_callable_param_source(param, callable_kind, callable_name)?;
            self.validate_callable_param_reference_shape(param, callable_kind, callable_name)?;
            self.validate_callable_param_state_authority(param, callable_kind, callable_name)?;
            self.validate_callable_param_mutability(param)?;
            let is_linear = self.is_linear_type(&param.ty) && !non_linear_params.contains(param.name.as_str());
            env.bind_new(param.name.clone(), param.ty.clone(), is_linear, param.is_mut, param.span)?;
        }
        Ok(())
    }

    fn validate_callable_param_source(&self, param: &Param, callable_kind: &str, callable_name: &str) -> Result<()> {
        if param.source == ParamSource::Default {
            return Ok(());
        }
        if param.source == ParamSource::Input {
            if callable_kind != "action" {
                return Err(CompileError::new(
                    format!(
                        "{} '{}' parameter '{}' cannot use input source classification; input parameters are action verifier bindings",
                        callable_kind, callable_name, param.name
                    ),
                    param.span,
                ));
            }
            if param.is_mut || param.is_ref || param.is_read_ref || matches!(param.ty, Type::Ref(_) | Type::MutRef(_)) {
                return Err(CompileError::new(
                    format!("input action parameter '{}' must use 'input name: T' without mut/ref/read_ref modifiers", param.name),
                    param.span,
                ));
            }
            let Some(name) = Self::base_type_name(&param.ty) else {
                return Err(CompileError::new(
                    format!("input action parameter '{}' must name a Cell-backed resource, shared cell, or receipt type", param.name),
                    param.span,
                ));
            };
            if self.resolve_cell_type_kind(name).is_none() {
                return Err(CompileError::new(
                    format!(
                        "input action parameter '{}' references non-Cell type {}; input only marks a consumed transaction input Cell",
                        param.name,
                        type_repr(&param.ty)
                    ),
                    param.span,
                ));
            }
            return Ok(());
        }
        if param.source == ParamSource::Output {
            return Err(CompileError::new(
                format!(
                    "{} '{}' parameter '{}' cannot use output source classification; bind transaction outputs on the action return side (`action f(...) -> name: T`)",
                    callable_kind, callable_name, param.name
                ),
                param.span,
            ));
        }
        if param.source == ParamSource::Witness {
            if callable_kind != "action" && callable_kind != "lock" {
                return Err(CompileError::new(
                    format!(
                        "{} '{}' parameter '{}' cannot use witness source classification; witness parameters are action or lock verifier bindings",
                        callable_kind, callable_name, param.name
                    ),
                    param.span,
                ));
            }
        } else if callable_kind != "lock" {
            return Err(CompileError::new(
                format!(
                    "{} '{}' parameter '{}' cannot use {} source classification; protected and lock_args are lock parameter source syntax",
                    callable_kind,
                    callable_name,
                    param.name,
                    param_source_repr(param.source)
                ),
                param.span,
            ));
        }

        match param.source {
            ParamSource::Protected => {
                if param.is_mut || param.is_read_ref || param.is_ref {
                    return Err(CompileError::new(
                        format!(
                            "protected lock parameter '{}' must use 'protected name: T' without mut/ref/read_ref modifiers",
                            param.name
                        ),
                        param.span,
                    ));
                }
                let Type::Ref(inner) = &param.ty else {
                    return Err(CompileError::new(
                        format!("protected lock parameter '{}' must lower to a read-only Cell view", param.name),
                        param.span,
                    ));
                };
                let Some(name) = Self::base_type_name(inner) else {
                    return Err(CompileError::new(
                        format!(
                            "protected lock parameter '{}' must name a Cell-backed resource, shared cell, or receipt type",
                            param.name
                        ),
                        param.span,
                    ));
                };
                if self.resolve_cell_type_kind(name).is_none() {
                    return Err(CompileError::new(
                        format!(
                            "protected lock parameter '{}' references non-Cell type {}; protected only marks the current lock invocation's Cell spend surface",
                            param.name,
                            type_repr(inner)
                        ),
                        param.span,
                    ));
                }
            }
            ParamSource::Witness => {
                if param.is_mut || param.is_read_ref || matches!(param.ty, Type::Ref(_) | Type::MutRef(_)) {
                    return Err(CompileError::new(
                        format!(
                            "witness lock parameter '{}' must be plain transaction witness data, not a Cell reference",
                            param.name
                        ),
                        param.span,
                    ));
                }
            }
            ParamSource::LockArgs => {
                if param.is_mut || param.is_read_ref || matches!(param.ty, Type::Ref(_) | Type::MutRef(_)) {
                    return Err(CompileError::new(
                        format!("lock_args lock parameter '{}' must be plain typed script args, not a Cell reference", param.name),
                        param.span,
                    ));
                }
                if lock_args_static_type_len(&param.ty).is_none() {
                    return Err(CompileError::new(
                        format!(
                            "lock_args parameter '{}' must have a fixed-width type that can be decoded from Script.args",
                            param.name
                        ),
                        param.span,
                    ));
                }
            }
            ParamSource::Default | ParamSource::Input | ParamSource::Output => {}
        }
        Ok(())
    }

    fn validate_callable_param_reference_shape(&self, param: &Param, callable_kind: &str, callable_name: &str) -> Result<()> {
        if matches!(param.ty, Type::MutRef(_)) {
            return Err(CompileError::new(
                format!(
                    "`&mut` Cell parameters are not valid at callable boundaries; use `action(before: T) -> after: T` plus `move` and `require` constraints in {} '{}'",
                    callable_kind, callable_name
                ),
                param.span,
            ));
        }
        if matches!(callable_kind, "action" | "lock")
            && matches!(param.ty, Type::Ref(_))
            && !param.is_read_ref
            && param.source != ParamSource::Protected
        {
            let help = if callable_kind == "lock" {
                "use `protected name: T` for the guarded lock cell or `read name: T` for a read-only referenced cell"
            } else {
                "use `read name: T` for a read-only referenced cell, or a signature input/output binding for consumed/proposed cells"
            };
            return Err(CompileError::new(
                format!(
                    "{} '{}' parameter '{}' cannot use bare `&T` at the verifier boundary; {}",
                    callable_kind, callable_name, param.name, help
                ),
                param.span,
            ));
        }
        let nested_reference = match &param.ty {
            Type::Ref(inner) | Type::MutRef(inner) => self.type_contains_reference(inner),
            ty => self.type_contains_reference(ty),
        };
        if nested_reference {
            return Err(CompileError::new(
                format!(
                    "parameter '{}' in {} '{}' cannot contain nested reference type {}; references are only supported as top-level callable parameter types",
                    param.name,
                    callable_kind,
                    callable_name,
                    type_repr(&param.ty)
                ),
                param.span,
            ));
        }
        if let Type::Ref(inner) | Type::MutRef(inner) = &param.ty {
            if self.reference_target_is_cell_backed_aggregate(inner) {
                return Err(CompileError::new(
                    format!(
                        "parameter '{}' in {} '{}' cannot use reference to aggregate containing cell-backed values {}; use a direct '&T' helper view or named action outputs instead",
                        param.name,
                        callable_kind,
                        callable_name,
                        type_repr(&param.ty)
                    ),
                    param.span,
                ));
            }
        }
        Ok(())
    }

    fn validate_callable_param_state_authority(&self, param: &Param, callable_kind: &str, callable_name: &str) -> Result<()> {
        if callable_kind != "action" && matches!(param.ty, Type::MutRef(_)) {
            return Err(CompileError::new(
                format!(
                    "{} '{}' parameter '{}' cannot use mutable reference type {}; use signature-direction outputs for Cell updates",
                    callable_kind,
                    callable_name,
                    param.name,
                    type_repr(&param.ty)
                ),
                param.span,
            ));
        }
        if callable_kind != "action" && self.type_contains_cell_backed_value(&param.ty) {
            return Err(CompileError::new(
                format!(
                    "{} '{}' parameter '{}' cannot use owned cell-backed type {}; use a read-only '&T' parameter for predicate/helper reads or move ownership transitions into an action",
                    callable_kind,
                    callable_name,
                    param.name,
                    type_repr(&param.ty)
                ),
                param.span,
            ));
        }
        Ok(())
    }

    fn validate_callable_param_mutability(&self, param: &Param) -> Result<()> {
        if !param.is_mut {
            return Ok(());
        }
        if param.is_read_ref || matches!(param.ty, Type::Ref(_)) {
            return Err(CompileError::new(
                format!(
                    "parameter '{}' is a read-only reference; Cell state updates must be modeled with `action(before: T) -> after: T` plus `move` and `require` constraints",
                    param.name
                ),
                param.span,
            ));
        }
        if matches!(param.ty, Type::MutRef(_)) {
            return Err(CompileError::new(
                format!(
                    "parameter '{}' cannot use `&mut` Cell syntax; use `action(before: T) -> after: T` plus `move` and `require` constraints",
                    param.name
                ),
                param.span,
            ));
        }
        if Self::base_type_name(&param.ty).and_then(|name| self.resolve_cell_type_kind(name)).is_some() {
            return Err(CompileError::new(
                format!(
                    "cell-backed parameter '{}' cannot use leading 'mut'; use named action outputs (`action(before: {}) -> after: {}`) or consume/create sugar",
                    param.name,
                    type_repr(&param.ty),
                    type_repr(&param.ty)
                ),
                param.span,
            ));
        }
        Ok(())
    }

    fn check_body_statements<'body>(&mut self, env: &mut TypeEnv, body: &'body [Stmt]) -> Result<Option<(TypeEnv, &'body Stmt)>> {
        let Some((last, prefix)) = body.split_last() else {
            return Ok(None);
        };
        for stmt in prefix {
            self.check_stmt(env, stmt)?;
        }
        let tail_base = env.clone();
        self.check_stmt(env, last)?;
        Ok(Some((tail_base, last)))
    }

    fn check_stmt(&mut self, env: &mut TypeEnv, stmt: &Stmt) -> Result<()> {
        match stmt {
            Stmt::Let(let_stmt) => {
                let ty = self.infer_let_value_type(env, let_stmt)?;
                if let Some(ref declared_ty) = let_stmt.ty {
                    self.validate_type(declared_ty)?;
                    if !self.types_equal(&ty, declared_ty) {
                        return Err(CompileError::new(
                            format!("type mismatch: expected {:?}, found {:?}", declared_ty, ty),
                            let_stmt.span,
                        ));
                    }
                }
                if matches!(ty, Type::Unit) {
                    return Err(CompileError::new("cannot bind the result of a function without a return value", let_stmt.span));
                }
                self.reject_local_reference_to_linear_root(env, &let_stmt.value, &ty, let_stmt.span)?;
                self.reject_local_mutable_reference_alias(&ty, let_stmt.span)?;
                self.mark_expr_as_moved(env, &let_stmt.value)?;
                self.bind_pattern(env, &let_stmt.pattern, &ty, let_stmt.is_mut, let_stmt.span)?;
                Ok(())
            }
            Stmt::Expr(expr) => {
                self.infer_expr(env, expr)?;
                Ok(())
            }
            Stmt::Return(None) => {
                if let Some(Some(expected)) = &self.current_return_type {
                    return Err(CompileError::new(
                        format!("return without value in function returning {:?}", expected),
                        stmt_span(stmt),
                    ));
                }
                Ok(())
            }
            Stmt::Return(Some(expr)) => {
                let ty = self.infer_expr(env, expr)?;
                match &self.current_return_type {
                    Some(Some(expected)) if !self.types_equal(expected, &ty) => {
                        return Err(CompileError::new(
                            format!("return type mismatch: expected {:?}, found {:?}", expected, ty),
                            expr_span(expr),
                        ));
                    }
                    Some(None) => {
                        return Err(CompileError::new(
                            "return value is not allowed in a function without a return type",
                            expr_span(expr),
                        ));
                    }
                    _ => {}
                }
                self.mark_expr_as_moved(env, expr)?;
                Ok(())
            }
            Stmt::If(if_stmt) => {
                let cond_ty = self.infer_expr(env, &if_stmt.condition)?;
                if !self.is_bool_type(&cond_ty) {
                    return Err(CompileError::new("if condition must be boolean", if_stmt.span));
                }
                let mut then_env = env.child();
                for stmt in &if_stmt.then_branch {
                    self.check_stmt(&mut then_env, stmt)?;
                }
                let then_returns = self.stmts_always_return(&if_stmt.then_branch);
                if let Some(ref else_branch) = if_stmt.else_branch {
                    let mut else_env = env.child();
                    for stmt in else_branch {
                        self.check_stmt(&mut else_env, stmt)?;
                    }
                    let else_returns = self.stmts_always_return(else_branch);
                    env.merge_branch_linear_states(&then_env, then_returns, Some(&else_env), else_returns, if_stmt.span)?;
                } else {
                    env.merge_branch_linear_states(&then_env, then_returns, None, false, if_stmt.span)?;
                }
                Ok(())
            }
            Stmt::For(for_stmt) => {
                let iter_ty = self.infer_expr(env, &for_stmt.iterable)?;
                let mut loop_env = env.child();
                let item_ty = self.iter_item_type(&iter_ty, for_stmt.span)?;
                self.bind_pattern(&mut loop_env, &for_stmt.pattern, &item_ty, false, for_stmt.span)?;
                for stmt in &for_stmt.body {
                    self.check_stmt(&mut loop_env, stmt)?;
                }
                loop_env.check_linear_complete()?;
                env.reject_loop_linear_state_changes(&loop_env, for_stmt.span)?;
                env.merge_existing_type_refinements_from(&loop_env);
                Ok(())
            }
            Stmt::While(while_stmt) => {
                let cond_ty = self.infer_expr(env, &while_stmt.condition)?;
                if !self.is_bool_type(&cond_ty) {
                    return Err(CompileError::new("while condition must be boolean", while_stmt.span));
                }
                let mut while_env = env.child();
                for stmt in &while_stmt.body {
                    self.check_stmt(&mut while_env, stmt)?;
                }
                while_env.check_linear_complete()?;
                env.reject_loop_linear_state_changes(&while_env, while_stmt.span)?;
                env.merge_existing_type_refinements_from(&while_env);
                Ok(())
            }
        }
    }

    fn check_no_unreachable_stmts(&self, stmts: &[Stmt]) -> Result<()> {
        let mut previous_guaranteed_return = false;
        for stmt in stmts {
            if previous_guaranteed_return {
                return Err(CompileError::new("unreachable statement after guaranteed return", stmt_span(stmt)));
            }
            self.check_no_unreachable_nested(stmt)?;
            previous_guaranteed_return = self.stmt_always_returns(stmt);
        }
        Ok(())
    }

    fn check_no_unreachable_nested(&self, stmt: &Stmt) -> Result<()> {
        match stmt {
            Stmt::If(if_stmt) => {
                self.check_no_unreachable_stmts(&if_stmt.then_branch)?;
                if let Some(else_branch) = &if_stmt.else_branch {
                    self.check_no_unreachable_stmts(else_branch)?;
                }
            }
            Stmt::For(for_stmt) => self.check_no_unreachable_stmts(&for_stmt.body)?,
            Stmt::While(while_stmt) => self.check_no_unreachable_stmts(&while_stmt.body)?,
            Stmt::Expr(Expr::Block(stmts)) => self.check_no_unreachable_stmts(stmts)?,
            _ => {}
        }
        Ok(())
    }

    fn infer_let_value_type(&mut self, env: &mut TypeEnv, let_stmt: &LetStmt) -> Result<Type> {
        if let Some(declared_ty) = &let_stmt.ty {
            if matches!(let_stmt.value, Expr::Array(_)) {
                return self.infer_expr_with_expected_type(env, &let_stmt.value, declared_ty, let_stmt.span);
            }
        }
        if let Expr::Array(elems) = &let_stmt.value {
            if elems.is_empty() {
                return match &let_stmt.ty {
                    Some(declared @ Type::Array(_, 0)) => Ok(declared.clone()),
                    Some(Type::Array(_, size)) => Err(CompileError::new(
                        format!("empty array literal cannot initialize non-empty array of length {}", size),
                        let_stmt.span,
                    )),
                    Some(_) => Err(CompileError::new("empty array literal requires an array type annotation", let_stmt.span)),
                    None => Err(CompileError::new("empty array literal requires an explicit array type annotation", let_stmt.span)),
                };
            }
        }
        self.infer_expr(env, &let_stmt.value)
    }

    fn infer_expr_with_expected_type(&mut self, env: &mut TypeEnv, expr: &Expr, expected_ty: &Type, span: Span) -> Result<Type> {
        match expr {
            Expr::Array(elems) => self.infer_array_literal_with_expected_type(env, elems, expected_ty, span),
            _ => self.infer_expr(env, expr),
        }
    }

    fn infer_array_literal_with_expected_type(
        &mut self,
        env: &mut TypeEnv,
        elems: &[Expr],
        expected_ty: &Type,
        span: Span,
    ) -> Result<Type> {
        if let Type::Named(name) = expected_ty {
            if let Some(item_ty) = self.parse_named_collection_item_type(name) {
                for elem in elems {
                    let actual_ty = self.infer_expr(env, elem)?;
                    if self.type_contains_reference(&actual_ty) {
                        return Err(CompileError::new(
                            format!(
                                "Vec literal cannot store reference type {}; Vec<T> values must use owned non-reference items",
                                type_repr(&actual_ty)
                            ),
                            expr_span(elem),
                        ));
                    }
                    if !self.types_equal(&actual_ty, &item_ty) {
                        return Err(CompileError::new(
                            format!("Vec literal type mismatch: expected {:?}, found {:?}", item_ty, actual_ty),
                            expr_span(elem),
                        ));
                    }
                }
                return Ok(expected_ty.clone());
            }
        }

        if let Type::Array(item_ty, expected_len) = expected_ty {
            if elems.len() != *expected_len {
                if elems.is_empty() {
                    return Err(CompileError::new(
                        format!("empty array literal cannot initialize non-empty array of length {}", expected_len),
                        span,
                    ));
                }
                return Err(CompileError::new(
                    format!("array literal length mismatch: expected {}, found {}", expected_len, elems.len()),
                    span,
                ));
            }
            for elem in elems {
                let actual_ty = self.infer_expr(env, elem)?;
                if !self.types_equal(&actual_ty, item_ty) {
                    return Err(CompileError::new(
                        format!("array literal type mismatch: expected {:?}, found {:?}", item_ty, actual_ty),
                        expr_span(elem),
                    ));
                }
            }
            return Ok(expected_ty.clone());
        }

        self.infer_expr(env, &Expr::Array(elems.to_vec()))
    }

    fn infer_expr(&mut self, env: &mut TypeEnv, expr: &Expr) -> Result<Type> {
        self.validate_expr_allowed_in_current_callable(expr)?;
        match expr {
            Expr::Integer(_) => Ok(Type::U64),
            Expr::Bool(_) => Ok(Type::Bool),
            Expr::String(_) => Ok(Type::Named("String".to_string())),
            Expr::ByteString(_) => Ok(Type::Array(Box::new(Type::U8), 0)),
            Expr::Identifier(name) => {
                if let Some(ty) = env.lookup(name).cloned() {
                    Ok(ty)
                } else if let Some(constant) = self.resolve_constant(name) {
                    Ok(constant.ty)
                } else if let Some(ty) = self.enum_variant_expr_type(name, expr_span(expr))? {
                    Ok(ty)
                } else if let Some(ty) = self.flow_state_expr_type(name, expr_span(expr))? {
                    Ok(ty)
                } else if let Some((prefix, _)) = name.split_once("::") {
                    Ok(Type::Named(prefix.to_string()))
                } else {
                    Err(CompileError::new(format!("undefined variable '{}'", name), Span::default()))
                }
            }
            Expr::Assign(assign) => self.infer_assign_expr(env, assign),
            Expr::Binary(bin) => {
                let left_ty = self.infer_expr(env, &bin.left)?;
                let right_ty = self.infer_expr(env, &bin.right)?;

                match bin.op {
                    BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                        if !self.is_numeric_type(&left_ty) || !self.is_numeric_type(&right_ty) {
                            return Err(CompileError::new("arithmetic operations require numeric types", bin.span));
                        }
                        Ok(left_ty)
                    }
                    BinaryOp::Eq | BinaryOp::Ne => {
                        if !self.types_equal(&left_ty, &right_ty) {
                            return Err(CompileError::new("comparison requires matching types", bin.span));
                        }
                        Ok(Type::Bool)
                    }
                    BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                        if !self.is_numeric_type(&left_ty) || !self.is_numeric_type(&right_ty) {
                            return Err(CompileError::new("ordering comparison requires numeric types", bin.span));
                        }
                        Ok(Type::Bool)
                    }
                    BinaryOp::And | BinaryOp::Or => {
                        if !self.is_bool_type(&left_ty) || !self.is_bool_type(&right_ty) {
                            return Err(CompileError::new("logical operations require boolean types", bin.span));
                        }
                        Ok(Type::Bool)
                    }
                }
            }
            Expr::Unary(unary) => {
                let expr_ty = self.infer_expr(env, &unary.expr)?;
                match unary.op {
                    UnaryOp::Neg => {
                        if !self.is_numeric_type(&expr_ty) {
                            return Err(CompileError::new("negation requires numeric type", unary.span));
                        }
                        Ok(expr_ty)
                    }
                    UnaryOp::Not => {
                        if !self.is_bool_type(&expr_ty) {
                            return Err(CompileError::new("logical not requires boolean type", unary.span));
                        }
                        Ok(Type::Bool)
                    }
                    UnaryOp::Ref => Ok(Type::Ref(Box::new(expr_ty))),
                    UnaryOp::Deref => match expr_ty {
                        Type::Ref(inner) | Type::MutRef(inner) => Ok((*inner).clone()),
                        _ => Err(CompileError::new("cannot dereference a non-reference value", unary.span)),
                    },
                }
            }
            Expr::Call(call) => {
                self.reject_forbidden_consensus_call(call)?;
                self.validate_runtime_call_allowed_in_current_callable(call)?;
                let mut arg_types = Vec::with_capacity(call.args.len());
                for arg in &call.args {
                    arg_types.push(self.infer_expr(env, arg)?);
                }
                let return_type = self.infer_call_type(env, call, &arg_types)?;
                for arg in &call.args {
                    self.mark_expr_as_moved(env, arg)?;
                }
                Ok(return_type)
            }
            Expr::FieldAccess(field) => {
                let expr_ty = self.infer_expr(env, &field.expr)?;
                let field_ty = self.lookup_field_type(&expr_ty, &field.field, field.span)?;
                if self.is_linear_type(&field_ty) {
                    return Err(CompileError::new(
                        "field access cannot move a linear value out of an aggregate; use destructuring to bind linear fields",
                        field.span,
                    ));
                }
                Ok(field_ty)
            }
            Expr::Index(index) => {
                let expr_ty = self.infer_expr(env, &index.expr)?;
                let index_ty = self.infer_expr(env, &index.index)?;
                if !self.is_numeric_type(&index_ty) {
                    return Err(CompileError::new("index expression requires a numeric index", index.span));
                }
                let item_ty = self.index_result_type(&expr_ty, index.span)?;
                if self.is_linear_type(&item_ty) {
                    return Err(CompileError::new(
                        "index access cannot move a linear value out of an aggregate; use destructuring or explicit iteration that handles each item",
                        index.span,
                    ));
                }
                Ok(item_ty)
            }
            Expr::Create(create) => {
                self.require_create_target_cell_backed(&create.ty, create.span)?;
                self.check_field_initializer(env, &create.ty, &create.fields, create.span, "create")?;
                if let Some(target) = &create.target {
                    let Some(target_ty) = env.lookup(target).cloned() else {
                        return Err(CompileError::new(
                            format!("create target '{}' is not declared as an action output binding", target),
                            create.span,
                        ));
                    };
                    let Type::Named(target_type_name) = target_ty else {
                        return Err(CompileError::new(
                            format!("create target '{}' must be a named Cell output binding", target),
                            create.span,
                        ));
                    };
                    if target_type_name.split('<').next().unwrap_or(target_type_name.as_str()) != create.ty {
                        return Err(CompileError::new(
                            format!(
                                "create target '{}' has type '{}', but initializer constructs '{}'",
                                target, target_type_name, create.ty
                            ),
                            create.span,
                        ));
                    }
                }
                Ok(Type::Named(create.ty.clone()))
            }
            Expr::Consume(consume) => {
                let (_consume_ty, name) = self.require_named_linear_cell_operand(env, &consume.expr, "consume", consume.span)?;
                env.consume(&name)?;
                Ok(Type::U64)
            }
            Expr::Destroy(destroy) => {
                let (destroy_ty, name) = self.require_named_linear_cell_operand(env, &destroy.expr, "destroy", destroy.span)?;
                self.require_capability(&destroy_ty, Capability::Destroy, "destroy", destroy.span)?;
                env.destroy(&name)?;
                Ok(Type::U64)
            }
            Expr::ReadRef(read_ref) => {
                self.require_read_ref_target_cell_backed(&read_ref.ty, read_ref.span)?;
                Ok(Type::Ref(Box::new(Type::Named(read_ref.ty.clone()))))
            }
            Expr::Assert(assert_expr) => {
                let cond_ty = self.infer_expr(env, &assert_expr.condition)?;
                if !self.is_bool_type(&cond_ty) {
                    return Err(CompileError::new("assert condition must be boolean", assert_expr.span));
                }
                if !matches!(assert_expr.message.as_ref(), Expr::String(_)) {
                    return Err(CompileError::new("assert message must be a string literal", expr_span(&assert_expr.message)));
                }
                Ok(Type::Unit)
            }
            Expr::Require(require_expr) => {
                Self::validate_require_condition_is_pure(&require_expr.condition, "require condition")?;
                let cond_ty = self.infer_expr(env, &require_expr.condition)?;
                if !self.is_bool_type(&cond_ty) {
                    return Err(CompileError::new("require condition must be boolean", require_expr.span));
                }
                if let Some(message) = &require_expr.message {
                    if !matches!(message.as_ref(), Expr::String(_)) {
                        return Err(CompileError::new("require message must be a string literal", expr_span(message)));
                    }
                }
                Ok(Type::Bool)
            }
            Expr::Block(stmts) => {
                let mut block_env = env.child();
                let last_ty = self.infer_tail_block_value(&mut block_env, stmts)?;
                block_env.check_linear_complete()?;
                env.merge_existing_linear_states_from(&block_env);
                Ok(last_ty)
            }
            Expr::Tuple(elems) => {
                let mut types = Vec::new();
                for elem in elems {
                    types.push(self.infer_expr(env, elem)?);
                }
                Ok(Type::Tuple(types))
            }
            Expr::Array(elems) => {
                if elems.is_empty() {
                    return Err(CompileError::new("empty array literal requires an explicit array type annotation", expr_span(expr)));
                }
                let elem_ty = self.infer_expr(env, &elems[0])?;
                for elem in elems.iter().skip(1) {
                    let next_ty = self.infer_expr(env, elem)?;
                    if !self.types_equal(&elem_ty, &next_ty) {
                        return Err(CompileError::new("array elements must have matching types", expr_span(elem)));
                    }
                }
                Ok(Type::Array(Box::new(elem_ty), elems.len()))
            }
            Expr::If(if_expr) => {
                let cond_ty = self.infer_expr(env, &if_expr.condition)?;
                if !self.is_bool_type(&cond_ty) {
                    return Err(CompileError::new("if expression condition must be boolean", if_expr.span));
                }
                let mut then_env = env.child();
                let then_ty = self.infer_expr(&mut then_env, &if_expr.then_branch)?;
                let mut else_env = env.child();
                let else_ty = self.infer_expr(&mut else_env, &if_expr.else_branch)?;
                if self.types_equal(&then_ty, &else_ty) {
                    env.merge_branch_linear_states(&then_env, false, Some(&else_env), false, if_expr.span)?;
                    Ok(then_ty)
                } else {
                    Err(CompileError::new(
                        format!("if expression branches must have matching types, got {:?} and {:?}", then_ty, else_ty),
                        if_expr.span,
                    ))
                }
            }
            Expr::Cast(cast) => {
                self.infer_expr(env, &cast.expr)?;
                Ok(cast.ty.clone())
            }
            Expr::Range(range) => {
                self.infer_expr(env, &range.start)?;
                self.infer_expr(env, &range.end)?;
                Ok(Type::Named("Range".to_string()))
            }
            Expr::StructInit(init) => {
                self.check_field_initializer(env, &init.ty, &init.fields, init.span, "struct literal")?;
                Ok(Type::Named(init.ty.clone()))
            }
            Expr::Match(match_expr) => {
                let scrutinee_ty = self.infer_expr(env, &match_expr.expr)?;
                self.check_match_patterns(&scrutinee_ty, match_expr)?;
                let mut arm_ty = None;
                let mut arm_envs = Vec::with_capacity(match_expr.arms.len());
                for arm in &match_expr.arms {
                    let mut arm_env = env.child();
                    let ty = self.infer_expr(&mut arm_env, &arm.value)?;
                    if arm_ty.as_ref().is_none_or(|existing| self.types_equal(existing, &ty)) {
                        arm_ty = Some(ty);
                    } else {
                        return Err(CompileError::new("match arms must have matching types", arm.span));
                    }
                    arm_envs.push(arm_env);
                }
                env.merge_match_linear_states(&arm_envs, match_expr.span)?;
                arm_ty.ok_or_else(|| CompileError::new("match expression must contain at least one arm", match_expr.span))
            }
            Expr::RequireBlock(require_block) => {
                for expr in &require_block.expressions {
                    Self::validate_require_condition_is_pure(expr, "require block")?;
                    let ty = self.infer_expr(env, expr)?;
                    if !self.is_bool_type(&ty) {
                        return Err(CompileError::new("require block expressions must be boolean", expr_span(expr)).with_code("E1004"));
                    }
                }
                Ok(Type::Unit)
            }
            Expr::Preserve(preserve) => {
                let output_ty = env.lookup(&preserve.output_name).cloned().ok_or_else(|| {
                    CompileError::new(format!("preserve: undefined output binding '{}'", preserve.output_name), preserve.span)
                })?;
                let input_ty = env.lookup(&preserve.input_name).cloned().ok_or_else(|| {
                    CompileError::new(format!("preserve: undefined input binding '{}'", preserve.input_name), preserve.span)
                })?;
                if preserve.fields.is_empty() {
                    return Err(CompileError::new("preserve block must list at least one field", preserve.span));
                }
                for field in &preserve.fields {
                    let output_field_ty = self.lookup_field_type(&output_ty, field, preserve.span).map_err(|_| {
                        CompileError::new(format!("field '{}' does not exist on output type '{:?}'", field, output_ty), preserve.span)
                            .with_code("E1002")
                    })?;
                    let input_field_ty = self.lookup_field_type(&input_ty, field, preserve.span).map_err(|_| {
                        CompileError::new(format!("field '{}' does not exist on input type '{:?}'", field, input_ty), preserve.span)
                            .with_code("E1003")
                    })?;
                    if !self.types_equal(&output_field_ty, &input_field_ty) {
                        return Err(CompileError::new(
                            format!(
                                "preserve field '{}' type mismatch: output has {}, input has {}",
                                field,
                                type_repr(&output_field_ty),
                                type_repr(&input_field_ty)
                            ),
                            preserve.span,
                        )
                        .with_code("E1004"));
                    }
                }
                Ok(Type::Bool)
            }
            Expr::StdlibCall(call) => self.infer_stdlib_call(env, call),
        }
    }

    fn validate_require_condition_is_pure(expr: &Expr, context: &str) -> Result<()> {
        match expr {
            Expr::Create(_) | Expr::Consume(_) | Expr::Destroy(_) | Expr::ReadRef(_) => {
                return Err(CompileError::new(
                    format!(
                        "{} contains cell/runtime operation; move state transition logic into a separate action statement",
                        context
                    ),
                    expr_span(expr),
                )
                .with_code("E1005"));
            }
            Expr::Require(_) | Expr::RequireBlock(_) | Expr::Preserve(_) | Expr::StdlibCall(_) => {
                return Err(CompileError::new(
                    format!("{} contains verifier-boundary syntax; require expressions must be pure boolean constraints", context),
                    expr_span(expr),
                )
                .with_code("E1005"));
            }
            Expr::If(_) | Expr::Match(_) | Expr::Block(_) => {
                return Err(CompileError::new(
                    format!("{} contains control flow; require expressions must be pure boolean constraints", context),
                    expr_span(expr),
                )
                .with_code("E1006"));
            }
            Expr::Assign(_) => {
                return Err(CompileError::new(
                    format!("{} contains assignment; require expressions must be pure boolean constraints", context),
                    expr_span(expr),
                )
                .with_code("E1005"));
            }
            Expr::Binary(binary) => {
                Self::validate_require_condition_is_pure(&binary.left, context)?;
                Self::validate_require_condition_is_pure(&binary.right, context)?;
            }
            Expr::Unary(unary) => Self::validate_require_condition_is_pure(&unary.expr, context)?,
            Expr::Call(call) => {
                Self::validate_require_condition_is_pure(&call.func, context)?;
                for arg in &call.args {
                    Self::validate_require_condition_is_pure(arg, context)?;
                }
            }
            Expr::FieldAccess(field) => Self::validate_require_condition_is_pure(&field.expr, context)?,
            Expr::Index(index) => {
                Self::validate_require_condition_is_pure(&index.expr, context)?;
                Self::validate_require_condition_is_pure(&index.index, context)?;
            }
            Expr::Assert(assert_expr) => {
                Self::validate_require_condition_is_pure(&assert_expr.condition, context)?;
                Self::validate_require_condition_is_pure(&assert_expr.message, context)?;
            }
            Expr::Tuple(items) | Expr::Array(items) => {
                for item in items {
                    Self::validate_require_condition_is_pure(item, context)?;
                }
            }
            Expr::Cast(cast) => Self::validate_require_condition_is_pure(&cast.expr, context)?,
            Expr::Range(range) => {
                Self::validate_require_condition_is_pure(&range.start, context)?;
                Self::validate_require_condition_is_pure(&range.end, context)?;
            }
            Expr::StructInit(init) => {
                for (_, value) in &init.fields {
                    Self::validate_require_condition_is_pure(value, context)?;
                }
            }
            Expr::Integer(_) | Expr::Bool(_) | Expr::String(_) | Expr::ByteString(_) | Expr::Identifier(_) => {}
        }
        Ok(())
    }

    fn infer_stdlib_call(&mut self, env: &mut TypeEnv, call: &StdlibCallExpr) -> Result<Type> {
        let qualified = format!("std::{}::{}", call.namespace, call.name);
        match qualified.as_str() {
            "std::cell::same_lock" | "std::cell::preserve_lock" | "std::cell::preserve_capacity" => {
                self.validate_stdlib_arity(&qualified, call, 2)?;
                self.require_named_cell_identifier(env, &call.args[0], &qualified, "output")?;
                self.require_named_cell_identifier(env, &call.args[1], &qualified, "input")?;
                Ok(Type::Bool)
            }
            "std::cell::same_type" | "std::cell::preserve_type" | "std::accounting::conserved" => {
                self.validate_stdlib_arity(&qualified, call, 2)?;
                let left_ty = self.infer_expr(env, &call.args[0])?;
                let right_ty = self.infer_expr(env, &call.args[1])?;
                if qualified == "std::accounting::conserved" {
                    let left_amount = self.lookup_field_type(&left_ty, "amount", call.span)?;
                    let right_amount = self.lookup_field_type(&right_ty, "amount", call.span)?;
                    if !self.types_equal(&left_amount, &right_amount) {
                        return Err(CompileError::new("std::accounting::conserved requires matching amount field types", call.span));
                    }
                }
                Ok(Type::Bool)
            }
            "std::lifecycle::transfer" => {
                self.validate_stdlib_arity(&qualified, call, 3)?;
                let (input_ty, input_name) = self.require_named_linear_cell_operand(env, &call.args[0], &qualified, call.span)?;
                let output_ty = self.require_named_cell_identifier(env, &call.args[1], &qualified, "output")?;
                let lock_ty = self.infer_expr(env, &call.args[2])?;
                if !matches!(lock_ty, Type::Address | Type::Hash) {
                    return Err(CompileError::new("std::lifecycle::transfer lock target must be Address or Hash", call.span));
                }
                if !self.types_equal(&input_ty, &output_ty) {
                    return Err(CompileError::new(
                        "std::lifecycle::transfer input and output must have the same Cell type",
                        call.span,
                    ));
                }
                self.validate_preserve_field_types(&output_ty, &input_ty, &call.preserve_fields, call.span)?;
                self.validate_stdlib_output_field_coverage(&qualified, &output_ty, &call.preserve_fields, call.span)?;
                env.consume(&input_name)?;
                Ok(Type::Bool)
            }
            "std::receipt::claim" => {
                self.validate_stdlib_arity(&qualified, call, 3)?;
                let (input_ty, input_name) = self.require_named_linear_cell_operand(env, &call.args[0], &qualified, call.span)?;
                let output_ty = self.require_named_cell_identifier(env, &call.args[1], &qualified, "output")?;
                self.validate_stdlib_lock_arg(&qualified, &call.args[2], env)?;
                self.validate_preserve_field_types(&output_ty, &input_ty, &call.preserve_fields, call.span)?;
                self.validate_stdlib_output_field_coverage(&qualified, &output_ty, &call.preserve_fields, call.span)?;
                self.validate_receipt_claim_pattern(&input_ty, &output_ty, call.span)?;
                env.consume(&input_name)?;
                Ok(Type::Bool)
            }
            "std::lifecycle::settle" => {
                self.validate_stdlib_arity(&qualified, call, 3)?;
                let (input_ty, input_name) = self.require_named_linear_cell_operand(env, &call.args[0], &qualified, call.span)?;
                let output_ty = self.require_named_cell_identifier(env, &call.args[1], &qualified, "output")?;
                self.validate_stdlib_lock_arg(&qualified, &call.args[2], env)?;
                self.validate_preserve_field_types(&output_ty, &input_ty, &call.preserve_fields, call.span)?;
                self.validate_stdlib_output_field_coverage(&qualified, &output_ty, &call.preserve_fields, call.span)?;
                env.consume(&input_name)?;
                Ok(Type::Bool)
            }
            _ => Err(CompileError::new(
                format!("unknown stdlib pattern '{}' — each stdlib primitive must have a canonical expansion", qualified),
                call.span,
            )),
        }
    }

    fn validate_stdlib_arity(&self, qualified: &str, call: &StdlibCallExpr, expected: usize) -> Result<()> {
        if call.args.len() == expected {
            Ok(())
        } else {
            Err(CompileError::new(format!("{} expects {} arguments, got {}", qualified, expected, call.args.len()), call.span))
        }
    }

    fn require_named_cell_identifier(&mut self, env: &mut TypeEnv, expr: &Expr, operation: &str, role: &str) -> Result<Type> {
        let ty = self.infer_expr(env, expr)?;
        if !self.is_linear_type(&ty) {
            return Err(CompileError::new(format!("{} {} must be a cell-backed value", operation, role), expr_span(expr)));
        }
        if !matches!(expr, Expr::Identifier(_)) {
            return Err(CompileError::new(format!("{} {} must be a named cell-backed binding", operation, role), expr_span(expr)));
        }
        Ok(ty)
    }

    fn validate_preserve_field_types(&self, output_ty: &Type, input_ty: &Type, fields: &[String], span: Span) -> Result<()> {
        for field in fields {
            let output_field_ty = self.lookup_field_type(output_ty, field, span)?;
            let input_field_ty = self.lookup_field_type(input_ty, field, span)?;
            if !self.types_equal(&output_field_ty, &input_field_ty) {
                return Err(CompileError::new(
                    format!(
                        "preserve field '{}' type mismatch: output has {}, input has {}",
                        field,
                        type_repr(&output_field_ty),
                        type_repr(&input_field_ty)
                    ),
                    span,
                ));
            }
        }
        Ok(())
    }

    fn validate_stdlib_output_field_coverage(&self, qualified: &str, output_ty: &Type, fields: &[String], span: Span) -> Result<()> {
        let Some(output_type_name) = Self::base_type_name(output_ty) else {
            return Err(CompileError::new(format!("{} output must be a named cell-backed type", qualified), span));
        };
        let Some(output_fields) = self.resolve_named_type_fields(output_type_name) else {
            return Err(CompileError::new(format!("{} output type '{}' has no known fields", qualified, output_type_name), span));
        };
        let preserved = fields.iter().map(String::as_str).collect::<HashSet<_>>();
        let missing = output_fields.keys().filter(|field| !preserved.contains(field.as_str())).cloned().collect::<Vec<_>>();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(CompileError::new(
                format!(
                    "{} output construction must cover every '{}' field; missing {}",
                    qualified,
                    output_type_name,
                    missing.join(", ")
                ),
                span,
            ))
        }
    }

    fn validate_stdlib_lock_arg(&mut self, qualified: &str, lock: &Expr, env: &mut TypeEnv) -> Result<()> {
        let lock_ty = self.infer_expr(env, lock)?;
        if matches!(lock_ty, Type::Address | Type::Hash) {
            Ok(())
        } else {
            Err(CompileError::new(format!("{} lock target must be Address or Hash", qualified), expr_span(lock)))
        }
    }

    fn validate_receipt_claim_pattern(&self, receipt_ty: &Type, output_ty: &Type, span: Span) -> Result<()> {
        let Some(receipt_name) = Self::base_type_name(receipt_ty) else {
            return Err(CompileError::new("std::receipt::claim requires a receipt cell input", span));
        };
        if self.resolve_cell_type_kind(receipt_name) != Some(CellTypeKind::Receipt) {
            return Err(CompileError::new("std::receipt::claim requires a receipt cell input", span));
        }
        let Some(Some(declared_output)) = self.resolve_receipt_claim_output(receipt_name) else {
            return Err(CompileError::new(
                format!("std::receipt::claim with an output requires receipt '{}' to declare a claim output type", receipt_name),
                span,
            ));
        };
        if self.types_equal(output_ty, &declared_output) {
            Ok(())
        } else {
            Err(CompileError::new(
                format!(
                    "std::receipt::claim output type mismatch: receipt '{}' declares {}, got {}",
                    receipt_name,
                    type_repr(&declared_output),
                    type_repr(output_ty)
                ),
                span,
            ))
        }
    }

    fn infer_tail_block_value(&mut self, env: &mut TypeEnv, stmts: &[Stmt]) -> Result<Type> {
        let Some((last, prefix)) = stmts.split_last() else {
            return Ok(Type::Unit);
        };
        for stmt in prefix {
            self.check_stmt(env, stmt)?;
        }
        match last {
            Stmt::Expr(expr) => {
                let ty = self.infer_expr(env, expr)?;
                if self.is_linear_type(&ty) {
                    self.mark_expr_as_moved(env, expr)?;
                }
                Ok(ty)
            }
            Stmt::If(if_stmt) if if_stmt.else_branch.is_some() => self.infer_tail_if_stmt_value(env, if_stmt),
            stmt => {
                self.check_stmt(env, stmt)?;
                Ok(Type::Unit)
            }
        }
    }

    fn infer_tail_if_stmt_value(&mut self, env: &mut TypeEnv, if_stmt: &IfStmt) -> Result<Type> {
        let cond_ty = self.infer_expr(env, &if_stmt.condition)?;
        if !self.is_bool_type(&cond_ty) {
            return Err(CompileError::new("if condition must be boolean", if_stmt.span));
        }
        let Some(else_branch) = &if_stmt.else_branch else {
            return Ok(Type::Unit);
        };

        let mut then_env = env.child();
        let then_ty = self.infer_tail_block_value(&mut then_env, &if_stmt.then_branch)?;
        then_env.check_linear_complete()?;

        let mut else_env = env.child();
        let else_ty = self.infer_tail_block_value(&mut else_env, else_branch)?;
        else_env.check_linear_complete()?;

        if !self.types_equal(&then_ty, &else_ty) {
            return Err(CompileError::new(
                format!("if expression branches must have matching types, got {:?} and {:?}", then_ty, else_ty),
                if_stmt.span,
            ));
        }

        env.merge_branch_linear_states(&then_env, false, Some(&else_env), false, if_stmt.span)?;
        Ok(then_ty)
    }

    fn check_match_patterns(&self, scrutinee_ty: &Type, match_expr: &MatchExpr) -> Result<()> {
        let Type::Named(enum_name) = scrutinee_ty else {
            return Ok(());
        };
        let Some(variants) = self.resolve_enum_variants(enum_name) else {
            return Ok(());
        };
        let variant_set = variants.iter().map(String::as_str).collect::<HashSet<_>>();
        let mut seen = HashSet::new();
        let mut has_wildcard = false;

        for arm in &match_expr.arms {
            if arm.pattern == "_" {
                has_wildcard = true;
                continue;
            }
            let Some(variant) = match_pattern_variant(enum_name, &arm.pattern) else {
                return Err(CompileError::new(
                    format!("match pattern '{}' does not match enum '{}'", arm.pattern, enum_name),
                    arm.span,
                ));
            };
            if !variant_set.contains(variant) {
                return Err(CompileError::new(
                    format!("unknown enum variant '{}::{}' in match pattern", enum_name, variant),
                    arm.span,
                ));
            }
            if self.enum_payload_variants.get(enum_name).is_some_and(|payloads| payloads.contains(variant)) {
                return Err(CompileError::new(
                    format!(
                        "match pattern '{}::{}' targets a payload enum variant; payload destructuring lowering is not implemented",
                        enum_name, variant
                    ),
                    arm.span,
                ));
            }
            if !seen.insert(variant.to_string()) {
                return Err(CompileError::new(format!("duplicate match arm for enum variant '{}::{}'", enum_name, variant), arm.span));
            }
        }

        if !has_wildcard && seen.len() != variants.len() {
            let missing = variants.iter().filter(|variant| !seen.contains(*variant)).cloned().collect::<Vec<_>>().join(", ");
            return Err(CompileError::new(
                format!("non-exhaustive match for enum '{}'; missing {}", enum_name, missing),
                match_expr.span,
            ));
        }

        Ok(())
    }

    fn resolve_enum_variants(&self, enum_name: &str) -> Option<Vec<String>> {
        if let Some(variants) = self.enum_variants.get(enum_name) {
            return Some(variants.clone());
        }
        self.resolver
            .zip(self.current_module.as_deref())
            .and_then(|(resolver, module)| resolver.resolve_type(module, enum_name))
            .and_then(|ty| match ty {
                TypeDef::Enum(enum_def) => Some(enum_def.variants.into_iter().map(|variant| variant.name).collect()),
                _ => None,
            })
    }

    fn check_field_initializer(
        &mut self,
        env: &mut TypeEnv,
        type_name: &str,
        fields: &[(String, Expr)],
        span: Span,
        context: &str,
    ) -> Result<()> {
        let Some(expected_fields) = self.resolve_named_type_fields(type_name) else {
            return Err(CompileError::new(format!("{} target type '{}' has no declared fields", context, type_name), span));
        };

        let mut seen = HashSet::new();
        for (field_name, value) in fields {
            if !seen.insert(field_name.clone()) {
                return Err(CompileError::new(format!("duplicate field '{}' in {} for '{}'", field_name, context, type_name), span));
            }
            let Some(expected_ty) = expected_fields.get(field_name) else {
                return Err(CompileError::new(format!("unknown field '{}' in {} for '{}'", field_name, context, type_name), span));
            };
            let actual_ty = if let Some(flow_ty) = self.flow_state_initializer_type(type_name, field_name, value, expected_ty)? {
                flow_ty
            } else {
                self.infer_expr_with_expected_type(env, value, expected_ty, expr_span(value))?
            };
            if !self.initializer_types_equal(&actual_ty, expected_ty) {
                return Err(CompileError::new(
                    format!(
                        "field '{}' in {} for '{}' has type mismatch: expected {:?}, found {:?}",
                        field_name, context, type_name, expected_ty, actual_ty
                    ),
                    expr_span(value),
                ));
            }
        }

        let missing = expected_fields
            .keys()
            .filter(|field_name| !seen.contains(*field_name))
            .filter(|field_name| {
                !(self.resolve_flow_states(type_name).is_some()
                    && !self.flows.contains_key(type_name)
                    && self.flow_state_fields.get(type_name).is_some_and(|state_field| state_field == field_name.as_str()))
            })
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(CompileError::new(
                format!("{} for '{}' is missing field(s): {}", context, type_name, missing.join(", ")),
                span,
            ));
        }

        Ok(())
    }

    fn flow_state_initializer_type(
        &self,
        type_name: &str,
        field_name: &str,
        value: &Expr,
        expected_ty: &Type,
    ) -> Result<Option<Type>> {
        if self.flow_state_fields.get(type_name).is_none_or(|state_field| state_field != field_name) {
            return Ok(None);
        }
        let Expr::Identifier(state_name) = value else {
            return Ok(None);
        };
        if let Type::Named(enum_name) = expected_ty {
            if self.enum_variant_expr_type(state_name, expr_span(value))?.is_some_and(|ty| self.types_equal(&ty, expected_ty)) {
                return Ok(Some(expected_ty.clone()));
            }
            let Some(states) = self.resolve_flow_states(type_name) else {
                return Ok(None);
            };
            if self.canonical_state_name_for_flow(type_name, Some(enum_name), &states, state_name, expr_span(value)).is_ok() {
                return Ok(Some(expected_ty.clone()));
            }
        } else if !is_state_storage_type(expected_ty) {
            return Ok(None);
        }
        let Some((qualified_type, qualified_state)) = state_name.rsplit_once("::") else {
            return Ok(self.flow_state_index(type_name, state_name).map(|_| expected_ty.clone()));
        };
        if qualified_type == type_name {
            if self.flow_state_index(type_name, state_name).is_some() {
                return Ok(Some(expected_ty.clone()));
            }
            return Err(CompileError::new(format!("unknown flow state '{}::{}'", qualified_type, qualified_state), expr_span(value)));
        }
        if self.resolve_flow_states(qualified_type).is_some() {
            return Err(CompileError::new(
                format!(
                    "flow field '{}.{}' cannot be initialized with '{}::{}'",
                    type_name, field_name, qualified_type, qualified_state
                ),
                expr_span(value),
            ));
        }
        Ok(None)
    }

    fn flow_state_expr_type(&self, name: &str, span: Span) -> Result<Option<Type>> {
        let Some((type_name, state_name)) = name.rsplit_once("::") else {
            return Ok(None);
        };
        let Some(states) = self.resolve_flow_states(type_name) else {
            return Ok(None);
        };
        if states.iter().any(|state| state == state_name) {
            if let Some(spec) = self.flows.get(type_name).and_then(|spec| spec.field_enum_type.as_ref()) {
                return Ok(Some(Type::Named(spec.clone())));
            }
            return Ok(Some(Type::U64));
        }
        Err(CompileError::new(format!("unknown flow state '{}::{}'", type_name, state_name), span))
    }

    fn flow_state_index(&self, type_name: &str, name: &str) -> Option<usize> {
        let states = self.resolve_flow_states(type_name)?;
        if let Some((qualified_type, state_name)) = name.rsplit_once("::") {
            if qualified_type != type_name {
                return None;
            }
            states.iter().position(|state| state == state_name)
        } else {
            states.iter().position(|state| state == name)
        }
    }

    fn require_create_target_cell_backed(&self, type_name: &str, span: Span) -> Result<()> {
        match self.resolve_cell_type_kind(type_name) {
            Some(CellTypeKind::Resource | CellTypeKind::Shared | CellTypeKind::Receipt) => Ok(()),
            None => Err(CompileError::new(
                format!("create target type '{}' must be a resource, shared, or receipt cell type", type_name),
                span,
            )),
        }
    }

    fn require_read_ref_target_cell_backed(&self, type_name: &str, span: Span) -> Result<()> {
        match self.resolve_cell_type_kind(type_name) {
            Some(CellTypeKind::Resource | CellTypeKind::Shared | CellTypeKind::Receipt) => Ok(()),
            None => Err(CompileError::new(
                format!("read_ref target type '{}' must be a resource, shared, or receipt cell type", type_name),
                span,
            )),
        }
    }

    fn enum_variant_expr_type(&self, name: &str, span: Span) -> Result<Option<Type>> {
        let Some((enum_name, variant)) = name.rsplit_once("::") else {
            return Ok(None);
        };
        let Some(variants) = self.resolve_enum_variants(enum_name) else {
            return Ok(None);
        };
        if !variants.iter().any(|candidate| candidate == variant) {
            return Err(CompileError::new(format!("unknown enum variant '{}::{}'", enum_name, variant), span));
        }
        if self.enum_variant_has_payload(enum_name, variant) {
            return Err(CompileError::new(
                format!(
                    "enum payload variant '{}::{}' cannot be used as a value until payload construction lowering is implemented",
                    enum_name, variant
                ),
                span,
            ));
        }
        Ok(Some(Type::Named(enum_name.to_string())))
    }

    fn enum_variant_has_payload(&self, enum_name: &str, variant: &str) -> bool {
        if self.enum_payload_variants.get(enum_name).is_some_and(|payloads| payloads.contains(variant)) {
            return true;
        }
        self.resolver
            .zip(self.current_module.as_deref())
            .and_then(|(resolver, module)| resolver.resolve_type(module, enum_name))
            .is_some_and(|ty| match ty {
                TypeDef::Enum(enum_def) => {
                    enum_def.variants.iter().any(|candidate| candidate.name == variant && !candidate.fields.is_empty())
                }
                _ => false,
            })
    }

    fn validate_expr_allowed_in_current_callable(&self, expr: &Expr) -> Result<()> {
        if matches!(expr, Expr::Require(_) | Expr::RequireBlock(_) | Expr::Preserve(_))
            && !matches!(self.current_callable, Some(CallableKind::Action | CallableKind::Lock))
        {
            return Err(CompileError::new(
                "require/preserve is verifier-boundary syntax for actions and locks; use ordinary boolean expressions inside pure functions",
                expr_span(expr),
            ));
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
            _ => None,
        };

        match (self.current_callable, operation) {
            (Some(CallableKind::Function), Some(operation)) => {
                return Err(CompileError::new(
                    format!(
                        "pure function cannot contain '{}' Cell/runtime operation; move state transition logic into an action",
                        operation
                    ),
                    expr_span(expr),
                ));
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
        if self.current_callable != Some(CallableKind::Function) {
            return Ok(());
        }

        match call.func.as_ref() {
            Expr::Identifier(name) if name.starts_with("env::") || name.starts_with("ckb::") => Err(CompileError::new(
                format!("pure function cannot call '{}' runtime builtin; move runtime-dependent logic into an action", name),
                call.span,
            )),
            Expr::FieldAccess(field) if field.field == "type_hash" => Err(CompileError::new(
                "pure function cannot call 'type_hash' Cell identity builtin; move Cell identity logic into an action",
                call.span,
            )),
            _ => Ok(()),
        }
    }

    fn initializer_types_equal(&self, actual: &Type, expected: &Type) -> bool {
        self.types_equal(actual, expected)
            || matches!((actual, expected), (Type::Named(actual), Type::Named(expected)) if actual == "Vec" && expected.starts_with("Vec<"))
    }

    fn bind_pattern(&self, env: &mut TypeEnv, pattern: &BindingPattern, ty: &Type, is_mut: bool, span: Span) -> Result<()> {
        match pattern {
            BindingPattern::Name(name) => {
                if name == "_" {
                    if self.is_linear_type(ty) {
                        return Err(CompileError::new("wildcard binding cannot discard a linear value", span));
                    }
                    return Ok(());
                }
                let is_linear = self.is_linear_type(ty);
                env.bind_new(name.clone(), ty.clone(), is_linear, is_mut, span)?;
                Ok(())
            }
            BindingPattern::Wildcard => {
                if self.is_linear_type(ty) {
                    return Err(CompileError::new("wildcard binding cannot discard a linear value", span));
                }
                Ok(())
            }
            BindingPattern::Tuple(items) => {
                let Type::Tuple(types) = ty else {
                    return Err(CompileError::new("tuple binding requires a tuple value", span));
                };
                if items.len() != types.len() {
                    return Err(CompileError::new(
                        format!("tuple binding arity mismatch: pattern has {}, value has {}", items.len(), types.len()),
                        span,
                    ));
                }
                for (item, item_ty) in items.iter().zip(types.iter()) {
                    self.bind_pattern(env, item, item_ty, is_mut, span)?;
                }
                Ok(())
            }
        }
    }

    fn mark_stmt_as_returned(&mut self, env: &mut TypeEnv, tail_base: &TypeEnv, stmt: &Stmt) -> Result<()> {
        match stmt {
            Stmt::Expr(expr) => self.mark_expr_as_moved(env, expr),
            Stmt::Return(Some(_)) => Ok(()),
            Stmt::If(if_stmt) if matches!(self.current_return_type, Some(Some(_))) => {
                let Some(else_branch) = &if_stmt.else_branch else {
                    return Ok(());
                };
                let then_env = self.branch_env_with_tail_return(tail_base, &if_stmt.then_branch)?;
                let else_env = self.branch_env_with_tail_return(tail_base, else_branch)?;
                let mut merged = tail_base.clone();
                merged.merge_branch_linear_states(&then_env, true, Some(&else_env), true, if_stmt.span)?;
                *env = merged;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn branch_env_with_tail_return(&mut self, base_env: &TypeEnv, branch: &[Stmt]) -> Result<TypeEnv> {
        let mut branch_env = base_env.child();
        let Some((last, prefix)) = branch.split_last() else {
            return Ok(branch_env);
        };
        for stmt in prefix {
            self.check_stmt(&mut branch_env, stmt)?;
        }
        let tail_base = branch_env.clone();
        self.check_stmt(&mut branch_env, last)?;
        self.mark_stmt_as_returned(&mut branch_env, &tail_base, last)?;
        Ok(branch_env)
    }

    fn stmts_always_return(&self, stmts: &[Stmt]) -> bool {
        stmts.iter().any(|stmt| self.stmt_always_returns(stmt))
    }

    fn stmt_always_returns(&self, stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Return(_) => true,
            Stmt::If(if_stmt) => {
                let Some(else_branch) = &if_stmt.else_branch else {
                    return false;
                };
                self.stmts_always_return(&if_stmt.then_branch) && self.stmts_always_return(else_branch)
            }
            Stmt::Expr(Expr::Block(stmts)) => self.stmts_always_return(stmts),
            _ => false,
        }
    }

    fn check_body_returns_or_tail_expr(
        &mut self,
        kind: &str,
        name: &str,
        body: &[Stmt],
        return_type: &Type,
        span: Span,
        env: &TypeEnv,
    ) -> Result<()> {
        if self.body_returns_or_tail_expr(body, return_type, env)? {
            return Ok(());
        }

        Err(CompileError::new(format!("{} '{}' with a return type must return a value on all paths", kind, name), span))
    }

    fn body_returns_or_tail_expr(&mut self, body: &[Stmt], return_type: &Type, env: &TypeEnv) -> Result<bool> {
        if self.stmts_always_return(body) {
            return Ok(true);
        }

        let Some((last, prefix)) = body.split_last() else {
            return Ok(false);
        };
        let mut tail_env = env.clone();
        for stmt in prefix {
            self.check_stmt(&mut tail_env, stmt)?;
        }

        if let Stmt::Expr(expr) = last {
            let tail_ty = self.infer_expr(&mut tail_env, expr)?;
            if self.types_equal(&tail_ty, return_type) {
                return Ok(true);
            }
            return Err(CompileError::new(
                format!("tail expression type mismatch: expected {:?}, found {:?}", return_type, tail_ty),
                expr_span(expr),
            ));
        }

        if let Stmt::If(if_stmt) = last {
            let Some(else_branch) = &if_stmt.else_branch else {
                return Ok(false);
            };
            let then_ok = self.body_returns_or_tail_expr(&if_stmt.then_branch, return_type, &tail_env.child())?;
            let else_ok = self.body_returns_or_tail_expr(else_branch, return_type, &tail_env.child())?;
            return Ok(then_ok && else_ok);
        }

        Ok(false)
    }

    fn infer_lock_terminal_stmt(&mut self, env: &mut TypeEnv, stmt: &Stmt) -> Result<Type> {
        match stmt {
            Stmt::Expr(expr) => self.infer_expr(env, expr),
            Stmt::Return(Some(expr)) => self.infer_expr(env, expr),
            Stmt::If(if_stmt) => {
                let cond_ty = self.infer_expr(env, &if_stmt.condition)?;
                if !self.is_bool_type(&cond_ty) {
                    return Err(CompileError::new("if condition must be boolean", if_stmt.span));
                }
                let mut then_env = env.child();
                let then_ty = if let Some(stmt) = if_stmt.then_branch.last() {
                    for stmt in &if_stmt.then_branch[..if_stmt.then_branch.len().saturating_sub(1)] {
                        self.check_stmt(&mut then_env, stmt)?;
                    }
                    self.infer_lock_terminal_stmt(&mut then_env, stmt)?
                } else {
                    return Err(CompileError::new("lock if branch must end with a bool expression", if_stmt.span));
                };
                let else_branch = if_stmt
                    .else_branch
                    .as_ref()
                    .ok_or_else(|| CompileError::new("lock if statement must have an else branch", if_stmt.span))?;
                let mut else_env = env.child();
                let else_ty = if let Some(stmt) = else_branch.last() {
                    for stmt in &else_branch[..else_branch.len().saturating_sub(1)] {
                        self.check_stmt(&mut else_env, stmt)?;
                    }
                    self.infer_lock_terminal_stmt(&mut else_env, stmt)?
                } else {
                    return Err(CompileError::new("lock else branch must end with a bool expression", if_stmt.span));
                };
                if !self.types_equal(&then_ty, &else_ty) {
                    return Err(CompileError::new("lock branches must return matching types", if_stmt.span));
                }
                Ok(then_ty)
            }
            _ => Err(CompileError::new("lock body must end with an expression or explicit return", stmt_span(stmt))),
        }
    }

    fn mark_expr_as_moved(&mut self, env: &mut TypeEnv, expr: &Expr) -> Result<()> {
        match expr {
            Expr::Identifier(name) => {
                if let Some(ty) = env.lookup(name).cloned() {
                    if self.is_linear_type(&ty) {
                        env.consume(name)?;
                    }
                }
                Ok(())
            }
            Expr::Tuple(items) | Expr::Array(items) => {
                for item in items {
                    self.mark_expr_as_moved(env, item)?;
                }
                Ok(())
            }
            Expr::Cast(cast) => self.mark_expr_as_moved(env, &cast.expr),
            Expr::Assign(assign) => self.mark_expr_as_moved(env, &assign.value),
            Expr::Preserve(_) => Ok(()),
            Expr::Assert(assert_expr) => self.mark_expr_as_moved(env, &assert_expr.condition),
            Expr::Require(require_expr) => {
                self.mark_expr_as_moved(env, &require_expr.condition)?;
                if let Some(message) = &require_expr.message {
                    self.mark_expr_as_moved(env, message)?;
                }
                Ok(())
            }
            Expr::If(if_expr) => {
                let mut then_env = env.child();
                self.mark_expr_as_moved(&mut then_env, &if_expr.then_branch)?;
                let mut else_env = env.child();
                self.mark_expr_as_moved(&mut else_env, &if_expr.else_branch)?;
                env.merge_branch_linear_states(&then_env, false, Some(&else_env), false, if_expr.span)
            }
            Expr::Match(match_expr) => {
                let mut arm_envs = Vec::with_capacity(match_expr.arms.len());
                for arm in &match_expr.arms {
                    let mut arm_env = env.child();
                    self.mark_expr_as_moved(&mut arm_env, &arm.value)?;
                    arm_envs.push(arm_env);
                }
                env.merge_match_linear_states(&arm_envs, match_expr.span)
            }
            Expr::Block(_) => Ok(()),
            _ => Ok(()),
        }
    }

    fn reject_local_reference_to_linear_root(&self, env: &TypeEnv, value: &Expr, ty: &Type, span: Span) -> Result<()> {
        self.reject_stored_linear_reference_alias(env, value, span)?;
        if matches!(value, Expr::Unary(_)) {
            self.reject_unrooted_linear_reference_type(ty, span)?;
        }
        Ok(())
    }

    fn reject_stored_linear_reference_alias(&self, env: &TypeEnv, expr: &Expr, span: Span) -> Result<()> {
        match expr {
            Expr::Unary(unary) if matches!(unary.op, UnaryOp::Ref) => {
                if let Some(root) = assignment_root_name(&unary.expr) {
                    if let Some(root_ty) = env.lookup(root) {
                        if self.is_linear_type(root_ty) {
                            return Err(CompileError::new(
                                format!(
                                    "local binding cannot store a read-only reference rooted at linear/resource value '{}'; pass the reference directly to a helper call",
                                    root
                                ),
                                span,
                            ));
                        }
                    }
                }
                Ok(())
            }
            Expr::Tuple(items) | Expr::Array(items) => {
                for item in items {
                    self.reject_stored_linear_reference_alias(env, item, span)?;
                }
                Ok(())
            }
            Expr::Cast(cast) => self.reject_stored_linear_reference_alias(env, &cast.expr, span),
            Expr::If(if_expr) => {
                self.reject_stored_linear_reference_alias(env, &if_expr.then_branch, span)?;
                self.reject_stored_linear_reference_alias(env, &if_expr.else_branch, span)
            }
            Expr::Match(match_expr) => {
                for arm in &match_expr.arms {
                    self.reject_stored_linear_reference_alias(env, &arm.value, span)?;
                }
                Ok(())
            }
            Expr::Block(stmts) => self.reject_stored_linear_reference_alias_in_tail_stmt(env, stmts, span),
            _ => Ok(()),
        }
    }

    fn reject_stored_linear_reference_alias_in_tail_stmt(&self, env: &TypeEnv, stmts: &[Stmt], span: Span) -> Result<()> {
        let Some(last) = stmts.last() else {
            return Ok(());
        };
        match last {
            Stmt::Expr(expr) | Stmt::Return(Some(expr)) => self.reject_stored_linear_reference_alias(env, expr, span),
            Stmt::If(if_stmt) => {
                self.reject_stored_linear_reference_alias_in_tail_stmt(env, &if_stmt.then_branch, span)?;
                if let Some(else_branch) = &if_stmt.else_branch {
                    self.reject_stored_linear_reference_alias_in_tail_stmt(env, else_branch, span)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn reject_unrooted_linear_reference_type(&self, ty: &Type, span: Span) -> Result<()> {
        if let Type::Ref(inner) = ty {
            if self.is_linear_type(inner) {
                return Err(CompileError::new(
                    "local binding cannot store a read-only reference to a linear/resource value; bind the cell value itself or pass the reference directly",
                    span,
                ));
            }
        }
        Ok(())
    }

    fn reject_local_mutable_reference_alias(&self, ty: &Type, span: Span) -> Result<()> {
        if Self::type_contains_mutable_reference(ty) {
            return Err(CompileError::new(
                format!(
                    "local binding cannot store mutable reference type {}; use signature-direction outputs for Cell updates",
                    type_repr(ty)
                ),
                span,
            ));
        }
        Ok(())
    }

    fn infer_assign_expr(&mut self, env: &mut TypeEnv, assign: &AssignExpr) -> Result<Type> {
        let value_ty = self.infer_expr(env, &assign.value)?;
        self.reject_assignment_reference_to_linear_root(env, &assign.value, assign.span)?;
        self.reject_assignment_mutable_reference_alias(&value_ty, assign.span)?;

        match assign.target.as_ref() {
            Expr::Identifier(name) => {
                let Some(target_ty) = env.lookup(name).cloned() else {
                    return Err(CompileError::new(format!("undefined variable '{}'", name), assign.span));
                };
                if self.is_linear_type(&target_ty) {
                    return Err(CompileError::new("assignment to linear/resource variables is not supported yet", assign.span));
                }
                if !env.is_mutable(name) {
                    return Err(CompileError::new(format!("variable '{}' is not mutable", name), assign.span));
                }
                match assign.op {
                    AssignOp::Assign => {
                        if !self.types_equal(&target_ty, &value_ty) {
                            return Err(CompileError::new("assignment requires matching types", assign.span));
                        }
                    }
                    AssignOp::AddAssign => {
                        if !self.is_numeric_type(&target_ty) || !self.is_numeric_type(&value_ty) {
                            return Err(CompileError::new("'+=' requires numeric types", assign.span));
                        }
                    }
                }
                Ok(target_ty)
            }
            Expr::FieldAccess(_) | Expr::Index(_) => {
                let Some(root) = assignment_root_name(assign.target.as_ref()) else {
                    return Err(CompileError::new("assignment target must be rooted at a named local or parameter", assign.span));
                };
                let Some(root_ty) = env.lookup(root).cloned() else {
                    return Err(CompileError::new(format!("undefined variable '{}'", root), assign.span));
                };
                if matches!(root_ty, Type::Ref(_)) {
                    return Err(CompileError::new(
                        format!("assignment target rooted at '{}' is a read-only reference", root),
                        assign.span,
                    ));
                }
                let root_is_mut_ref = matches!(root_ty, Type::MutRef(_));
                if !root_is_mut_ref && self.is_linear_type(&root_ty) {
                    return Err(CompileError::new(
                        format!(
                            "assignment target rooted at linear/resource value '{}' is not supported; use explicit input/output parameters or consume/create sugar for ownership transitions",
                            root
                        ),
                        assign.span,
                    ));
                }
                if !env.is_mutable(root) && !root_is_mut_ref {
                    return Err(CompileError::new(format!("assignment target rooted at '{}' is not mutable", root), assign.span));
                }
                let target_ty = self.infer_expr(env, &assign.target)?;
                match assign.op {
                    AssignOp::Assign => {
                        if !self.types_equal(&target_ty, &value_ty) {
                            return Err(CompileError::new("assignment requires matching types", assign.span));
                        }
                    }
                    AssignOp::AddAssign => {
                        if !self.is_numeric_type(&target_ty) || !self.is_numeric_type(&value_ty) {
                            return Err(CompileError::new("'+=' requires numeric types", assign.span));
                        }
                    }
                }
                Ok(target_ty)
            }
            _ => Err(CompileError::new("invalid assignment target", assign.span)),
        }
    }

    fn reject_assignment_reference_to_linear_root(&self, env: &TypeEnv, value: &Expr, span: Span) -> Result<()> {
        self.reject_stored_linear_reference_alias(env, value, span)
    }

    fn reject_assignment_mutable_reference_alias(&self, ty: &Type, span: Span) -> Result<()> {
        if Self::type_contains_mutable_reference(ty) {
            return Err(CompileError::new(
                format!(
                    "assignment cannot store mutable reference type {}; use signature-direction outputs for Cell updates",
                    type_repr(ty)
                ),
                span,
            ));
        }
        Ok(())
    }

    fn index_result_type(&self, ty: &Type, span: Span) -> Result<Type> {
        match ty {
            Type::Array(elem, _) => Ok((**elem).clone()),
            Type::Ref(inner) | Type::MutRef(inner) => self.index_result_type(inner, span),
            Type::Named(name) => self
                .parse_named_collection_item_type(name)
                .ok_or_else(|| CompileError::new(format!("indexing is not supported for type '{}'", name), span)),
            _ => Err(CompileError::new("indexing requires an array-like value", span)),
        }
    }

    fn iter_item_type(&self, ty: &Type, span: Span) -> Result<Type> {
        match ty {
            Type::Array(elem, _) => Ok((**elem).clone()),
            Type::Ref(inner) => Ok(Type::Ref(Box::new(self.iter_item_type(inner, span)?))),
            Type::MutRef(inner) => Ok(Type::MutRef(Box::new(self.iter_item_type(inner, span)?))),
            Type::Named(name) if name == "Range" => Ok(Type::U64),
            Type::Named(name) => self
                .parse_named_collection_item_type(name)
                .ok_or_else(|| CompileError::new(format!("cannot iterate over type '{}'", name), span)),
            _ => Err(CompileError::new("for-loop iterable must be a range or collection type", span)),
        }
    }

    fn parse_named_collection_item_type(&self, name: &str) -> Option<Type> {
        if let Some(inner) = name.strip_prefix("Vec<").and_then(|rest| rest.strip_suffix('>')) {
            return Some(self.parse_named_type_repr(inner));
        }
        None
    }

    fn supports_collection_len(&self, ty: &Type) -> bool {
        match ty {
            Type::Array(_, _) => true,
            Type::Ref(inner) | Type::MutRef(inner) => self.supports_collection_len(inner),
            Type::Named(name) => name == "Vec" || self.parse_named_collection_item_type(name).is_some(),
            _ => false,
        }
    }

    fn slice_item_type(ty: &Type) -> Option<Type> {
        match ty {
            Type::Array(inner, _) => Some((**inner).clone()),
            Type::Ref(inner) | Type::MutRef(inner) => Self::slice_item_type(inner),
            _ => None,
        }
    }

    fn parse_named_type_repr(&self, repr: &str) -> Type {
        match repr.trim() {
            "u8" => Type::U8,
            "u16" => Type::U16,
            "u32" => Type::U32,
            "u64" => Type::U64,
            "u128" => Type::U128,
            "bool" => Type::Bool,
            "Address" => Type::Address,
            "Hash" => Type::Hash,
            other => Type::Named(other.to_string()),
        }
    }

    fn lookup_field_type(&self, ty: &Type, field: &str, span: Span) -> Result<Type> {
        match ty {
            Type::Address | Type::Hash => {
                if field == "0" {
                    return Ok(Type::Array(Box::new(Type::U8), 32));
                }
                Err(CompileError::new(format!("builtin value '{:?}' only exposes tuple field '0'", ty), span))
            }
            Type::Tuple(items) => {
                if let Ok(index) = field.parse::<usize>() {
                    if let Some(item_ty) = items.get(index) {
                        return Ok(item_ty.clone());
                    }
                    return Err(CompileError::new(format!("tuple field '{}' is out of bounds", field), span));
                }
                Err(CompileError::new(format!("tuple field '{}' must be a numeric index", field), span))
            }
            Type::Ref(inner) | Type::MutRef(inner) => self.lookup_field_type(inner, field, span),
            Type::Named(name) => {
                let base_name = name.split('<').next().unwrap_or(name.as_str());
                if let Some(fields) = self.resolve_named_type_fields(base_name) {
                    if let Some(field_ty) = fields.get(field) {
                        return Ok(field_ty.clone());
                    }
                }
                Err(CompileError::new(format!("unknown field '{}' on type '{}'", field, base_name), span))
            }
            _ => Err(CompileError::new(format!("type '{:?}' does not support field access", ty), span)),
        }
    }

    fn resolve_named_type_fields(&self, type_name: &str) -> Option<HashMap<String, Type>> {
        let base_name = type_name.split('<').next().unwrap_or(type_name);
        if let Some(fields) = self.type_fields.get(base_name) {
            return Some(fields.clone());
        }
        self.resolver
            .zip(self.current_module.as_deref())
            .and_then(|(resolver, module)| resolver.type_fields(module, base_name))
            .map(|fields| fields.into_iter().collect())
    }

    fn resolve_flow_states(&self, type_name: &str) -> Option<Vec<String>> {
        let base_name = type_name.split('<').next().unwrap_or(type_name);
        if let Some(states) = self.flow_states.get(base_name) {
            return Some(states.clone());
        }
        None
    }

    fn infer_call_type(&mut self, env: &mut TypeEnv, call: &CallExpr, arg_types: &[Type]) -> Result<Type> {
        match call.func.as_ref() {
            Expr::Identifier(name) => {
                if let Some(signature) = self.functions.get(name).cloned() {
                    self.validate_call_allowed(name, signature.kind, call.span)?;
                    self.validate_call_args(name, &signature.params, arg_types, &call.args, call.span)?;
                    return Ok(signature.return_type.unwrap_or(Type::Unit));
                }
                if let Some(function) = self.resolve_function(name) {
                    self.validate_call_allowed(name, function_def_kind(&function), call.span)?;
                    let params = function_def_param_types(&function);
                    self.validate_call_args(name, &params, arg_types, &call.args, call.span)?;
                    return Ok(self.function_return_type(&function).unwrap_or(Type::Unit));
                }
                if let Some((prefix, suffix)) = name.rsplit_once("::") {
                    if self.current_module.as_deref() == Some(prefix) {
                        if let Some(signature) = self.functions.get(suffix).cloned() {
                            self.validate_call_allowed(name, signature.kind, call.span)?;
                            self.validate_call_args(name, &signature.params, arg_types, &call.args, call.span)?;
                            return Ok(signature.return_type.unwrap_or(Type::Unit));
                        }
                    }
                    return Ok(match (prefix, suffix) {
                        ("env", "current_timepoint") => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            Type::U64
                        }
                        ("ckb", "header_epoch_number" | "header_epoch_start_block_number" | "header_epoch_length" | "input_since") => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            Type::U64
                        }
                        ("Address", "zero") => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            Type::Address
                        }
                        ("Hash", "zero") => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            Type::Hash
                        }
                        ("Vec", "with_capacity") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if arg_types[0] != Type::U64 {
                                return Err(CompileError::new("Vec::with_capacity expects a u64 capacity", call.span));
                            }
                            Type::Named("Vec".to_string())
                        }
                        (_, "new") => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            self.validate_namespaced_type_constructor(prefix, suffix, call.span)?;
                            Type::Named(prefix.to_string())
                        }
                        (_, "zero") => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            self.validate_namespaced_type_constructor(prefix, suffix, call.span)?;
                            Type::Named(prefix.to_string())
                        }
                        _ => return Err(CompileError::new(format!("unknown namespaced function '{}'", name), call.span)),
                    });
                }
                if name == "min" || name == "max" || name == "isqrt" {
                    self.validate_numeric_builtin_call(name, arg_types, call.span)?;
                    return Ok(Type::U64);
                }
                Err(CompileError::new(format!("unknown function '{}'", name), call.span))
            }
            Expr::FieldAccess(field) => {
                let receiver_ty = self.infer_expr(env, &field.expr)?;
                match field.field.as_str() {
                    "type_hash" => {
                        self.validate_builtin_arity(&field.field, 0, arg_types, call.span)?;
                        Ok(Type::Hash)
                    }
                    "len" => {
                        self.validate_builtin_arity(&field.field, 0, arg_types, call.span)?;
                        if self.supports_collection_len(&receiver_ty) {
                            Ok(Type::U64)
                        } else {
                            Err(CompileError::new("len is only supported on array or Vec values", call.span))
                        }
                    }
                    "is_empty" => {
                        self.validate_builtin_arity(&field.field, 0, arg_types, call.span)?;
                        if self.supports_collection_len(&receiver_ty) {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::new("is_empty is only supported on array or Vec values", call.span))
                        }
                    }
                    "capacity" => {
                        self.validate_builtin_arity("Vec.capacity", 0, arg_types, call.span)?;
                        match &receiver_ty {
                            Type::Named(name) if self.parse_named_collection_item_type(name).is_some() => Ok(Type::U64),
                            Type::Named(name) if name == "Vec" => Err(CompileError::new(
                                "Vec.capacity requires a typed Vec<T>; push or annotate the Vec before reading capacity",
                                call.span,
                            )),
                            _ => Err(CompileError::new("capacity is only supported on Vec values", call.span)),
                        }
                    }
                    "first" | "last" => {
                        self.validate_builtin_arity(&format!("Vec.{}", field.field), 0, arg_types, call.span)?;
                        match &receiver_ty {
                            Type::Named(name) => {
                                let Some(item_ty) = self.parse_named_collection_item_type(name) else {
                                    return Err(CompileError::new(
                                        format!(
                                            "Vec.{} requires a typed Vec<T>; push or annotate the Vec before reading",
                                            field.field
                                        ),
                                        call.span,
                                    ));
                                };
                                Ok(item_ty)
                            }
                            _ => Err(CompileError::new(format!("{} is only supported on Vec values", field.field), call.span)),
                        }
                    }
                    "push" => {
                        self.validate_builtin_arity("Vec.push", 1, arg_types, call.span)?;
                        let arg_ty = &arg_types[0];
                        if self.type_contains_reference(arg_ty) {
                            return Err(CompileError::new(
                                format!(
                                    "Vec.push cannot store reference type {}; Vec<T> values must use owned non-reference items",
                                    type_repr(arg_ty)
                                ),
                                call.span,
                            ));
                        }
                        if let Type::Named(name) = &receiver_ty {
                            if name == "Vec" {
                                if let Expr::Identifier(receiver_name) = field.expr.as_ref() {
                                    env.update_type(receiver_name, Type::Named(format!("Vec<{}>", type_repr(arg_ty))));
                                }
                                return Ok(Type::Unit);
                            }
                            if let Some(item_ty) = self.parse_named_collection_item_type(name) {
                                if !self.types_equal(&item_ty, arg_ty) {
                                    return Err(CompileError::new(
                                        format!("Vec.push type mismatch: expected {:?}, found {:?}", item_ty, arg_ty),
                                        call.span,
                                    ));
                                }
                                return Ok(Type::Unit);
                            }
                        }
                        Err(CompileError::new("push is only supported on Vec values", call.span))
                    }
                    "clear" => {
                        self.validate_builtin_arity("Vec.clear", 0, arg_types, call.span)?;
                        match &receiver_ty {
                            Type::Named(name) if name == "Vec" || self.parse_named_collection_item_type(name).is_some() => {
                                Ok(Type::Unit)
                            }
                            _ => Err(CompileError::new("clear is only supported on Vec values", call.span)),
                        }
                    }
                    "reverse" => {
                        self.validate_builtin_arity("Vec.reverse", 0, arg_types, call.span)?;
                        match &receiver_ty {
                            Type::Named(name) if name == "Vec" || self.parse_named_collection_item_type(name).is_some() => {
                                Ok(Type::Unit)
                            }
                            _ => Err(CompileError::new("reverse is only supported on Vec values", call.span)),
                        }
                    }
                    "truncate" => {
                        self.validate_builtin_arity("Vec.truncate", 1, arg_types, call.span)?;
                        if arg_types[0] != Type::U64 {
                            return Err(CompileError::new("Vec.truncate expects a u64 length", call.span));
                        }
                        match &receiver_ty {
                            Type::Named(name) if name == "Vec" || self.parse_named_collection_item_type(name).is_some() => {
                                Ok(Type::Unit)
                            }
                            _ => Err(CompileError::new("truncate is only supported on Vec values", call.span)),
                        }
                    }
                    "swap" => {
                        self.validate_builtin_arity("Vec.swap", 2, arg_types, call.span)?;
                        if arg_types[0] != Type::U64 || arg_types[1] != Type::U64 {
                            return Err(CompileError::new("Vec.swap expects u64 indices", call.span));
                        }
                        match &receiver_ty {
                            Type::Named(name) if name == "Vec" || self.parse_named_collection_item_type(name).is_some() => {
                                Ok(Type::Unit)
                            }
                            _ => Err(CompileError::new("swap is only supported on Vec values", call.span)),
                        }
                    }
                    "contains" => {
                        self.validate_builtin_arity("Vec.contains", 1, arg_types, call.span)?;
                        let arg_ty = &arg_types[0];
                        match &receiver_ty {
                            Type::Named(name) if name == "Vec" => {
                                if let Expr::Identifier(receiver_name) = field.expr.as_ref() {
                                    env.update_type(receiver_name, Type::Named(format!("Vec<{}>", type_repr(arg_ty))));
                                }
                                Ok(Type::Bool)
                            }
                            Type::Named(name) => {
                                let Some(item_ty) = self.parse_named_collection_item_type(name) else {
                                    return Err(CompileError::new("contains is only supported on Vec values", call.span));
                                };
                                if !self.types_equal(&item_ty, arg_ty) {
                                    return Err(CompileError::new(
                                        format!("Vec.contains type mismatch: expected {:?}, found {:?}", item_ty, arg_ty),
                                        call.span,
                                    ));
                                }
                                Ok(Type::Bool)
                            }
                            _ => Err(CompileError::new("contains is only supported on Vec values", call.span)),
                        }
                    }
                    "remove" => {
                        self.validate_builtin_arity("Vec.remove", 1, arg_types, call.span)?;
                        if arg_types[0] != Type::U64 {
                            return Err(CompileError::new("Vec.remove expects a u64 index", call.span));
                        }
                        match &receiver_ty {
                            Type::Named(name) => {
                                let Some(item_ty) = self.parse_named_collection_item_type(name) else {
                                    return Err(CompileError::new(
                                        "Vec.remove requires a typed Vec<T>; push or annotate the Vec before removing",
                                        call.span,
                                    ));
                                };
                                Ok(item_ty)
                            }
                            _ => Err(CompileError::new("remove is only supported on Vec values", call.span)),
                        }
                    }
                    "pop" => {
                        self.validate_builtin_arity("Vec.pop", 0, arg_types, call.span)?;
                        match &receiver_ty {
                            Type::Named(name) => {
                                let Some(item_ty) = self.parse_named_collection_item_type(name) else {
                                    return Err(CompileError::new(
                                        "Vec.pop requires a typed Vec<T>; push or annotate the Vec before popping",
                                        call.span,
                                    ));
                                };
                                Ok(item_ty)
                            }
                            _ => Err(CompileError::new("pop is only supported on Vec values", call.span)),
                        }
                    }
                    "insert" => {
                        self.validate_builtin_arity("Vec.insert", 2, arg_types, call.span)?;
                        if arg_types[0] != Type::U64 {
                            return Err(CompileError::new("Vec.insert expects a u64 index", call.span));
                        }
                        let arg_ty = &arg_types[1];
                        if self.type_contains_reference(arg_ty) {
                            return Err(CompileError::new(
                                format!(
                                    "Vec.insert cannot store reference type {}; Vec<T> values must use owned non-reference items",
                                    type_repr(arg_ty)
                                ),
                                call.span,
                            ));
                        }
                        match &receiver_ty {
                            Type::Named(name) if name == "Vec" => {
                                if let Expr::Identifier(receiver_name) = field.expr.as_ref() {
                                    env.update_type(receiver_name, Type::Named(format!("Vec<{}>", type_repr(arg_ty))));
                                }
                                Ok(Type::Unit)
                            }
                            Type::Named(name) => {
                                let Some(item_ty) = self.parse_named_collection_item_type(name) else {
                                    return Err(CompileError::new("insert is only supported on Vec values", call.span));
                                };
                                if !self.types_equal(&item_ty, arg_ty) {
                                    return Err(CompileError::new(
                                        format!("Vec.insert type mismatch: expected {:?}, found {:?}", item_ty, arg_ty),
                                        call.span,
                                    ));
                                }
                                Ok(Type::Unit)
                            }
                            _ => Err(CompileError::new("insert is only supported on Vec values", call.span)),
                        }
                    }
                    "set" => {
                        self.validate_builtin_arity("Vec.set", 2, arg_types, call.span)?;
                        if arg_types[0] != Type::U64 {
                            return Err(CompileError::new("Vec.set expects a u64 index", call.span));
                        }
                        let arg_ty = &arg_types[1];
                        if self.type_contains_reference(arg_ty) {
                            return Err(CompileError::new(
                                format!(
                                    "Vec.set cannot store reference type {}; Vec<T> values must use owned non-reference items",
                                    type_repr(arg_ty)
                                ),
                                call.span,
                            ));
                        }
                        match &receiver_ty {
                            Type::Named(name) if name == "Vec" => {
                                if let Expr::Identifier(receiver_name) = field.expr.as_ref() {
                                    env.update_type(receiver_name, Type::Named(format!("Vec<{}>", type_repr(arg_ty))));
                                }
                                Ok(Type::Unit)
                            }
                            Type::Named(name) => {
                                let Some(item_ty) = self.parse_named_collection_item_type(name) else {
                                    return Err(CompileError::new("set is only supported on Vec values", call.span));
                                };
                                if !self.types_equal(&item_ty, arg_ty) {
                                    return Err(CompileError::new(
                                        format!("Vec.set type mismatch: expected {:?}, found {:?}", item_ty, arg_ty),
                                        call.span,
                                    ));
                                }
                                Ok(Type::Unit)
                            }
                            _ => Err(CompileError::new("set is only supported on Vec values", call.span)),
                        }
                    }
                    "extend_from_slice" => {
                        self.validate_builtin_arity("Vec.extend_from_slice", 1, arg_types, call.span)?;
                        let Some(slice_item_ty) = Self::slice_item_type(&arg_types[0]) else {
                            return Err(CompileError::new("Vec.extend_from_slice expects an array or byte-slice source", call.span));
                        };
                        if self.type_contains_reference(&slice_item_ty) {
                            return Err(CompileError::new(
                                format!(
                                    "Vec.extend_from_slice cannot store reference type {}; Vec<T> values must use owned non-reference items",
                                    type_repr(&slice_item_ty)
                                ),
                                call.span,
                            ));
                        }
                        match &receiver_ty {
                            Type::Named(name) if name == "Vec" => {
                                if let Expr::Identifier(receiver_name) = field.expr.as_ref() {
                                    env.update_type(receiver_name, Type::Named(format!("Vec<{}>", type_repr(&slice_item_ty))));
                                }
                                Ok(Type::Unit)
                            }
                            Type::Named(name) => {
                                let Some(item_ty) = self.parse_named_collection_item_type(name) else {
                                    return Err(CompileError::new("extend_from_slice is only supported on Vec values", call.span));
                                };
                                if !self.types_equal(&item_ty, &slice_item_ty) {
                                    return Err(CompileError::new(
                                        format!(
                                            "Vec.extend_from_slice type mismatch: expected {:?}, found {:?}",
                                            item_ty, slice_item_ty
                                        ),
                                        call.span,
                                    ));
                                }
                                Ok(Type::Unit)
                            }
                            _ => Err(CompileError::new("extend_from_slice is only supported on Vec values", call.span)),
                        }
                    }
                    _ => self.lookup_field_type(&receiver_ty, &field.field, field.span),
                }
            }
            _ => Err(CompileError::new("unsupported call target", call.span)),
        }
    }

    fn validate_namespaced_type_constructor(&self, type_name: &str, constructor: &str, span: Span) -> Result<()> {
        if type_name == "Vec" {
            return Ok(());
        }
        self.validate_named_type(type_name)
            .map_err(|_| CompileError::new(format!("unknown namespaced function '{}::{}'", type_name, constructor), span))
    }

    fn validate_call_args(&self, callee_name: &str, expected: &[Type], actual: &[Type], args: &[Expr], span: Span) -> Result<()> {
        if actual.len() != expected.len() {
            return Err(CompileError::new(
                format!(
                    "function '{}' expects {} argument{}, found {}",
                    callee_name,
                    expected.len(),
                    if expected.len() == 1 { "" } else { "s" },
                    actual.len()
                ),
                span,
            ));
        }

        for (index, (expected_ty, actual_ty)) in expected.iter().zip(actual.iter()).enumerate() {
            if !self.call_argument_type_compatible(expected_ty, actual_ty) {
                return Err(CompileError::new(
                    format!(
                        "function '{}' argument {} type mismatch: expected {}, found {}",
                        callee_name,
                        index + 1,
                        type_repr(expected_ty),
                        type_repr(actual_ty)
                    ),
                    span,
                ));
            }
        }

        self.reject_duplicate_mutable_reference_call_roots(callee_name, expected, actual, args, span)?;

        Ok(())
    }

    fn reject_duplicate_mutable_reference_call_roots(
        &self,
        callee_name: &str,
        expected: &[Type],
        actual: &[Type],
        args: &[Expr],
        span: Span,
    ) -> Result<()> {
        let mut roots: HashMap<&str, bool> = HashMap::new();
        for (expected_ty, (actual_ty, arg)) in expected.iter().zip(actual.iter().zip(args.iter())) {
            if !matches!(actual_ty, Type::MutRef(_)) {
                continue;
            }
            let participates_in_mutable_alias = matches!(expected_ty, Type::MutRef(_));
            for root in mutable_reference_root_names(arg) {
                if let Some(prior_participated) = roots.get(root).copied() {
                    if participates_in_mutable_alias || prior_participated {
                        return Err(CompileError::new(
                            format!(
                                "function '{}' cannot receive mutable reference root '{}' more than once in one call; use signature-direction outputs for Cell updates",
                                callee_name, root
                            ),
                            span,
                        ));
                    }
                } else {
                    roots.insert(root, participates_in_mutable_alias);
                }
            }
        }
        Ok(())
    }

    fn call_argument_type_compatible(&self, expected: &Type, actual: &Type) -> bool {
        match (expected, actual) {
            (Type::Ref(expected_inner), Type::MutRef(actual_inner)) => self.types_equal(expected_inner, actual_inner),
            _ => self.types_equal(expected, actual),
        }
    }

    fn validate_builtin_arity(&self, name: &str, expected: usize, actual: &[Type], span: Span) -> Result<()> {
        if actual.len() == expected {
            Ok(())
        } else {
            Err(CompileError::new(
                format!("{} expects {} argument{}, found {}", name, expected, if expected == 1 { "" } else { "s" }, actual.len()),
                span,
            ))
        }
    }

    fn validate_numeric_builtin_call(&self, name: &str, arg_types: &[Type], span: Span) -> Result<()> {
        let expected = if name == "isqrt" { 1 } else { 2 };
        self.validate_builtin_arity(name, expected, arg_types, span)?;
        for (index, arg_ty) in arg_types.iter().enumerate() {
            if !self.is_numeric_type(arg_ty) {
                return Err(CompileError::new(
                    format!("{} argument {} must be numeric, found {}", name, index + 1, type_repr(arg_ty)),
                    span,
                ));
            }
        }
        Ok(())
    }

    fn reject_forbidden_consensus_call(&self, call: &CallExpr) -> Result<()> {
        if let Some(name) = forbidden_consensus_call_name(call.func.as_ref()) {
            return Err(CompileError::new(
                format!("{} is forbidden in consensus CellScript; use explicit control flow and checked error handling instead", name),
                call.span,
            ));
        }
        Ok(())
    }

    fn resolve_function(&self, name: &str) -> Option<FunctionDef> {
        self.resolver.zip(self.current_module.as_deref()).and_then(|(resolver, module)| resolver.resolve_function(module, name))
    }

    fn resolve_constant(&self, name: &str) -> Option<crate::resolve::ConstantDef> {
        self.resolver.zip(self.current_module.as_deref()).and_then(|(resolver, module)| resolver.resolve_constant(module, name))
    }

    fn function_return_type(&self, function: &FunctionDef) -> Option<Type> {
        match function {
            FunctionDef::Action(action) => action.return_type.clone(),
            FunctionDef::Function(function) => function.return_type.clone(),
            FunctionDef::Lock(_) => Some(Type::Bool),
        }
    }

    fn validate_call_allowed(&self, callee_name: &str, callee_kind: CallableKind, span: Span) -> Result<()> {
        match (self.current_callable, callee_kind) {
            (Some(CallableKind::Function), CallableKind::Action) => Err(CompileError::new(
                format!("pure function cannot call action '{}'; move state transition logic into an action", callee_name),
                span,
            )),
            (Some(CallableKind::Function), CallableKind::Lock) => {
                Err(CompileError::new(format!("pure function cannot call lock '{}'", callee_name), span))
            }
            (Some(CallableKind::Lock), CallableKind::Action) => {
                Err(CompileError::new(format!("lock cannot call action '{}'", callee_name), span))
            }
            (Some(CallableKind::Lock), CallableKind::Lock) => {
                Err(CompileError::new(format!("lock cannot call lock '{}'", callee_name), span))
            }
            _ => Ok(()),
        }
    }

    fn validate_type(&self, ty: &Type) -> Result<()> {
        match ty {
            Type::Unit => Ok(()),
            Type::Array(elem_ty, _) => self.validate_type(elem_ty),
            Type::Tuple(types) => {
                for t in types {
                    self.validate_type(t)?;
                }
                Ok(())
            }
            Type::Ref(inner) | Type::MutRef(inner) => self.validate_type(inner),
            Type::Named(name) => self.validate_named_type(name),
            _ => Ok(()),
        }
    }

    fn validate_named_type(&self, name: &str) -> Result<()> {
        let base_name = name.split('<').next().unwrap_or(name);
        match base_name {
            "Option" | "Result" => {
                return Err(CompileError::new(
                    format!("type '{}' is reserved for the explicit error model but is not implemented yet", base_name),
                    Span::default(),
                ));
            }
            _ => {}
        }

        if name.contains('<') && base_name != "Vec" {
            return Err(CompileError::new(
                format!(
                    "generic type '{}' is post-v1 template/codegen syntax, not CellScript v1 executable core; use a concrete schema type or generate a specialized .cell module",
                    name
                ),
                Span::default(),
            ));
        }
        if base_name == "Vec" && name.contains('<') && self.named_type_contains_reference(name) {
            return Err(CompileError::new(
                format!("type '{}' cannot contain reference type; Vec<T> values must use owned non-reference items", name),
                Span::default(),
            ));
        }

        match base_name {
            "String" | "Range" | "Vec" | "usize" | "isize" => return Ok(()),
            _ => {}
        }

        if self.type_fields.contains_key(base_name)
            || self.enum_variants.contains_key(base_name)
            || self.cell_type_kinds.contains_key(base_name)
            || self
                .resolver
                .zip(self.current_module.as_deref())
                .and_then(|(resolver, module)| resolver.resolve_type(module, base_name))
                .is_some()
        {
            Ok(())
        } else {
            Err(CompileError::new(format!("unknown type '{}'", name), Span::default()))
        }
    }

    fn types_equal(&self, a: &Type, b: &Type) -> bool {
        if self.is_numeric_type(a) && self.is_numeric_type(b) {
            return true;
        }
        match (a, b) {
            (Type::U8, Type::U8) => true,
            (Type::U16, Type::U16) => true,
            (Type::U32, Type::U32) => true,
            (Type::U64, Type::U64) => true,
            (Type::U128, Type::U128) => true,
            (Type::Bool, Type::Bool) => true,
            (Type::Unit, Type::Unit) => true,
            (Type::Address, Type::Address) => true,
            (Type::Hash, Type::Hash) => true,
            (Type::Array(a1, n1), Type::Array(b1, n2)) => n1 == n2 && self.types_equal(a1, b1),
            (Type::Tuple(a1), Type::Tuple(b1)) => {
                a1.len() == b1.len() && a1.iter().zip(b1.iter()).all(|(x, y)| self.types_equal(x, y))
            }
            (Type::Named(a1), Type::Named(b1)) => a1 == b1,
            (Type::Ref(a1), Type::Ref(b1)) => self.types_equal(a1, b1),
            (Type::MutRef(a1), Type::MutRef(b1)) => self.types_equal(a1, b1),
            _ => false,
        }
    }

    fn base_type_name(ty: &Type) -> Option<&str> {
        match ty {
            Type::Named(name) => Some(name.split('<').next().unwrap_or(name.as_str())),
            Type::Ref(inner) | Type::MutRef(inner) => Self::base_type_name(inner),
            _ => None,
        }
    }

    fn resolve_cell_type_kind(&self, name: &str) -> Option<CellTypeKind> {
        if let Some(kind) = self.cell_type_kinds.get(name).copied() {
            return Some(kind);
        }
        let (resolver, module) = (self.resolver?, self.current_module.as_ref()?);
        match resolver.resolve_type(module, name)? {
            TypeDef::Resource(_) => Some(CellTypeKind::Resource),
            TypeDef::Shared(_) => Some(CellTypeKind::Shared),
            TypeDef::Receipt(_) => Some(CellTypeKind::Receipt),
            TypeDef::Struct(_) | TypeDef::Enum(_) => None,
        }
    }

    fn validate_receipt_claim_output(&self, output: &Type, span: Span) -> Result<()> {
        let Some(type_name) = Self::base_type_name(output) else {
            return Err(CompileError::new("receipt claim output must be a cell-backed resource or shared type", span));
        };
        match self.resolve_cell_type_kind(type_name) {
            Some(CellTypeKind::Resource | CellTypeKind::Shared) => Ok(()),
            Some(CellTypeKind::Receipt) => Err(CompileError::new("receipt claim output must not be another receipt", span)),
            None => Err(CompileError::new("receipt claim output must be a cell-backed resource or shared type", span)),
        }
    }

    fn resolve_receipt_claim_output(&self, name: &str) -> Option<Option<Type>> {
        if let Some(output) = self.receipt_claim_outputs.get(name) {
            return Some(output.clone());
        }
        let (resolver, module) = (self.resolver?, self.current_module.as_ref()?);
        match resolver.resolve_type(module, name)? {
            TypeDef::Receipt(receipt) => Some(receipt.claim_output),
            TypeDef::Resource(_) | TypeDef::Shared(_) | TypeDef::Struct(_) | TypeDef::Enum(_) => None,
        }
    }

    fn require_named_linear_cell_operand(
        &mut self,
        env: &mut TypeEnv,
        expr: &Expr,
        operation: &str,
        span: Span,
    ) -> Result<(Type, String)> {
        let ty = self.infer_expr(env, expr)?;
        if !self.is_linear_type(&ty) {
            return Err(CompileError::new(format!("{} requires a cell-backed linear value", operation), span));
        }
        match expr {
            Expr::Identifier(name) => Ok((ty, name.clone())),
            _ => Err(CompileError::new(
                format!("{} requires a named cell-backed value so the compiler can track linear ownership", operation),
                span,
            )),
        }
    }

    fn resolve_capabilities(&self, name: &str) -> Option<HashSet<Capability>> {
        if let Some(capabilities) = self.type_capabilities.get(name) {
            return Some(capabilities.clone());
        }
        let (resolver, module) = (self.resolver?, self.current_module.as_ref()?);
        match resolver.resolve_type(module, name)? {
            TypeDef::Resource(resource) => Some(resource.capabilities.into_iter().collect()),
            TypeDef::Shared(shared) => Some(shared.capabilities.into_iter().collect()),
            TypeDef::Receipt(receipt) => Some(receipt.capabilities.into_iter().collect()),
            TypeDef::Struct(_) | TypeDef::Enum(_) => None,
        }
    }

    fn require_capability(&self, ty: &Type, capability: Capability, operation: &str, span: Span) -> Result<()> {
        let Some(type_name) = Self::base_type_name(ty) else {
            return Err(CompileError::new(format!("{} requires a cell-backed value", operation), span));
        };
        let Some(capabilities) = self.resolve_capabilities(type_name) else {
            return Err(CompileError::new(format!("{} requires a cell-backed value", operation), span));
        };
        if capabilities.contains(&capability) {
            Ok(())
        } else {
            Err(CompileError::new(
                format!(
                    "type '{}' does not declare '{}' capability required by {}",
                    type_name,
                    capability_name(capability),
                    operation
                ),
                span,
            ))
        }
    }

    fn is_linear_type(&self, ty: &Type) -> bool {
        match ty {
            Type::Array(inner, _) => self.is_linear_type(inner),
            Type::Tuple(items) => items.iter().any(|item| self.is_linear_type(item)),
            Type::Named(name) => {
                let base_name = name.split('<').next().unwrap_or(name.as_str());
                self.linear_types.contains(base_name)
                    || self
                        .resolver
                        .zip(self.current_module.as_ref())
                        .is_some_and(|(resolver, module)| resolver.type_is_linear(module, base_name))
            }
            _ => false,
        }
    }

    fn is_numeric_type(&self, ty: &Type) -> bool {
        matches!(ty, Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::U128)
            || matches!(ty, Type::Named(name) if name == "usize" || name == "isize")
    }

    fn is_bool_type(&self, ty: &Type) -> bool {
        matches!(ty, Type::Bool)
    }
}

fn stmt_span(stmt: &Stmt) -> Span {
    match stmt {
        Stmt::Let(let_stmt) => let_stmt.span,
        Stmt::Expr(expr) => expr_span(expr),
        Stmt::Return(Some(expr)) => expr_span(expr),
        Stmt::Return(None) => Span::default(),
        Stmt::If(if_stmt) => if_stmt.span,
        Stmt::For(for_stmt) => for_stmt.span,
        Stmt::While(while_stmt) => while_stmt.span,
    }
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
    }
}

fn collect_required_output_fields(expr: &Expr, outputs: &HashSet<String>, fields: &mut HashSet<String>) {
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
                if outputs.contains(field) {
                    fields.insert(field.clone());
                }
            }
        }
    }
}

fn collect_required_output_fields_from_stmt(stmt: &Stmt, outputs: &HashSet<String>, fields: &mut HashSet<String>) {
    match stmt {
        Stmt::Let(let_stmt) => collect_required_output_fields(&let_stmt.value, outputs, fields),
        Stmt::Expr(expr) | Stmt::Return(Some(expr)) => collect_required_output_fields(expr, outputs, fields),
        Stmt::Return(None) => {}
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
    }
}

fn output_field_constraint(expr: &Expr, outputs: &HashSet<String>) -> Option<String> {
    let Expr::FieldAccess(field) = expr else {
        return None;
    };
    let Expr::Identifier(base) = field.expr.as_ref() else {
        return None;
    };
    if outputs.contains(base) {
        Some(format!("{}.{}", base, field.field))
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
        Type::U32 => Some(4),
        Type::U64 => Some(8),
        Type::U128 => Some(16),
        Type::Address | Type::Hash => Some(32),
        Type::Array(inner, len) => lock_args_static_type_len(inner).map(|inner_len| inner_len * len),
        Type::Tuple(items) => items.iter().try_fold(0usize, |acc, item| lock_args_static_type_len(item).map(|len| acc + len)),
        Type::Unit => Some(0),
        Type::Named(_) | Type::Ref(_) | Type::MutRef(_) => None,
    }
}

fn item_symbol_name_and_span(item: &Item) -> Option<(&str, Span)> {
    match item {
        Item::Resource(def) => Some((&def.name, def.span)),
        Item::Shared(def) => Some((&def.name, def.span)),
        Item::Receipt(def) => Some((&def.name, def.span)),
        Item::Struct(def) => Some((&def.name, def.span)),
        Item::Flow(def) => def.name.as_deref().map(|name| (name, def.span)),
        Item::Enum(def) => Some((&def.name, def.span)),
        Item::Const(def) => Some((&def.name, def.span)),
        Item::Action(def) => Some((&def.name, def.span)),
        Item::Function(def) => Some((&def.name, def.span)),
        Item::Lock(def) => Some((&def.name, def.span)),
        Item::Use(_) => None,
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
            Some(type_name.split('<').next().unwrap_or(type_name.as_str()))
        }
        _ => None,
    })
}

fn action_param_output_named_type<'a>(action: &'a ActionDef, name: &str) -> Option<&'a str> {
    if let Some(output) = action.outputs.iter().find(|output| output.name == name) {
        if let Type::Named(type_name) = &output.ty {
            return Some(type_name.split('<').next().unwrap_or(type_name.as_str()));
        }
    }
    None
}

fn action_output_binding_names(action: &ActionDef) -> HashMap<String, ActionOutputBinding> {
    let mut bindings = HashMap::new();
    for output in &action.outputs {
        if let Type::Named(type_name) = &output.ty {
            bindings.insert(
                output.name.clone(),
                ActionOutputBinding { type_name: type_name.split('<').next().unwrap_or(type_name.as_str()).to_string() },
            );
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
            Stmt::Let(let_stmt) => collect_consumed_bindings_from_expr(&let_stmt.value, bindings),
            Stmt::Expr(expr) | Stmt::Return(Some(expr)) => collect_consumed_bindings_from_expr(expr, bindings),
            Stmt::Return(None) => {}
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
                    if !call.args.is_empty() {
                        if let Expr::Identifier(name) = &call.args[0] {
                            bindings.insert(name.clone());
                        }
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
        Stmt::Expr(expr) | Stmt::Return(Some(expr)) => collect_mutable_reference_root_names(expr, roots),
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

fn capability_name(capability: Capability) -> &'static str {
    match capability {
        Capability::Store => "store",
        Capability::Transfer => "transfer",
        Capability::Destroy => "destroy",
    }
}

fn is_state_storage_type(ty: &Type) -> bool {
    matches!(ty, Type::U8 | Type::U16 | Type::U32 | Type::U64)
}

pub fn check(module: &Module) -> Result<()> {
    let mut checker = TypeChecker::new();
    checker.check_module(module)
}

pub fn check_with_resolver(module: &Module, resolver: &ModuleResolver, current_module: &str) -> Result<()> {
    let mut checker = TypeChecker::with_resolver(resolver, current_module);
    checker.check_module(module)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer, parser, resolve::ModuleResolver};
    use camino::Utf8PathBuf;

    fn example_module(name: &str) -> Module {
        let path = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples").join(name);
        let source = std::fs::read_to_string(path).unwrap();
        let tokens = lexer::lex(&source).unwrap();
        parser::parse(&tokens).unwrap()
    }

    fn source_module(source: &str) -> Module {
        let tokens = lexer::lex(source).unwrap();
        parser::parse(&tokens).unwrap()
    }

    #[test]
    fn imported_token_type_is_treated_as_linear() {
        let token = example_module("token.cell");
        let launch = example_module("launch.cell");

        let mut resolver = ModuleResolver::new();
        resolver.register_module(token).unwrap();
        resolver.register_module(launch.clone()).unwrap();

        let checker = TypeChecker::with_resolver(&resolver, launch.name.clone());
        assert!(checker.is_linear_type(&Type::Named("Token".to_string())));
    }

    #[test]
    fn launch_module_type_checks_with_registered_imports() {
        let token = example_module("token.cell");
        let amm = example_module("amm_pool.cell");
        let launch = example_module("launch.cell");

        let mut resolver = ModuleResolver::new();
        resolver.register_module(token).unwrap();
        resolver.register_module(amm).unwrap();
        resolver.register_module(launch.clone()).unwrap();

        check_with_resolver(&launch, &resolver, &launch.name).unwrap();
    }

    #[test]
    fn imported_type_ids_must_not_collide_in_visible_module_scope() {
        let left = source_module(
            r#"
module cellscript::left

#[type_id("cellscript::asset::Token:v1")]
resource TokenA has store {
    amount: u64
}
"#,
        );
        let right = source_module(
            r#"
module cellscript::right

#[type_id("cellscript::asset::Token:v1")]
resource TokenB has store {
    amount: u64
}
"#,
        );
        let app = source_module(
            r#"
module app

use cellscript::left::TokenA
use cellscript::right::TokenB

action main(a: TokenA) -> u64
where
    return a.amount
"#,
        );

        let mut resolver = ModuleResolver::new();
        resolver.register_module(left).unwrap();
        resolver.register_module(right).unwrap();
        resolver.register_module(app.clone()).unwrap();

        let err = check_with_resolver(&app, &resolver, &app.name).unwrap_err();

        assert!(err.message.contains("duplicate type_id 'cellscript::asset::Token:v1'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn imported_linear_argument_is_marked_consumed_after_call() {
        let token = example_module("token.cell");
        let amm = example_module("amm_pool.cell");
        let launch = example_module("launch.cell");

        let mut resolver = ModuleResolver::new();
        resolver.register_module(token).unwrap();
        resolver.register_module(amm).unwrap();
        resolver.register_module(launch.clone()).unwrap();

        let action = launch
            .items
            .iter()
            .find_map(|item| match item {
                Item::Action(action) if action.name == "launch_token" => Some(action.clone()),
                _ => None,
            })
            .unwrap();

        let mut checker = TypeChecker::with_resolver(&resolver, launch.name.clone());
        let mut env = checker.env.child();
        for param in &action.params {
            let is_linear = checker.is_linear_type(&param.ty);
            env.insert(param.name.clone(), param.ty.clone(), is_linear, param.is_mut);
        }
        checker.bind_action_outputs(&mut env, &action).unwrap();

        for stmt in &action.body {
            checker.check_stmt(&mut env, stmt).unwrap();
            if let Stmt::Let(let_stmt) = stmt {
                if matches!(&let_stmt.pattern, BindingPattern::Tuple(_)) {
                    break;
                }
            }
        }

        assert_eq!(env.linear_states.get("pool_paired_token"), Some(&LinearState::Consumed));
    }

    #[test]
    fn preserve_rejects_mismatched_field_types() {
        let module = source_module(
            r#"
module test

resource A has store, transfer, destroy {
    amount: u64
}

resource B has store, transfer, destroy {
    amount: bool
}

action bad_preserve(a: A) -> b: B
where
    consume a
    preserve b from a { amount }
    create b = B { amount: true }
"#,
        );

        let err = check(&module).unwrap_err();
        assert!(err.message.contains("preserve field 'amount' type mismatch"), "unexpected error: {}", err.message);
    }

    #[test]
    fn require_rejects_nested_cell_operation() {
        let module = source_module(
            r#"
module test

resource Coin has store, transfer, destroy {
    amount: u64
}

action hidden_consume(coin: Coin)
where
    require (consume coin) == 0
"#,
        );

        let err = check(&module).unwrap_err();
        assert!(err.message.contains("require condition contains cell/runtime operation"), "unexpected error: {}", err.message);
    }

    #[test]
    fn require_block_rejects_lifecycle_stdlib_call() {
        let module = source_module(
            r#"
module test

receipt Voucher has destroy {
    amount: u64
}

action hidden_claim(voucher: Voucher)
where
    require {
        std::receipt::claim(voucher, voucher, voucher.amount)
    }
"#,
        );

        let err = check(&module).unwrap_err();
        assert!(err.message.contains("require block contains verifier-boundary syntax"), "unexpected error: {}", err.message);
    }

    #[test]
    fn require_block_rejects_assignment_expression() {
        let module = source_module(
            r#"
module test

action hidden_mutation(flag: bool)
where
    let mut ok = flag
    require {
        ok = false
    }
"#,
        );

        let err = check(&module).unwrap_err();
        assert!(err.message.contains("require block contains assignment"), "unexpected error: {}", err.message);
    }

    #[test]
    fn stdlib_claim_rejects_non_receipt_input() {
        let module = source_module(
            r#"
module test

resource Coin has store, transfer, destroy {
    amount: u64
}

action bad_claim(coin: Coin, to: Address) -> next_coin: Coin
where
    std::receipt::claim(coin, next_coin, to) { amount }
"#,
        );

        let err = check(&module).unwrap_err();
        assert!(err.message.contains("std::receipt::claim requires a receipt cell input"), "unexpected error: {}", err.message);
    }

    #[test]
    fn stdlib_claim_rejects_declared_output_type_mismatch() {
        let module = source_module(
            r#"
module test

resource Coin has store, transfer, destroy {
    amount: u64
}

resource Badge has store, transfer, destroy {
    amount: u64
}

receipt Voucher -> Coin has destroy {
    amount: u64
}

action bad_claim(voucher: Voucher, to: Address) -> badge: Badge
where
    std::receipt::claim(voucher, badge, to) { amount }
"#,
        );

        let err = check(&module).unwrap_err();
        assert!(err.message.contains("std::receipt::claim output type mismatch"), "unexpected error: {}", err.message);
    }

    #[test]
    fn stdlib_claim_rejects_extra_arguments() {
        let module = source_module(
            r#"
module test

resource Coin has store, transfer, destroy {
    amount: u64
}

receipt Voucher has destroy {
    amount: u64
}

action bad_claim(voucher: Voucher) -> coin: Coin
where
    std::receipt::claim(voucher, coin, coin, coin)
"#,
        );

        let err = check(&module).unwrap_err();
        assert!(err.message.contains("std::receipt::claim expects 3 arguments"), "unexpected error: {}", err.message);
    }

    #[test]
    fn stdlib_claim_output_requires_declared_claim_output_type() {
        let module = source_module(
            r#"
module test

resource Coin has store, transfer, destroy {
    amount: u64
}

receipt Voucher has destroy {
    amount: u64
    owner: Address
}

action bad_claim(voucher: Voucher) -> coin: Coin
where
    std::receipt::claim(voucher, coin, voucher.owner) { amount }
"#,
        );

        let err = check(&module).unwrap_err();
        assert!(
            err.message.contains("std::receipt::claim with an output requires receipt 'Voucher' to declare a claim output type"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn stdlib_claim_requires_explicit_output_and_lock_arguments() {
        let module = source_module(
            r#"
module test

resource Coin has store, transfer, destroy {
    amount: u64
}

receipt Voucher -> Coin has destroy {
    amount: u64
}

action bad_claim(voucher: Voucher) -> coin: Coin
where
    std::receipt::claim(voucher, coin) { amount }
"#,
        );

        let err = check(&module).unwrap_err();
        assert!(err.message.contains("std::receipt::claim expects 3 arguments"), "unexpected error: {}", err.message);
    }

    #[test]
    fn stdlib_settle_requires_explicit_output_and_lock_arguments() {
        let module = source_module(
            r#"
module test

resource Coin has store, transfer, destroy {
    amount: u64
}

action bad_settle(coin: Coin) -> next_coin: Coin
where
    std::lifecycle::settle(coin, next_coin) { amount }
"#,
        );

        let err = check(&module).unwrap_err();
        assert!(err.message.contains("std::lifecycle::settle expects 3 arguments"), "unexpected error: {}", err.message);
    }

    #[test]
    fn stdlib_transfer_output_requires_complete_field_coverage() {
        let module = source_module(
            r#"
module test

resource Coin has store, transfer, destroy {
    amount: u64
    owner: Address
}

action bad_transfer(coin: Coin, to: Address) -> next_coin: Coin
where
    std::lifecycle::transfer(coin, next_coin, to) { amount }
"#,
        );

        let err = check(&module).unwrap_err();
        assert!(
            err.message.contains("std::lifecycle::transfer output construction must cover every 'Coin' field; missing owner"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn stdlib_claim_output_requires_complete_field_coverage() {
        let module = source_module(
            r#"
module test

resource Coin has store, transfer, destroy {
    amount: u64
    owner: Address
}

receipt Voucher -> Coin has destroy {
    amount: u64
    owner: Address
}

action bad_claim(voucher: Voucher) -> coin: Coin
where
    std::receipt::claim(voucher, coin, voucher.owner) { amount }
"#,
        );

        let err = check(&module).unwrap_err();
        assert!(
            err.message.contains("std::receipt::claim output construction must cover every 'Coin' field; missing owner"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn stdlib_transfer_rejects_extra_arguments() {
        let module = source_module(
            r#"
module test

resource Coin has store, transfer, destroy {
    amount: u64
}

action bad_transfer(coin: Coin, to: Address) -> next_coin: Coin
where
    std::lifecycle::transfer(coin, next_coin, to, to) { amount }
"#,
        );

        let err = check(&module).unwrap_err();
        assert!(err.message.contains("std::lifecycle::transfer expects 3 arguments"), "unexpected error: {}", err.message);
    }
}
