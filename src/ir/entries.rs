use super::{
    bindings, collect_protocol_role_equalities_from_statements, effective_reachable_blocks, protocol_role_actor_param,
    protocol_role_field_access, protocol_role_name, protocol_role_param_source, protocol_role_source_rank, source_absence_policy,
    source_identity_policy, source_lock_expression, IrAction, IrAuditClaim, IrGenerator, IrLock, IrProtocolRoleCandidate,
    IrSourceDisposition, IrSourceDispositionKind, IrSourceLockTreatment, IrStateTransitionEdge, IrType, SchedulerHints,
};
use crate::ast::{action_core_input_binding_names, ActionDef, Expr, LockDef, ParamSource, Type};
use cellscript_artifact_checker::effect_facts::EffectFacts;
use std::collections::HashSet;

impl IrGenerator<'_> {
    pub(super) fn gen_action(&mut self, action: &ActionDef) -> IrAction {
        self.var_counter = 0;
        self.block_counter = 0;
        self.successor_relations.clear();
        self.aggregate_fields.clear();
        self.aggregate_field_contexts.clear();
        self.branch_path.clear();
        self.branch_epoch = 0;
        self.schema_field_roots.clear();
        self.aggregate_elements.clear();
        self.mutated_fields.clear();
        self.mutated_field_transitions.clear();
        self.transition_param_ids.clear();
        self.transition_coverable_value_ids.clear();
        let core_input_bindings = action_core_input_binding_names(action);
        let return_type = action.return_type.as_ref().map(|ty| self.convert_type(ty));
        let (params, mut body) =
            self.lower_signature_and_body(&action.params, &action.outputs, &action.body, return_type.clone(), &core_input_bindings);
        let context = if action.next_surface.is_some() {
            bindings::BindingContext::DeclaredTransaction
        } else {
            bindings::BindingContext::LegacyEntry
        };
        body.cell_bindings = bindings::resolve(&params, &body, context, &self.type_kinds);

        let reachable = effective_reachable_blocks(&body);
        let mut successor_relations = std::mem::take(&mut self.successor_relations)
            .into_iter()
            .filter(|relation| {
                reachable.contains(&relation.consume_block)
                    && reachable.contains(&relation.create_block)
                    && reachable.contains(&relation.success_block)
            })
            .collect::<Vec<_>>();
        for (occurrence, relation) in successor_relations.iter_mut().enumerate() {
            relation.occurrence = occurrence;
        }

        let effects = EffectFacts::empty();
        let touches_shared = self.infer_touches_shared(&params, &body);
        let estimated_cycles = self.estimate_cycles(&body);

        IrAction {
            name: action.name.clone(),
            ordered_cell_roles: action.next_surface.is_some(),
            entry_cells: None,
            source_dispositions: action
                .next_surface
                .as_ref()
                .map_or_else(Vec::new, |surface| self.lower_source_dispositions(&action.name, &surface.dispositions)),
            successor_relations,
            audit_claims: action.next_surface.as_ref().map_or_else(Vec::new, |surface| Self::lower_audit_claims(&surface.audits)),
            params,
            return_type,
            state_transition_edges: self.action_state_transition_edges(action),
            protocol_role_candidates: self.action_protocol_role_candidates(action),
            body,
            effects,
            scheduler_hints: action
                .scheduler_hint
                .as_ref()
                .map(|hint| SchedulerHints {
                    parallelizable: hint.parallelizable,
                    touches_shared: touches_shared.clone(),
                    estimated_cycles: hint.estimated_cycles.max(estimated_cycles),
                })
                .unwrap_or(SchedulerHints { parallelizable: touches_shared.is_empty(), touches_shared, estimated_cycles }),
        }
    }

    fn lower_source_dispositions(&self, entry_name: &str, dispositions: &[crate::ast::NextDisposition]) -> Vec<IrSourceDisposition> {
        dispositions
            .iter()
            .flat_map(|disposition| match disposition {
                crate::ast::NextDisposition::Replace(_) => Vec::new(),
                crate::ast::NextDisposition::Pool(pool) => {
                    let pool_id = format!("pool:{entry_name}:{}", pool.name);
                    let accounting_obligation = pool
                        .data_fields
                        .iter()
                        .filter_map(|field| match &field.treatment {
                            crate::ast::NextPoolFieldTreatment::Conserve => {
                                Some(format!("checked-u128-field-sum-equality:{}", field.field))
                            }
                            crate::ast::NextPoolFieldTreatment::Set(_) => None,
                        })
                        .collect::<Vec<_>>()
                        .join("+");
                    let mut lowered = pool
                        .inputs
                        .iter()
                        .map(|input| IrSourceDisposition {
                            kind: IrSourceDispositionKind::PooledInput {
                                pool_id: pool_id.clone(),
                                accounting_obligation: accounting_obligation.clone(),
                            },
                            input_binding: Some(input.clone()),
                            output_binding: None,
                            data_fields: pool
                                .data_fields
                                .iter()
                                .map(|field| {
                                    let treatment = match &field.treatment {
                                        crate::ast::NextPoolFieldTreatment::Conserve => {
                                            format!("contributes-to:{}:conserve", pool_id)
                                        }
                                        crate::ast::NextPoolFieldTreatment::Set(_) => format!("available-to:{}:output-plan", pool_id),
                                    };
                                    (field.field.clone(), treatment)
                                })
                                .collect(),
                            lock_expression: None,
                            lock_treatment: IrSourceLockTreatment::None,
                            span: pool.span,
                        })
                        .collect::<Vec<_>>();
                    lowered.extend(pool.outputs.iter().map(|output| {
                        IrSourceDisposition {
                            kind: IrSourceDispositionKind::PoolResult {
                                pool_id: pool_id.clone(),
                                accounting_obligation: accounting_obligation.clone(),
                            },
                            input_binding: None,
                            output_binding: Some(output.clone()),
                            data_fields: pool
                                .data_fields
                                .iter()
                                .map(|field| {
                                    let treatment = match &field.treatment {
                                        crate::ast::NextPoolFieldTreatment::Conserve => {
                                            format!("checked-by:{}:field-sum-equality", pool_id)
                                        }
                                        crate::ast::NextPoolFieldTreatment::Set(assignments) => assignments
                                            .iter()
                                            .find(|(candidate, _)| candidate == output)
                                            .map(|(_, expression)| {
                                                format!("set-from-expression:{}", crate::fmt::format_expression(expression))
                                            })
                                            .expect("validated pool output field assignment"),
                                    };
                                    (field.field.clone(), treatment)
                                })
                                .collect(),
                            lock_expression: pool
                                .output_locks
                                .iter()
                                .find(|(candidate, _)| candidate == output)
                                .and_then(|(_, treatment)| source_lock_expression(treatment))
                                .map(crate::fmt::format_expression),
                            lock_treatment: pool
                                .output_locks
                                .iter()
                                .find(|(candidate, _)| candidate == output)
                                .map(|(_, treatment)| IrSourceLockTreatment::from_source(treatment))
                                .unwrap_or(IrSourceLockTreatment::None),
                            span: pool.span,
                        }
                    }));
                    lowered
                }
                crate::ast::NextDisposition::Retire(retirement) => vec![IrSourceDisposition {
                    kind: IrSourceDispositionKind::Retired { absence_policy: source_absence_policy(&retirement.absence_policy) },
                    input_binding: Some(retirement.input.clone()),
                    output_binding: None,
                    data_fields: Vec::new(),
                    lock_expression: None,
                    lock_treatment: IrSourceLockTreatment::None,
                    span: retirement.span,
                }],
                crate::ast::NextDisposition::Fresh(fresh) => vec![IrSourceDisposition {
                    kind: IrSourceDispositionKind::Fresh { identity_policy: source_identity_policy(&fresh.identity) },
                    input_binding: None,
                    output_binding: Some(fresh.output.clone()),
                    data_fields: fresh
                        .data_fields
                        .iter()
                        .map(|(field, value)| (field.clone(), crate::fmt::format_expression(value)))
                        .collect(),
                    lock_expression: source_lock_expression(&fresh.lock).map(crate::fmt::format_expression),
                    lock_treatment: IrSourceLockTreatment::from_source(&fresh.lock),
                    span: fresh.span,
                }],
            })
            .collect()
    }

    fn lower_audit_claims(audits: &[crate::ast::NextAudit]) -> Vec<IrAuditClaim> {
        audits
            .iter()
            .map(|audit| IrAuditClaim {
                name: audit.name.clone(),
                evidence: match audit.evidence {
                    crate::ast::NextAuditEvidence::ExternalPolicy => "external-policy".to_string(),
                },
                subject: crate::fmt::format_expression(&audit.subject),
                span: audit.span,
            })
            .collect()
    }

    fn action_protocol_role_candidates(&self, action: &ActionDef) -> Vec<IrProtocolRoleCandidate> {
        let mut candidates = Vec::new();
        let mut equalities = Vec::new();
        collect_protocol_role_equalities_from_statements(&action.body, &mut equalities);

        for equality in equalities {
            let predicate = crate::fmt::format_expression(&Expr::Binary(equality.clone()));
            for (field_side, actor_side) in
                [(equality.left.as_ref(), equality.right.as_ref()), (equality.right.as_ref(), equality.left.as_ref())]
            {
                let Some((cell_binding, field_name)) = protocol_role_field_access(field_side) else {
                    continue;
                };
                let Expr::Identifier(actor_binding) = actor_side else {
                    continue;
                };
                let Some(cell_param) = action.params.iter().find(|param| param.name == cell_binding) else {
                    continue;
                };
                let Some(type_name) = Self::named_type_name_from_ast_type(&cell_param.ty) else {
                    continue;
                };
                if !self.type_fields.get(type_name).and_then(|fields| fields.get(field_name)).is_some_and(|ty| *ty == IrType::Address)
                {
                    continue;
                }
                let Some(actor_param) = action.params.iter().find(|param| param.name == *actor_binding) else {
                    continue;
                };
                if !protocol_role_actor_param(actor_param) {
                    continue;
                }
                let role = protocol_role_name(field_name).unwrap_or_else(|| field_name.to_ascii_lowercase());
                candidates.push(IrProtocolRoleCandidate {
                    role,
                    source: "verification-predicate".to_string(),
                    strength: "explicit".to_string(),
                    actor_binding: Some(actor_binding.clone()),
                    actor_source: Some(protocol_role_param_source(actor_param.source).to_string()),
                    cell_binding: Some(cell_binding.to_string()),
                    field_name: Some(field_name.to_string()),
                    predicate: Some(predicate.clone()),
                    span: Some(equality.span),
                });
            }
        }

        for param in &action.params {
            if !matches!(&param.ty, Type::Address) {
                continue;
            }
            let Some(role) = protocol_role_name(&param.name) else {
                continue;
            };
            let (source, actor_source) = match param.source {
                ParamSource::Witness => ("witness-binding", "witness"),
                ParamSource::LockArgs => ("lock-args-binding", "lock_args"),
                ParamSource::Default => ("witness-binding", "entry-witness-default"),
                ParamSource::Input | ParamSource::Output | ParamSource::Protected => continue,
            };
            candidates.push(IrProtocolRoleCandidate {
                role,
                source: source.to_string(),
                strength: "binding".to_string(),
                actor_binding: Some(param.name.clone()),
                actor_source: Some(actor_source.to_string()),
                cell_binding: None,
                field_name: None,
                predicate: None,
                span: Some(param.span),
            });
        }

        for param in &action.params {
            let Some(type_name) = Self::named_type_name_from_ast_type(&param.ty) else {
                continue;
            };
            let Some(fields) = self.type_fields.get(type_name) else {
                continue;
            };
            let mut fields = fields.iter().collect::<Vec<_>>();
            fields.sort_by_key(|(name, _)| *name);
            for (field_name, field_ty) in fields {
                if *field_ty != IrType::Address {
                    continue;
                }
                let Some(role) = protocol_role_name(field_name) else {
                    continue;
                };
                candidates.push(IrProtocolRoleCandidate {
                    role,
                    source: "field-name".to_string(),
                    strength: "weak".to_string(),
                    actor_binding: None,
                    actor_source: None,
                    cell_binding: Some(param.name.clone()),
                    field_name: Some(field_name.clone()),
                    predicate: None,
                    span: None,
                });
            }
        }

        candidates.sort_by(|left, right| {
            protocol_role_source_rank(&left.source)
                .cmp(&protocol_role_source_rank(&right.source))
                .then_with(|| left.role.cmp(&right.role))
                .then_with(|| left.actor_binding.cmp(&right.actor_binding))
                .then_with(|| left.cell_binding.cmp(&right.cell_binding))
                .then_with(|| left.field_name.cmp(&right.field_name))
                .then_with(|| left.predicate.cmp(&right.predicate))
        });
        candidates.dedup();
        candidates
    }

    fn action_state_transition_edges(&self, action: &ActionDef) -> Vec<IrStateTransitionEdge> {
        let mut edges = Vec::new();
        for state_edge in &action.state_edges {
            let path = &state_edge.path;
            let Some(param) = action.params.iter().find(|param| param.name == path.base) else {
                continue;
            };
            let Some(type_name) = Self::named_type_name_from_ast_type(&param.ty) else {
                continue;
            };
            if let Some(mut lowered) = self.state_transition_edge_for(type_name, &path.field, &state_edge.from, &state_edge.to) {
                lowered.input_binding = Some(path.base.clone());
                lowered.output_binding = Some(state_edge.to_path.base.clone());
                edges.push(lowered);
            }
        }

        let mut unique = Vec::new();
        for state_edge in edges {
            if !unique.iter().any(|existing: &IrStateTransitionEdge| {
                existing.type_name == state_edge.type_name
                    && existing.field_name == state_edge.field_name
                    && existing.input_binding == state_edge.input_binding
                    && existing.output_binding == state_edge.output_binding
                    && existing.from_index == state_edge.from_index
                    && existing.to_index == state_edge.to_index
            }) {
                unique.push(state_edge);
            }
        }
        unique
    }

    fn state_transition_edge_for(&self, type_name: &str, field_name: &str, from: &str, to: &str) -> Option<IrStateTransitionEdge> {
        if self.flow_state_fields.get(type_name).is_some_and(|field| field != field_name) {
            return None;
        }
        let states = self.flow_states.get(type_name)?;
        let from = self.canonical_state_name(type_name, from);
        let to = self.canonical_state_name(type_name, to);
        let from_index = states.iter().position(|state| state == &from)?;
        let to_index = states.iter().position(|state| state == &to)?;
        Some(IrStateTransitionEdge {
            input_binding: None,
            output_binding: None,
            type_name: type_name.to_string(),
            field_name: field_name.to_string(),
            from,
            to,
            from_index,
            to_index,
        })
    }

    pub(super) fn gen_lock(&mut self, lock: &LockDef) -> IrLock {
        self.var_counter = 0;
        self.block_counter = 0;
        self.successor_relations.clear();
        self.aggregate_fields.clear();
        self.aggregate_field_contexts.clear();
        self.branch_path.clear();
        self.branch_epoch = 0;
        self.schema_field_roots.clear();
        self.aggregate_elements.clear();
        self.mutated_fields.clear();
        self.mutated_field_transitions.clear();
        self.transition_param_ids.clear();
        self.transition_coverable_value_ids.clear();
        let previous_lock_entry = self.lowering_lock_entry;
        self.lowering_lock_entry = true;
        let (params, mut body) = self.lower_signature_and_body(&lock.params, &[], &lock.body, Some(IrType::Bool), &HashSet::new());
        let context = if lock.next_surface.is_some() {
            bindings::BindingContext::NativeLockGroup
        } else {
            bindings::BindingContext::LegacyLockEntry
        };
        body.cell_bindings = bindings::resolve(&params, &body, context, &self.type_kinds);
        self.lowering_lock_entry = previous_lock_entry;

        IrLock {
            name: lock.name.clone(),
            effects: EffectFacts::empty(),
            params,
            audit_claims: lock.next_surface.as_ref().map_or_else(Vec::new, |surface| Self::lower_audit_claims(&surface.audits)),
            body,
        }
    }
}
