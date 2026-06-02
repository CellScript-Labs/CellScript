//! Call emission and outgoing argument handling for CellScript codegen.
//!
//! Contains direct/internal call emission, CKB fixed-hash helper dispatch,
//! ABI argument placement for calls (scalar, pointer, length, type_hash),
//! outgoing stack argument area management, frame-local staging,
//! and ABI register name resolution.

use crate::error::{CompileError, Result};
use crate::ir::*;
use crate::runtime_errors::CellScriptRuntimeError;

use super::abi::{abi_arg_label, call_abi_arg_count, outgoing_stack_arg_bytes, CallLengthKind};
use super::runtime::{ckb_checked_runtime_status_reg, is_ckb_checked_runtime_helper, is_ckb_fixed_hash_helper};
use super::schema::{
    const_ir_type, const_usize_operand, fixed_aggregate_pointer_param_width, fixed_byte_pointer_param_width, named_type_name,
    storage_type,
};
use super::{CodeGenerator, RUNTIME_SCRATCH_BUFFER_SIZE};

impl CodeGenerator {
    fn emit_fixed_u64_le_call(&mut self, dest: Option<&IrVar>, func: &str, args: &[IrOperand]) -> Result<bool> {
        if func != "__cellscript_fixed_u64_le" {
            return Ok(false);
        }
        self.emit(format!("# call {}", func));
        let Some(dest) = dest else {
            self.emit("# cellscript abi: fail closed because fixed_u64_le result has no destination");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(bytes_operand) = args.first() else {
            self.emit("# cellscript abi: fail closed because fixed_u64_le is missing fixed bytes input");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(index_operand) = args.get(1) else {
            self.emit("# cellscript abi: fail closed because fixed_u64_le is missing word_index");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(word_index) = const_usize_operand(index_operand) else {
            self.emit("# cellscript abi: fail closed because fixed_u64_le word_index is not a static integer");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(width) = fixed_u64_le_operand_width(bytes_operand) else {
            self.emit("# cellscript abi: fail closed because fixed_u64_le input is not Hash, Address, or [u8; N]");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(start) = word_index.checked_mul(8) else {
            self.emit("# cellscript abi: fail closed because fixed_u64_le word_index overflows byte offset");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(end) = start.checked_add(8) else {
            self.emit("# cellscript abi: fail closed because fixed_u64_le byte window overflows");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        if end > width {
            self.emit("# cellscript abi: fail closed because fixed_u64_le byte window exceeds fixed input width");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        }
        let Some(source) = self.expected_fixed_byte_source(bytes_operand, width) else {
            self.emit("# cellscript abi: fail closed because fixed_u64_le input is not materializable as fixed bytes");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };

        self.emit(format!("# cellscript abi: fixed_u64_le word={} offset={} width={}", word_index, start, width));
        self.emit_prepare_fixed_byte_source(&source, width, "fixed_u64_le input");
        self.emit_fixed_byte_source_scalar_to("a0", "t1", "t2", &source, start, 8);
        self.emit_stack_store("a0", dest.id * 8);
        Ok(true)
    }

    fn emit_ckb_fixed_hash_call(&mut self, dest: Option<&IrVar>, func: &str, args: &[IrOperand]) -> Result<bool> {
        if !is_ckb_fixed_hash_helper(func)
            || matches!(
                func,
                "__ckb_hash_blake2b_packed"
                    | "__ckb_hash_data_packed"
                    | "__ckb_input_previous_tx_hash"
                    | "__ckb_cell_data_hash"
                    | "__ckb_cell_lock_args32"
            )
        {
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

    fn emit_hash_blake2b_packed_call(&mut self, dest: Option<&IrVar>, func: &str, args: &[IrOperand]) -> Result<bool> {
        if !matches!(func, "__ckb_hash_blake2b_packed" | "__ckb_hash_data_packed") {
            return Ok(false);
        }
        self.emit(format!("# call {}", func));
        let Some(dest) = dest else {
            self.emit("# cellscript abi: fail closed because packed hash result has no destination");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(arg) = args.first() else {
            self.emit("# cellscript abi: fail closed because packed hash is missing input");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(dest_offset) = self.fixed_byte_local_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: fail closed because packed hash output buffer was not allocated");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(width) = packed_hash_operand_width(self, arg) else {
            self.emit("# cellscript abi: fail closed because hash_blake2b_packed input is not fixed-width");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let type_name = packed_hash_operand_type_name(arg);
        if func == "__ckb_hash_blake2b_packed" && type_name.is_none() {
            self.emit("# cellscript abi: fail closed because hash_blake2b_packed input type name is not canonical");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        }
        let mut header = Vec::new();
        if func == "__ckb_hash_blake2b_packed" {
            let type_name = type_name.as_deref().unwrap_or_default();
            header.extend_from_slice(b"CellScriptPackedHashV0\0");
            header.extend_from_slice(type_name.as_bytes());
            header.push(0);
            header.extend_from_slice(&(width as u32).to_le_bytes());
        }
        let preimage_len = header.len().saturating_add(width);
        if preimage_len > RUNTIME_SCRATCH_BUFFER_SIZE {
            self.emit(format!(
                "# cellscript abi: fail closed because packed hash preimage size {} exceeds scratch buffer {}",
                preimage_len, RUNTIME_SCRATCH_BUFFER_SIZE
            ));
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        }

        let buffer_offset = self.runtime_scratch_buffer_offset();
        if func == "__ckb_hash_blake2b_packed" {
            self.emit(format!(
                "# cellscript abi: hash_blake2b_packed type={} packed_len={} preimage_len={}",
                type_name.as_deref().unwrap_or_default(),
                width,
                preimage_len
            ));
        } else {
            self.emit(format!("# cellscript abi: ckb::hash_data_packed packed_len={} preimage_len={}", width, preimage_len));
        }
        self.emit_sp_addi("t4", buffer_offset);
        for (index, byte) in header.iter().enumerate() {
            self.emit(format!("li t0, {}", byte));
            self.emit(format!("sb t0, {}(t4)", index));
        }
        if !self.emit_packed_operand_bytes_to_scratch(arg, width, buffer_offset + header.len(), "hash_blake2b_packed input") {
            self.emit("# cellscript abi: fail closed because packed hash input is not materializable as fixed bytes");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        }

        self.emit_sp_addi("a0", buffer_offset);
        self.emit(format!("li a1, {}", preimage_len));
        self.emit_sp_addi("a2", dest_offset);
        self.emit("call __ckb_hash_blake2b_var");
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_sp_addi("t0", dest_offset);
        self.emit_stack_store("t0", dest.id * 8);
        Ok(true)
    }

    fn emit_packed_operand_bytes_to_scratch(&mut self, operand: &IrOperand, width: usize, dest_offset: usize, context: &str) -> bool {
        if let Some(source) = self.expected_fixed_byte_source(operand, width) {
            self.emit_prepare_fixed_byte_source(&source, width, context);
            if !self.emit_fixed_byte_source_pointer_or_const_to("t5", &source) {
                return false;
            }
            self.emit_sp_addi("t4", dest_offset);
            self.emit_copy_fixed_bytes_from_t5_to_t4(width);
            return true;
        }

        let IrOperand::Var(var) = operand else {
            return false;
        };
        let Some(fields) = self.tuple_aggregate_fields.get(&var.id).cloned() else {
            return false;
        };
        let mut offset = 0usize;
        for field in fields {
            let Some(field_width) = packed_hash_operand_width(self, &field) else {
                return false;
            };
            if !self.emit_packed_operand_bytes_to_scratch(&field, field_width, dest_offset + offset, context) {
                return false;
            }
            let Some(next) = offset.checked_add(field_width) else {
                return false;
            };
            offset = next;
        }
        offset == width
    }

    fn emit_copy_fixed_bytes_from_t5_to_t4(&mut self, width: usize) {
        self.emit(format!("li t0, {}", width));
        self.emit("li t1, 0");
        let loop_label = self.fresh_label("packed_hash_copy_loop");
        let done_label = self.fresh_label("packed_hash_copy_done");
        self.emit_label(&loop_label);
        self.emit(format!("beq t1, t0, {}", done_label));
        self.emit("add t2, t5, t1");
        self.emit("lbu t3, 0(t2)");
        self.emit("add t2, t4, t1");
        self.emit("sb t3, 0(t2)");
        self.emit("addi t1, t1, 1");
        self.emit(format!("j {}", loop_label));
        self.emit_label(&done_label);
    }

    fn emit_ckb_fixed_hash_query_call(&mut self, dest: Option<&IrVar>, func: &str, args: &[IrOperand]) -> Result<bool> {
        if !matches!(func, "__ckb_input_previous_tx_hash" | "__ckb_cell_data_hash" | "__ckb_cell_lock_args32") {
            return Ok(false);
        }
        self.emit(format!("# call {}", func));
        let Some(dest) = dest else {
            self.emit("# cellscript abi: fail closed because CKB hash query result has no destination");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(source_view) = args.first() else {
            self.emit("# cellscript abi: fail closed because CKB hash query is missing source view");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        let Some(dest_offset) = self.fixed_byte_local_offsets.get(&dest.id).copied() else {
            self.emit("# cellscript abi: fail closed because CKB hash query output buffer was not allocated");
            self.emit_fail(CellScriptRuntimeError::FixedByteComparisonUnresolved);
            return Ok(true);
        };
        self.emit_operand_to_register("a0", source_view);
        self.emit_sp_addi("a1", dest_offset);
        self.emit(format!("call {}", func));
        self.emit_checked_runtime_status(func);
        self.emit_sp_addi("t0", dest_offset);
        self.emit_stack_store("t0", dest.id * 8);
        Ok(true)
    }

    pub(crate) fn emit_call(&mut self, dest: Option<&IrVar>, func: &str, args: &[IrOperand]) -> Result<()> {
        if self.emit_fixed_u64_le_call(dest, func, args)? {
            return Ok(());
        }
        if self.emit_hash_blake2b_packed_call(dest, func, args)? {
            return Ok(());
        }
        if self.emit_ckb_fixed_hash_query_call(dest, func, args)? {
            return Ok(());
        }
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
        let abi_arg_count = call_abi_arg_count(abi.as_ref(), args);
        let outgoing_stack_arg_value_bytes = abi_arg_count.saturating_sub(8) * 8;
        let outgoing_stack_arg_bytes = outgoing_stack_arg_bytes(abi_arg_count);
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
            self.emit_copy_outgoing_call_stack_args(outgoing_stack_arg_bytes, outgoing_stack_arg_value_bytes);
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
        let status_reg = ckb_checked_runtime_status_reg(func);
        self.emit(format!("# cellscript abi: {} returns status in {}; fail closed on nonzero", func, status_reg));
        self.emit(format!("beqz {}, {}", status_reg, ok_label));
        self.emit(format!("mv a0, {}", status_reg));
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
        if stack_slot_offset + 8 > outgoing_stack_arg_bytes || stack_slot_offset + 8 > self.outgoing_stack_arg_staging_bytes {
            self.record_fatal_error(format!("call stack arg{} exceeds outgoing ABI staging area", abi_index));
            return;
        }
        let Some(staging_offset) = self.outgoing_stack_arg_staging_offset.checked_add(stack_slot_offset) else {
            self.record_fatal_error("call stack argument staging offset overflow");
            return;
        };
        self.emit(format!("# cellscript abi: stage outgoing stack arg{} in frame staging +{}", abi_index, stack_slot_offset));
        self.emit_stack_store(register, staging_offset);
    }

    fn emit_copy_outgoing_call_stack_args(&mut self, outgoing_stack_arg_bytes: usize, outgoing_stack_arg_value_bytes: usize) {
        for stack_slot_offset in (0..outgoing_stack_arg_value_bytes).step_by(8) {
            let Some(source_offset) = outgoing_stack_arg_bytes
                .checked_add(self.outgoing_stack_arg_staging_offset)
                .and_then(|offset| offset.checked_add(stack_slot_offset))
            else {
                self.record_fatal_error("call stack argument copy offset overflow");
                return;
            };
            self.emit(format!("# cellscript abi: copy staged stack arg at +{}", stack_slot_offset));
            self.emit_stack_load("t0", source_offset);
            self.emit_stack_store("t0", stack_slot_offset);
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

fn fixed_u64_le_operand_width(operand: &IrOperand) -> Option<usize> {
    match operand {
        IrOperand::Const(IrConst::Address(_)) | IrOperand::Const(IrConst::Hash(_)) => Some(32),
        IrOperand::Const(IrConst::Array(values)) if values.iter().all(|value| matches!(value, IrConst::U8(_))) => Some(values.len()),
        IrOperand::Var(var) => match &var.ty {
            IrType::Address | IrType::Hash => Some(32),
            IrType::Array(inner, len) if matches!(inner.as_ref(), IrType::U8) => Some(*len),
            _ => None,
        },
        _ => None,
    }
}

fn packed_hash_operand_width(codegen: &CodeGenerator, operand: &IrOperand) -> Option<usize> {
    match operand {
        IrOperand::Const(IrConst::Bool(_)) | IrOperand::Const(IrConst::U8(_)) => Some(1),
        IrOperand::Const(IrConst::U16(_)) => Some(2),
        IrOperand::Const(IrConst::U32(_)) => Some(4),
        IrOperand::Const(IrConst::U64(_)) => Some(8),
        IrOperand::Const(IrConst::U128(_)) => Some(16),
        IrOperand::Const(IrConst::Address(_)) | IrOperand::Const(IrConst::Hash(_)) => Some(32),
        IrOperand::Const(IrConst::Array(values)) if values.iter().all(|value| matches!(value, IrConst::U8(_))) => Some(values.len()),
        IrOperand::Var(var) => codegen.fixed_byte_like_width(&var.ty),
        _ => None,
    }
}

fn packed_hash_operand_type_name(operand: &IrOperand) -> Option<String> {
    match operand {
        IrOperand::Const(value) => Some(canonical_ir_type_name(&const_ir_type(value))),
        IrOperand::Var(var) => Some(canonical_ir_type_name(&var.ty)),
    }
}

fn canonical_ir_type_name(ty: &IrType) -> String {
    match storage_type(ty) {
        IrType::U8 => "u8".to_string(),
        IrType::U16 => "u16".to_string(),
        IrType::U32 => "u32".to_string(),
        IrType::U64 => "u64".to_string(),
        IrType::U128 => "u128".to_string(),
        IrType::Bool => "bool".to_string(),
        IrType::Unit => "()".to_string(),
        IrType::Address => "Address".to_string(),
        IrType::Hash => "Hash".to_string(),
        IrType::Array(inner, len) => format!("[{};{}]", canonical_ir_type_name(inner), len),
        IrType::Tuple(items) => {
            let items = items.iter().map(canonical_ir_type_name).collect::<Vec<_>>().join(",");
            format!("({items})")
        }
        IrType::Named(name) => name.clone(),
        IrType::Ref(inner) | IrType::MutRef(inner) => canonical_ir_type_name(inner),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::{CodeGenerator, CodegenOptions};

    #[test]
    fn fixed_u64_le_width_accepts_hashes_and_byte_arrays() {
        assert_eq!(fixed_u64_le_operand_width(&IrOperand::Const(IrConst::Hash([0; 32]))), Some(32));
        assert_eq!(fixed_u64_le_operand_width(&IrOperand::Const(IrConst::Array(vec![IrConst::U8(1), IrConst::U8(2)]))), Some(2));
        assert_eq!(fixed_u64_le_operand_width(&IrOperand::Const(IrConst::U64(1))), None);
    }

    #[test]
    fn packed_hash_width_uses_codegen_fixed_byte_type_rules() {
        let codegen = CodeGenerator::new(CodegenOptions::default());
        let operand = IrOperand::Var(IrVar { id: 0, name: "hash".to_string(), ty: IrType::Hash });

        assert_eq!(packed_hash_operand_width(&codegen, &operand), Some(32));
        assert_eq!(packed_hash_operand_type_name(&operand), Some("Hash".to_string()));
    }

    #[test]
    fn canonical_type_names_strip_reference_wrappers() {
        assert_eq!(canonical_ir_type_name(&IrType::Ref(Box::new(IrType::U64))), "u64");
        assert_eq!(
            canonical_ir_type_name(&IrType::Tuple(vec![IrType::Bool, IrType::Array(Box::new(IrType::U8), 4)])),
            "(bool,[u8;4])"
        );
    }
}
