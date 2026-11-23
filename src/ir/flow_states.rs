use super::{IrFlowRule, IrGenerator, IrType};
use crate::ast::{FlowDef, Item};

impl IrGenerator<'_> {
    pub(super) fn register_flows(&mut self, items: &[Item]) {
        for item in items {
            let Item::Flow(machine) = item else {
                continue;
            };
            let type_name = machine.target.base.clone();
            let field_name = machine.target.field.clone();
            let states = self.flow_states_for_decl(machine);
            let rules = machine
                .transitions
                .iter()
                .filter_map(|transition| {
                    let from = self.canonical_state_name(&type_name, &transition.from);
                    let to = self.canonical_state_name(&type_name, &transition.to);
                    let from_index = states.iter().position(|state| state == &from)?;
                    let to_index = states.iter().position(|state| state == &to)?;
                    Some(IrFlowRule { from, to, from_index, to_index })
                })
                .collect::<Vec<_>>();

            self.flow_states.insert(type_name.clone(), states);
            self.flow_state_fields.insert(type_name.clone(), field_name);
            self.flow_rules.insert(type_name.clone(), rules);

            let initial_state = machine.initial_states.first().map(|state| self.canonical_state_name(&type_name, state));
            let terminal_states =
                machine.terminal_states.iter().map(|state| self.canonical_state_name(&type_name, state)).collect::<Vec<_>>();
            if let Some(initial_state) = initial_state.clone() {
                self.flow_initial_states.insert(type_name.clone(), initial_state);
            }
            if !terminal_states.is_empty() {
                self.flow_terminal_states.insert(type_name.clone(), terminal_states);
            }

            let state_model = self
                .type_fields
                .get(&type_name)
                .and_then(|fields| fields.get(&machine.target.field))
                .map_or("legacy-numeric", |ty| if matches!(ty, IrType::Named(_)) { "enum-backed" } else { "legacy-numeric" });
            self.flow_state_models.insert(type_name.clone(), state_model.to_string());
            let mut warnings = Vec::new();
            if machine.initial_states.is_empty() && machine.terminal_states.is_empty() {
                warnings.push(
                    "legacy flow omits 0.22 initial/terminal declarations; migrate before claiming 0.22 terminal coverage".to_string(),
                );
            }
            if state_model == "legacy-numeric" {
                warnings.push("legacy numeric flow state; canonical 0.22 flows use a no-payload enum-backed state field".to_string());
            }
            if let Some(initial_state) = initial_state {
                let has_outgoing = machine
                    .transitions
                    .iter()
                    .any(|transition| self.canonical_state_name(&type_name, &transition.from) == initial_state);
                if !has_outgoing {
                    warnings.push(format!("initial state '{}' has no outgoing transition", initial_state));
                }
            }
            if !warnings.is_empty() {
                self.flow_audit_warnings.insert(type_name, warnings);
            }
        }
    }

    fn flow_states_for_decl(&self, machine: &FlowDef) -> Vec<String> {
        if let Some(fields) = self.type_fields.get(&machine.target.base)
            && let Some(IrType::Named(enum_name)) = fields.get(&machine.target.field)
            && let Some(variants) = self.enum_variants.get(enum_name)
        {
            let mut ordered = variants.iter().map(|(name, ordinal)| (*ordinal, name.clone())).collect::<Vec<_>>();
            ordered.sort_by_key(|(ordinal, _)| *ordinal);
            return ordered.into_iter().map(|(_, name)| name).collect();
        }

        let mut states = Vec::new();
        for transition in &machine.transitions {
            for raw in [&transition.from, &transition.to] {
                let state = self.canonical_state_name(&machine.target.base, raw);
                if !states.iter().any(|existing| existing == &state) {
                    states.push(state);
                }
            }
        }
        states
    }

    pub(super) fn canonical_state_name(&self, _type_name: &str, raw: &str) -> String {
        raw.rsplit_once("::").map_or(raw, |(_, state)| state).to_string()
    }
}
