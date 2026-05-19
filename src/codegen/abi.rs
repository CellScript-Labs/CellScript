//! ABI, calling convention, and entry witness envelope for CellScript codegen.
//!
//! Contains the `CallableAbi` registry, entry witness frame layout constants,
//! witness wrapper emission, ABI parameter marshalling helpers, and the
//! parameter-counting functions that bridge IR parameters to RISC-V64 calling
//! conventions.

use std::collections::{BTreeSet, HashMap};

use crate::ast::ParamSource;
use crate::error::Result;
use crate::ir::*;
use crate::ENTRY_WITNESS_ABI_MAGIC;

use super::{
    fixed_aggregate_pointer_param_width, fixed_byte_pointer_param_width, fixed_register_width, named_type_name, type_static_length,
    CellScriptRuntimeError, CodeGenerator,
};
pub(crate) const ENTRY_WITNESS_LABEL: &str = "_cellscript_entry";
pub(crate) const ENTRY_WITNESS_MAGIC: &[u8; 8] = ENTRY_WITNESS_ABI_MAGIC;
pub(crate) const ENTRY_WITNESS_HEADER_SIZE: usize = 8;
pub(crate) const ENTRY_WITNESS_BUFFER_SIZE: usize = 1024;
pub(crate) const ENTRY_SCRIPT_SIZE_OFFSET: usize = ENTRY_WITNESS_BUFFER_OFFSET + ENTRY_WITNESS_BUFFER_SIZE;
pub(crate) const ENTRY_SCRIPT_ARGS_START_OFFSET: usize = ENTRY_SCRIPT_SIZE_OFFSET + 8;
pub(crate) const ENTRY_SCRIPT_ARGS_LEN_OFFSET: usize = ENTRY_SCRIPT_ARGS_START_OFFSET + 8;
pub(crate) const ENTRY_SCRIPT_ARGS_CURSOR_OFFSET: usize = ENTRY_SCRIPT_ARGS_LEN_OFFSET + 8;
pub(crate) const ENTRY_SCRIPT_BUFFER_OFFSET: usize = ENTRY_SCRIPT_ARGS_CURSOR_OFFSET + 8;
pub(crate) const ENTRY_SCRIPT_BUFFER_SIZE: usize = 1024;
pub(crate) const ENTRY_WITNESS_FRAME_SIZE: usize = 2304;
pub(crate) const ENTRY_WITNESS_SIZE_OFFSET: usize = 0;
pub(crate) const ENTRY_WITNESS_BUFFER_OFFSET: usize = 8;
pub(crate) const ENTRY_WITNESS_RA_OFFSET: usize = ENTRY_WITNESS_FRAME_SIZE - 8;
#[derive(Debug, Clone)]
pub(crate) struct CallableAbi {
    pub(crate) params: Vec<IrParam>,
    pub(crate) type_hash_param_indices: BTreeSet<usize>,
    pub(crate) runtime_bound_param_indices: BTreeSet<usize>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum CallLengthKind {
    Schema,
    FixedBytes,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct EntryWitnessPayloadArg {
    pub(crate) width: usize,
    pub(crate) schema_dynamic: bool,
    pub(crate) unsupported: bool,
}
pub(crate) fn abi_arg_label(index: usize) -> String {
    if index < 8 {
        format!("a{}", index)
    } else {
        format!("stack+{}", (index - 8) * 8)
    }
}

pub(crate) fn call_abi_arg_count(abi: Option<&CallableAbi>, args: &[IrOperand]) -> usize {
    let mut count = 0usize;
    for (arg_index, _) in args.iter().enumerate() {
        if let Some(abi) = abi {
            if let Some(param) = abi.params.get(arg_index) {
                count += call_param_abi_arg_count(param, abi.type_hash_param_indices.contains(&arg_index));
                continue;
            }
        }
        count += 1;
    }
    count
}

pub(crate) fn entry_abi_arg_count(params: &[IrParam], abi: Option<&CallableAbi>) -> usize {
    let type_hash_param_indices = abi.map(|abi| &abi.type_hash_param_indices);
    params
        .iter()
        .enumerate()
        .map(|(index, param)| call_param_abi_arg_count(param, type_hash_param_indices.is_some_and(|indices| indices.contains(&index))))
        .sum()
}

pub(crate) fn call_param_abi_arg_count(param: &IrParam, needs_type_hash: bool) -> usize {
    if named_type_name(&param.ty).is_some() {
        return 2 + usize::from(needs_type_hash) * 2;
    }
    if fixed_byte_pointer_param_width(&param.ty).or_else(|| fixed_aggregate_pointer_param_width(&param.ty)).is_some() {
        return 2;
    }
    1
}
pub(crate) fn entry_witness_payload_layout(
    params: &[IrParam],
    runtime_bound_param_indices: &BTreeSet<usize>,
) -> Vec<EntryWitnessPayloadArg> {
    params
        .iter()
        .enumerate()
        .map(|(index, param)| {
            if !entry_param_consumes_witness_payload(param, index, runtime_bound_param_indices) {
                EntryWitnessPayloadArg { width: 0, schema_dynamic: false, unsupported: false }
            } else if entry_witness_dynamic_schema_param(&param.ty) {
                EntryWitnessPayloadArg { width: 4, schema_dynamic: true, unsupported: false }
            } else if let Some(width) =
                fixed_byte_pointer_param_width(&param.ty).or_else(|| fixed_aggregate_pointer_param_width(&param.ty))
            {
                EntryWitnessPayloadArg { width, schema_dynamic: false, unsupported: false }
            } else if let Some(width) = entry_witness_register_param_width(&param.ty) {
                EntryWitnessPayloadArg { width, schema_dynamic: false, unsupported: false }
            } else {
                EntryWitnessPayloadArg { width: 0, schema_dynamic: false, unsupported: true }
            }
        })
        .collect()
}

fn entry_param_consumes_witness_payload(param: &IrParam, index: usize, runtime_bound_param_indices: &BTreeSet<usize>) -> bool {
    param.source != ParamSource::LockArgs
        && !runtime_bound_param_indices.contains(&index)
        && !matches!(param.ty, IrType::Ref(_) | IrType::MutRef(_))
}

fn entry_witness_dynamic_schema_param(ty: &IrType) -> bool {
    fixed_byte_pointer_param_width(ty).is_none()
        && fixed_aggregate_pointer_param_width(ty).is_none()
        && entry_witness_register_param_width(ty).is_none()
}

fn entry_witness_register_param_width(ty: &IrType) -> Option<usize> {
    fixed_register_width(ty, type_static_length(ty)).or_else(|| match ty {
        IrType::Array(_, _) | IrType::Tuple(_) => type_static_length(ty).filter(|width| (1..=8).contains(width)),
        IrType::Unit => Some(0),
        _ => None,
    })
}
// ---------------------------------------------------------------------------
// CodeGenerator ABI methods
// ---------------------------------------------------------------------------

impl CodeGenerator {
    pub(crate) fn register_callable_abis(&mut self, ir: &IrModule) {
        self.callable_abis.clear();
        for item in &ir.items {
            let (name, params, body) = match item {
                IrItem::Action(action) => (&action.name, &action.params, &action.body),
                IrItem::PureFn(function) => (&function.name, &function.params, &function.body),
                IrItem::Lock(lock) => (&lock.name, &lock.params, &lock.body),
                IrItem::TypeDef(_) | IrItem::Invariant(_) => continue,
            };
            let param_indices = params.iter().enumerate().map(|(index, param)| (param.binding.id, index)).collect::<HashMap<_, _>>();
            let mut type_hash_param_indices = BTreeSet::new();
            let mut runtime_bound_param_indices = params
                .iter()
                .enumerate()
                .filter_map(|(index, param)| self.param_is_runtime_bound(param).then_some(index))
                .collect::<BTreeSet<_>>();
            for pattern in body.consume_set.iter().chain(body.read_refs.iter()) {
                if let Some(param) = params.iter().position(|param| param.name == pattern.binding) {
                    runtime_bound_param_indices.insert(param);
                }
            }
            for pattern in &body.mutate_set {
                if let Some(param) = params.iter().position(|param| param.name == pattern.binding) {
                    runtime_bound_param_indices.insert(param);
                }
            }
            for block in &body.blocks {
                for instruction in &block.instructions {
                    if let IrInstruction::TypeHash { operand: IrOperand::Var(var), .. } = instruction {
                        if let Some(index) = param_indices.get(&var.id).copied() {
                            type_hash_param_indices.insert(index);
                        }
                    }
                }
            }
            self.callable_abis
                .insert(name.clone(), CallableAbi { params: params.clone(), type_hash_param_indices, runtime_bound_param_indices });
        }
        for external in &ir.external_callable_abis {
            if self.callable_abis.contains_key(&external.name) {
                continue;
            }
            let runtime_bound_param_indices = external
                .params
                .iter()
                .enumerate()
                .filter_map(|(index, param)| self.param_is_runtime_bound(param).then_some(index))
                .collect();
            self.callable_abis.insert(
                external.name.clone(),
                CallableAbi {
                    params: external.params.clone(),
                    type_hash_param_indices: external.type_hash_param_indices.clone(),
                    runtime_bound_param_indices,
                },
            );
        }
    }
    pub(crate) fn emit_entry_abi_marker(&mut self, name: &str) {
        self.assembly.push(format!("# cellscript entry abi: {} requires-explicit-parameter-abi", name));
    }

    pub(crate) fn emit_entry_direct_wrapper(&mut self, target: &str) {
        self.emit_global(ENTRY_WITNESS_LABEL);
        self.emit_label(ENTRY_WITNESS_LABEL);
        self.emit(format!("# cellscript entry abi: {} tail-calls no-arg {}", ENTRY_WITNESS_LABEL, target));
        self.emit(format!("j {}", target));
    }

    pub(crate) fn emit_entry_witness_wrapper(&mut self, target: &str, params: &[IrParam]) -> Result<()> {
        let callable_abi = self.callable_abis.get(target).cloned();
        let type_hash_param_indices = callable_abi.as_ref().map(|abi| abi.type_hash_param_indices.clone()).unwrap_or_default();
        let runtime_bound_param_indices = callable_abi.as_ref().map(|abi| abi.runtime_bound_param_indices.clone()).unwrap_or_default();
        let outgoing_stack_arg_bytes = entry_abi_arg_count(params, callable_abi.as_ref()).saturating_sub(8) * 8;
        let payload = entry_witness_payload_layout(params, &runtime_bound_param_indices);
        let payload_len = payload.iter().map(|arg| arg.width).sum::<usize>();
        let has_witness_payload = payload.iter().any(|arg| arg.width > 0 || arg.unsupported);
        let has_lock_args = params.iter().any(|param| param.source == ParamSource::LockArgs);
        let has_dynamic_payload = payload.iter().any(|arg| arg.schema_dynamic);
        let min_witness_len = ENTRY_WITNESS_HEADER_SIZE + payload_len;
        let loaded_label = self.fresh_label("entry_witness_loaded");
        let buffer_ok_label = self.fresh_label("entry_witness_buffer_ok");
        let size_ok_label = self.fresh_label("entry_witness_size_ok");
        let fail_label = self.fresh_label("entry_witness_fail");
        let done_label = self.fresh_label("entry_witness_done");

        self.emit_global(ENTRY_WITNESS_LABEL);
        self.emit_label(ENTRY_WITNESS_LABEL);
        self.emit(format!("# cellscript entry abi: {} loads GroupInput witness args for {}", ENTRY_WITNESS_LABEL, target));
        self.emit("# cellscript entry abi: witness magic CSARGv1 followed by positional fixed/scalar payload");
        self.emit_large_addi("sp", "sp", -(ENTRY_WITNESS_FRAME_SIZE as i64));
        self.emit_stack_store("ra", ENTRY_WITNESS_RA_OFFSET);
        if has_lock_args {
            self.emit_entry_load_script_args(&fail_label);
        }
        if has_witness_payload {
            self.emit_load_witness_syscall_to_offsets(
                "entry_args",
                self.runtime_abi().source_group_input,
                0,
                ENTRY_WITNESS_SIZE_OFFSET,
                ENTRY_WITNESS_BUFFER_OFFSET,
                ENTRY_WITNESS_BUFFER_SIZE,
            );
            self.emit(format!("beqz a0, {}", loaded_label));
            self.emit(format!("j {}", fail_label));
            self.emit_label(&loaded_label);

            self.emit_stack_load("t0", ENTRY_WITNESS_SIZE_OFFSET);
            self.emit("# cellscript entry abi: reject witnesses larger than the local entry buffer");
            self.emit(format!("li t1, {}", ENTRY_WITNESS_BUFFER_SIZE + 1));
            self.emit("sltu t2, t0, t1");
            self.emit(format!("bnez t2, {}", buffer_ok_label));
            self.emit(format!("j {}", fail_label));
            self.emit_label(&buffer_ok_label);
            self.emit(format!("li t1, {}", min_witness_len));
            self.emit("sltu t2, t0, t1");
            self.emit(format!("beqz t2, {}", size_ok_label));
            self.emit(format!("j {}", fail_label));
            self.emit_label(&size_ok_label);

            for (index, byte) in ENTRY_WITNESS_MAGIC.iter().enumerate() {
                self.emit_stack_load_byte("t0", ENTRY_WITNESS_BUFFER_OFFSET + index);
                self.emit(format!("li t1, {}", byte));
                self.emit("sub t2, t0, t1");
                self.emit(format!("bnez t2, {}", fail_label));
            }

            if !has_dynamic_payload {
                let exact_size_label = self.fresh_label("entry_witness_exact_size_ok");
                self.emit("# cellscript entry abi: reject trailing witness payload bytes");
                self.emit_stack_load("t0", ENTRY_WITNESS_SIZE_OFFSET);
                self.emit(format!("li t1, {}", min_witness_len));
                self.emit("sub t2, t0, t1");
                self.emit(format!("beqz t2, {}", exact_size_label));
                self.emit(format!("j {}", fail_label));
                self.emit_label(&exact_size_label);
            }
        }

        if payload.iter().any(|arg| arg.unsupported) {
            self.emit("# cellscript entry abi: unsupported witness parameter shape; fail closed");
            self.emit(format!("j {}", fail_label));
        } else if has_dynamic_payload {
            let mut abi_index = 0usize;
            self.emit("# cellscript entry abi: witness payload contains schema-backed dynamic segments");
            self.emit_stack_load("t5", ENTRY_WITNESS_SIZE_OFFSET);
            self.emit(format!("li t6, {}", ENTRY_WITNESS_HEADER_SIZE));
            for (param_index, param) in params.iter().enumerate() {
                let param_is_runtime_bound =
                    runtime_bound_param_indices.contains(&param_index) || matches!(param.ty, IrType::Ref(_) | IrType::MutRef(_));
                if param.source == ParamSource::LockArgs {
                    self.emit_entry_lock_args_param(&mut abi_index, param, outgoing_stack_arg_bytes, &fail_label);
                } else if param_is_runtime_bound {
                    self.emit(format!("# cellscript entry abi: runtime-bound param {} is loaded from transaction cells", param.name));
                    self.emit_entry_abi_zero_arg(abi_index, outgoing_stack_arg_bytes);
                    self.emit_entry_abi_zero_arg(abi_index + 1, outgoing_stack_arg_bytes);
                    abi_index += 2;
                    if type_hash_param_indices.contains(&param_index) {
                        self.emit(format!(
                            "# cellscript entry abi: runtime-bound param {} TypeHash witness bytes unavailable; pass null ABI bytes",
                            param.name
                        ));
                        self.emit_entry_abi_zero_arg(abi_index, outgoing_stack_arg_bytes);
                        self.emit_entry_abi_zero_arg(abi_index + 1, outgoing_stack_arg_bytes);
                        abi_index += 2;
                    }
                } else if entry_witness_dynamic_schema_param(&param.ty) {
                    let len_ok_label = self.fresh_label("entry_witness_schema_len_ok");
                    let bytes_ok_label = self.fresh_label("entry_witness_schema_bytes_ok");
                    self.emit(format!(
                        "# cellscript entry abi: schema param {} -> {}={} {}={} (length-prefixed witness bytes)",
                        param.name,
                        abi_arg_label(abi_index),
                        "ptr",
                        abi_arg_label(abi_index + 1),
                        "len"
                    ));
                    self.emit("addi t1, t6, 4");
                    self.emit("sltu t2, t5, t1");
                    self.emit(format!("beqz t2, {}", len_ok_label));
                    self.emit(format!("j {}", fail_label));
                    self.emit_label(&len_ok_label);
                    self.emit("add t0, sp, t6");
                    self.emit(format!("addi t0, t0, {}", ENTRY_WITNESS_BUFFER_OFFSET));
                    self.emit("li t4, 0");
                    for byte_index in 0..4 {
                        self.emit(format!("lbu t1, {}(t0)", byte_index));
                        if byte_index != 0 {
                            self.emit(format!("slli t1, t1, {}", byte_index * 8));
                        }
                        self.emit("or t4, t4, t1");
                    }
                    self.emit("addi t1, t6, 4");
                    self.emit("add t1, t1, t4");
                    self.emit("sltu t2, t5, t1");
                    self.emit(format!("beqz t2, {}", bytes_ok_label));
                    self.emit(format!("j {}", fail_label));
                    self.emit_label(&bytes_ok_label);
                    self.emit_entry_abi_pointer_from_dynamic_offset(abi_index, "t6", 4, "t0", outgoing_stack_arg_bytes);
                    self.emit_entry_abi_reg_arg(abi_index + 1, "t4", outgoing_stack_arg_bytes);
                    abi_index += 2;
                    self.emit("addi t6, t6, 4");
                    self.emit("add t6, t6, t4");
                    if type_hash_param_indices.contains(&param_index) {
                        self.emit(format!(
                            "# cellscript entry abi: schema param {} TypeHash witness bytes unavailable; pass null ABI bytes",
                            param.name
                        ));
                        self.emit_entry_abi_zero_arg(abi_index, outgoing_stack_arg_bytes);
                        self.emit_entry_abi_zero_arg(abi_index + 1, outgoing_stack_arg_bytes);
                        abi_index += 2;
                    }
                } else if let Some(width) =
                    fixed_byte_pointer_param_width(&param.ty).or_else(|| fixed_aggregate_pointer_param_width(&param.ty))
                {
                    let bytes_ok_label = self.fresh_label("entry_witness_fixed_bytes_ok");
                    self.emit(format!(
                        "# cellscript entry abi: fixed-byte param {} pointer={} length={} size={}",
                        param.name,
                        abi_arg_label(abi_index),
                        abi_arg_label(abi_index + 1),
                        width
                    ));
                    self.emit(format!("addi t1, t6, {}", width));
                    self.emit("sltu t2, t5, t1");
                    self.emit(format!("beqz t2, {}", bytes_ok_label));
                    self.emit(format!("j {}", fail_label));
                    self.emit_label(&bytes_ok_label);
                    self.emit_entry_abi_pointer_from_dynamic_offset(abi_index, "t6", 0, "t0", outgoing_stack_arg_bytes);
                    self.emit_entry_abi_immediate_arg(abi_index + 1, width as u64, outgoing_stack_arg_bytes);
                    self.emit(format!("addi t6, t6, {}", width));
                    abi_index += 2;
                } else if let Some(width) = entry_witness_register_param_width(&param.ty) {
                    let bytes_ok_label = self.fresh_label("entry_witness_scalar_bytes_ok");
                    self.emit(format!(
                        "# cellscript entry abi: scalar param {} -> {} size={}",
                        param.name,
                        abi_arg_label(abi_index),
                        width
                    ));
                    self.emit(format!("addi t1, t6, {}", width));
                    self.emit("sltu t2, t5, t1");
                    self.emit(format!("beqz t2, {}", bytes_ok_label));
                    self.emit(format!("j {}", fail_label));
                    self.emit_label(&bytes_ok_label);
                    self.emit("add t0, sp, t6");
                    self.emit(format!("addi t0, t0, {}", ENTRY_WITNESS_BUFFER_OFFSET));
                    if abi_index < 8 {
                        self.emit_entry_witness_scalar_load_from_reg(&format!("a{}", abi_index), "t0", width);
                    } else {
                        let caller_stack_offset = (abi_index - 8) * 8;
                        self.emit_entry_witness_scalar_load_from_reg("t3", "t0", width);
                        self.emit(format!(
                            "# cellscript entry abi: scalar param {} stored to caller stack +{}",
                            param.name, caller_stack_offset
                        ));
                        self.emit_entry_abi_reg_arg(abi_index, "t3", outgoing_stack_arg_bytes);
                    }
                    self.emit(format!("addi t6, t6, {}", width));
                    abi_index += 1;
                } else {
                    self.emit(format!("# cellscript entry abi: unsupported param {} shape; fail closed", param.name));
                    self.emit(format!("j {}", fail_label));
                }
            }
            let exact_size_label = self.fresh_label("entry_witness_exact_size_ok");
            self.emit("# cellscript entry abi: reject trailing witness payload bytes");
            self.emit_stack_load("t5", ENTRY_WITNESS_SIZE_OFFSET);
            self.emit("sub t2, t5, t6");
            self.emit(format!("beqz t2, {}", exact_size_label));
            self.emit(format!("j {}", fail_label));
            self.emit_label(&exact_size_label);
            if has_lock_args {
                self.emit_entry_lock_args_exact_size_check(&fail_label);
            }
            self.emit_entry_call_target(target, outgoing_stack_arg_bytes);
            self.emit(format!("j {}", done_label));
        } else {
            let mut abi_index = 0usize;
            let mut payload_cursor = 0usize;
            for (param_index, param) in params.iter().enumerate() {
                let param_is_runtime_bound =
                    runtime_bound_param_indices.contains(&param_index) || matches!(param.ty, IrType::Ref(_) | IrType::MutRef(_));
                if param.source == ParamSource::LockArgs {
                    self.emit_entry_lock_args_param(&mut abi_index, param, outgoing_stack_arg_bytes, &fail_label);
                } else if param_is_runtime_bound {
                    self.emit(format!("# cellscript entry abi: runtime-bound param {} is loaded from transaction cells", param.name));
                    self.emit_entry_abi_zero_arg(abi_index, outgoing_stack_arg_bytes);
                    self.emit_entry_abi_zero_arg(abi_index + 1, outgoing_stack_arg_bytes);
                    abi_index += 2;
                    if type_hash_param_indices.contains(&param_index) {
                        self.emit(format!(
                            "# cellscript entry abi: runtime-bound param {} TypeHash witness bytes unavailable; pass null ABI bytes",
                            param.name
                        ));
                        self.emit_entry_abi_zero_arg(abi_index, outgoing_stack_arg_bytes);
                        self.emit_entry_abi_zero_arg(abi_index + 1, outgoing_stack_arg_bytes);
                        abi_index += 2;
                    }
                } else if entry_witness_dynamic_schema_param(&param.ty) {
                    self.emit(format!("# cellscript entry abi: schema param {} is runtime-loaded; pass null ABI bytes", param.name));
                    self.emit_entry_abi_zero_arg(abi_index, outgoing_stack_arg_bytes);
                    self.emit_entry_abi_zero_arg(abi_index + 1, outgoing_stack_arg_bytes);
                    abi_index += 2;
                    if type_hash_param_indices.contains(&param_index) {
                        self.emit(format!(
                            "# cellscript entry abi: schema param {} TypeHash witness bytes unavailable; pass null ABI bytes",
                            param.name
                        ));
                        self.emit_entry_abi_zero_arg(abi_index, outgoing_stack_arg_bytes);
                        self.emit_entry_abi_zero_arg(abi_index + 1, outgoing_stack_arg_bytes);
                        abi_index += 2;
                    }
                } else if let Some(width) =
                    fixed_byte_pointer_param_width(&param.ty).or_else(|| fixed_aggregate_pointer_param_width(&param.ty))
                {
                    self.emit(format!(
                        "# cellscript entry abi: fixed-byte param {} pointer={} length={} size={}",
                        param.name,
                        abi_arg_label(abi_index),
                        abi_arg_label(abi_index + 1),
                        width
                    ));
                    self.emit_entry_abi_pointer_arg(
                        abi_index,
                        ENTRY_WITNESS_BUFFER_OFFSET + ENTRY_WITNESS_HEADER_SIZE + payload_cursor,
                        outgoing_stack_arg_bytes,
                    );
                    self.emit_entry_abi_immediate_arg(abi_index + 1, width as u64, outgoing_stack_arg_bytes);
                    payload_cursor += width;
                    abi_index += 2;
                } else if let Some(width) = entry_witness_register_param_width(&param.ty) {
                    self.emit(format!(
                        "# cellscript entry abi: scalar param {} -> {} size={}",
                        param.name,
                        abi_arg_label(abi_index),
                        width
                    ));
                    let stack_offset = ENTRY_WITNESS_BUFFER_OFFSET + ENTRY_WITNESS_HEADER_SIZE + payload_cursor;
                    if abi_index < 8 {
                        self.emit_entry_witness_scalar_load(&format!("a{}", abi_index), stack_offset, width);
                    } else {
                        let caller_stack_offset = (abi_index - 8) * 8;
                        self.emit_entry_witness_scalar_load("t3", stack_offset, width);
                        self.emit(format!(
                            "# cellscript entry abi: scalar param {} stored to caller stack +{}",
                            param.name, caller_stack_offset
                        ));
                        self.emit_entry_abi_reg_arg(abi_index, "t3", outgoing_stack_arg_bytes);
                    }
                    payload_cursor += width;
                    abi_index += 1;
                } else {
                    self.emit(format!("# cellscript entry abi: unsupported param {} shape; fail closed", param.name));
                    self.emit(format!("j {}", fail_label));
                }
            }
            if has_lock_args {
                self.emit_entry_lock_args_exact_size_check(&fail_label);
            }
            self.emit_entry_call_target(target, outgoing_stack_arg_bytes);
            self.emit(format!("j {}", done_label));
        }

        self.emit_label(&fail_label);
        self.emit_runtime_error_comment(CellScriptRuntimeError::EntryWitnessAbiInvalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::EntryWitnessAbiInvalid.code()));
        self.emit_label(&done_label);
        self.emit_stack_load("ra", ENTRY_WITNESS_RA_OFFSET);
        self.emit_large_addi("sp", "sp", ENTRY_WITNESS_FRAME_SIZE as i64);
        self.emit("ret");
        Ok(())
    }

    fn emit_entry_call_target(&mut self, target: &str, outgoing_stack_arg_bytes: usize) {
        if outgoing_stack_arg_bytes > 0 {
            self.emit(format!("# cellscript entry abi: reserve {} bytes for outgoing stack call arguments", outgoing_stack_arg_bytes));
            self.emit_large_addi("sp", "sp", -(outgoing_stack_arg_bytes as i64));
        }
        self.emit(format!("call {}", target));
        if outgoing_stack_arg_bytes > 0 {
            self.emit_large_addi("sp", "sp", outgoing_stack_arg_bytes as i64);
        }
    }

    fn emit_entry_abi_zero_arg(&mut self, abi_index: usize, outgoing_stack_arg_bytes: usize) {
        self.emit_entry_abi_immediate_arg(abi_index, 0, outgoing_stack_arg_bytes);
    }

    fn emit_entry_abi_reg_arg(&mut self, abi_index: usize, source_reg: &str, outgoing_stack_arg_bytes: usize) {
        if abi_index < 8 {
            self.emit(format!("addi a{}, {}, 0", abi_index, source_reg));
        } else {
            self.emit_entry_outgoing_stack_arg_store(source_reg, abi_index, outgoing_stack_arg_bytes);
        }
    }

    fn emit_entry_abi_immediate_arg(&mut self, abi_index: usize, value: u64, outgoing_stack_arg_bytes: usize) {
        if abi_index < 8 {
            self.emit(format!("li a{}, {}", abi_index, value));
        } else {
            self.emit(format!("# cellscript entry abi: stack arg{} <- {}", abi_index, value));
            self.emit(format!("li t0, {}", value));
            self.emit_entry_outgoing_stack_arg_store("t0", abi_index, outgoing_stack_arg_bytes);
        }
    }

    fn emit_entry_abi_pointer_arg(&mut self, abi_index: usize, stack_offset: usize, outgoing_stack_arg_bytes: usize) {
        if abi_index < 8 {
            self.emit_sp_addi(&format!("a{}", abi_index), stack_offset);
        } else {
            self.emit(format!("# cellscript entry abi: stack arg{} <- sp+{}", abi_index, stack_offset));
            self.emit_sp_addi("t0", stack_offset);
            self.emit_entry_outgoing_stack_arg_store("t0", abi_index, outgoing_stack_arg_bytes);
        }
    }

    fn emit_entry_abi_pointer_from_dynamic_offset(
        &mut self,
        abi_index: usize,
        offset_reg: &str,
        extra_offset: usize,
        temp_reg: &str,
        outgoing_stack_arg_bytes: usize,
    ) {
        self.emit(format!("add {}, sp, {}", temp_reg, offset_reg));
        if ENTRY_WITNESS_BUFFER_OFFSET + extra_offset != 0 {
            self.emit(format!("addi {}, {}, {}", temp_reg, temp_reg, ENTRY_WITNESS_BUFFER_OFFSET + extra_offset));
        }
        self.emit_entry_abi_reg_arg(abi_index, temp_reg, outgoing_stack_arg_bytes);
    }

    fn emit_entry_outgoing_stack_arg_store(&mut self, register: &str, abi_index: usize, outgoing_stack_arg_bytes: usize) {
        let stack_slot_offset = (abi_index - 8) * 8;
        let offset = i64::try_from(stack_slot_offset).expect("entry call stack slot should fit in i64")
            - i64::try_from(outgoing_stack_arg_bytes).expect("entry call stack argument area should fit in i64");
        self.emit(format!(
            "# cellscript entry abi: stage stack arg{} at pre-call sp{}{}",
            abi_index,
            if offset < 0 { "" } else { "+" },
            offset
        ));
        self.emit_sp_store_signed(register, offset);
    }

    fn emit_entry_witness_scalar_load(&mut self, dest_reg: &str, stack_offset: usize, width: usize) {
        self.emit(format!("li {}, 0", dest_reg));
        for byte_index in 0..width {
            self.emit_stack_load_byte("t0", stack_offset + byte_index);
            if byte_index != 0 {
                self.emit(format!("slli t0, t0, {}", byte_index * 8));
            }
            self.emit(format!("or {}, {}, t0", dest_reg, dest_reg));
        }
    }

    fn emit_entry_witness_scalar_load_from_reg(&mut self, dest_reg: &str, base_reg: &str, width: usize) {
        self.emit(format!("li {}, 0", dest_reg));
        for byte_index in 0..width {
            self.emit(format!("lbu t0, {}({})", byte_index, base_reg));
            if byte_index != 0 {
                self.emit(format!("slli t0, t0, {}", byte_index * 8));
            }
            self.emit(format!("or {}, {}, t0", dest_reg, dest_reg));
        }
    }

    fn emit_entry_load_u32_from_stack(&mut self, dest_reg: &str, stack_offset: usize) {
        self.emit(format!("li {}, 0", dest_reg));
        for byte_index in 0..4 {
            self.emit_stack_load_byte("t0", stack_offset + byte_index);
            if byte_index != 0 {
                self.emit(format!("slli t0, t0, {}", byte_index * 8));
            }
            self.emit(format!("or {}, {}, t0", dest_reg, dest_reg));
        }
    }

    fn emit_entry_load_u32_from_reg(&mut self, dest_reg: &str, base_reg: &str) {
        self.emit(format!("li {}, 0", dest_reg));
        for byte_index in 0..4 {
            self.emit(format!("lbu t0, {}({})", byte_index, base_reg));
            if byte_index != 0 {
                self.emit(format!("slli t0, t0, {}", byte_index * 8));
            }
            self.emit(format!("or {}, {}, t0", dest_reg, dest_reg));
        }
    }

    fn emit_entry_load_script_args(&mut self, fail_label: &str) {
        let loaded_label = self.fresh_label("entry_script_loaded");
        let buffer_ok_label = self.fresh_label("entry_script_buffer_ok");
        let total_ok_label = self.fresh_label("entry_script_total_ok");
        let table_header_ok_label = self.fresh_label("entry_script_table_header_ok");
        let args_offset_min_ok_label = self.fresh_label("entry_script_args_offset_min_ok");
        let args_offset_ok_label = self.fresh_label("entry_script_args_offset_ok");
        let args_span_ok_label = self.fresh_label("entry_script_args_span_ok");

        self.emit("# cellscript entry abi: lock_args parameters are decoded from the executing Script.args bytes");
        self.emit_load_script_syscall_to_offsets(
            "entry_lock_args",
            ENTRY_SCRIPT_SIZE_OFFSET,
            ENTRY_SCRIPT_BUFFER_OFFSET,
            ENTRY_SCRIPT_BUFFER_SIZE,
        );
        self.emit(format!("beqz a0, {}", loaded_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&loaded_label);

        self.emit_stack_load("t0", ENTRY_SCRIPT_SIZE_OFFSET);
        self.emit(format!("li t1, {}", ENTRY_SCRIPT_BUFFER_SIZE + 1));
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", buffer_ok_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&buffer_ok_label);

        self.emit_entry_load_u32_from_stack("t3", ENTRY_SCRIPT_BUFFER_OFFSET);
        self.emit_stack_load("t0", ENTRY_SCRIPT_SIZE_OFFSET);
        self.emit("sub t2, t0, t3");
        self.emit(format!("beqz t2, {}", total_ok_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&total_ok_label);

        self.emit("li t1, 16");
        self.emit("sltu t2, t3, t1");
        self.emit(format!("beqz t2, {}", table_header_ok_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&table_header_ok_label);

        self.emit_entry_load_u32_from_stack("t4", ENTRY_SCRIPT_BUFFER_OFFSET + 12);
        self.emit("li t1, 16");
        self.emit("sltu t2, t4, t1");
        self.emit(format!("beqz t2, {}", args_offset_min_ok_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&args_offset_min_ok_label);
        self.emit("addi t1, t4, 4");
        self.emit("sltu t2, t3, t1");
        self.emit(format!("beqz t2, {}", args_offset_ok_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&args_offset_ok_label);

        self.emit_sp_addi("t0", ENTRY_SCRIPT_BUFFER_OFFSET);
        self.emit("add t0, t0, t4");
        self.emit_entry_load_u32_from_reg("t5", "t0");
        self.emit("addi t6, t4, 4");
        self.emit("add t1, t6, t5");
        self.emit("sltu t2, t3, t1");
        self.emit(format!("beqz t2, {}", args_span_ok_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&args_span_ok_label);
        self.emit_stack_store("t6", ENTRY_SCRIPT_ARGS_START_OFFSET);
        self.emit_stack_store("t5", ENTRY_SCRIPT_ARGS_LEN_OFFSET);
        self.emit("li t0, 0");
        self.emit_stack_store("t0", ENTRY_SCRIPT_ARGS_CURSOR_OFFSET);
    }

    fn emit_entry_lock_args_param(
        &mut self,
        abi_index: &mut usize,
        param: &IrParam,
        outgoing_stack_arg_bytes: usize,
        fail_label: &str,
    ) {
        let fixed_byte_width = fixed_byte_pointer_param_width(&param.ty).or_else(|| fixed_aggregate_pointer_param_width(&param.ty));
        let scalar_width = entry_witness_register_param_width(&param.ty);
        let Some(width) = fixed_byte_width.or(scalar_width) else {
            self.emit(format!("# cellscript entry abi: unsupported lock_args param {} shape; fail closed", param.name));
            self.emit(format!("j {}", fail_label));
            return;
        };
        let bytes_ok_label = self.fresh_label("entry_lock_args_bytes_ok");
        self.emit(format!("# cellscript entry abi: lock_args param {} consumes {} script arg byte(s)", param.name, width));
        self.emit_stack_load("t6", ENTRY_SCRIPT_ARGS_CURSOR_OFFSET);
        self.emit_stack_load("t5", ENTRY_SCRIPT_ARGS_LEN_OFFSET);
        self.emit(format!("addi t1, t6, {}", width));
        self.emit("sltu t2, t5, t1");
        self.emit(format!("beqz t2, {}", bytes_ok_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&bytes_ok_label);
        self.emit_stack_load("t3", ENTRY_SCRIPT_ARGS_START_OFFSET);
        self.emit("add t3, t3, t6");
        self.emit_sp_addi("t0", ENTRY_SCRIPT_BUFFER_OFFSET);
        self.emit("add t0, t0, t3");

        if fixed_byte_width.is_some() {
            self.emit_entry_abi_reg_arg(*abi_index, "t0", outgoing_stack_arg_bytes);
            self.emit_entry_abi_immediate_arg(*abi_index + 1, width as u64, outgoing_stack_arg_bytes);
            *abi_index += 2;
        } else if *abi_index < 8 {
            self.emit_entry_witness_scalar_load_from_reg(&format!("a{}", *abi_index), "t0", width);
            *abi_index += 1;
        } else {
            self.emit_entry_witness_scalar_load_from_reg("t4", "t0", width);
            self.emit_entry_abi_reg_arg(*abi_index, "t4", outgoing_stack_arg_bytes);
            *abi_index += 1;
        }

        self.emit_stack_load("t6", ENTRY_SCRIPT_ARGS_CURSOR_OFFSET);
        self.emit(format!("addi t6, t6, {}", width));
        self.emit_stack_store("t6", ENTRY_SCRIPT_ARGS_CURSOR_OFFSET);
    }

    fn emit_entry_lock_args_exact_size_check(&mut self, fail_label: &str) {
        let exact_label = self.fresh_label("entry_lock_args_exact_size_ok");
        self.emit("# cellscript entry abi: reject trailing Script.args bytes after typed lock_args");
        self.emit_stack_load("t0", ENTRY_SCRIPT_ARGS_CURSOR_OFFSET);
        self.emit_stack_load("t1", ENTRY_SCRIPT_ARGS_LEN_OFFSET);
        self.emit("sub t2, t1, t0");
        self.emit(format!("beqz t2, {}", exact_label));
        self.emit(format!("j {}", fail_label));
        self.emit_label(&exact_label);
    }
}
