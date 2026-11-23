use super::{
    bindings, IrAggregateInvariant, IrEnumFieldLayout, IrEnumLayout, IrEnumVariantLayout, IrField, IrFunction, IrGenerator,
    IrIdentityPolicy, IrInvariant, IrType, IrTypeDef, IrTypeKind,
};
use crate::ast::{Field, FnDef, InvariantDef, ReceiptDef, ResourceDef, SharedDef, StructDef};
use crate::error::Span;
use cellscript_artifact_checker::effect_facts::EffectFacts;
use std::collections::{HashMap, HashSet};

impl IrGenerator<'_> {
    pub(super) fn gen_resource(&mut self, resource: &ResourceDef) -> IrTypeDef {
        IrTypeDef {
            name: resource.name.clone(),
            type_id: resource.type_id.as_ref().map(|type_id| type_id.value.clone()),
            default_hash_type: resource.default_hash_type.as_ref().map(|hash_type| hash_type.value.clone()),
            capacity_floor_shannons: resource.capacity_floor.as_ref().map(|floor| floor.shannons),
            kind: IrTypeKind::Resource,
            fields: self.layout_fields(&resource.fields),
            capabilities: resource.capabilities.clone(),
            value_abilities: Vec::new(),
            claim_output: None,
            flow_states: self.flow_states.get(&resource.name).cloned(),
            flow_state_field: self.flow_state_fields.get(&resource.name).cloned(),
            flow_rules: self.flow_rules.get(&resource.name).cloned().unwrap_or_default(),
            flow_initial_state: self.flow_initial_states.get(&resource.name).cloned(),
            flow_terminal_states: self.flow_terminal_states.get(&resource.name).cloned().unwrap_or_default(),
            flow_terminal_discharge: self
                .flow_terminal_states
                .contains_key(&resource.name)
                .then(|| "terminal-by-output-state".to_string()),
            flow_state_model: self.flow_state_models.get(&resource.name).cloned(),
            flow_audit_warnings: self.flow_audit_warnings.get(&resource.name).cloned().unwrap_or_default(),
            validity_predicates: self.type_validity.get(&resource.name).cloned().unwrap_or_default(),
            identity: Self::lower_identity_policy(&resource.identity),
        }
    }

    pub(super) fn gen_shared(&mut self, shared: &SharedDef) -> IrTypeDef {
        IrTypeDef {
            name: shared.name.clone(),
            type_id: shared.type_id.as_ref().map(|type_id| type_id.value.clone()),
            default_hash_type: shared.default_hash_type.as_ref().map(|hash_type| hash_type.value.clone()),
            capacity_floor_shannons: shared.capacity_floor.as_ref().map(|floor| floor.shannons),
            kind: IrTypeKind::Shared,
            fields: self.layout_fields(&shared.fields),
            capabilities: shared.capabilities.clone(),
            value_abilities: Vec::new(),
            claim_output: None,
            flow_states: self.flow_states.get(&shared.name).cloned(),
            flow_state_field: self.flow_state_fields.get(&shared.name).cloned(),
            flow_rules: self.flow_rules.get(&shared.name).cloned().unwrap_or_default(),
            flow_initial_state: self.flow_initial_states.get(&shared.name).cloned(),
            flow_terminal_states: self.flow_terminal_states.get(&shared.name).cloned().unwrap_or_default(),
            flow_terminal_discharge: self
                .flow_terminal_states
                .contains_key(&shared.name)
                .then(|| "terminal-by-output-state".to_string()),
            flow_state_model: self.flow_state_models.get(&shared.name).cloned(),
            flow_audit_warnings: self.flow_audit_warnings.get(&shared.name).cloned().unwrap_or_default(),
            validity_predicates: self.type_validity.get(&shared.name).cloned().unwrap_or_default(),
            identity: Self::lower_identity_policy(&shared.identity),
        }
    }

    pub(super) fn gen_receipt(&mut self, receipt: &ReceiptDef) -> IrTypeDef {
        IrTypeDef {
            name: receipt.name.clone(),
            type_id: receipt.type_id.as_ref().map(|type_id| type_id.value.clone()),
            default_hash_type: receipt.default_hash_type.as_ref().map(|hash_type| hash_type.value.clone()),
            capacity_floor_shannons: receipt.capacity_floor.as_ref().map(|floor| floor.shannons),
            kind: IrTypeKind::Receipt,
            fields: self.layout_fields(&receipt.fields),
            capabilities: receipt.capabilities.clone(),
            value_abilities: Vec::new(),
            claim_output: receipt.claim_output.as_ref().map(|ty| self.convert_type(ty)),
            flow_states: self.flow_states.get(&receipt.name).cloned(),
            flow_state_field: self.flow_state_fields.get(&receipt.name).cloned(),
            flow_rules: self.flow_rules.get(&receipt.name).cloned().unwrap_or_default(),
            flow_initial_state: self.flow_initial_states.get(&receipt.name).cloned(),
            flow_terminal_states: self.flow_terminal_states.get(&receipt.name).cloned().unwrap_or_default(),
            flow_terminal_discharge: self
                .flow_terminal_states
                .contains_key(&receipt.name)
                .then(|| "terminal-by-output-state".to_string()),
            flow_state_model: self.flow_state_models.get(&receipt.name).cloned(),
            flow_audit_warnings: self.flow_audit_warnings.get(&receipt.name).cloned().unwrap_or_default(),
            validity_predicates: self.type_validity.get(&receipt.name).cloned().unwrap_or_default(),
            identity: Self::lower_identity_policy(&receipt.identity),
        }
    }

    pub(super) fn gen_struct(&mut self, struct_def: &StructDef) -> IrTypeDef {
        IrTypeDef {
            name: struct_def.name.clone(),
            type_id: struct_def.type_id.as_ref().map(|type_id| type_id.value.clone()),
            default_hash_type: struct_def.default_hash_type.as_ref().map(|hash_type| hash_type.value.clone()),
            capacity_floor_shannons: struct_def.capacity_floor.as_ref().map(|floor| floor.shannons),
            kind: IrTypeKind::Struct,
            fields: self.layout_fields(&struct_def.fields),
            capabilities: Vec::new(),
            value_abilities: struct_def.abilities.clone(),
            claim_output: None,
            flow_states: self.flow_states.get(&struct_def.name).cloned(),
            flow_state_field: self.flow_state_fields.get(&struct_def.name).cloned(),
            flow_rules: self.flow_rules.get(&struct_def.name).cloned().unwrap_or_default(),
            flow_initial_state: self.flow_initial_states.get(&struct_def.name).cloned(),
            flow_terminal_states: self.flow_terminal_states.get(&struct_def.name).cloned().unwrap_or_default(),
            flow_terminal_discharge: self
                .flow_terminal_states
                .contains_key(&struct_def.name)
                .then(|| "terminal-by-output-state".to_string()),
            flow_state_model: self.flow_state_models.get(&struct_def.name).cloned(),
            flow_audit_warnings: self.flow_audit_warnings.get(&struct_def.name).cloned().unwrap_or_default(),
            validity_predicates: self.type_validity.get(&struct_def.name).cloned().unwrap_or_default(),
            identity: IrIdentityPolicy::None,
        }
    }

    pub(super) fn gen_invariant(&self, invariant: &InvariantDef) -> IrInvariant {
        IrInvariant {
            name: invariant.name.clone(),
            trigger: invariant.trigger.clone(),
            scope: invariant.scope.clone(),
            reads: invariant.reads.clone(),
            aggregates: invariant
                .aggregates
                .iter()
                .map(|aggregate| IrAggregateInvariant {
                    kind: aggregate.kind,
                    target: aggregate.target.clone(),
                    scope: aggregate.scope.clone(),
                    argument: aggregate.argument.clone(),
                    relation: aggregate.relation,
                    rhs: aggregate.rhs.clone(),
                    span: aggregate.span,
                })
                .collect(),
            quantifiers: invariant.quantifiers.clone(),
            assert_count: invariant.asserts.len(),
            span: invariant.span,
        }
    }

    pub(super) fn gen_function(&mut self, function: &FnDef) -> IrFunction {
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
        let return_type = function.return_type.as_ref().map(|ty| self.convert_type(ty));
        let (params, mut body) =
            self.lower_signature_and_body(&function.params, &[], &function.body, return_type.clone(), &HashSet::new());
        body.cell_bindings = bindings::resolve(&params, &body, bindings::BindingContext::Helper, &self.type_kinds);
        let effects = EffectFacts::empty();
        IrFunction { name: function.name.clone(), params, return_type, body, effects }
    }

    fn layout_fields(&self, fields: &[Field]) -> Vec<IrField> {
        let mut next_offset = Some(0usize);
        fields
            .iter()
            .map(|field| {
                let ty = self.convert_type(&field.ty);
                let fixed_size = self.fixed_encoded_size(&ty);
                let offset = next_offset.unwrap_or(0);
                next_offset = next_offset.and_then(|current| fixed_size.and_then(|size| current.checked_add(size)));
                IrField { name: field.name.clone(), ty, offset, fixed_size }
            })
            .collect()
    }

    pub(super) fn register_enum_layouts(&mut self) {
        let names = self.enum_definitions.keys().cloned().collect::<Vec<_>>();
        let mut layouts = self.module.enum_layouts.clone();
        for name in names {
            let mut visiting = HashSet::new();
            match self.build_enum_layout(&name, &mut layouts, &mut visiting) {
                Some(layout) => {
                    self.module.enum_fixed_sizes.insert(name.clone(), layout.encoded_size);
                    layouts.insert(name, layout);
                }
                None => self.record_error(
                    format!(
                        "enum '{}' has no fixed-width runtime layout; generic, dynamic, and recursive payload ADTs are deferred",
                        name
                    ),
                    self.enum_definitions.get(&name).map_or(Span::default(), |definition| definition.span),
                ),
            }
        }
        self.module.enum_layouts = layouts;
    }

    fn build_enum_layout(
        &self,
        name: &str,
        layouts: &mut HashMap<String, IrEnumLayout>,
        visiting: &mut HashSet<String>,
    ) -> Option<IrEnumLayout> {
        if let Some(layout) = layouts.get(name) {
            return Some(layout.clone());
        }
        if !visiting.insert(name.to_string()) {
            return None;
        }
        let definition = self.enum_definitions.get(name)?;
        if definition.variants.is_empty() || definition.variants.len() > u8::MAX as usize + 1 {
            visiting.remove(name);
            return None;
        }

        let mut contains_linear_payload = false;
        let mut max_payload_width = 0usize;
        let mut variants = Vec::with_capacity(definition.variants.len());
        for (tag, variant) in definition.variants.iter().enumerate() {
            let mut next_offset = 1usize;
            let mut fields = Vec::with_capacity(variant.fields.len());
            for (index, field_ty) in variant.fields.iter().enumerate() {
                let ir_ty = self.convert_type(field_ty);
                let (width, linear) = self.enum_payload_runtime_layout(&ir_ty, layouts, visiting)?;
                fields.push(IrEnumFieldLayout { index, ty: ir_ty, offset: next_offset, width, linear });
                next_offset = next_offset.checked_add(width)?;
                contains_linear_payload |= linear;
            }
            let payload_width = next_offset.checked_sub(1)?;
            max_payload_width = max_payload_width.max(payload_width);
            variants.push(IrEnumVariantLayout { name: variant.name.clone(), tag: u8::try_from(tag).ok()?, payload_width, fields });
        }
        visiting.remove(name);
        Some(IrEnumLayout {
            name: name.to_string(),
            value_abilities: definition.abilities.clone(),
            tag_width: 1,
            encoded_size: 1usize.checked_add(max_payload_width)?,
            variants,
            contains_linear_payload,
        })
    }

    fn enum_payload_runtime_layout(
        &self,
        ty: &IrType,
        layouts: &mut HashMap<String, IrEnumLayout>,
        visiting: &mut HashSet<String>,
    ) -> Option<(usize, bool)> {
        match ty {
            IrType::Commitment(_) => Some((32, false)),
            IrType::Opening(_) | IrType::OpenHandle(_) | IrType::WitnessBytes(_) | IrType::Selection(_) | IrType::Applied(_) => None,
            IrType::U8 | IrType::Bool => Some((1, false)),
            IrType::U16 => Some((2, false)),
            IrType::U32 | IrType::I32 => Some((4, false)),
            IrType::U64 => Some((8, false)),
            IrType::U128 => Some((16, false)),
            IrType::Address | IrType::Hash => Some((32, false)),
            IrType::Unit => Some((0, false)),
            IrType::Array(inner, len) => {
                let (width, linear) = self.enum_payload_runtime_layout(inner, layouts, visiting)?;
                Some((width.checked_mul(*len)?, linear))
            }
            IrType::Tuple(items) => items.iter().try_fold((0usize, false), |(total, any_linear), item| {
                let (width, linear) = self.enum_payload_runtime_layout(item, layouts, visiting)?;
                Some((total.checked_add(width)?, any_linear || linear))
            }),
            IrType::Named(name) => {
                let base_name = name.as_str();
                if name.contains(['<', '>']) {
                    return None;
                }
                if matches!(self.type_kinds.get(base_name), Some(IrTypeKind::Resource | IrTypeKind::Shared | IrTypeKind::Receipt)) {
                    return Some((8, true));
                }
                if self.enum_definitions.contains_key(base_name) || layouts.contains_key(base_name) {
                    let layout = self.build_enum_layout(base_name, layouts, visiting)?;
                    let result = (layout.encoded_size, layout.contains_linear_payload);
                    layouts.insert(base_name.to_string(), layout);
                    return Some(result);
                }
                if !visiting.insert(base_name.to_string()) {
                    return None;
                }
                let result = self.type_fields.get(base_name)?.values().try_fold((0usize, false), |(total, any_linear), field_ty| {
                    let (width, linear) = self.enum_payload_runtime_layout(field_ty, layouts, visiting)?;
                    Some((total.checked_add(width)?, any_linear || linear))
                });
                visiting.remove(base_name);
                result
            }
            IrType::Context(_) | IrType::Vector(_) | IrType::Bounded(_) | IrType::Nested(_) | IrType::Ref(_) | IrType::MutRef(_) => {
                None
            }
        }
    }

    pub(super) fn fixed_encoded_size(&self, ty: &IrType) -> Option<usize> {
        self.fixed_encoded_size_with_seen(ty, &mut HashSet::new())
    }

    fn fixed_encoded_size_with_seen(&self, ty: &IrType, seen: &mut HashSet<String>) -> Option<usize> {
        match ty {
            IrType::Commitment(_) => Some(32),
            IrType::Opening(value) => self.fixed_encoded_size_with_seen(value, seen),
            IrType::Applied(_) | IrType::OpenHandle(_) | IrType::WitnessBytes(_) | IrType::Selection(_) => None,
            IrType::U8 | IrType::Bool => Some(1),
            IrType::U16 => Some(2),
            IrType::U32 => Some(4),
            IrType::I32 => Some(4),
            IrType::U64 => Some(8),
            IrType::U128 => Some(16),
            IrType::Address | IrType::Hash => Some(32),
            IrType::Array(inner, len) => {
                self.fixed_encoded_size_with_seen(inner, seen).and_then(|inner_size| inner_size.checked_mul(*len))
            }
            IrType::Tuple(items) => items
                .iter()
                .try_fold(0usize, |acc, item| self.fixed_encoded_size_with_seen(item, seen).and_then(|size| acc.checked_add(size))),
            IrType::Unit => Some(0),
            IrType::Named(name) => {
                let base_name = name.as_str();
                if let Some(size) = self.module.enum_fixed_sizes.get(base_name).copied() {
                    return Some(size);
                }
                if !seen.insert(base_name.to_string()) {
                    return None;
                }
                let size = self.type_fields.get(base_name).and_then(|fields| {
                    fields.values().try_fold(0usize, |acc, field_ty| {
                        self.fixed_encoded_size_with_seen(field_ty, seen).map(|field_size| acc + field_size)
                    })
                });
                seen.remove(base_name);
                size
            }
            IrType::Context(_) | IrType::Vector(_) | IrType::Bounded(_) | IrType::Nested(_) | IrType::Ref(_) | IrType::MutRef(_) => {
                None
            }
        }
    }
}
