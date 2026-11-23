use super::{
    direct_bounded_plan_field, is_verifier_coverable_output_field_type, type_hash_for_name, BlockId, CellPattern, CreatePattern,
    IrBlock, IrBody, IrBoundedCollectionOp, IrGenerator, IrIdentityPolicy, IrInstruction, IrOperand, IrParam, IrTerminator, IrType,
    IrTypeKind, IrVar, MutatePattern, WriteIntent, BOUNDED_CELL_RUNTIME_MAX_DATA_BYTES, BOUNDED_OUTPUT_PLAN_MAX_BYTES,
};
use crate::ast::{ParamSource, Type};
use std::collections::{BTreeSet, HashMap, HashSet};

impl IrGenerator<'_> {
    pub(super) fn bounded_consume_runtime_width(
        &self,
        element_type: &IrType,
        source: ParamSource,
        max_elements: usize,
    ) -> Option<usize> {
        if source != ParamSource::Input || !(1..=1024).contains(&max_elements) {
            return None;
        }
        let IrType::Named(element_name) = element_type else { return None };
        if self.type_kinds.get(element_name) != Some(&IrTypeKind::Resource) {
            return None;
        }
        let width = self.fixed_encoded_size(element_type)?;
        (width > 0 && width <= BOUNDED_CELL_RUNTIME_MAX_DATA_BYTES).then_some(width)
    }

    pub(super) fn bounded_create_runtime_width(&self, operation: &IrBoundedCollectionOp) -> Option<usize> {
        if operation.source != ParamSource::Witness || !(1..=1024).contains(&operation.max_elements) {
            return None;
        }
        let IrType::Named(element_name) = &operation.element_type else { return None };
        if self.type_kinds.get(element_name) != Some(&IrTypeKind::Struct) {
            return None;
        }
        let plan_width = self.fixed_encoded_size(&operation.element_type)?;
        let encoded_max = operation.max_elements.checked_mul(plan_width)?.checked_add(12)?;
        if plan_width == 0 || encoded_max > BOUNDED_OUTPUT_PLAN_MAX_BYTES {
            return None;
        }
        let output_type = operation.output_type.as_ref()?;
        if self.type_kinds.get(output_type) != Some(&IrTypeKind::Resource)
            || self
                .fixed_encoded_size(&IrType::Named(output_type.clone()))
                .is_none_or(|width| width == 0 || width > BOUNDED_CELL_RUNTIME_MAX_DATA_BYTES)
            || self.type_capacity_floors.get(output_type).copied().is_none_or(|floor| floor == 0)
            || self.type_identities.get(output_type) != Some(&IrIdentityPolicy::None)
        {
            return None;
        }
        let template = operation.create_template.as_ref()?;
        let required_fields = self.type_fields.get(output_type)?;
        let plan_fields = self.type_fields.get(element_name)?;
        if template.lock.is_none()
            || template.fields.len() != required_fields.len()
            || !required_fields.keys().all(|field| template.fields.iter().any(|(name, _)| name == field))
        {
            return None;
        }
        for (output_field, rendered) in &template.fields {
            let plan_field = direct_bounded_plan_field(rendered, &operation.binding)?;
            if required_fields.get(output_field) != plan_fields.get(plan_field) {
                return None;
            }
        }
        let lock_field = direct_bounded_plan_field(template.lock.as_deref()?, &operation.binding)?;
        let lock_ty = plan_fields.get(lock_field)?;
        if self.fixed_encoded_size(lock_ty) != Some(32) {
            return None;
        }
        Some(plan_width)
    }

    pub(super) fn collect_write_intents(create_set: &[CreatePattern], mutate_set: &[MutatePattern]) -> Vec<WriteIntent> {
        let create_intents = create_set.iter().enumerate().map(|(index, pattern)| WriteIntent {
            operation: pattern.operation.clone(),
            ty: pattern.ty.clone(),
            binding: pattern.binding.clone(),
            index,
            fields: pattern.fields.iter().map(|(field, _)| field.clone()).collect(),
        });
        let mutate_intents = mutate_set.iter().map(|pattern| WriteIntent {
            operation: pattern.operation.clone(),
            ty: pattern.ty.clone(),
            binding: pattern.binding.clone(),
            index: pattern.output_index,
            fields: pattern.fields.clone(),
        });

        create_intents.chain(mutate_intents).collect()
    }

    pub(super) fn collect_consume_patterns(
        &self,
        blocks: &[IrBlock],
        params: &[IrParam],
        core_input_bindings: &HashSet<String>,
    ) -> Vec<CellPattern> {
        let mut patterns = params
            .iter()
            .filter(|param| core_input_bindings.contains(param.name.as_str()))
            .filter_map(|param| self.cell_pattern_from_var(&param.binding, "input"))
            .collect::<Vec<_>>();
        for block in blocks {
            for instruction in &block.instructions {
                if let IrInstruction::Consume { operand } = instruction {
                    if let Some(pattern) = self.cell_pattern_from_operand(operand, "consume") {
                        patterns.push(pattern);
                    }
                } else if let IrInstruction::Transfer { operand, .. } = instruction {
                    if let Some(pattern) = self.cell_pattern_from_operand(operand, "transfer") {
                        patterns.push(pattern);
                    }
                } else if let IrInstruction::Destroy { operand, .. } = instruction {
                    if let Some(pattern) = self.cell_pattern_from_operand(operand, "destroy") {
                        patterns.push(pattern);
                    }
                } else if let IrInstruction::ReplaceUnique { operand, .. } = instruction {
                    if let Some(pattern) = self.cell_pattern_from_operand(operand, "replace_unique") {
                        patterns.push(pattern);
                    }
                } else if let IrInstruction::Claim { receipt, .. } = instruction {
                    if let Some(pattern) = self.cell_pattern_from_operand(receipt, "claim") {
                        patterns.push(pattern);
                    }
                } else if let IrInstruction::Settle { operand, .. } = instruction
                    && let Some(pattern) = self.cell_pattern_from_operand(operand, "settle")
                {
                    patterns.push(pattern);
                }
            }
        }
        patterns
    }

    pub(super) fn collect_read_ref_patterns(&self, blocks: &[IrBlock]) -> Vec<CellPattern> {
        let mut patterns = Vec::new();
        for block in blocks {
            for instruction in &block.instructions {
                if let IrInstruction::ReadRef { dest, ty } = instruction {
                    patterns.push(CellPattern {
                        operation: "read_ref".to_string(),
                        type_hash: Some(type_hash_for_name(ty)),
                        binding: dest.name.clone(),
                        fields: Vec::new(),
                    });
                }
            }
        }
        patterns
    }

    pub(super) fn collect_read_ref_param_patterns(&self, params: &[IrParam]) -> Vec<CellPattern> {
        params
            .iter()
            .filter(|param| param.is_read_ref)
            .filter_map(|param| {
                Self::named_type_name_from_ir_type(&param.ty).map(|type_name| CellPattern {
                    operation: "read_ref".to_string(),
                    type_hash: Some(type_hash_for_name(type_name)),
                    binding: param.name.clone(),
                    fields: Vec::new(),
                })
            })
            .collect()
    }

    pub(super) fn collect_create_patterns(&self, blocks: &[IrBlock], params: &[IrParam]) -> Vec<CreatePattern> {
        let output_bindings =
            params.iter().filter(|param| param.source == ParamSource::Output).map(|param| param.name.clone()).collect::<HashSet<_>>();
        let mut patterns = params
            .iter()
            .filter(|param| param.source == ParamSource::Output)
            .filter_map(|param| self.create_pattern_from_var(&param.binding, "output"))
            .collect::<Vec<_>>();
        for block in blocks {
            for instruction in &block.instructions {
                if let IrInstruction::Create { pattern, .. } = instruction {
                    if pattern.operation == "output" && output_bindings.contains(&pattern.binding) {
                        if let Some(existing) = patterns.iter_mut().find(|existing| existing.binding == pattern.binding) {
                            *existing = pattern.clone();
                        } else {
                            patterns.push(pattern.clone());
                        }
                    } else {
                        patterns.push(pattern.clone());
                    }
                } else if let IrInstruction::CreateUnique { pattern, .. } = instruction {
                    patterns.push(pattern.clone());
                } else if let IrInstruction::ReplaceUnique { pattern, .. } = instruction {
                    patterns.push(pattern.clone());
                } else if let IrInstruction::Transfer { dest, to, .. } = instruction {
                    if let Some(pattern) = self.create_pattern_from_var_with_lock(dest, "transfer", Some(to.clone())) {
                        patterns.push(pattern);
                    }
                } else if let IrInstruction::Claim { dest, .. } = instruction {
                    if let Some(pattern) = self.create_pattern_from_var(dest, "claim") {
                        patterns.push(pattern);
                    }
                } else if let IrInstruction::Settle { dest, .. } = instruction
                    && let Some(pattern) = self.create_pattern_from_var(dest, "settle")
                {
                    patterns.push(pattern);
                }
            }
        }
        patterns
    }

    pub(super) fn collect_mutate_param_patterns(
        &self,
        params: &[IrParam],
        consume_count: usize,
        create_count: usize,
    ) -> Vec<MutatePattern> {
        let mut patterns = Vec::new();
        for param in params {
            if !(param.is_mut || matches!(param.ty, IrType::MutRef(_))) {
                continue;
            }
            let Some(type_name) = Self::named_type_name_from_ir_type(&param.ty) else {
                continue;
            };
            if !matches!(self.type_kinds.get(type_name), Some(IrTypeKind::Resource | IrTypeKind::Shared | IrTypeKind::Receipt)) {
                continue;
            }
            let fields = self
                .mutated_fields
                .get(&param.binding.id)
                .map(|fields| fields.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            let mutated = fields.iter().cloned().collect::<BTreeSet<_>>();
            let preserved_fields = self
                .type_fields
                .get(type_name)
                .map(|fields| {
                    fields
                        .keys()
                        .filter(|field| !mutated.contains(*field))
                        .cloned()
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let transitions = self
                .mutated_field_transitions
                .get(&param.binding.id)
                .map(|fields| fields.values().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            let mutation_index = patterns.len();
            patterns.push(MutatePattern {
                operation: "mutate".to_string(),
                ty: type_name.to_string(),
                binding: param.name.clone(),
                fields,
                preserved_fields,
                transitions,
                input_index: consume_count + mutation_index,
                output_index: create_count + mutation_index,
                preserve_type_hash: true,
                preserve_lock_hash: true,
            });
        }
        patterns
    }

    fn cell_pattern_from_operand(&self, operand: &IrOperand, operation: &str) -> Option<CellPattern> {
        let IrOperand::Var(var) = operand else {
            return None;
        };
        self.cell_pattern_from_var(var, operation)
    }

    fn cell_pattern_from_var(&self, var: &IrVar, operation: &str) -> Option<CellPattern> {
        let type_name = match &var.ty {
            IrType::Named(name) => Some(name.as_str()),
            IrType::Ref(inner) | IrType::MutRef(inner) => match inner.as_ref() {
                IrType::Named(name) => Some(name.as_str()),
                _ => None,
            },
            _ => None,
        }?;
        Some(CellPattern {
            operation: operation.to_string(),
            type_hash: Some(type_hash_for_name(type_name)),
            binding: var.name.clone(),
            fields: Vec::new(),
        })
    }

    fn create_pattern_from_var(&self, var: &IrVar, operation: &str) -> Option<CreatePattern> {
        self.create_pattern_from_var_with_lock(var, operation, None)
    }

    fn create_pattern_from_var_with_lock(&self, var: &IrVar, operation: &str, lock: Option<IrOperand>) -> Option<CreatePattern> {
        let type_name = match &var.ty {
            IrType::Named(name) => Some(name.as_str()),
            IrType::Ref(inner) | IrType::MutRef(inner) => match inner.as_ref() {
                IrType::Named(name) => Some(name.as_str()),
                _ => None,
            },
            _ => None,
        }?;
        let fields = self
            .aggregate_fields
            .get(&var.id)
            .map(|field_vars| {
                let mut fields =
                    field_vars.iter().map(|(field, var)| (field.clone(), IrOperand::Var(var.clone()))).collect::<Vec<_>>();
                fields.sort_by(|(left, _), (right, _)| left.cmp(right));
                fields
            })
            .unwrap_or_default();
        Some(CreatePattern {
            operation: operation.to_string(),
            ty: type_name.to_string(),
            binding: var.name.clone(),
            fields,
            lock,
            identity: IrIdentityPolicy::None,
        })
    }

    pub(super) fn named_type_name_from_ir_type(ty: &IrType) -> Option<&str> {
        match ty {
            IrType::Named(name) => Some(name.as_str()),
            IrType::Ref(inner) | IrType::MutRef(inner) => Self::named_type_name_from_ir_type(inner),
            _ => None,
        }
    }

    pub(super) fn claim_output_type_for_operand(&self, operand: &IrOperand) -> IrType {
        let ty = self.operand_type(operand);
        Self::named_type_name_from_ir_type(&ty)
            .and_then(|name| self.receipt_claim_outputs.get(name))
            .and_then(Clone::clone)
            .unwrap_or(IrType::U64)
    }

    pub(super) fn materialize_matching_output_fields(
        &mut self,
        source: &IrOperand,
        output_ty: &IrType,
        active: BlockId,
        blocks: &mut [IrBlock],
    ) -> HashMap<String, IrVar> {
        let (IrOperand::Var(source_var), Some(output_type_name)) = (source, Self::named_type_name_from_ir_type(output_ty)) else {
            return HashMap::new();
        };
        let Some(output_fields) = self.type_fields.get(output_type_name).cloned() else {
            return HashMap::new();
        };

        let mut field_names = output_fields.keys().cloned().collect::<Vec<_>>();
        field_names.sort();
        let mut field_vars = HashMap::new();
        for field_name in field_names {
            let Some(output_field_ty) = output_fields.get(&field_name) else {
                continue;
            };
            let Some(source_field_ty) = self.lookup_field_ir_type(&source_var.ty, &field_name) else {
                continue;
            };
            if &source_field_ty != output_field_ty || !is_verifier_coverable_output_field_type(output_field_ty) {
                continue;
            }
            if let Some(field_var) = self.materialize_schema_field(source_var, &field_name, active, blocks) {
                field_vars.insert(field_name, field_var);
            }
        }
        field_vars
    }

    pub(super) fn named_type_name_from_ast_type(ty: &Type) -> Option<&str> {
        match ty {
            Type::Named(name) => Some(name.as_str()),
            Type::Ref(inner) | Type::MutRef(inner) => Self::named_type_name_from_ast_type(inner),
            _ => None,
        }
    }

    pub(super) fn infer_touches_shared(&self, params: &[IrParam], body: &IrBody) -> Vec<[u8; 32]> {
        let shared_hashes = self
            .type_kinds
            .iter()
            .filter_map(|(name, kind)| (*kind == IrTypeKind::Shared).then_some(type_hash_for_name(name)))
            .collect::<Vec<_>>();
        let mut hashes = Vec::new();
        for param in params {
            if let Some(type_name) = self.param_shared_type_name(param) {
                hashes.push(type_hash_for_name(type_name));
            }
        }
        for pattern in body.read_refs.iter().chain(body.consume_set.iter()) {
            if let Some(type_hash) = pattern.type_hash
                && shared_hashes.contains(&type_hash)
            {
                hashes.push(type_hash);
            }
        }
        for pattern in &body.create_set {
            if self.type_kinds.get(&pattern.ty) == Some(&IrTypeKind::Shared) {
                hashes.push(type_hash_for_name(&pattern.ty));
            }
        }
        for block in &body.blocks {
            for instruction in &block.instructions {
                if let IrInstruction::Call { dest: Some(dest), .. } = instruction {
                    self.collect_shared_type_hashes_from_type(&dest.ty, &mut hashes);
                }
            }
        }
        hashes.sort();
        hashes.dedup();
        hashes
    }

    fn param_shared_type_name<'a>(&self, param: &'a IrParam) -> Option<&'a str> {
        let type_name = Self::named_type_name_from_ir_type(&param.ty)?;
        (self.type_kinds.get(type_name) == Some(&IrTypeKind::Shared)).then_some(type_name)
    }

    fn collect_shared_type_hashes_from_type(&self, ty: &IrType, hashes: &mut Vec<[u8; 32]>) {
        match ty {
            IrType::Named(name) if self.type_kinds.get(name) == Some(&IrTypeKind::Shared) => {
                hashes.push(type_hash_for_name(name));
            }
            IrType::Ref(inner) | IrType::MutRef(inner) | IrType::Array(inner, _) => {
                self.collect_shared_type_hashes_from_type(inner, hashes);
            }
            IrType::Tuple(items) => {
                for item in items {
                    self.collect_shared_type_hashes_from_type(item, hashes);
                }
            }
            _ => {}
        }
    }

    pub(super) fn estimate_cycles(&self, body: &IrBody) -> u64 {
        let instruction_count = body.blocks.iter().map(|block| block.instructions.len() as u64).sum::<u64>();
        let branch_count =
            body.blocks.iter().filter(|block| matches!(block.terminator, IrTerminator::Jump(_) | IrTerminator::Branch { .. })).count()
                as u64;
        let cell_ops = (body.consume_set.len() + body.read_refs.len() + body.create_set.len()) as u64;
        (instruction_count * 8) + (branch_count * 4) + (cell_ops * 128) + 32
    }
}
