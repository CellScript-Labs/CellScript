//! Structured invocation-local collection operations. The declaration remains
//! attached until independent allocation contracts are emitted and checked.
use super::*;

pub use cellscript_artifact_checker::allocated_values::Operation as AllocatedOperation;

#[derive(Debug, Clone)]
pub struct IrAllocatedValue {
    pub dest: Option<IrVar>,
    pub owner: Option<IrOperand>,
    pub declaration: CollectionsDef,
    pub member: Option<usize>,
    pub operation: AllocatedOperation,
    pub args: Vec<IrOperand>,
}

impl IrAllocatedValue {
    pub(crate) fn contract(&self) -> cellscript_artifact_checker::allocated_values::Contract {
        use cellscript_artifact_checker::allocated_values::{Contract, Member, MemberKind, PROFILE};
        // Keep this representation available to the metadata-only WASM build;
        // native typed-semantic emission is intentionally absent there.
        let ty = |ty: &Type| match ty {
            Type::Hash => "hash".into(),
            Type::Address => "address".into(),
            _ => ir_type_display(&ast_type_to_ir(ty)),
        };
        Contract {
            profile: PROFILE.into(),
            owner: self.declaration.name.clone(),
            bytes: self.declaration.bytes,
            allocations: self.declaration.allocations,
            members: self
                .declaration
                .members
                .iter()
                .map(|member| Member {
                    name: member.name.clone(),
                    items: member.items,
                    shape: match &member.kind {
                        ValueCollectionKind::Vec(item) => MemberKind::Vec { item: ty(item) },
                        ValueCollectionKind::OrderedSet(key) => MemberKind::OrderedSet { key: ty(key) },
                        ValueCollectionKind::OrderedMap { key, value } => MemberKind::OrderedMap { key: ty(key), value: ty(value) },
                    },
                })
                .collect(),
        }
    }
}

impl IrGenerator {
    pub(super) fn lower_allocated_for(
        &mut self,
        stmt: &ForStmt,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        return_type: Option<&IrType>,
    ) -> Option<Option<BlockId>> {
        let (owner, declaration, member) = self.allocated_member(&stmt.iterable, vars)?;
        let index = self.new_var("allocated_index", IrType::U64);
        let length = self.new_var("allocated_length", IrType::U64);
        self.block_mut(blocks, current)
            .instructions
            .push(IrInstruction::Move { dest: index.clone(), src: IrOperand::Const(IrConst::U64(0)) });
        self.block_mut(blocks, current).instructions.push(IrInstruction::AllocatedValue(IrAllocatedValue {
            dest: Some(length.clone()),
            owner: Some(IrOperand::Var(owner.clone())),
            declaration: declaration.clone(),
            member: Some(member),
            operation: AllocatedOperation::Len,
            args: vec![],
        }));
        let cond = self.push_block(blocks);
        let body = self.push_block(blocks);
        let step = self.push_block(blocks);
        let exit = self.push_block(blocks);
        self.block_mut(blocks, current).terminator = IrTerminator::Jump(cond);
        let predicate = self.new_var("allocated_more", IrType::Bool);
        self.block_mut(blocks, cond).instructions.push(IrInstruction::Binary {
            dest: predicate.clone(),
            op: BinaryOp::Lt,
            left: IrOperand::Var(index.clone()),
            right: IrOperand::Var(length),
        });
        self.block_mut(blocks, cond).terminator =
            IrTerminator::Branch { cond: IrOperand::Var(predicate), then_block: body, else_block: exit };
        let types = match &declaration.members[member].kind {
            ValueCollectionKind::Vec(item) | ValueCollectionKind::OrderedSet(item) => vec![ast_type_to_ir(item)],
            ValueCollectionKind::OrderedMap { key, value } => vec![ast_type_to_ir(key), ast_type_to_ir(value)],
        };
        let mut fields = Vec::new();
        for (field, ty) in types.iter().enumerate() {
            let dest = self.new_var("allocated_item", ty.clone());
            self.block_mut(blocks, body).instructions.push(IrInstruction::AllocatedValue(IrAllocatedValue {
                dest: Some(dest.clone()),
                owner: Some(IrOperand::Var(owner.clone())),
                declaration: declaration.clone(),
                member: Some(member),
                operation: if field == 0 { AllocatedOperation::EntryKey } else { AllocatedOperation::EntryValue },
                args: vec![IrOperand::Var(index.clone())],
            }));
            fields.push(dest);
        }
        let value = if fields.len() == 1 {
            fields[0].clone()
        } else {
            let tuple = self.new_var("allocated_pair", IrType::Tuple(types));
            self.aggregate_fields
                .insert(tuple.id, fields.iter().enumerate().map(|(index, var)| (index.to_string(), var.clone())).collect());
            self.block_mut(blocks, body)
                .instructions
                .push(IrInstruction::Tuple { dest: tuple.clone(), fields: fields.into_iter().map(IrOperand::Var).collect() });
            tuple
        };
        let mut body_vars = vars.clone();
        self.bind_pattern(&stmt.pattern, IrOperand::Var(value), self.block_mut(blocks, body), &mut body_vars);
        self.branch_depth += 1;
        self.branch_epoch += 1;
        self.loop_targets.push(LoopTarget { label: stmt.label.clone(), break_block: exit, continue_block: step });
        let body_exit = self.lower_stmts(&stmt.body, body, blocks, &mut body_vars, return_type, false);
        self.loop_targets.pop();
        self.branch_depth -= 1;
        if let Some(body_exit) = body_exit {
            self.block_mut(blocks, body_exit).terminator = IrTerminator::Jump(step);
        }
        let next = self.new_var("allocated_next", IrType::U64);
        self.block_mut(blocks, step).instructions.push(IrInstruction::Binary {
            dest: next.clone(),
            op: BinaryOp::Add,
            left: IrOperand::Var(index.clone()),
            right: IrOperand::Const(IrConst::U64(1)),
        });
        self.block_mut(blocks, step).instructions.push(IrInstruction::Move { dest: index, src: IrOperand::Var(next) });
        self.block_mut(blocks, step).terminator = IrTerminator::Jump(cond);
        Some(Some(exit))
    }

