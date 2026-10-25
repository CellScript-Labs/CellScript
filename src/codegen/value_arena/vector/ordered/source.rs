//! Source operations over disjoint invocation-local arena storage. Read-only
//! helpers retain the caller's arena and use separate callee copy scratch.
use super::*;
use crate::ast::{CollectionsDef, Type, ValueCollectionKind};
use crate::ir::allocated::{AllocatedOperation as Op, IrAllocatedValue};
use std::collections::HashSet;

impl CodeGenerator {
    fn allocated_ir_width(&self, ty: &IrType, visiting: &mut HashSet<String>) -> Option<usize> {
        if !matches!(ty, IrType::Named(_)) {
            return fixed_byte_width(ty, type_static_length(ty)).filter(|width| (1..=64).contains(width));
        }
        let IrType::Named(name) = ty else { unreachable!() };
        if self.cell_type_names.contains(name)
            || self.enum_layouts.contains_key(name)
            || visiting.len() >= 32
            || !visiting.insert(name.clone())
        {
            return None;
        }
        let size = self.type_layouts.get(name).and_then(|fields| {
            fields.values().try_fold(0usize, |size, field| size.checked_add(self.allocated_ir_width(&field.ty, visiting)?))
        });
        visiting.remove(name);
        size.filter(|size| (1..=64).contains(size) && self.type_fixed_sizes.get(name) == Some(size))
    }

    fn allocated_value_width(&self, ty: &Type) -> Result<usize> {
        self.allocated_ir_width(&crate::ir::ast_type_to_ir(ty), &mut HashSet::new())
            .ok_or_else(|| CompileError::without_span("allocated operation requires an admitted fixed non-Cell value layout"))
    }

    fn allocated_member_width(&self, kind: &ValueCollectionKind) -> Result<usize> {
        match kind {
            ValueCollectionKind::Vec(item) | ValueCollectionKind::OrderedSet(item) => self.allocated_value_width(item),
            ValueCollectionKind::OrderedMap { key, value } => {
                Ok(self.allocated_value_width(key)? + self.allocated_value_width(value)?)
            }
        }
    }

