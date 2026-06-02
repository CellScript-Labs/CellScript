//! Scalar expression helper emission for CellScript codegen.
//!
//! Contains constant/variable loading, truncation, bounds checking,
//! boolean canonicalisation, division guards, binary arithmetic and
//! comparison emission, dynamic byte comparison, unary emission,
//! move/cast/tuple emission, and operand-to-register/comment utilities.

use crate::ast::{BinaryOp, UnaryOp};
use crate::error::{CompileError, Result};
use crate::ir::*;
use crate::runtime_errors::CellScriptRuntimeError;

use super::schema::{const_ir_type, fixed_byte_const_bytes, operand_fixed_byte_width};
use super::CodeGenerator;

impl CodeGenerator {
    pub(crate) fn emit_load_const(&mut self, dest: &IrVar, value: &IrConst) -> Result<()> {
        match value {
            IrConst::Poisoned => {
                return Err(CompileError::without_span(format!("codegen received poisoned lowering constant for var{}", dest.id)));
            }
            IrConst::Unit => self.emit("li t0, 0"),
            IrConst::U8(n) => self.emit(format!("li t0, {}", n)),
            IrConst::U16(n) => self.emit(format!("li t0, {}", n)),
            IrConst::U32(n) => self.emit(format!("li t0, {}", n)),
            IrConst::U64(n) => self.emit(format!("li t0, {}", n)),
            IrConst::U128(value) => {
                if let Some(offset) = self.fixed_byte_local_offsets.get(&dest.id).copied() {
                    self.emit_store_const_bytes_to_stack(&value.to_le_bytes(), offset);
                    self.emit_sp_addi("t0", offset);
                    self.emit_stack_store("t0", dest.id * 8);
                    return Ok(());
                }
                self.record_fatal_error(format!("u128 const destination var{} has no fixed-byte storage", dest.id));
                return Ok(());
            }
            IrConst::Bool(b) => self.emit(format!("li t0, {}", if *b { 1 } else { 0 })),
            IrConst::Address(_) | IrConst::Hash(_) | IrConst::Array(_) => {
                let Some(bytes) = fixed_byte_const_bytes(value) else {
                    self.emit("# cellscript abi: fail closed because fixed-byte constant bytes are not materializable");
                    self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
                    self.emit("li t0, 0");
                    self.emit_stack_store("t0", dest.id * 8);
                    return Ok(());
                };
                let label = self.const_data_label_for_bytes(bytes);
                self.emit(format!("la t0, {}", label));
            }
        }
        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    pub(crate) fn emit_load_var(&mut self, dest: &IrVar, name: &str) -> Result<()> {
        self.emit(format!("# load var {}", name));
        let Some(offset) = self.named_var_offsets.get(name).copied() else {
            self.emit("# cellscript abi: fail closed because named variable slot was not allocated");
            self.emit_fail(CellScriptRuntimeError::ConsumeInvalidOperand);
            return Ok(());
        };
        self.emit_stack_load("t0", offset);
        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    pub(crate) fn emit_store_var(&mut self, name: &str, src: &IrOperand) -> Result<()> {
        self.emit(format!("# store var {}", name));
        let Some(offset) = self.named_var_offsets.get(name).copied() else {
            self.emit("# cellscript abi: fail closed because named variable slot was not allocated");
            self.emit_fail(CellScriptRuntimeError::ConsumeInvalidOperand);
            return Ok(());
        };
        self.emit_operand_to_register("t0", src);
        self.emit_stack_store("t0", offset);
        Ok(())
    }

    fn emit_truncate_register_to_type(&mut self, register: &str, ty: &IrType) {
        match ty {
            IrType::U8 => self.emit(format!("andi {}, {}, 255", register, register)),
            IrType::U16 => self.emit_truncate_register_to_width(register, 16),
            IrType::U32 => self.emit_truncate_register_to_width(register, 32),
            _ => {}
        }
    }

    fn emit_truncate_register_to_width(&mut self, register: &str, width: u32) {
        if width >= 64 {
            return;
        }
        let shift = 64 - width;
        self.emit(format!("slli {}, {}, {}", register, register, shift));
        self.emit(format!("srli {}, {}, {}", register, register, shift));
    }

    fn emit_checked_scalar_fits(&mut self, register: &str, width: u32) {
        if width >= 64 {
            return;
        }
        let ok_label = self.fresh_label("cast_fit_ok");
        self.emit(format!("srli t2, {}, {}", register, width));
        self.emit(format!("beqz t2, {}", ok_label));
        self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
        self.emit_label(&ok_label);
    }

    pub(crate) fn emit_bool_canonical_check(&mut self, register: &str) {
        let ok_label = self.fresh_label("bool_canonical_ok");
        self.emit(format!("beqz {}, {}", register, ok_label));
        self.emit("li t2, 1");
        self.emit(format!("beq {}, t2, {}", register, ok_label));
        self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
        self.emit_label(&ok_label);
    }

    pub(crate) fn emit_divisor_nonzero_guard(&mut self, register: &str) {
        let ok_label = self.fresh_label("divisor_nonzero");
        self.emit(format!("bnez {}, {}", register, ok_label));
        self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
        self.emit_label(&ok_label);
    }

    pub(crate) fn emit_binary(&mut self, dest: &IrVar, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> Result<()> {
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) && self.emit_dynamic_byte_comparison(dest, op, left, right) {
            return Ok(());
        }
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne)
            && (operand_fixed_byte_width(left).is_some() || operand_fixed_byte_width(right).is_some())
        {
            if self.emit_fixed_byte_comparison(dest, op, left, right) {
                return Ok(());
            }
            if self.emit_generic_fixed_byte_comparison(dest, op, left, right) {
                return Ok(());
            }
            // Final fallback: emit a fail-closed trap with specific error code
            self.emit(format!("# binary {:?} over fixed-byte operands (unresolved)", op));
            self.emit("# cellscript abi: fail closed because fixed-byte operand sources are not available");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(());
        }

        if self.emit_u128_add_sub_with_u64(dest, op, left, right) {
            return Ok(());
        }
        if dest.ty == IrType::U128 || self.operand_is_u128(left) || self.operand_is_u128(right) {
            self.emit(format!("# binary {:?} over unsupported u128 operand shape", op));
            self.emit("# cellscript abi: fail closed because generic u128 arithmetic/comparison shape is not lowered");
            self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
            return Ok(());
        }

        self.emit_operand_to_register("t0", left);
        self.emit_operand_to_register("t1", right);
        if matches!(op, BinaryOp::Div | BinaryOp::Mod) {
            self.emit_truncate_register_to_type("t0", &dest.ty);
            self.emit_truncate_register_to_type("t1", &dest.ty);
        }

        match op {
            BinaryOp::Add => self.emit("add t0, t0, t1"),
            BinaryOp::Sub => self.emit("sub t0, t0, t1"),
            BinaryOp::Mul => self.emit("mul t0, t0, t1"),
            BinaryOp::Div => {
                self.emit_divisor_nonzero_guard("t1");
                self.emit("divu t0, t0, t1");
            }
            BinaryOp::Mod => {
                self.emit_divisor_nonzero_guard("t1");
                self.emit("remu t0, t0, t1");
            }
            BinaryOp::Eq => {
                self.emit("sub t0, t0, t1");
                self.emit("seqz t0, t0");
            }
            BinaryOp::Ne => {
                self.emit("sub t0, t0, t1");
                self.emit("snez t0, t0");
            }
            BinaryOp::Lt => self.emit("sltu t0, t0, t1"),
            BinaryOp::Le => {
                self.emit("sgtu t0, t0, t1");
                self.emit("xori t0, t0, 1");
            }
            BinaryOp::Gt => self.emit("sgtu t0, t0, t1"),
            BinaryOp::Ge => {
                self.emit("sltu t0, t0, t1");
                self.emit("xori t0, t0, 1");
            }
            BinaryOp::And => {
                self.emit_bool_canonical_check("t0");
                self.emit_bool_canonical_check("t1");
                self.emit("and t0, t0, t1");
            }
            BinaryOp::Or => {
                self.emit_bool_canonical_check("t0");
                self.emit_bool_canonical_check("t1");
                self.emit("or t0, t0, t1");
            }
        }

        if matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod) {
            self.emit_truncate_register_to_type("t0", &dest.ty);
        }
        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    fn emit_dynamic_byte_comparison(&mut self, dest: &IrVar, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> bool {
        let (IrOperand::Var(left_var), IrOperand::Var(right_var)) = (left, right) else {
            return false;
        };
        let Some(left_len_offset) = self.dynamic_value_size_offsets.get(&left_var.id).copied() else {
            return false;
        };
        let Some(right_len_offset) = self.dynamic_value_size_offsets.get(&right_var.id).copied() else {
            return false;
        };

        let equal_value = if matches!(op, BinaryOp::Eq) { 1 } else { 0 };
        let mismatch_value = if matches!(op, BinaryOp::Eq) { 0 } else { 1 };
        let len_equal_label = self.fresh_label("dynamic_bytes_len_equal");
        let bytes_equal_label = self.fresh_label("dynamic_bytes_equal");
        let done_label = self.fresh_label("dynamic_bytes_cmp_done");

        self.emit(format!("# binary {:?} over dynamic byte operands", op));
        self.emit_stack_load("t0", left_len_offset);
        self.emit_stack_load("t1", right_len_offset);
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", len_equal_label));
        self.emit(format!("li t0, {}", mismatch_value));
        self.emit_stack_store("t0", dest.id * 8);
        self.emit(format!("j {}", done_label));

        self.emit_label(&len_equal_label);
        self.emit_stack_load("a0", left_var.id * 8);
        self.emit_stack_load("a1", right_var.id * 8);
        self.emit_stack_load("a2", left_len_offset);
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("beqz a0, {}", bytes_equal_label));
        self.emit(format!("li t0, {}", mismatch_value));
        self.emit_stack_store("t0", dest.id * 8);
        self.emit(format!("j {}", done_label));

        self.emit_label(&bytes_equal_label);
        self.emit(format!("li t0, {}", equal_value));
        self.emit_stack_store("t0", dest.id * 8);
        self.emit_label(&done_label);
        true
    }

    pub(crate) fn emit_unary(&mut self, dest: &IrVar, op: UnaryOp, operand: &IrOperand) -> Result<()> {
        self.emit_operand_to_register("t0", operand);

        match op {
            UnaryOp::Neg => {
                return Err(CompileError::new("unary negation is not supported for unsigned integers", crate::error::Span::default()));
            }
            UnaryOp::Not => {
                self.emit_bool_canonical_check("t0");
                self.emit("xori t0, t0, 1");
            }
            UnaryOp::Ref | UnaryOp::Deref => self.emit("# reference conversion (no-op in asm backend)"),
        }

        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    pub(crate) fn emit_move(&mut self, dest: &IrVar, src: &IrOperand) -> Result<()> {
        if dest.ty == IrType::U128 {
            let Some(dest_offset) = self.fixed_byte_local_offsets.get(&dest.id).copied() else {
                self.emit("# cellscript abi: fail closed because u128 move destination has no fixed-byte storage");
                self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
                return Ok(());
            };
            let Some(source) = self.expected_fixed_byte_source(src, 16) else {
                self.emit("# cellscript abi: fail closed because u128 move source is not addressable");
                self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
                return Ok(());
            };
            self.emit_fixed_byte_source_scalar_to("t0", "t2", "t4", &source, 0, 8);
            self.emit_fixed_byte_source_scalar_to("t1", "t2", "t4", &source, 8, 8);
            self.emit_stack_store("t0", dest_offset);
            self.emit_stack_store("t1", dest_offset + 8);
            self.emit_sp_addi("t0", dest_offset);
            self.emit_stack_store("t0", dest.id * 8);
            return Ok(());
        }
        if let IrOperand::Var(src_var) = src {
            if src_var.ty != dest.ty {
                self.emit("# cellscript abi: fail closed because Move cannot change value type");
                self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
                return Ok(());
            }
        }
        self.emit_operand_to_register("t0", src);
        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    pub(crate) fn emit_cast(&mut self, dest: &IrVar, src: &IrOperand) -> Result<()> {
        if dest.ty == IrType::U128 {
            self.emit("# cellscript abi: fail closed because runtime casts to u128 are not supported");
            self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
            return Ok(());
        }

        self.emit_operand_to_register("t0", src);
        let src_ty = match src {
            IrOperand::Var(var) => var.ty.clone(),
            IrOperand::Const(value) => const_ir_type(value),
        };
        if src_ty == IrType::Bool {
            self.emit_bool_canonical_check("t0");
        }

        match dest.ty {
            IrType::Bool => self.emit_checked_scalar_fits("t0", 1),
            IrType::U8 => {
                self.emit_checked_scalar_fits("t0", 8);
                self.emit("andi t0, t0, 255");
            }
            IrType::U16 => {
                self.emit_checked_scalar_fits("t0", 16);
                self.emit_truncate_register_to_width("t0", 16);
            }
            IrType::U32 => {
                self.emit_checked_scalar_fits("t0", 32);
                self.emit_truncate_register_to_width("t0", 32);
            }
            IrType::U64 => {}
            _ => {
                self.emit("# cellscript abi: fail closed because runtime cast target is unsupported");
                self.emit_fail(CellScriptRuntimeError::NumericOrDiscriminantInvalid);
                return Ok(());
            }
        }

        self.emit_stack_store("t0", dest.id * 8);
        Ok(())
    }

    pub(crate) fn emit_tuple(&mut self, dest: &IrVar, fields: &[IrOperand]) -> Result<()> {
        self.emit(format!("# cellscript abi: construct tuple aggregate var{} fields={}", dest.id, fields.len()));
        // Tuple field values are tracked in tuple_aggregate_fields; this stack slot is only the aggregate sentinel.
        self.emit_stack_store("zero", dest.id * 8);
        Ok(())
    }

    pub(crate) fn emit_operand_to_register(&mut self, register: &str, operand: &IrOperand) {
        match operand {
            IrOperand::Const(IrConst::Poisoned) => {
                self.record_fatal_error("codegen received poisoned lowering operand");
                self.emit(format!("li {}, 0", register));
            }
            IrOperand::Const(IrConst::U8(n)) => self.emit(format!("li {}, {}", register, n)),
            IrOperand::Const(IrConst::U16(n)) => self.emit(format!("li {}, {}", register, n)),
            IrOperand::Const(IrConst::U32(n)) => self.emit(format!("li {}, {}", register, n)),
            IrOperand::Const(IrConst::U64(n)) => self.emit(format!("li {}, {}", register, n)),
            IrOperand::Const(IrConst::Bool(b)) => self.emit(format!("li {}, {}", register, if *b { 1 } else { 0 })),
            IrOperand::Const(value) => {
                if let Some(bytes) = fixed_byte_const_bytes(value) {
                    let label = self.const_data_label_for_bytes(bytes);
                    self.emit(format!("la {}, {}", register, label));
                } else {
                    self.emit(format!("li {}, 0", register));
                }
            }
            IrOperand::Var(v) => self.emit_stack_load(register, v.id * 8),
        }
    }
    pub(crate) fn emit_operand_comment(&mut self, label: &str, operand: &IrOperand) {
        let rendered = match operand {
            IrOperand::Var(var) => format!("{}: {}", label, var.name),
            IrOperand::Const(IrConst::U64(n)) => format!("{}: {}", label, n),
            IrOperand::Const(IrConst::Bool(b)) => format!("{}: {}", label, b),
            IrOperand::Const(IrConst::Address(_)) => format!("{}: <address>", label),
            IrOperand::Const(IrConst::Hash(_)) => format!("{}: <hash>", label),
            IrOperand::Const(IrConst::Array(items)) => format!("{}: <array:{}>", label, items.len()),
            IrOperand::Const(_) => format!("{}: <const>", label),
        };
        self.emit(format!("#   {}", rendered));
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen::{CodeGenerator, CodegenOptions};

    #[test]
    fn bool_canonical_check_emits_zero_one_guard() {
        let mut codegen = CodeGenerator::new(CodegenOptions::default());

        codegen.emit_bool_canonical_check("t0");

        assert!(codegen.assembly.iter().any(|line| line.trim_start().starts_with("beqz t0, .Lbool_canonical_ok_")));
        assert!(codegen.assembly.iter().any(|line| line.trim_start() == "li t2, 1"));
        assert!(codegen.assembly.iter().any(|line| line.trim_start().starts_with("beq t0, t2, .Lbool_canonical_ok_")));
        assert!(codegen.assembly.iter().any(|line| line.contains("cellscript runtime error 20")));
    }

    #[test]
    fn divisor_nonzero_guard_fails_closed_on_zero() {
        let mut codegen = CodeGenerator::new(CodegenOptions::default());

        codegen.emit_divisor_nonzero_guard("t1");

        assert!(codegen.assembly.iter().any(|line| line.trim_start().starts_with("bnez t1, .Ldivisor_nonzero_")));
        assert!(codegen.assembly.iter().any(|line| line.contains("cellscript runtime error 20")));
    }
}
