use super::{BlockId, IrBlock, IrGenerator, IrInstruction, IrOperand, IrType, IrTypeKind, IrVar};

impl IrGenerator<'_> {
    pub(super) fn is_mutable_value_aggregate(&self, var: &IrVar) -> bool {
        self.mutable_local_ids.contains(&var.id)
            && match &var.ty {
                IrType::Array(_, _) | IrType::Tuple(_) => true,
                IrType::Named(name) => self.type_kinds.get(name) == Some(&IrTypeKind::Struct),
                _ => false,
            }
    }

    pub(super) fn enter_branch_scope(&mut self) {
        self.branch_epoch += 1;
        self.branch_path.push(self.branch_epoch);
    }

    pub(super) fn leave_branch_scope(&mut self) {
        self.branch_path.pop();
    }

    /// A cached value can be reused only inside the lexical branch ancestry
    /// that defined it. This also excludes sibling descendants and reads after
    /// a join from reusing one branch's private initializer locals.
    pub(super) fn aggregate_field_read_dominates(&self, field_var_id: usize) -> bool {
        self.aggregate_field_contexts.get(&field_var_id).is_none_or(|path| self.branch_path.starts_with(path))
    }

    pub(super) fn materialize_schema_field(
        &mut self,
        base_var: &IrVar,
        field: &str,
        current: BlockId,
        blocks: &mut [IrBlock],
    ) -> Option<IrVar> {
        let field_ty = self.lookup_field_ir_type(&base_var.ty, field)?;
        let field_var = self.new_var(format!("{}_{}", base_var.name, field), field_ty);
        self.block_mut(blocks, current).instructions.push(IrInstruction::FieldAccess {
            dest: field_var.clone(),
            obj: IrOperand::Var(base_var.clone()),
            field: field.to_string(),
        });
        self.aggregate_fields.entry(base_var.id).or_default().insert(field.to_string(), field_var.clone());
        self.schema_field_roots.insert(field_var.id, (base_var.id, field.to_string()));
        self.aggregate_field_contexts.insert(field_var.id, self.branch_path.clone());
        Some(field_var)
    }

    pub(super) fn copy_aggregate_metadata(&mut self, source: &IrOperand, dest_id: usize) {
        let IrOperand::Var(source_var) = source else {
            return;
        };
        if self.is_mutable_value_aggregate(source_var) {
            return;
        }
        if let Some(fields) = self.aggregate_fields.get(&source_var.id).cloned() {
            self.aggregate_fields.insert(dest_id, fields);
        }
        if let Some(root) = self.schema_field_roots.get(&source_var.id).cloned() {
            self.schema_field_roots.insert(dest_id, root);
        }
        if let Some(context) = self.aggregate_field_contexts.get(&source_var.id).cloned() {
            self.aggregate_field_contexts.insert(dest_id, context);
        }
        if let Some(elements) = self.aggregate_elements.get(&source_var.id).cloned() {
            self.aggregate_elements.insert(dest_id, elements);
        }
    }
}