    fn allocated_vector_layout(&self, offset: usize, declaration: &CollectionsDef, index: usize) -> Result<Vector> {
        let member =
            declaration.members.get(index).ok_or_else(|| CompileError::without_span("allocated member ordinal is invalid"))?;
        let arena = Arena::new(offset, declaration.bytes as usize, declaration.allocations as usize)?;
        Vector::new(
            arena,
            offset + STATE + declaration.bytes as usize + index * DESCRIPTOR,
            self.allocated_member_width(&member.kind)?,
            member.items as usize,
        )
    }
    pub(in crate::codegen) fn reserve_allocated_owner_frames(&mut self, body: &IrBody, next: &mut usize) -> Result<()> {
        self.allocated_owner_frames.clear();
        self.allocated_owner_extents.clear();
        self.allocated_borrow_scratch = None;
        for instruction in body.blocks.iter().flat_map(|block| &block.instructions) {
            if let IrInstruction::AllocatedValue(op) = instruction
                && op.operation == Op::New
            {
                crate::allocated_values::validate_resolved_declaration(&op.declaration, |ty| {
                    self.allocated_value_width(ty).ok().map(|width| width as u32)
                })?;
                let dest =
                    op.dest.as_ref().ok_or_else(|| CompileError::without_span("allocated construction needs an owner destination"))?;
                *next = align_up(*next, 16);
                self.allocated_owner_frames.insert(dest.id, *next);
                self.allocated_owner_extents
                    .insert(dest.id, STATE + op.declaration.bytes as usize + DESCRIPTOR * op.declaration.members.len());
                *next += STATE + op.declaration.bytes as usize + DESCRIPTOR * op.declaration.members.len() + 128;
                if *next > 1024 * 1024 {
                    return Err(CompileError::without_span("allocated owners exceed the 1 MiB invocation frame bound"));
                }
            }
        }
        // Moves and lexical aliases share storage, never copy descriptors. The
        // type checker rejects ambiguous reassignment and escaping aliases.
        for _ in 0..=body.blocks.iter().map(|block| block.instructions.len()).sum::<usize>() {
            let mut changed = false;
            for instruction in body.blocks.iter().flat_map(|block| &block.instructions) {
                if let IrInstruction::Move { dest, src: IrOperand::Var(src) }
                | IrInstruction::Unary { dest, operand: IrOperand::Var(src), op: UnaryOp::Ref } = instruction
                    && let Some(offset) = self.allocated_owner_frames.get(&src.id).copied()
                    && !self.allocated_owner_frames.contains_key(&dest.id)
                {
                    self.allocated_owner_frames.insert(dest.id, offset);
                    self.allocated_owner_extents.insert(dest.id, self.allocated_owner_extents[&src.id]);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        if body.blocks.iter().flat_map(|block| &block.instructions).any(|instruction| {
            matches!(instruction, IrInstruction::AllocatedValue(op)
                if matches!(&op.owner, Some(IrOperand::Var(owner)) if !self.allocated_owner_frames.contains_key(&owner.id)))
        }) {
            *next = align_up(*next, 16);
            self.allocated_borrow_scratch = Some(*next);
            *next += 128;
            if *next > 1024 * 1024 {
                return Err(CompileError::without_span("allocated helper scratch exceeds the 1 MiB invocation frame bound"));
            }
        }
        Ok(())
    }

    fn allocated_copy_argument(&mut self, operand: &IrOperand, width: usize, output: usize) -> Result<()> {
        if width <= 8 && fixed_scalar_operand_width(operand).is_some() {
            self.emit_operand_to_register("t1", operand);
            for index in 0..width {
                self.emit_stack_store_byte("t1", output + index);
                if index + 1 < width {
                    self.emit("srli t1, t1, 8");
                }
            }
        } else {
            let source = self
                .expected_fixed_byte_source(operand, width)
                .ok_or_else(|| CompileError::without_span("allocated fixed-byte argument has no checked copy source"))?;
            self.emit_prepare_fixed_byte_source(&source, width, "allocated value copy");
            for index in 0..width {
                self.emit_fixed_byte_source_byte_to("t1", "t6", &source, index);
                self.emit_stack_store_byte("t1", output + index);
            }
        }
        Ok(())
    }

    fn allocated_copy_result(&mut self, dest: &IrVar, input: usize, width: usize) -> Result<()> {
        if width <= 8 && is_fixed_scalar_ir_type(&dest.ty) {
            let marker = self.fresh_label(&format!("allocated_result_{}_{}_{}", dest.id, input, width));
            self.emit_label(&format!("{marker}_begin"));
            self.emit_sp_addi("t4", input);
            self.emit_unaligned_scalar_load("t4", "t0", "t2", 0, width);
            if dest.ty == IrType::I32 {
                self.emit("slli t0, t0, 32");
                self.emit("srai t0, t0, 32");
            }
            self.emit_stack_store("t0", self.scalar_slot_offset(dest.id));
            self.emit_label(&format!("{marker}_end"));
        } else {
            let output = self
                .fixed_byte_local_offsets
                .get(&dest.id)
                .or_else(|| self.u128_value_offsets.get(&dest.id))
                .copied()
                .ok_or_else(|| CompileError::without_span("allocated result has no fixed-byte destination"))?;
            let marker = self.fresh_label(&format!("allocated_result_{}_{}_{}_{}", dest.id, input, width, output));
            self.emit_label(&format!("{marker}_begin"));
            for index in 0..width {
                self.emit_stack_load_byte("t0", input + index);
                self.emit_stack_store_byte("t0", output + index);
            }
            self.emit_sp_addi("t0", output);
            self.emit_stack_store("t0", self.scalar_slot_offset(dest.id));
            self.emit_label(&format!("{marker}_end"));
        }
        Ok(())
    }

    fn allocated_index(&mut self, owner: &IrVar, member: usize, operand: &IrOperand) {
        let marker = self.fresh_label(&format!("allocated_index_{}_{member}", owner.id));
        self.emit_label(&format!("{marker}_begin"));
        self.emit_operand_to_register("a1", operand);
        self.emit_label(&format!("{marker}_end"));
    }

    pub(in crate::codegen) fn emit_allocated_value(&mut self, op: &IrAllocatedValue) -> Result<()> {
        let owner = if op.operation == Op::New {
            op.dest.as_ref()
        } else {
            op.owner.as_ref().and_then(|operand| if let IrOperand::Var(var) = operand { Some(var) } else { None })
        }
        .ok_or_else(|| CompileError::without_span("allocated operation has no named owner"))?;
        let local = self.allocated_owner_frames.get(&owner.id).copied();
        let scratch = local
            .map(|offset| offset + STATE + op.declaration.bytes as usize + DESCRIPTOR * op.declaration.members.len())
            .or(self.allocated_borrow_scratch)
            .ok_or_else(|| CompileError::without_span("allocated operation has no private scratch"))?;
        let failure = self.fresh_label("allocated_write_bounds");
        let done = self.fresh_label("allocated_write_done");
        self.allocated_write_guard =
            Some(super::super::super::WriteGuard { start: local.unwrap_or(scratch), end: scratch + 128, failure: failure.clone() });
        let result = self.emit_allocated_value_inner(op);
        self.allocated_write_guard = None;
        result?;
        self.emit(format!("j {done}"));
        self.emit_label(&failure);
        self.emit_process_failure(CellScriptRuntimeError::CollectionBoundsInvalid);
        self.emit_label(&done);
        Ok(())
    }

    fn emit_allocated_value_inner(&mut self, op: &IrAllocatedValue) -> Result<()> {
        let owner = if op.operation == Op::New {
            op.dest.as_ref()
        } else {
            op.owner.as_ref().and_then(|owner| match owner {
                IrOperand::Var(var) => Some(var),
                _ => None,
            })
        }
        .ok_or_else(|| CompileError::without_span("allocated operation has no named owner"))?;
        let declaration = &op.declaration;
        let local = self.allocated_owner_frames.get(&owner.id).copied();
        let scratch = if let Some(offset) = local {
            offset + STATE + declaration.bytes as usize + DESCRIPTOR * declaration.members.len()
        } else {
            if !matches!(&owner.ty, IrType::Ref(inner) if **inner == IrType::Named(declaration.name.clone()))
                || !matches!(
                    op.operation,
                    Op::Len | Op::Capacity | Op::IsEmpty | Op::Get | Op::GetOr | Op::Contains | Op::EntryKey | Op::EntryValue
                )
            {
                return Err(CompileError::without_span("allocated helper operations require a read-only owner reference"));
            }
            let size_slot = self
                .schema_pointer_size_offsets
                .get(&owner.id)
                .copied()
                .ok_or_else(|| CompileError::without_span("allocated helper reference has no retained arena extent"))?;
            let marker = self.fresh_label(&format!("allocated_borrowed_extent_{}_{size_slot}_0", self.scalar_slot_offset(owner.id)));
            self.emit_label(&format!("{marker}_begin"));
            self.emit_stack_load("t0", size_slot);
            self.emit(format!("li t1, {}", STATE + declaration.bytes as usize + DESCRIPTOR * declaration.members.len()));
            let valid = self.fresh_label("allocated_borrow_extent");
            self.emit(format!("beq t0, t1, {valid}"));
            self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
            self.emit_label(&valid);
            self.emit_label(&format!("{marker}_end"));
            self.allocated_borrow_scratch.ok_or_else(|| CompileError::without_span("allocated helper copy scratch is missing"))?
        };
        let result = scratch + 64;
        if op.operation == Op::New {
            let offset = local.ok_or_else(|| CompileError::without_span("allocated constructor has no owned frame"))?;
            let begin = self.fresh_label(&format!("allocated_init_{}_begin", owner.id));
            let end = self.fresh_label(&format!("allocated_init_{}_end", owner.id));
            self.emit_label(&begin);
            self.emit_value_arena_init(Arena::new(offset, declaration.bytes as usize, declaration.allocations as usize)?);
            for index in 0..declaration.members.len() {
                self.emit_value_vector_init(self.allocated_vector_layout(offset, declaration, index)?);
            }
            self.emit_sp_addi("t0", offset);
            self.emit_stack_store("t0", self.scalar_slot_offset(owner.id));
            self.emit_label(&end);
            return Ok(());
        }
        if op.operation == Op::Drop {
            let offset = local.ok_or_else(|| CompileError::without_span("allocated drop requires an owned frame"))?;
            for index in 0..declaration.members.len() {
                self.emit_value_vector_drop(self.allocated_vector_layout(offset, declaration, index)?);
            }
            return Ok(());
        }
        let index = op.member.ok_or_else(|| CompileError::without_span("allocated member operation needs a member"))?;
        let member = &declaration.members[index];
        let vector = if let Some(offset) = local {
            self.allocated_vector_layout(offset, declaration, index)?
        } else {
            let arena =
                Arena::borrowed(self.scalar_slot_offset(owner.id), declaration.bytes as usize, declaration.allocations as usize)?;
            Vector::new(
                arena,
                STATE + declaration.bytes as usize + index * DESCRIPTOR,
                self.allocated_member_width(&member.kind)?,
                member.items as usize,
            )?
        };
        let arg = |index: usize| op.args.get(index).ok_or_else(|| CompileError::without_span("allocated operation argument missing"));
        let dest = || op.dest.as_ref().ok_or_else(|| CompileError::without_span("allocated result destination missing"));
        if matches!(op.operation, Op::EntryKey | Op::EntryValue) {
            self.allocated_index(owner, index, arg(0)?);
            self.emit_value_vector_get(vector, result)?;
            let (skip, count) = match (&member.kind, op.operation) {
                (ValueCollectionKind::OrderedMap { key, value }, Op::EntryValue) => {
                    (self.allocated_value_width(key)?, self.allocated_value_width(value)?)
                }
                (ValueCollectionKind::OrderedMap { key, .. }, _) => (0, self.allocated_value_width(key)?),
                _ => (0, vector.width),
            };
            return self.allocated_copy_result(dest()?, result + skip, count);
        }
        match op.operation {
            Op::Len | Op::Capacity | Op::IsEmpty => {
                let marker = if vector.arena.pointer_slot.is_some() {
                    self.begin_value_element_marker(
                        vector,
                        match op.operation {
                            Op::Len => "len",
                            Op::Capacity => "capacity",
                            _ => "empty",
                        },
                    )
                } else {
                    None
                };
                self.emit_value_vector_check(vector);
                self.emit(match op.operation {
                    Op::Len => "mv a0, t1",
                    Op::Capacity => "mv a0, t2",
                    _ => "seqz a0, t1",
                });
                self.end_value_element_marker(marker);
            }
            Op::Reserve => {
                self.allocated_index(owner, index, arg(0)?);
                self.emit_value_vector_reserve(vector);
            }
            _ => match &member.kind {
                ValueCollectionKind::Vec(_) => match op.operation {
                    Op::Push => {
                        self.allocated_copy_argument(arg(0)?, vector.width, scratch)?;
                        self.emit_value_vector_push(vector, scratch)?;
                    }
                    Op::Insert | Op::Set => {
                        self.allocated_copy_argument(arg(1)?, vector.width, scratch)?;
                        self.allocated_index(owner, index, arg(0)?);
                        if op.operation == Op::Insert {
                            self.emit_value_vector_insert(vector, scratch)?;
                        } else {
                            self.emit_value_vector_set(vector, scratch)?;
                        }
                    }
                    Op::Get | Op::Remove | Op::Pop => {
                        let marker = if op.operation == Op::Pop {
                            let marker = self.begin_value_element_marker(vector, "pop");
                            self.emit_value_vector_check(vector);
                            self.emit("addi a1, t1, -1");
                            marker
                        } else {
                            self.allocated_index(owner, index, arg(0)?);
                            if op.operation == Op::Remove {
                                self.begin_value_element_marker(vector, "take")
                            } else {
                                None
                            }
                        };
                        self.emit_value_vector_get(vector, result)?;
                        if op.operation != Op::Get {
                            self.emit_value_vector_remove(vector);
                        }
                        self.end_value_element_marker(marker);
                        return self.allocated_copy_result(dest()?, result, vector.width);
                    }
                    _ => return Err(CompileError::without_span("unsupported allocated Vec operation")),
                },
                ValueCollectionKind::OrderedMap { key, .. } | ValueCollectionKind::OrderedSet(key) => {
                    let key_width = self.allocated_value_width(key)?;
                    let order = if matches!(key, Type::U8 | Type::U16 | Type::U32 | Type::U64) {
                        KeyOrder::UnsignedLittleEndian
                    } else {
                        KeyOrder::LexicographicBytes
                    };
                    let ordered = Ordered::new(vector, key_width, order)?;
                    if op.operation == Op::LoadSorted {
                        let input = arg(0)?;
                        let IrOperand::Var(input_var) = input else {
                            return Err(CompileError::without_span("sorted construction requires typed array storage"));
                        };
                        let IrType::Array(_, count) = &input_var.ty else {
                            return Err(CompileError::without_span("sorted construction requires a fixed input array"));
                        };
                        if *count > 0 {
                            let width = vector.width * count;
                            let source = self
                                .expected_fixed_byte_source(input, width)
                                .ok_or_else(|| CompileError::without_span("sorted input has no exact bounded copy source"))?;
                            self.emit_prepare_fixed_byte_source(&source, width, "sorted collection input");
                            if !self.emit_fixed_byte_source_pointer_or_const_to("a4", &source) {
                                return Err(CompileError::without_span("sorted input has no addressable storage"));
                            }
                            self.emit_stack_store("a4", scratch);
                        }
                        return self.emit_value_ordered_load_sorted(ordered, scratch, *count);
                    }
                    self.allocated_copy_argument(arg(0)?, key_width, scratch)?;
                    match op.operation {
                        Op::Insert => {
                            if vector.width > key_width {
                                self.allocated_copy_argument(arg(1)?, vector.width - key_width, scratch + key_width)?;
                            }
                            self.emit_value_ordered_put(ordered, scratch)?;
                        }
                        Op::Remove => self.emit_value_ordered_remove(ordered, scratch)?,
                        Op::Contains => self.emit_value_ordered_find(ordered, scratch)?,
                        Op::Get | Op::GetOr => {
                            if op.operation == Op::GetOr {
                                self.allocated_copy_argument(arg(1)?, vector.width - key_width, result + key_width)?;
                            }
                            let marker =
                                if op.operation == Op::Get { self.begin_value_element_marker(vector, "required") } else { None };
                            self.emit_value_ordered_get(ordered, scratch, result)?;
                            if op.operation == Op::Get {
                                let found = self.fresh_label("allocated_map_found");
                                self.emit(format!("bnez a0, {found}"));
                                self.emit_fail(CellScriptRuntimeError::CollectionBoundsInvalid);
                                self.emit_label(&found);
                            }
                            self.end_value_element_marker(marker);
                            return self.allocated_copy_result(dest()?, result + key_width, vector.width - key_width);
                        }
                        _ => return Err(CompileError::without_span("unsupported allocated ordered operation")),
                    }
                }
            },
        }
        if let Some(dest) = &op.dest {
            self.emit_stack_store("a0", self.scalar_slot_offset(dest.id));
        }
        Ok(())
    }
}
