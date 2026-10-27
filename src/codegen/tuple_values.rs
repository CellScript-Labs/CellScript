//! Fixed value tuples retain the a0..a7 field ABI. Receive every field into
//! caller-owned storage before another call can reuse the callee's frame.
//! Resource tuples have separate register/ownership handling in calls.rs.

use super::*;

impl CodeGenerator {
    pub(super) fn emit_fixed_tuple_return(&mut self, operand: &IrOperand) -> bool {
        let IrOperand::Var(var) = operand else { return false };
        let IrType::Tuple(items) = &var.ty else { return false };
        let Some(width) = type_static_length(&var.ty) else { return false };
        if items.len() > 8 {
            self.emit("# cellscript abi: fixed tuple exceeds eight return registers; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return true;
        }
        if width == 0 {
            for index in 0..items.len().max(1) {
                self.emit(format!("li a{index}, 0"));
            }
            self.emit_epilogue();
            return true;
        }
        let Some(source) = self.expected_fixed_byte_source(operand, width) else {
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return true;
        };
        // Resolve/validate storage before populating return registers; source
        // preparation may call helpers. Afterwards only t-registers are scratch.
        self.emit_prepare_fixed_byte_source(&source, width, "fixed tuple return");
        if !self.emit_fixed_byte_source_pointer_or_const_to("t4", &source) {
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return true;
        }
        self.emit(format!("# cellscript abi: return fixed tuple var{} size={width}", var.id));
        let mut offset = 0;
        for (index, item) in items.iter().enumerate() {
            let field_width = type_static_length(item).expect("fixed tuple field width");
            let register = format!("a{index}");
            if field_width <= 8 {
                self.emit_unaligned_scalar_load("t4", &register, "t2", offset, field_width);
                if *item == IrType::I32 {
                    self.emit_sign_extend_i32(&register);
                }
            } else {
                self.emit_large_addi(&register, "t4", offset as i64);
            }
            offset += field_width;
        }
        self.emit_epilogue();
        true
    }

    pub(super) fn emit_fixed_tuple_call_result(&mut self, dest: &IrVar) -> bool {
        let IrType::Tuple(items) = &dest.ty else { return false };
        let Some(width) = type_static_length(&dest.ty) else { return false };
        if items.len() > 8 {
            self.emit("# cellscript abi: fixed tuple exceeds eight return registers; fail closed");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return true;
        }
        let root_slot = self.scalar_slot_offset(dest.id);
        let dest_offset = if width <= 8 {
            self.emit_stack_store("zero", root_slot);
            root_slot
        } else if let Some(offset) = self.fixed_byte_local_offsets.get(&dest.id).copied() {
            offset
        } else {
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return true;
        };
        self.emit(format!("# cellscript abi: receive fixed tuple var{} size={width}", dest.id));
        let mut offset = dest_offset;
        // Keep a0..a7 live throughout. In particular do not call memcpy: a
        // returned wide field can still point below the restored caller sp.
        for (index, item) in items.iter().enumerate() {
            let field_width = type_static_length(item).expect("fixed tuple field width");
            if field_width <= 8 {
                self.emit(format!("mv t0, a{index}"));
                for byte in 0..field_width {
                    self.emit_stack_store_byte("t0", offset + byte);
                    if byte + 1 < field_width {
                        self.emit("srli t0, t0, 8");
                    }
                }
            } else {
                self.emit(format!("mv t1, a{index}"));
                self.emit_sp_addi("t2", offset);
                self.emit(format!("li t3, {field_width}"));
                let copy = self.fresh_label("tuple_result_copy");
                self.emit_label(&copy);
                self.emit("lbu t0, 0(t1)");
                self.emit("sb t0, 0(t2)");
                self.emit("addi t1, t1, 1");
                self.emit("addi t2, t2, 1");
                self.emit("addi t3, t3, -1");
                self.emit(format!("bnez t3, {copy}"));
            }
            offset += field_width;
        }
        if width > 8 {
            self.emit_sp_addi("t0", dest_offset);
            self.emit_stack_store("t0", root_slot);
        }
        true
    }
}
