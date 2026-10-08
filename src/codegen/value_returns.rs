//! Fixed ordinary struct results use an explicit caller-owned output buffer.
//! The hidden pointer follows all ordinary machine arguments (including their
//! lengths/type hashes), so normal outgoing-stack ownership applies. No pointer
//! into a callee frame or register tuple escapes as a nominal struct value.

use super::*;

impl CodeGenerator {
    /// A returned struct forwarded through a named local still owns exact-width
    /// storage. Require every store to have matching proven nominal storage;
    /// never replace a borrowed/schema parameter's actual length with its type.
    pub(super) fn set_fixed_struct_local_widths(&mut self, body: &IrBody) {
        loop {
            let mut named = HashMap::<String, Option<(IrType, usize)>>::new();
            for instruction in body.blocks.iter().flat_map(|block| &block.instructions) {
                if let IrInstruction::StoreVar { name, src } = instruction {
                    let proven = match src {
                        IrOperand::Var(var) => self
                            .local_schema_value_widths
                            .get(&var.id)
                            .copied()
                            .filter(|width| self.fixed_struct_return_width(&var.ty) == Some(*width))
                            .map(|width| (var.ty.clone(), width)),
                        _ => None,
                    };
                    named
                        .entry(name.clone())
                        .and_modify(|current| {
                            if *current != proven {
                                *current = None;
                            }
                        })
                        .or_insert(proven);
                }
            }
            let mut changed = false;
            for instruction in body.blocks.iter().flat_map(|block| &block.instructions) {
                let proven = match instruction {
                    IrInstruction::LoadVar { dest, name } => named
                        .get(name)
                        .and_then(|proven| proven.as_ref())
                        .filter(|(ty, _)| ty == &dest.ty)
                        .map(|(_, width)| (dest.id, *width)),
                    IrInstruction::Move { dest, src: IrOperand::Var(src) } if dest.ty == src.ty => {
                        self.local_schema_value_widths.get(&src.id).map(|width| (dest.id, *width))
                    }
                    _ => None,
                };
                if let Some((id, width)) = proven {
                    changed |= self.local_schema_value_widths.insert(id, width) != Some(width);
                }
            }
            if !changed {
                break;
            }
        }
    }

    pub(super) fn fixed_struct_return_width(&self, ty: &IrType) -> Option<usize> {
        let IrType::Named(name) = ty else { return None };
        if self.cell_type_names.contains(name) || self.enum_fixed_sizes.contains_key(name) || !self.type_layouts.contains_key(name) {
            return None;
        }
        self.type_fixed_sizes.get(name).copied()
    }

    pub(super) fn current_return_buffer_bytes(&self) -> Option<usize> {
        self.current_function.as_ref().and_then(|name| self.callable_abis.get(name)).and_then(|abi| abi.return_buffer_bytes)
    }

    pub(super) fn emit_value_return(&mut self, operand: &IrOperand) -> Result<bool> {
        let Some(width) = self.current_return_buffer_bytes() else { return Ok(false) };
        let offset = self
            .return_buffer_pointer_offset
            .ok_or_else(|| CompileError::without_span("fixed struct return lacks its saved output pointer"))?;
        self.emit(format!("# cellscript abi: fixed struct return copies {width} bytes into caller-owned storage"));
        let marker = self.fresh_label(&format!("result_v1_copy_{offset}_{width}"));
        if width != 0 {
            let source = self
                .expected_fixed_byte_source(operand, width)
                .ok_or_else(|| CompileError::without_span("fixed struct return source has no bounded storage"))?;
            self.emit_prepare_fixed_byte_source(&source, width, "fixed struct return");
            let IrOperand::Var(value) = operand else {
                return Err(CompileError::without_span("fixed struct return requires a typed storage local"));
            };
            let (base, field_offset) = match &source {
                ExpectedFixedByteSource::PointerBytes { var_id, .. }
                | ExpectedFixedByteSource::ParamBytes { var_id, .. }
                | ExpectedFixedByteSource::LoadedBytes { var_id, .. } => (*var_id, 0),
                ExpectedFixedByteSource::SchemaField(field) if self.type_fixed_sizes.contains_key(&field.type_name) => {
                    (field.obj_var_id, field.layout.offset)
                }
                _ => return Err(CompileError::without_span("fixed struct return source needs fixed pointer storage")),
            };
            let source_marker = self.fresh_label(&format!("result_v1_source_{}_{}_{}", value.id, base, field_offset));
            self.emit_label(&source_marker);
            self.emit_stack_load("a0", self.scalar_slot_offset(base));
            if field_offset != 0 {
                self.emit_large_addi("a0", "a0", field_offset as i64);
            }
            self.emit_label(&format!("{source_marker}_end"));
            self.emit_label(&marker);
            // a0 is live while the saved destination is loaded; memcpy then
            // owns a0/a1/a2.
            self.emit_stack_load_with_avoid("a1", offset, &["a0"]);
            self.emit(format!("li a2, {width}"));
            self.emit("call __cellscript_memcpy_fixed");
        } else {
            self.emit_label(&marker);
        }
        self.emit("li a0, 0");
        self.emit_epilogue();
        self.emit_label(&format!("{marker}_end"));
        Ok(true)
    }
}
