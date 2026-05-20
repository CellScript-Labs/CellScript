//! Call emission and outgoing argument handling for CellScript codegen.
//!
//! Contains direct/internal call emission, CKB fixed-hash helper dispatch,
//! ABI argument placement for calls (scalar, pointer, length, type_hash),
//! outgoing stack argument area management, signed SP-relative store,
//! and ABI register name resolution.

use crate::error::{CompileError, Result};
use crate::ir::*;
use crate::runtime_errors::CellScriptRuntimeError;

use super::abi::{abi_arg_label, call_abi_arg_count, outgoing_stack_arg_bytes, CallLengthKind};
use super::assembler::{scratch_register_avoiding, small_signed_immediate};
use super::runtime::{is_ckb_checked_runtime_helper, is_ckb_fixed_hash_helper};
use super::schema::{fixed_aggregate_pointer_param_width, fixed_byte_pointer_param_width, named_type_name};
use super::CodeGenerator;

impl CodeGenerator {
    fn emit_ckb_fixed_hash_call(&mut self, dest: Option<&IrVar>, func: &str, args: &[IrOperand]) -> Result<bool> {
        if !is_ckb_fixed_hash_helper(func) {
            return Ok(false);
        }
        self.emit(format!("# call {}", func));
        let Some(dest) = dest else {
            self.emit("# cellscript abi: fail closed because hash helper result has no destination");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(arg) = args.first() else {
            self.emit("# cellscript abi: fail closed because hash helper is missing input");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(dest_offset) = self.fixed_byte_local_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: fail closed because hash helper output buffer was not allocated");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(source) = self.expected_fixed_byte_source(arg, 32) else {
            self.emit("# cellscript abi: fail closed because hash helper input is not a 32-byte value");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        self.emit_prepare_fixed_byte_source(&source, 32, "hash_blake2b input");
        if !self.emit_fixed_byte_source_pointer_or_const_to("a0", &source) {
            self.emit("# cellscript abi: fail closed because hash helper input pointer is not materializable");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        }
        self.emit_sp_addi("a1", dest_offset);
        self.emit("call __ckb_hash_blake2b");
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_sp_addi("t0", dest_offset);
        self.emit_stack_store("t0", dest.id * 8);
        Ok(true)
    }

    pub(crate) fn emit_call(&mut self, dest: Option<&IrVar>, func: &str, args: &[IrOperand]) -> Result<()> {
        if self.emit_ckb_fixed_hash_call(dest, func, args)? {
            return Ok(());
        }
        if func.contains("::") {
            return Err(CompileError::new(
                format!(
                    "external function call '{}' is not linkable yet; importable function summaries are only used for type/effect checking",
                    func
                ),
                crate::error::Span::default(),
            ));
        }
        self.emit(format!("# call {}", func));

        let abi = self.callable_abis.get(func).cloned();
        let outgoing_stack_arg_bytes = outgoing_stack_arg_bytes(call_abi_arg_count(abi.as_ref(), args));
        let mut abi_index = 0usize;
        for (arg_index, arg) in args.iter().enumerate() {
            if let Some(abi) = &abi {
                if let Some(param) = abi.params.get(arg_index) {
                    let needs_type_hash = abi.type_hash_param_indices.contains(&arg_index);
                    if !self.emit_call_param_arg(func, param, needs_type_hash, &mut abi_index, arg, outgoing_stack_arg_bytes) {
                        return Ok(());
                    }
                    continue;
                }
            }
            if !self.emit_call_scalar_arg(func, &format!("arg{}", arg_index), &mut abi_index, arg, outgoing_stack_arg_bytes) {
                return Ok(());
            }
        }

        if outgoing_stack_arg_bytes > 0 {
            self.emit(format!("# cellscript abi: reserve {} bytes for outgoing stack call arguments", outgoing_stack_arg_bytes));
            self.emit_large_addi("sp", "sp", -(outgoing_stack_arg_bytes as i64));
        }
        self.emit(format!("call {}", func));
        if outgoing_stack_arg_bytes > 0 {
            self.emit_large_addi("sp", "sp", outgoing_stack_arg_bytes as i64);
        }
        if is_ckb_checked_runtime_helper(func) {
            self.emit_checked_runtime_status(func);
        }

        if let Some(d) = dest {
            if let IrType::Tuple(items) = &d.ty {
                self.emit_stack_store("a0", d.id * 8);
                for index in 0..items.len().min(8) {
                    let field = index.to_string();
                    if let Some(field_var_id) = self.tuple_call_return_field_slots.get(&(d.id, field)).copied() {
                        self.emit_stack_store(&format!("a{}", index), field_var_id * 8);
                    }
                }
            } else {
                self.emit_stack_store("a0", d.id * 8);
            }
        }

        Ok(())
    }

    fn emit_checked_runtime_status(&mut self, func: &str) {
        let ok_label = self.fresh_label("runtime_helper_ok");
        self.emit(format!("# cellscript abi: {} returns status in a1; fail closed on nonzero", func));
        self.emit(format!("beqz a1, {}", ok_label));
        self.emit("mv a0, a1");
        self.emit_epilogue();
        self.emit_label(&ok_label);
    }

    fn emit_call_param_arg(
        &mut self,
        func: &str,
        param: &IrParam,
        needs_type_hash: bool,
        abi_index: &mut usize,
        arg: &IrOperand,
        outgoing_stack_arg_bytes: usize,
    ) -> bool {
        if named_type_name(&param.ty).is_some() {
            self.emit(format!(
                "# cellscript abi: call {} schema param {} pointer={} length={}",
                func,
                param.name,
                abi_arg_label(*abi_index),
                abi_arg_label(*abi_index + 1)
            ));
            if !self.emit_call_pointer_arg(func, &param.name, abi_index, arg, None, outgoing_stack_arg_bytes) {
                return false;
            }
            if !self.emit_call_length_arg(func, &param.name, abi_index, arg, CallLengthKind::Schema, outgoing_stack_arg_bytes) {
                return false;
            }
            if needs_type_hash {
                self.emit(format!(
                    "# cellscript abi: call {} schema param {} type_hash pointer={} length={} size=32",
                    func,
                    param.name,
                    abi_arg_label(*abi_index),
                    abi_arg_label(*abi_index + 1)
                ));
                if !self.emit_call_type_hash_pointer_arg(func, &param.name, abi_index, arg, outgoing_stack_arg_bytes) {
                    return false;
                }
                if !self.emit_call_type_hash_length_arg(func, &param.name, abi_index, arg, outgoing_stack_arg_bytes) {
                    return false;
                }
            }
            return true;
        }

        let fixed_pointer_width = fixed_byte_pointer_param_width(&param.ty).or_else(|| fixed_aggregate_pointer_param_width(&param.ty));
        if let Some(width) = fixed_pointer_width {
            self.emit(format!(
                "# cellscript abi: call {} fixed-byte param {} pointer={} length={} size={}",
                func,
                param.name,
                abi_arg_label(*abi_index),
                abi_arg_label(*abi_index + 1),
                width
            ));
            if !self.emit_call_pointer_arg(func, &param.name, abi_index, arg, Some(width), outgoing_stack_arg_bytes) {
                return false;
            }
            if !self.emit_call_length_arg(func, &param.name, abi_index, arg, CallLengthKind::FixedBytes, outgoing_stack_arg_bytes) {
                return false;
            }
            return true;
        }

        self.emit_call_scalar_arg(func, &param.name, abi_index, arg, outgoing_stack_arg_bytes)
    }

    fn emit_call_scalar_arg(
        &mut self,
        func: &str,
        label: &str,
        abi_index: &mut usize,
        arg: &IrOperand,
        outgoing_stack_arg_bytes: usize,
    ) -> bool {
        let register = self.call_abi_register(*abi_index);
        self.emit(format!("# cellscript abi: call {} scalar {} -> {}", func, label, register));
        self.emit_operand_to_register(&register, arg);
        self.emit_outgoing_call_stack_arg_store(&register, *abi_index, outgoing_stack_arg_bytes);
        *abi_index += 1;
        true
    }

    fn emit_call_pointer_arg(
        &mut self,
        func: &str,
        label: &str,
        abi_index: &mut usize,
        arg: &IrOperand,
        const_width: Option<usize>,
        outgoing_stack_arg_bytes: usize,
    ) -> bool {
        let register = self.call_abi_register(*abi_index);
        if const_width.is_some() && matches!(arg, IrOperand::Const(_)) {
            self.emit(format!(
                "# cellscript abi: call {} pointer param {} uses a constant unsupported by the call ABI; pass null pointer",
                func, label
            ));
            self.emit(format!("li {}, 0", register));
        } else {
            self.emit_operand_to_register(&register, arg);
        }
        self.emit_outgoing_call_stack_arg_store(&register, *abi_index, outgoing_stack_arg_bytes);
        *abi_index += 1;
        true
    }

    fn emit_call_length_arg(
        &mut self,
        func: &str,
        label: &str,
        abi_index: &mut usize,
        arg: &IrOperand,
        kind: CallLengthKind,
        outgoing_stack_arg_bytes: usize,
    ) -> bool {
        let register = self.call_abi_register(*abi_index);
        let size_offset = match (arg, kind) {
            (IrOperand::Var(var), CallLengthKind::Schema) => self.schema_pointer_size_offsets.get(&var.id).copied(),
            (IrOperand::Var(var), CallLengthKind::FixedBytes) => self.fixed_byte_param_size_offsets.get(&var.id).copied(),
            _ => None,
        };
        if let Some(size_offset) = size_offset {
            self.emit_stack_load(&register, size_offset);
        } else if let CallLengthKind::FixedBytes = kind {
            if matches!(arg, IrOperand::Const(_)) {
                self.emit(format!(
                    "# cellscript abi: call {} fixed-byte const param {} has no materialized pointer; pass zero length to fail closed",
                    func, label
                ));
                self.emit(format!("li {}, 0", register));
            } else {
                self.emit(format!(
                    "# cellscript abi: call {} fixed-byte param {} has no tracked ABI length; pass zero length to fail closed",
                    func, label
                ));
                self.emit(format!("li {}, 0", register));
            }
        } else {
            self.emit(format!(
                "# cellscript abi: call {} schema param {} has no tracked ABI length; pass zero length to fail closed",
                func, label
            ));
            self.emit(format!("li {}, 0", register));
        }
        self.emit_outgoing_call_stack_arg_store(&register, *abi_index, outgoing_stack_arg_bytes);
        *abi_index += 1;
        true
    }

    fn emit_call_type_hash_pointer_arg(
        &mut self,
        func: &str,
        label: &str,
        abi_index: &mut usize,
        arg: &IrOperand,
        outgoing_stack_arg_bytes: usize,
    ) -> bool {
        let register = self.call_abi_register(*abi_index);
        if let IrOperand::Var(var) = arg {
            if let Some(pointer_offset) = self.param_type_hash_pointer_offsets.get(&var.id).copied() {
                self.emit_stack_load(&register, pointer_offset);
            } else {
                self.emit(format!(
                    "# cellscript abi: call {} schema param {} has no tracked TypeHash pointer; pass null pointer",
                    func, label
                ));
                self.emit(format!("li {}, 0", register));
            }
        } else {
            self.emit(format!(
                "# cellscript abi: call {} schema param {} TypeHash source is not a variable; pass null pointer",
                func, label
            ));
            self.emit(format!("li {}, 0", register));
        }
        self.emit_outgoing_call_stack_arg_store(&register, *abi_index, outgoing_stack_arg_bytes);
        *abi_index += 1;
        true
    }

    fn emit_call_type_hash_length_arg(
        &mut self,
        func: &str,
        label: &str,
        abi_index: &mut usize,
        arg: &IrOperand,
        outgoing_stack_arg_bytes: usize,
    ) -> bool {
        let register = self.call_abi_register(*abi_index);
        if let IrOperand::Var(var) = arg {
            if let Some(size_offset) = self.param_type_hash_size_offsets.get(&var.id).copied() {
                self.emit_stack_load(&register, size_offset);
            } else {
                self.emit(format!(
                    "# cellscript abi: call {} schema param {} has no tracked TypeHash length; pass zero length to fail closed",
                    func, label
                ));
                self.emit(format!("li {}, 0", register));
            }
        } else {
            self.emit(format!(
                "# cellscript abi: call {} schema param {} TypeHash length source is not a variable; pass zero length",
                func, label
            ));
            self.emit(format!("li {}, 0", register));
        }
        self.emit_outgoing_call_stack_arg_store(&register, *abi_index, outgoing_stack_arg_bytes);
        *abi_index += 1;
        true
    }

    fn emit_outgoing_call_stack_arg_store(&mut self, register: &str, abi_index: usize, outgoing_stack_arg_bytes: usize) {
        if abi_index < 8 {
            return;
        }
        let stack_slot_offset = (abi_index - 8) * 8;
        let offset = i64::try_from(stack_slot_offset).expect("call stack slot should fit in i64")
            - i64::try_from(outgoing_stack_arg_bytes).expect("call stack argument area should fit in i64");
        self.emit(format!(
            "# cellscript abi: stage outgoing stack arg{} at pre-call sp{}{}",
            abi_index,
            if offset < 0 { "" } else { "+" },
            offset
        ));
        self.emit_sp_store_signed(register, offset);
    }

    pub(crate) fn emit_sp_store_signed(&mut self, register: &str, offset: i64) {
        if small_signed_immediate(offset) {
            self.emit(format!("sd {}, {}(sp)", register, offset));
        } else {
            let scratch = scratch_register_avoiding(&[register]);
            self.emit(format!("li {}, {}", scratch, offset));
            self.emit(format!("add {}, sp, {}", scratch, scratch));
            self.emit(format!("sd {}, 0({})", register, scratch));
        }
    }

    fn call_abi_register(&self, abi_index: usize) -> String {
        if abi_index < 8 {
            format!("a{}", abi_index)
        } else {
            "t0".to_string()
        }
    }
}
