use super::{
    action_param_output_named_type, action_param_owned_named_type, is_state_storage_type, CallableKind, FlowSpec, TypeChecker,
};
use crate::ast::{FlowDef, Item, Module, StateTransition, Type};
use crate::error::{CompileError, Result, Span};
use std::collections::{HashMap, HashSet};

impl TypeChecker<'_> {
    pub(super) fn register_flows(&mut self, module: &Module) -> Result<()> {
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
            let has_terminal_contract = !machine.initial_states.is_empty() || !machine.terminal_states.is_empty();
            let (_initial_state, terminal_states) = if has_terminal_contract {
                if machine.initial_states.len() != 1 {
                    return Err(CompileError::new(
                        format!(
                            "flow '{}.{}' must declare exactly one initial state; found {}",
                            type_name,
                            field_name,
                            machine.initial_states.len()
                        ),
                        machine.span,
                    ));
                }
                if machine.terminal_states.is_empty() {
                    return Err(CompileError::new(
                        format!("flow '{}.{}' must declare at least one terminal state", type_name, field_name),
                        machine.span,
                    ));
                }
                let initial = self.canonical_state_name_for_flow(
                    &type_name,
                    field_enum_type.as_deref(),
                    &states,
                    &machine.initial_states[0],
                    machine.span,
                )?;
                let mut terminals = Vec::new();
                for raw_terminal in &machine.terminal_states {
                    let terminal = self.canonical_state_name_for_flow(
                        &type_name,
                        field_enum_type.as_deref(),
                        &states,
                        raw_terminal,
                        machine.span,
                    )?;
                    if !terminals.iter().any(|existing| existing == &terminal) {
                        terminals.push(terminal);
                    } else {
                        return Err(CompileError::new(format!("duplicate terminal state '{}'", terminal), machine.span));
                    }
                }
                if terminals.iter().any(|terminal| terminal == &initial) {
                    return Err(CompileError::new(
                        format!("flow state '{}' cannot be both initial and terminal", initial),
                        machine.span,
                    ));
                }
                (Some(initial), terminals)
            } else {
                (None, Vec::new())
            };
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
                if !seen_transitions.insert((from.clone(), to.clone())) {
                    return Err(CompileError::new(format!("duplicate state transition '{} -> {}'", from, to), transition.span));
                }
                if terminal_states.iter().any(|terminal| terminal == &from) {
                    return Err(CompileError::new(
                        format!("terminal state '{}' cannot have an outgoing transition", from),
                        transition.span,
                    ));
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

    pub(super) fn validate_flow_action_edges(&self, module: &Module) -> Result<()> {
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
                            "state transition action '{}' is bound to '{}.{} {} -> {}' and must declare the exact field-to-field transition",
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
        if let Type::Named(enum_name) = field_ty
            && let Some(variants) = self.resolve_enum_variants(enum_name)
        {
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
                self.canonical_state_name_for_flow(&machine.target.base, Some(enum_name), &variants, &transition.to, transition.span)?;
            }
            return Ok((variants, Some(enum_name.clone())));
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

    pub(super) fn canonical_state_name_for_flow(
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
}
