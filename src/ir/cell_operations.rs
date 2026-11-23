use super::{
    BlockId, CreatePattern, IrBlock, IrConst, IrDestructionPolicy, IrGenerator, IrIdentityPolicy, IrInstruction, IrOperand, IrType,
    IrValidityEvidence, IrVar, LoweredExpr,
};
use crate::ast::{
    ClaimExpr, ConsumeExpr, CreateExpr, CreateUniqueExpr, DestroyExpr, DestructionPolicy, Expr, IdentityPolicy, ReadRefExpr,
    ReplaceUniqueExpr, RequireExpr, SettleExpr,
};
use std::collections::HashMap;

impl IrGenerator<'_> {
    pub(super) fn lower_create_expr(
        &mut self,
        create: &CreateExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let dest = if let Some(target) = &create.target {
            vars.get(target).cloned().unwrap_or_else(|| self.new_var(target.clone(), IrType::Named(create.ty.clone())))
        } else {
            self.new_var(format!("create_{}", create.ty), IrType::Named(create.ty.clone()))
        };
        let mut active = current;
        let mut lowered_fields = Vec::with_capacity(create.fields.len());
        let mut field_vars = HashMap::new();

        for (field_name, field_expr) in &create.fields {
            let expected_ty = self.type_fields.get(&create.ty).and_then(|fields| fields.get(field_name)).cloned();
            let lowered = if let Some(state_operand) = self.lower_flow_state_initializer(&create.ty, field_name, field_expr) {
                LoweredExpr { operand: state_operand, current: Some(active) }
            } else if let Some(expected_ty) = expected_ty {
                self.lower_expr_with_expected_type(field_expr, &expected_ty, active, blocks, vars)
            } else {
                self.lower_expr(field_expr, active, blocks, vars)
            };
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;
            lowered_fields.push((field_name.clone(), lowered.operand.clone()));

            let field_ty = match &lowered.operand {
                IrOperand::Var(var) => var.ty.clone(),
                IrOperand::Const(value) => self.const_type(value),
            };
            let field_var = self.new_var(format!("{}_{}", create.ty, field_name), field_ty);
            self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: field_var.clone(), src: lowered.operand });
            field_vars.insert(field_name.clone(), field_var);
        }

        let lowered_lock = if let Some(lock_expr) = &create.lock {
            let lowered = self.lower_expr(lock_expr, active, blocks, vars);
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;
            Some(lowered.operand)
        } else {
            None
        };

        let Some(active) = self.lower_type_validity_checks(&create.ty, &field_vars, active, blocks, vars) else {
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: None };
        };

        let pattern = CreatePattern {
            operation: if create.target.is_some() { "output".to_string() } else { "create".to_string() },
            ty: create.ty.clone(),
            binding: dest.name.clone(),
            fields: lowered_fields,
            lock: lowered_lock,
            identity: IrIdentityPolicy::None,
        };
        self.block_mut(blocks, active).instructions.push(IrInstruction::Create { dest: dest.clone(), pattern });
        for field in field_vars.values() {
            self.aggregate_field_contexts.insert(field.id, self.branch_path.clone());
        }
        self.aggregate_fields.insert(dest.id, field_vars);
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
    }

    pub(super) fn lower_type_validity_checks(
        &mut self,
        type_name: &str,
        field_vars: &HashMap<String, IrVar>,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &HashMap<String, IrVar>,
    ) -> Option<BlockId> {
        let predicates = self.type_validity.get(type_name).cloned().unwrap_or_default();
        let mut active = current;
        for predicate in predicates.into_iter().filter(|predicate| predicate.evidence == IrValidityEvidence::CheckedRuntime) {
            let mut validity_vars = vars.clone();
            validity_vars.extend(field_vars.iter().map(|(name, var)| (name.clone(), var.clone())));
            let require = RequireExpr {
                condition: Box::new(predicate.expression),
                message: predicate
                    .message
                    .map(Box::new)
                    .or_else(|| Some(Box::new(Expr::String(format!("type validity failed: {type_name}"))))),
                span: predicate.span,
            };
            let lowered = self.lower_require_expr(&require, active, blocks, &mut validity_vars);
            active = lowered.current?;
        }
        Some(active)
    }

    pub(super) fn lower_flow_state_initializer(&self, type_name: &str, field_name: &str, expr: &Expr) -> Option<IrOperand> {
        if self.flow_state_fields.get(type_name).is_none_or(|state_field| state_field != field_name) {
            return None;
        }
        let Expr::Identifier(state_name) = expr else {
            return None;
        };
        self.lower_flow_state_operand(type_name, state_name)
    }

    pub(super) fn lower_consume_expr(
        &mut self,
        consume: &ConsumeExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered = self.lower_expr(&consume.expr, current, blocks, vars);
        let Some(active) = lowered.current else {
            return lowered;
        };
        self.block_mut(blocks, active).instructions.push(IrInstruction::Consume { operand: lowered.operand.clone() });
        LoweredExpr { operand: lowered.operand, current: Some(active) }
    }

    pub(super) fn lower_destroy_expr(
        &mut self,
        destroy: &DestroyExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered = self.lower_expr(&destroy.expr, current, blocks, vars);
        let Some(active) = lowered.current else {
            return lowered;
        };
        self.block_mut(blocks, active).instructions.push(IrInstruction::Destroy {
            operand: lowered.operand.clone(),
            policy: Self::lower_destruction_policy(&destroy.policy),
        });
        LoweredExpr { operand: lowered.operand, current: Some(active) }
    }

    fn lower_destruction_policy(policy: &DestructionPolicy) -> IrDestructionPolicy {
        match policy {
            DestructionPolicy::Default => IrDestructionPolicy::Default,
            DestructionPolicy::SingletonType => IrDestructionPolicy::SingletonType,
            DestructionPolicy::Unique { identity } => IrDestructionPolicy::Unique { identity: identity.clone() },
            DestructionPolicy::Instance { identity_field } => IrDestructionPolicy::Instance { identity_field: identity_field.clone() },
            DestructionPolicy::BurnAmount { field } => IrDestructionPolicy::BurnAmount { field: field.clone() },
        }
    }

    pub(super) fn lower_identity_policy(policy: &IdentityPolicy) -> IrIdentityPolicy {
        match policy {
            IdentityPolicy::None => IrIdentityPolicy::None,
            IdentityPolicy::CkbTypeId => IrIdentityPolicy::CkbTypeId,
            IdentityPolicy::Field(path) => IrIdentityPolicy::Field(path.clone()),
            IdentityPolicy::ScriptArgs => IrIdentityPolicy::ScriptArgs,
            IdentityPolicy::SingletonType => IrIdentityPolicy::SingletonType,
        }
    }

    pub(super) fn lower_create_unique_expr(
        &mut self,
        cu: &CreateUniqueExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let dest = if let Some(target) = &cu.target {
            vars.get(target).cloned().unwrap_or_else(|| self.new_var(target.clone(), IrType::Named(cu.ty.clone())))
        } else {
            self.new_var(format!("create_unique_{}", cu.ty), IrType::Named(cu.ty.clone()))
        };
        let mut active = current;
        let mut lowered_fields = Vec::with_capacity(cu.fields.len());
        let mut field_vars = HashMap::new();

        for (field_name, field_expr) in &cu.fields {
            let expected_ty = self.type_fields.get(&cu.ty).and_then(|fields| fields.get(field_name)).cloned();
            let lowered = if let Some(expected_ty) = expected_ty {
                self.lower_expr_with_expected_type(field_expr, &expected_ty, active, blocks, vars)
            } else {
                self.lower_expr(field_expr, active, blocks, vars)
            };
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;
            lowered_fields.push((field_name.clone(), lowered.operand.clone()));

            let field_ty = match &lowered.operand {
                IrOperand::Var(var) => var.ty.clone(),
                IrOperand::Const(value) => self.const_type(value),
            };
            let field_var = self.new_var(format!("{}_{}", cu.ty, field_name), field_ty);
            self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: field_var.clone(), src: lowered.operand });
            field_vars.insert(field_name.clone(), field_var);
        }

        let lowered_lock = if let Some(lock_expr) = &cu.lock {
            let lowered = self.lower_expr(lock_expr, active, blocks, vars);
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;
            Some(lowered.operand)
        } else {
            None
        };

        let Some(active) = self.lower_type_validity_checks(&cu.ty, &field_vars, active, blocks, vars) else {
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: None };
        };

        let identity = Self::lower_identity_policy(&cu.identity);
        let pattern = CreatePattern {
            operation: "create_unique".to_string(),
            ty: cu.ty.clone(),
            binding: dest.name.clone(),
            fields: lowered_fields,
            lock: lowered_lock,
            identity: identity.clone(),
        };
        self.block_mut(blocks, active).instructions.push(IrInstruction::CreateUnique { dest: dest.clone(), pattern, identity });
        for field in field_vars.values() {
            self.aggregate_field_contexts.insert(field.id, self.branch_path.clone());
        }
        self.aggregate_fields.insert(dest.id, field_vars);
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
    }

    pub(super) fn lower_replace_unique_expr(
        &mut self,
        ru: &ReplaceUniqueExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        // Lower the input cell expression
        let lowered_input = self.lower_expr(&ru.expr, current, blocks, vars);
        let Some(active) = lowered_input.current else {
            return lowered_input;
        };

        // Lower the replacement output fields
        let dest_ty = self.operand_type(&lowered_input.operand);
        let dest = self.new_var(format!("replace_unique_{}", ru.ty), dest_ty);
        let mut active = active;
        let mut lowered_fields = Vec::with_capacity(ru.fields.len());
        let mut field_vars = HashMap::new();

        for (field_name, field_expr) in &ru.fields {
            let expected_ty = self.type_fields.get(&ru.ty).and_then(|fields| fields.get(field_name)).cloned();
            let lowered = if let Some(expected_ty) = expected_ty {
                self.lower_expr_with_expected_type(field_expr, &expected_ty, active, blocks, vars)
            } else {
                self.lower_expr(field_expr, active, blocks, vars)
            };
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;
            lowered_fields.push((field_name.clone(), lowered.operand.clone()));

            let field_ty = match &lowered.operand {
                IrOperand::Var(var) => var.ty.clone(),
                IrOperand::Const(value) => self.const_type(value),
            };
            let field_var = self.new_var(format!("{}_{}", ru.ty, field_name), field_ty);
            self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: field_var.clone(), src: lowered.operand });
            field_vars.insert(field_name.clone(), field_var);
        }

        let Some(active) = self.lower_type_validity_checks(&ru.ty, &field_vars, active, blocks, vars) else {
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: None };
        };

        let identity = Self::lower_identity_policy(&ru.identity);
        let pattern = CreatePattern {
            operation: "replace_unique".to_string(),
            ty: ru.ty.clone(),
            binding: dest.name.clone(),
            fields: lowered_fields,
            lock: None,
            identity: identity.clone(),
        };
        self.block_mut(blocks, active).instructions.push(IrInstruction::ReplaceUnique {
            dest: dest.clone(),
            operand: lowered_input.operand,
            pattern,
            identity,
        });
        for field in field_vars.values() {
            self.aggregate_field_contexts.insert(field.id, self.branch_path.clone());
        }
        self.aggregate_fields.insert(dest.id, field_vars);
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
    }

    pub(super) fn lower_read_ref_expr(&mut self, read_ref: &ReadRefExpr, current: BlockId, blocks: &mut [IrBlock]) -> LoweredExpr {
        let dest = self.new_var(format!("read_ref_{}", read_ref.ty), IrType::Ref(Box::new(IrType::Named(read_ref.ty.clone()))));
        self.block_mut(blocks, current).instructions.push(IrInstruction::ReadRef { dest: dest.clone(), ty: read_ref.ty.clone() });
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) }
    }

    pub(super) fn lower_claim_expr(
        &mut self,
        claim: &ClaimExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_receipt = self.lower_expr(&claim.receipt, current, blocks, vars);
        let Some(active) = lowered_receipt.current else {
            return lowered_receipt;
        };
        let dest_ty = self.claim_output_type_for_operand(&lowered_receipt.operand);
        let dest = self.new_var("claim_tmp", dest_ty);
        let claim_output_fields = self.materialize_matching_output_fields(&lowered_receipt.operand, &dest.ty, active, blocks);
        let validity_active = if let IrType::Named(type_name) = &dest.ty {
            self.lower_type_validity_checks(type_name, &claim_output_fields, active, blocks, vars)
        } else {
            Some(active)
        };
        let Some(active) = validity_active else {
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: None };
        };
        self.block_mut(blocks, active)
            .instructions
            .push(IrInstruction::Claim { dest: dest.clone(), receipt: lowered_receipt.operand });
        if !claim_output_fields.is_empty() {
            self.aggregate_fields.insert(dest.id, claim_output_fields);
        }
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
    }

    pub(super) fn lower_settle_expr(
        &mut self,
        settle: &SettleExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered = self.lower_expr(&settle.expr, current, blocks, vars);
        let Some(active) = lowered.current else {
            return lowered;
        };
        let dest_ty = self.operand_type(&lowered.operand);
        let dest = self.new_var("settle_tmp", dest_ty);
        let settle_output_fields = self.materialize_matching_output_fields(&lowered.operand, &dest.ty, active, blocks);
        let validity_active = if let IrType::Named(type_name) = &dest.ty {
            self.lower_type_validity_checks(type_name, &settle_output_fields, active, blocks, vars)
        } else {
            Some(active)
        };
        let Some(active) = validity_active else {
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: None };
        };
        self.block_mut(blocks, active).instructions.push(IrInstruction::Settle { dest: dest.clone(), operand: lowered.operand });
        if !settle_output_fields.is_empty() {
            self.aggregate_fields.insert(dest.id, settle_output_fields);
        }
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
    }
}