    fn allocated_receiver(&self, expr: &Expr, vars: &HashMap<String, IrVar>) -> Option<(IrVar, CollectionsDef)> {
        let Expr::Identifier(name) = expr else { return None };
        let owner = vars.get(name)?;
        let ty = match &owner.ty {
            IrType::Ref(inner) => inner.as_ref(),
            ty => ty,
        };
        let IrType::Named(name) = ty else { return None };
        Some((owner.clone(), self.collection_pools.get(name)?.clone()))
    }

    fn allocated_member(&self, expr: &Expr, vars: &HashMap<String, IrVar>) -> Option<(IrVar, CollectionsDef, usize)> {
        let Expr::FieldAccess(field) = expr else { return None };
        let (owner, declaration) = self.allocated_receiver(&field.expr, vars)?;
        let member = declaration.members.iter().position(|member| member.name == field.field)?;
        Some((owner, declaration, member))
    }

    pub(super) fn lower_allocated_call(
        &mut self,
        call: &CallExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<LoweredExpr> {
        if let Expr::Identifier(path) = call.func.as_ref()
            && let Some((owner, "new")) = path.rsplit_once("::")
            && let Some(declaration) = self.collection_pools.get(owner).cloned()
        {
            let dest = self.new_var("owner", IrType::Named(owner.into()));
            self.block_mut(blocks, current).instructions.push(IrInstruction::AllocatedValue(IrAllocatedValue {
                dest: Some(dest.clone()),
                owner: None,
                declaration,
                member: None,
                operation: AllocatedOperation::New,
                args: vec![],
            }));
            return Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) });
        }
        let Expr::FieldAccess(method) = call.func.as_ref() else { return None };
        if let Some((owner, declaration)) = self.allocated_receiver(&method.expr, vars) {
            if method.field != "drop" {
                return None;
            }
            self.block_mut(blocks, current).instructions.push(IrInstruction::AllocatedValue(IrAllocatedValue {
                dest: None,
                owner: Some(IrOperand::Var(owner)),
                declaration,
                member: None,
                operation: AllocatedOperation::Drop,
                args: vec![],
            }));
            return Some(LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(current) });
        }
        let (owner, declaration, member) = self.allocated_member(&method.expr, vars)?;
        let operation = match method.field.as_str() {
            "len" => AllocatedOperation::Len,
            "capacity" => AllocatedOperation::Capacity,
            "is_empty" => AllocatedOperation::IsEmpty,
            "reserve" => AllocatedOperation::Reserve,
            "load_sorted" => AllocatedOperation::LoadSorted,
            "push" => AllocatedOperation::Push,
            "insert" => AllocatedOperation::Insert,
            "set" => AllocatedOperation::Set,
            "get" => AllocatedOperation::Get,
            "get_or" => AllocatedOperation::GetOr,
            "remove" => AllocatedOperation::Remove,
            "pop" => AllocatedOperation::Pop,
            "contains" | "contains_key" => AllocatedOperation::Contains,
            _ => return None,
        };
        Some(self.lower_allocated_operation(owner, declaration, member, operation, &call.args, current, blocks, vars))
    }

    pub(super) fn lower_allocated_index(
        &mut self,
        index: &IndexExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<LoweredExpr> {
        let (owner, declaration, member) = self.allocated_member(&index.expr, vars)?;
        Some(self.lower_allocated_operation(
            owner,
            declaration,
            member,
            AllocatedOperation::Get,
            std::slice::from_ref(index.index.as_ref()),
            current,
            blocks,
            vars,
        ))
    }

    #[allow(clippy::too_many_arguments)] // One checked declaration/member operation, not positional source API.
    fn lower_allocated_operation(
        &mut self,
        owner: IrVar,
        declaration: CollectionsDef,
        member: usize,
        operation: AllocatedOperation,
        args: &[Expr],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        use AllocatedOperation as Op;
        let ty = match (&declaration.members[member].kind, operation) {
            (_, Op::Len | Op::Capacity) => IrType::U64,
            (_, Op::IsEmpty | Op::Contains) => IrType::Bool,
            (ValueCollectionKind::Vec(item), Op::Get | Op::Remove | Op::Pop) => ast_type_to_ir(item),
            (ValueCollectionKind::OrderedMap { value, .. }, Op::Get | Op::GetOr) => ast_type_to_ir(value),
            (ValueCollectionKind::OrderedMap { .. } | ValueCollectionKind::OrderedSet(_), Op::Insert | Op::Remove) => IrType::Bool,
            _ => IrType::Unit,
        };
        let mut active = current;
        let mut operands = Vec::with_capacity(args.len());
        for (index, arg) in args.iter().enumerate() {
            if operation == Op::LoadSorted {
                let lowered = if let Expr::Array(items) = arg {
                    let element = match &declaration.members[member].kind {
                        ValueCollectionKind::OrderedMap { key, value } => {
                            IrType::Tuple(vec![ast_type_to_ir(key), ast_type_to_ir(value)])
                        }
                        ValueCollectionKind::OrderedSet(key) => ast_type_to_ir(key),
                        _ => unreachable!("type checking restricts sorted construction"),
                    };
                    self.lower_expr_with_expected_type(arg, &IrType::Array(Box::new(element), items.len()), active, blocks, vars)
                } else {
                    self.lower_expr(arg, active, blocks, vars)
                };
                let Some(next) = lowered.current else { return lowered };
                active = next;
                operands.push(lowered.operand);
                continue;
            }
            let expected = match (&declaration.members[member].kind, operation, index) {
                (_, Op::Reserve, _) => IrType::U64,
                (ValueCollectionKind::Vec(item), Op::Push, _) | (ValueCollectionKind::Vec(item), Op::Insert | Op::Set, 1) => {
                    ast_type_to_ir(item)
                }
                (ValueCollectionKind::Vec(_), _, _) => IrType::U64,
                (ValueCollectionKind::OrderedMap { key, .. } | ValueCollectionKind::OrderedSet(key), _, 0) => ast_type_to_ir(key),
                (ValueCollectionKind::OrderedMap { value, .. }, _, _) => ast_type_to_ir(value),
                _ => IrType::U64,
            };
            let lowered = self.lower_expr_with_expected_type(arg, &expected, active, blocks, vars);
            let Some(next) = lowered.current else { return lowered };
            active = next;
            let operand =
                self.materialize_operand_with_type("allocated_argument", lowered.operand, expected, self.block_mut(blocks, active));
            operands.push(IrOperand::Var(operand));
        }
        let dest = (ty != IrType::Unit).then(|| self.new_var("collection_result", ty));
        let operand = dest.clone().map(IrOperand::Var).unwrap_or(IrOperand::Const(IrConst::Unit));
        self.block_mut(blocks, active).instructions.push(IrInstruction::AllocatedValue(IrAllocatedValue {
            dest,
            owner: Some(IrOperand::Var(owner)),
            declaration,
            member: Some(member),
            operation,
            args: operands,
        }));
        LoweredExpr { operand, current: Some(active) }
    }
}
