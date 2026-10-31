//! Rebuild local value aggregates after projected assignment. Every intermediate
//! has separate storage; Cell resource transitions use their existing lowering.
use super::*;

enum Projection<'a> {
    Field(&'a str),
    Index(&'a Expr),
}

#[derive(Clone, PartialEq, Eq)]
enum Part {
    Field(String),
    Index(usize),
}

impl IrGenerator<'_> {
    pub(super) fn lower_local_projection_assign(
        &mut self,
        assign: &AssignExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<LoweredExpr> {
        let mut target = assign.target.as_ref();
        let mut path = Vec::new();
        let root = loop {
            match target {
                Expr::FieldAccess(field) => {
                    path.push(Projection::Field(&field.field));
                    target = &field.expr;
                }
                Expr::Index(index) => {
                    path.push(Projection::Index(&index.index));
                    target = &index.expr;
                }
                Expr::Identifier(name) => break vars.get(name)?.clone(),
                _ => return None,
            }
        };
        if path.is_empty() || !self.is_mutable_value_aggregate(&root) {
            return None;
        }
        path.reverse();
        let mut leaf_type = root.ty.clone();
        for projection in &path {
            let next = match projection {
                Projection::Field(field) => self.lookup_field_ir_type(&leaf_type, field),
                Projection::Index(_) => match &leaf_type {
                    IrType::Array(element, _) => Some((**element).clone()),
                    _ => None,
                },
            };
            let Some(next) = next else {
                self.record_error("local aggregate assignment has no declared projection type", assign.span);
                return Some(LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(current) });
            };
            leaf_type = next;
        }
        // Preserve assignment evaluation order and type literals against the
        // destination (e.g. u8), not the default u64 type.
        let value = self.lower_expr_with_expected_type(&assign.value, &leaf_type, current, blocks, vars);
        let Some(mut active) = value.current else { return Some(value) };
        let mut parents = Vec::with_capacity(path.len());
        let mut base = root.clone();
        for (ordinal, projection) in path.iter().enumerate() {
            let part = match projection {
                Projection::Field(field) => Part::Field((*field).to_string()),
                Projection::Index(expr) => {
                    let index = self.lower_expr(expr, active, blocks, vars);
                    let Some(next) = index.current else { return Some(index) };
                    active = next;
                    let Some(index) = const_usize_operand(&index.operand) else {
                        self.record_error("local fixed-array assignment requires a compile-time constant index", assign.span);
                        return Some(LoweredExpr { operand: value.operand, current: Some(active) });
                    };
                    let IrType::Array(_, len) = &base.ty else { unreachable!("checked projection type") };
                    if index >= *len {
                        self.record_error(
                            format!("array index {index} is out of bounds for local fixed array of length {len}"),
                            assign.span,
                        );
                        return Some(LoweredExpr { operand: value.operand, current: Some(active) });
                    }
                    Part::Index(index)
                }
            };
            parents.push((base.clone(), part.clone()));
            if ordinal + 1 < path.len() || matches!(assign.op, AssignOp::AddAssign) {
                let Some(projected) = self.project_local_value(&base, &part, active, blocks) else {
                    self.record_error("local aggregate assignment cannot read its projection", assign.span);
                    return Some(LoweredExpr { operand: value.operand, current: Some(active) });
                };
                base = projected;
            }
        }
        let assigned = self.new_var("assigned_value", leaf_type);
        let instruction = match assign.op {
            AssignOp::Assign => IrInstruction::Move { dest: assigned.clone(), src: value.operand },
            AssignOp::AddAssign => {
                IrInstruction::Binary { dest: assigned.clone(), op: BinaryOp::Add, left: IrOperand::Var(base), right: value.operand }
            }
        };
        self.block_mut(blocks, active).instructions.push(instruction);
        let mut replacement = assigned.clone();
        for (parent, selected) in parents.into_iter().rev() {
            let Some(parts) = self.local_value_parts(&parent.ty) else {
                self.record_error("local aggregate assignment requires a fixed record, tuple or array", assign.span);
                return Some(LoweredExpr { operand: IrOperand::Var(assigned), current: Some(active) });
            };
            let mut fields = Vec::with_capacity(parts.len());
            for part in parts {
                let value = if part == selected {
                    replacement.clone()
                } else {
                    let Some(value) = self.project_local_value(&parent, &part, active, blocks) else {
                        self.record_error("local aggregate assignment cannot read an unchanged component", assign.span);
                        return Some(LoweredExpr { operand: IrOperand::Var(assigned), current: Some(active) });
                    };
                    value
                };
                fields.push(IrOperand::Var(value));
            }
            replacement = self.new_var("updated_value", parent.ty);
            self.block_mut(blocks, active).instructions.push(IrInstruction::Tuple { dest: replacement.clone(), fields });
        }
        self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: root.clone(), src: IrOperand::Var(replacement) });
        self.invalidate_local_value_projections(root.id);
        Some(LoweredExpr { operand: IrOperand::Var(assigned), current: Some(active) })
    }

    fn local_value_parts(&self, ty: &IrType) -> Option<Vec<Part>> {
        match ty {
            IrType::Named(name) if self.type_kinds.get(name) == Some(&IrTypeKind::Struct) => {
                Some(self.type_field_order.get(name)?.iter().cloned().map(Part::Field).collect())
            }
            IrType::Tuple(fields) => Some((0..fields.len()).map(|index| Part::Field(index.to_string())).collect()),
            IrType::Array(_, len) => Some((0..*len).map(Part::Index).collect()),
            _ => None,
        }
    }

    fn project_local_value(&mut self, base: &IrVar, part: &Part, current: BlockId, blocks: &mut [IrBlock]) -> Option<IrVar> {
        let (ty, field, index) = match part {
            Part::Field(name) => (self.lookup_field_ir_type(&base.ty, name)?, Some(name), None),
            Part::Index(index) => match &base.ty {
                IrType::Array(element, len) if index < len => ((**element).clone(), None, Some(*index)),
                _ => return None,
            },
        };
        let dest = self.new_var("value_component", ty);
        let instruction = if let Some(field) = field {
            IrInstruction::FieldAccess { dest: dest.clone(), obj: IrOperand::Var(base.clone()), field: field.clone() }
        } else {
            IrInstruction::Index {
                dest: dest.clone(),
                arr: IrOperand::Var(base.clone()),
                idx: IrOperand::Const(IrConst::U64(index? as u64)),
            }
        };
        self.block_mut(blocks, current).instructions.push(instruction);
        Some(dest)
    }

    pub(super) fn invalidate_local_value_projections(&mut self, id: usize) {
        self.aggregate_fields.remove(&id);
        self.aggregate_elements.remove(&id);
        self.schema_field_roots.remove(&id);
        self.aggregate_field_contexts.remove(&id);
        self.mutated_fields.remove(&id);
        self.mutated_field_transitions.remove(&id);
    }
}
