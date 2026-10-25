//! Typed, read-only field decoding through the existing transaction-view ABI.
use super::*;

impl CodeGenerator {
    pub(super) fn emit_cell_data_view_field(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> Result<bool> {
        let IrOperand::Var(view) = obj else { return Ok(false) };
        let IrType::Named(name) = &view.ty else { return Ok(false) };
        if field == "data" && crate::cell_data_view::transaction_schema(name).is_some() {
            self.emit_operand_to_register("t0", obj);
            self.emit_stack_store("t0", self.scalar_slot_offset(dest.id));
            return Ok(true);
        }
        let Some(schema) = crate::cell_data_view::schema(name) else { return Ok(false) };
        if let Some(table) = self.nested_cell_layout(schema)? {
            self.emit_nested_data_view_field(dest, obj, schema, field, &table)?;
            return Ok(true);
        }
        let layout = self
            .type_layouts
            .get(schema)
            .and_then(|fields| fields.get(field))
            .cloned()
            .ok_or_else(|| CompileError::without_span("Cell data field has no fixed schema layout"))?;
        let total = self
            .type_fixed_sizes
            .get(schema)
            .copied()
            .filter(|width| (1..=512).contains(width))
            .ok_or_else(|| CompileError::without_span("Cell data view requires a fixed 1–512 byte schema"))?;
        let width = layout_fixed_byte_width(&layout)
            .or_else(|| layout_fixed_scalar_width(&layout))
            .filter(|width| *width > 0 && layout.offset.checked_add(*width).is_some_and(|end| end <= total))
            .ok_or_else(|| CompileError::without_span("Cell data field exceeds its fixed schema"))?;
        let size = self.runtime_scratch_size_offset();
        let buffer = self.runtime_scratch_buffer_offset();
        let invalid = self.fresh_label("cell_data_view_invalid");
        let failed = self.fresh_label("cell_data_view_load_failed");
        let wrong_size = self.fresh_label("cell_data_view_size_failed");
        let done = self.fresh_label("cell_data_view_done");
        // Decoding leaves only t1=index and t2=source live. Stack helpers may
        // use t6 while preparing destinations; no argument aliases t6.
        self.emit_operand_to_register("a0", obj);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", CKB_SOURCE_CELL_DEP));
        self.emit(format!("beq t2, t0, {invalid}"));
        self.emit(format!("li t0, {}", CKB_SOURCE_HEADER_DEP));
        self.emit(format!("beq t2, t0, {invalid}"));
        self.emit("mv a3, t1");
        self.emit("mv a4, t2");
        self.emit(format!("li t0, {total}"));
        self.emit_stack_store("t0", size);
        self.emit_sp_addi("a0", buffer);
        self.emit_sp_addi("a1", size);
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_CELL_DATA));
        self.emit("ecall");
        self.emit(format!("bnez a0, {failed}"));
        self.emit_stack_load("t0", size);
        self.emit(format!("li t1, {total}"));
        self.emit(format!("bne t0, t1, {wrong_size}"));
        if width > 8 {
            let output = self
                .fixed_byte_local_offsets
                .get(&dest.id)
                .copied()
                .ok_or_else(|| CompileError::without_span("Cell data field has no fixed-byte result storage"))?;
            for byte in 0..width {
                self.emit_stack_load_byte("t0", buffer + layout.offset + byte);
                self.emit_stack_store_byte("t0", output + byte);
            }
            self.emit_sp_addi("t0", output);
        } else {
            self.emit("li t0, 0");
            for byte in 0..width {
                self.emit_stack_load_byte("t1", buffer + layout.offset + byte);
                if byte != 0 {
                    self.emit(format!("slli t1, t1, {}", byte * 8));
                }
                self.emit("or t0, t0, t1");
            }
        }
        self.emit_stack_store("t0", self.scalar_slot_offset(dest.id));
        self.emit(format!("j {done}"));
        self.emit_label(&invalid);
        self.emit_process_failure(CellScriptRuntimeError::CkbSourceViewInvalid);
        self.emit_label(&failed);
        self.emit_process_failure(CellScriptRuntimeError::CellLoadFailed);
        self.emit_label(&wrong_size);
        self.emit_process_failure(CellScriptRuntimeError::DynamicFieldBoundsInvalid);
        self.emit_label(&done);
        Ok(true)
    }

    fn emit_nested_data_view_field(
        &mut self,
        dest: &IrVar,
        obj: &IrOperand,
        schema: &str,
        field: &str,
        table: &super::nested::NestedCellLayout,
    ) -> Result<()> {
        let field = self
            .type_layouts
            .get(schema)
            .and_then(|fields| fields.get(field))
            .cloned()
            .ok_or_else(|| CompileError::without_span("unknown nested Cell data field"))?;
        let size = *self
            .cell_data_view_buffers
            .get(&dest.id)
            .ok_or_else(|| CompileError::without_span("nested Cell data read has no owned buffer"))?;
        let buffer = size + 8;
        let invalid = self.fresh_label("cell_data_view_invalid");
        let failed = self.fresh_label("cell_data_view_load_failed");
        let done = self.fresh_label("cell_data_view_done");
        self.emit_operand_to_register("a0", obj);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        for source in [CKB_SOURCE_CELL_DEP, CKB_SOURCE_HEADER_DEP] {
            self.emit(format!("li t0, {source}"));
            self.emit(format!("beq t2, t0, {invalid}"));
        }
        self.emit("mv a3, t1");
        self.emit("mv a4, t2");
        self.emit_store_data_args_at(table.capacity, size, buffer);
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_CELL_DATA));
        self.emit("ecall");
        self.emit(format!("bnez a0, {failed}"));
        let codec = self.fresh_label("data_view_table");
        self.emit_nested_loaded_cell_validation(size, buffer, table, &codec);
        self.emit_sp_addi("t4", buffer);
        // Load the terminal offset before t5 holds the field start. A large
        // stack load into t6 may otherwise use live t5 as its address scratch.
        let nested = Self::nested_type(&field.ty).is_some();
        if nested && field.index + 1 == table.fields.len() {
            self.emit_stack_load("a1", size);
        }
        self.emit_u32_le_from_base_to("t5", "t4", 4 * (field.index + 1), "t2");
        if nested {
            if field.index + 1 < table.fields.len() {
                self.emit_u32_le_from_base_to("t6", "t4", 4 * (field.index + 2), "t2");
            } else {
                self.emit("mv t6, a1");
            }
            self.emit("sub t1, t6, t5");
            let length = self.nested_size_offset(dest).ok_or_else(|| CompileError::without_span("nested field has no length slot"))?;
            self.emit_schema_size_store("t1", length);
            self.emit("add t0, t4, t5");
        } else {
            let width = field.fixed_size.ok_or_else(|| CompileError::without_span("nested table sibling has no fixed width"))?;
            self.emit("add t4, t4, t5");
            if width > 8 {
                let output = *self
                    .fixed_byte_local_offsets
                    .get(&dest.id)
                    .ok_or_else(|| CompileError::without_span("fixed field has no result buffer"))?;
                for byte in 0..width {
                    self.emit(format!("lbu t0, {byte}(t4)"));
                    self.emit_stack_store_byte("t0", output + byte);
                }
                self.emit_sp_addi("t0", output);
            } else {
                self.emit("li t0, 0");
                for byte in 0..width {
                    self.emit(format!("lbu t1, {byte}(t4)"));
                    if byte > 0 {
                        self.emit(format!("slli t1, t1, {}", byte * 8));
                    }
                    self.emit("or t0, t0, t1");
                }
            }
        }
        self.emit_stack_store("t0", self.scalar_slot_offset(dest.id));
        self.emit(format!("j {done}"));
        for (label, error) in
            [(&invalid, CellScriptRuntimeError::CkbSourceViewInvalid), (&failed, CellScriptRuntimeError::CellLoadFailed)]
        {
            self.emit_label(label);
            self.emit_process_failure(error);
        }
        self.emit_label(&done);
        Ok(())
    }
}
