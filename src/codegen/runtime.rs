//! Runtime support emission for CellScript codegen.
//!
//! Contains runtime helper function emission (memcmp, memzero, size guards,
//! Blake2b-256 hash, CKB syscall wrappers, v0.14 surface helpers) and the
//! `generate_runtime_support` entry point that emits them into the `.text`
//! section.

use std::collections::BTreeSet;

use crate::ir::*;
use crate::syscalls::{
    checked_runtime_helper_spec, fail_closed_helper_spec, fail_closed_runtime_helper_specs, runtime_helper_symbols,
    source_constant_specs, vm2_helper_specs, SyscallSpec, CKB_LOAD_CELL_BY_FIELD_SYSCALL_NUMBER, CKB_LOAD_CELL_DATA_SYSCALL_NUMBER,
    CKB_LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER, CKB_LOAD_INPUT_BY_FIELD_SYSCALL_NUMBER, CKB_LOAD_SCRIPT_SYSCALL_NUMBER,
    CKB_LOAD_WITNESS_SYSCALL_NUMBER, CKB_SOURCE_CELL_DEP, CKB_SOURCE_GROUP_INPUT, CKB_SOURCE_GROUP_OUTPUT, CKB_SOURCE_HEADER_DEP,
};
use crate::TargetProfile;

use super::{
    CellScriptRuntimeError, CodeGenerator, CKB_HEADER_FIELD_EPOCH_LENGTH, CKB_HEADER_FIELD_EPOCH_NUMBER,
    CKB_HEADER_FIELD_EPOCH_START_BLOCK_NUMBER, CKB_INPUT_FIELD_SINCE,
};

// ---------------------------------------------------------------------------
// Syscall ABI
// ---------------------------------------------------------------------------

pub(crate) struct RuntimeSyscallAbi {
    pub(crate) load_header_by_field: u64,
    pub(crate) load_input_by_field: u64,
    pub(crate) load_witness: u64,
    pub(crate) load_script: u64,
    pub(crate) load_cell_by_field: u64,
    pub(crate) load_cell_data: u64,
    pub(crate) source_group_input: u64,
    pub(crate) source_group_output: u64,
    pub(crate) source_header_dep: u64,
}

const CKB_RUNTIME_SYSCALL_ABI: RuntimeSyscallAbi = RuntimeSyscallAbi {
    load_header_by_field: CKB_LOAD_HEADER_BY_FIELD_SYSCALL_NUMBER,
    load_input_by_field: CKB_LOAD_INPUT_BY_FIELD_SYSCALL_NUMBER,
    load_witness: CKB_LOAD_WITNESS_SYSCALL_NUMBER,
    load_script: CKB_LOAD_SCRIPT_SYSCALL_NUMBER,
    load_cell_by_field: CKB_LOAD_CELL_BY_FIELD_SYSCALL_NUMBER,
    load_cell_data: CKB_LOAD_CELL_DATA_SYSCALL_NUMBER,
    source_group_input: CKB_SOURCE_GROUP_INPUT,
    source_group_output: CKB_SOURCE_GROUP_OUTPUT,
    source_header_dep: CKB_SOURCE_HEADER_DEP,
};

pub(crate) fn runtime_syscall_abi(profile: TargetProfile) -> RuntimeSyscallAbi {
    match profile {
        TargetProfile::Ckb => CKB_RUNTIME_SYSCALL_ABI,
    }
}

// ---------------------------------------------------------------------------
// v0.14 runtime helper analysis
// ---------------------------------------------------------------------------

fn referenced_v014_runtime_helpers(ir: &IrModule) -> BTreeSet<String> {
    let mut helpers = BTreeSet::new();
    for item in &ir.items {
        let body = match item {
            IrItem::Action(action) => Some(&action.body),
            IrItem::PureFn(function) => Some(&function.body),
            IrItem::Lock(lock) => Some(&lock.body),
            IrItem::TypeDef(_) | IrItem::Invariant(_) => None,
        };
        let Some(body) = body else {
            continue;
        };
        for block in &body.blocks {
            for instruction in &block.instructions {
                let IrInstruction::Call { func, .. } = instruction else {
                    continue;
                };
                if is_v014_runtime_helper(func) {
                    helpers.insert(func.clone());
                }
            }
        }
    }
    helpers
}

fn is_v014_runtime_helper(func: &str) -> bool {
    runtime_helper_symbols().any(|symbol| symbol == func)
}

pub(crate) fn is_ckb_fixed_hash_helper(func: &str) -> bool {
    matches!(func, "__ckb_hash_chain" | "__ckb_hash_blake2b")
}

pub(crate) fn is_ckb_checked_runtime_helper(func: &str) -> bool {
    checked_runtime_helper_spec(func).is_some()
        || fail_closed_helper_spec(func).is_some()
        || matches!(
            func,
            "__env_current_timepoint"
                | "__ckb_header_epoch_number"
                | "__ckb_header_epoch_start_block_number"
                | "__ckb_header_epoch_length"
                | "__ckb_input_since"
        )
}

pub(crate) fn ckb_checked_runtime_status_reg(func: &str) -> &'static str {
    checked_runtime_helper_spec(func).map_or("a1", |spec| spec.semantic_status_reg)
}

// ---------------------------------------------------------------------------
// CodeGenerator runtime support methods
// ---------------------------------------------------------------------------

impl CodeGenerator {
    fn emit_runtime_fail_ret(&mut self, error: CellScriptRuntimeError) {
        self.emit_runtime_error_comment(error);
        self.emit(format!("li a0, {}", error.code()));
        self.emit("ret");
    }

    fn emit_runtime_status_fail_ret(&mut self, error: CellScriptRuntimeError) {
        self.emit_runtime_error_comment(error);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", error.code()));
        self.emit("ret");
    }

    pub(crate) fn generate_runtime_support(&mut self, ir: &IrModule) {
        self.emit_section(".text");
        self.emit_runtime_memcmp_fixed();
        self.emit_runtime_memzero_fixed();
        self.emit_runtime_size_guards();
        self.emit_runtime_molecule_table_offset_guard();
        // CKB exposes epoch-number based timepoints here, not Unix timestamps.
        self.emit_runtime_header_field_u64(
            "__env_current_timepoint",
            "ckb_epoch_number",
            CKB_HEADER_FIELD_EPOCH_NUMBER,
            true,
            "env::current_timepoint is required for CKB profile",
        );
        self.emit_runtime_header_field_u64(
            "__ckb_header_epoch_number",
            "ckb_epoch_number",
            CKB_HEADER_FIELD_EPOCH_NUMBER,
            self.options.target_profile == TargetProfile::Ckb,
            "ckb::header_epoch_number is rejected outside the ckb target profile",
        );
        self.emit_runtime_header_field_u64(
            "__ckb_header_epoch_start_block_number",
            "ckb_epoch_start_block_number",
            CKB_HEADER_FIELD_EPOCH_START_BLOCK_NUMBER,
            self.options.target_profile == TargetProfile::Ckb,
            "ckb::header_epoch_start_block_number is rejected outside the ckb target profile",
        );
        self.emit_runtime_header_field_u64(
            "__ckb_header_epoch_length",
            "ckb_epoch_length",
            CKB_HEADER_FIELD_EPOCH_LENGTH,
            self.options.target_profile == TargetProfile::Ckb,
            "ckb::header_epoch_length is rejected outside the ckb target profile",
        );
        self.emit_runtime_input_field_u64(
            "__ckb_input_since",
            "ckb_input_since",
            CKB_INPUT_FIELD_SINCE,
            self.options.target_profile == TargetProfile::Ckb,
            "ckb::input_since is rejected outside the ckb target profile",
        );
        let v014_helpers = referenced_v014_runtime_helpers(ir);
        self.emit_runtime_ckb_v014_surface_helpers(&v014_helpers);
    }

    fn emit_runtime_ckb_v014_surface_helpers(&mut self, referenced_helpers: &BTreeSet<String>) {
        let enabled = self.options.target_profile == TargetProfile::Ckb;
        for spec in vm2_helper_specs() {
            if !referenced_helpers.contains(spec.symbol) {
                continue;
            }
            self.emit_global(spec.symbol);
            self.emit_label(spec.symbol);
            self.emit(format!("# cellscript abi: CKB VM v2 syscall {} ({})", spec.number, spec.detail));
            if !enabled {
                self.emit_runtime_status_fail_ret(CellScriptRuntimeError::SyscallFailed);
            } else {
                self.emit_runtime_vm2_helper(*spec);
            }
        }

        for spec in source_constant_specs() {
            if !referenced_helpers.contains(spec.symbol) {
                continue;
            }
            self.emit_global(spec.symbol);
            self.emit_label(spec.symbol);
            self.emit(format!("# cellscript abi: v0.14 CKB semantic helper ({})", spec.detail));
            if !enabled {
                self.emit_runtime_fail_ret(CellScriptRuntimeError::SyscallFailed);
            } else {
                self.emit(format!("li a0, {}", spec.value));
                self.emit("ret");
            }
        }

        for spec in fail_closed_runtime_helper_specs() {
            if !referenced_helpers.contains(spec.symbol) {
                continue;
            }
            self.emit_global(spec.symbol);
            self.emit_label(spec.symbol);
            self.emit(format!("# cellscript abi: v0.14 CKB semantic helper ({})", spec.detail));
            if !enabled {
                self.emit_runtime_status_fail_ret(CellScriptRuntimeError::SyscallFailed);
            } else {
                self.emit("# cellscript abi: helper is not executable yet; fail closed instead of returning a forged success value");
                self.emit_runtime_status_fail_ret(CellScriptRuntimeError::SyscallFailed);
            }
        }

        if referenced_helpers.contains("__ckb_hash_chain") {
            self.emit_global("__ckb_hash_chain");
            self.emit_label("__ckb_hash_chain");
            self.emit("# cellscript abi: hash_chain aliases CKB Blake2b-256 over one 32-byte Hash input");
            if !enabled {
                self.emit_runtime_fail_ret(CellScriptRuntimeError::SyscallFailed);
            } else {
                self.emit("j __ckb_hash_blake2b");
            }
        }
        if referenced_helpers.contains("__ckb_hash_chain") || referenced_helpers.contains("__ckb_hash_blake2b") {
            self.emit_runtime_blake2b_hash32(enabled);
        }
    }

    fn emit_runtime_vm2_helper(&mut self, spec: SyscallSpec) {
        self.emit(format!("# cellscript abi: executable CKB VM v2 syscall {} ({})", spec.number, spec.detail));
        match spec.symbol {
            "__ckb_spawn" => self.emit_runtime_vm2_spawn(spec.number),
            "__ckb_wait" => self.emit_runtime_vm2_wait(spec.number),
            "__ckb_process_id" => self.emit_runtime_vm2_process_id(spec.number),
            "__ckb_pipe" => self.emit_runtime_vm2_pipe(spec.number),
            "__ckb_pipe_write" => self.emit_runtime_vm2_pipe_write(spec.number),
            "__ckb_pipe_read" => self.emit_runtime_vm2_pipe_read(spec.number),
            "__ckb_inherited_fd" => self.emit_runtime_vm2_inherited_fd(spec.number),
            "__ckb_close" => self.emit_runtime_vm2_close(spec.number),
            _ => self.emit_runtime_status_fail_ret(CellScriptRuntimeError::SyscallFailed),
        }
    }

    fn emit_runtime_vm2_spawn(&mut self, syscall_number: u64) {
        self.emit("# cellscript abi: spawn resolves the static target to CellDep#0 with no argv and no inherited fds");
        self.emit_large_addi("sp", "sp", -80);
        self.emit_stack_store("zero", 0);
        self.emit_stack_store("zero", 8);
        self.emit_stack_store("zero", 16);
        self.emit_stack_store("zero", 24);
        self.emit_stack_store("zero", 32);
        self.emit_sp_addi("t0", 0);
        self.emit_stack_store("t0", 40);
        self.emit_sp_addi("t0", 8);
        self.emit_stack_store("t0", 48);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CKB_SOURCE_CELL_DEP));
        self.emit("li a2, 0");
        self.emit("li a3, 0");
        self.emit_sp_addi("a4", 24);
        self.emit(format!("li a7, {}", syscall_number));
        self.emit("ecall");
        self.emit("mv a1, a0");
        self.emit_stack_load("a0", 0);
        self.emit_large_addi("sp", "sp", 80);
        self.emit("ret");
    }

    fn emit_runtime_vm2_wait(&mut self, syscall_number: u64) {
        self.emit("# cellscript abi: wait consumes child pid in a0 and supplies exit-code pointer in a1");
        self.emit_large_addi("sp", "sp", -16);
        self.emit_stack_store("zero", 0);
        self.emit_sp_addi("a1", 0);
        self.emit(format!("li a7, {}", syscall_number));
        self.emit("ecall");
        self.emit("mv a1, a0");
        self.emit_stack_load("a0", 0);
        self.emit_large_addi("sp", "sp", 16);
        self.emit("ret");
    }

    fn emit_runtime_vm2_process_id(&mut self, syscall_number: u64) {
        self.emit(format!("li a7, {}", syscall_number));
        self.emit("ecall");
        self.emit("li a1, 0");
        self.emit("ret");
    }

    fn emit_runtime_vm2_pipe(&mut self, syscall_number: u64) {
        self.emit_large_addi("sp", "sp", -32);
        self.emit_stack_store("zero", 0);
        self.emit_stack_store("zero", 8);
        self.emit_sp_addi("a0", 0);
        self.emit(format!("li a7, {}", syscall_number));
        self.emit("ecall");
        self.emit("mv a2, a0");
        self.emit_stack_load("a0", 0);
        self.emit_stack_load("a1", 8);
        self.emit_large_addi("sp", "sp", 32);
        self.emit("ret");
    }

    fn emit_runtime_vm2_pipe_write(&mut self, syscall_number: u64) {
        let fail = self.fresh_label("runtime_vm2_pipe_write_fail");
        let done = self.fresh_label("runtime_vm2_pipe_write_done");
        self.emit_large_addi("sp", "sp", -32);
        self.emit_stack_store("a1", 0);
        self.emit("li t0, 8");
        self.emit_stack_store("t0", 8);
        self.emit_sp_addi("a1", 0);
        self.emit_sp_addi("a2", 8);
        self.emit(format!("li a7, {}", syscall_number));
        self.emit("ecall");
        self.emit("mv t0, a0");
        self.emit(format!("bnez t0, {}", fail));
        self.emit_stack_load("t1", 8);
        self.emit("li t2, 8");
        self.emit(format!("bne t1, t2, {}", fail));
        self.emit("li a0, 0");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&fail);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit_label(&done);
        self.emit_large_addi("sp", "sp", 32);
        self.emit("ret");
    }

    fn emit_runtime_vm2_pipe_read(&mut self, syscall_number: u64) {
        let fail = self.fresh_label("runtime_vm2_pipe_read_fail");
        let done = self.fresh_label("runtime_vm2_pipe_read_done");
        self.emit_large_addi("sp", "sp", -32);
        self.emit_stack_store("zero", 0);
        self.emit("li t0, 8");
        self.emit_stack_store("t0", 8);
        self.emit_sp_addi("a1", 0);
        self.emit_sp_addi("a2", 8);
        self.emit(format!("li a7, {}", syscall_number));
        self.emit("ecall");
        self.emit("mv t0, a0");
        self.emit(format!("bnez t0, {}", fail));
        self.emit_stack_load("t1", 8);
        self.emit("li t2, 8");
        self.emit(format!("bne t1, t2, {}", fail));
        self.emit_stack_load("a0", 0);
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&fail);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit_label(&done);
        self.emit_large_addi("sp", "sp", 32);
        self.emit("ret");
    }

    fn emit_runtime_vm2_inherited_fd(&mut self, syscall_number: u64) {
        let status_fail = self.fresh_label("runtime_vm2_inherited_fd_status_fail");
        let bounds_fail = self.fresh_label("runtime_vm2_inherited_fd_bounds_fail");
        let done = self.fresh_label("runtime_vm2_inherited_fd_done");
        self.emit_large_addi("sp", "sp", -96);
        self.emit_stack_store("a0", 8);
        self.emit("li t0, 8");
        self.emit_stack_store("t0", 0);
        self.emit_sp_addi("a0", 16);
        self.emit_sp_addi("a1", 0);
        self.emit(format!("li a7, {}", syscall_number));
        self.emit("ecall");
        self.emit("mv t0, a0");
        self.emit(format!("bnez t0, {}", status_fail));
        self.emit_stack_load("t1", 0);
        self.emit_stack_load("t2", 8);
        self.emit(format!("bgeu t2, t1, {}", bounds_fail));
        self.emit("li t3, 8");
        self.emit(format!("bgeu t2, t3, {}", bounds_fail));
        self.emit("slli t2, t2, 3");
        self.emit_sp_addi("t3", 16);
        self.emit("add t3, t3, t2");
        self.emit("ld a0, 0(t3)");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&status_fail);
        self.emit("li a0, 0");
        self.emit("mv a1, t0");
        self.emit(format!("j {}", done));
        self.emit_label(&bounds_fail);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit_label(&done);
        self.emit_large_addi("sp", "sp", 96);
        self.emit("ret");
    }

    fn emit_runtime_vm2_close(&mut self, syscall_number: u64) {
        self.emit(format!("li a7, {}", syscall_number));
        self.emit("ecall");
        self.emit("mv a1, a0");
        self.emit("li a0, 0");
        self.emit("ret");
    }

    fn emit_runtime_blake2b_hash32(&mut self, enabled: bool) {
        self.emit_global("__ckb_hash_blake2b");
        self.emit_label("__ckb_hash_blake2b");
        self.emit("# cellscript abi: CKB Blake2b-256 helper; a0=input[32], a1=output[32], returns a0=0");
        if !enabled {
            self.emit_runtime_fail_ret(CellScriptRuntimeError::SyscallFailed);
            return;
        }

        const IV: [u64; 8] = [
            0x6a09e667f3bcc908,
            0xbb67ae8584caa73b,
            0x3c6ef372fe94f82b,
            0xa54ff53a5f1d36f1,
            0x510e527fade682d1,
            0x9b05688c2b3e6c1f,
            0x1f83d9abfb41bd6b,
            0x5be0cd19137e2179,
        ];
        const SIGMA: [[usize; 16]; 12] = [
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
            [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
            [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
            [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
            [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
            [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
            [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
            [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
            [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
            [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
            [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
        ];

        const H_BASE: usize = 0;
        const V_BASE: usize = 64;
        const M_BASE: usize = 192;
        const FRAME: usize = 320;

        let personal0 = u64::from_le_bytes(*b"ckb-defa");
        let personal1 = u64::from_le_bytes(*b"ult-hash");
        let h = [IV[0] ^ 0x01010020, IV[1], IV[2], IV[3], IV[4], IV[5], IV[6] ^ personal0, IV[7] ^ personal1];

        self.emit_large_addi("sp", "sp", -(FRAME as i64));
        for (index, value) in h.iter().enumerate() {
            self.emit_blake2b_store_const(*value, H_BASE + index * 8);
        }
        for index in 0..4 {
            self.emit_blake2b_load_input_word(index, M_BASE + index * 8);
        }
        for index in 4..16 {
            self.emit_stack_store("zero", M_BASE + index * 8);
        }
        for index in 0..8 {
            self.emit_stack_load("t0", H_BASE + index * 8);
            self.emit_stack_store("t0", V_BASE + index * 8);
        }
        for (index, value) in IV.iter().enumerate() {
            self.emit_blake2b_store_const(*value, V_BASE + (index + 8) * 8);
        }
        self.emit_stack_load("t0", V_BASE + 12 * 8);
        self.emit("xori t0, t0, 32");
        self.emit_stack_store("t0", V_BASE + 12 * 8);
        self.emit_stack_load("t0", V_BASE + 14 * 8);
        self.emit("xori t0, t0, -1");
        self.emit_stack_store("t0", V_BASE + 14 * 8);

        for round in SIGMA {
            self.emit_blake2b_g(V_BASE, M_BASE, 0, 4, 8, 12, round[0], round[1]);
            self.emit_blake2b_g(V_BASE, M_BASE, 1, 5, 9, 13, round[2], round[3]);
            self.emit_blake2b_g(V_BASE, M_BASE, 2, 6, 10, 14, round[4], round[5]);
            self.emit_blake2b_g(V_BASE, M_BASE, 3, 7, 11, 15, round[6], round[7]);
            self.emit_blake2b_g(V_BASE, M_BASE, 0, 5, 10, 15, round[8], round[9]);
            self.emit_blake2b_g(V_BASE, M_BASE, 1, 6, 11, 12, round[10], round[11]);
            self.emit_blake2b_g(V_BASE, M_BASE, 2, 7, 8, 13, round[12], round[13]);
            self.emit_blake2b_g(V_BASE, M_BASE, 3, 4, 9, 14, round[14], round[15]);
        }

        for index in 0..8 {
            self.emit_stack_load("t0", H_BASE + index * 8);
            self.emit_stack_load("t1", V_BASE + index * 8);
            self.emit("xor t0, t0, t1");
            self.emit_stack_load("t1", V_BASE + (index + 8) * 8);
            self.emit("xor t0, t0, t1");
            self.emit_stack_store("t0", H_BASE + index * 8);
        }
        for index in 0..4 {
            self.emit_stack_load("t0", H_BASE + index * 8);
            self.emit(format!("sd t0, {}(a1)", index * 8));
        }
        self.emit_large_addi("sp", "sp", FRAME as i64);
        self.emit("li a0, 0");
        self.emit("ret");
    }

    fn emit_blake2b_store_const(&mut self, value: u64, stack_offset: usize) {
        let label = self.const_data_label_for_bytes(value.to_le_bytes().to_vec());
        self.emit(format!("la t0, {}", label));
        self.emit("ld t0, 0(t0)");
        self.emit_stack_store("t0", stack_offset);
    }

    fn emit_blake2b_load_input_word(&mut self, word_index: usize, stack_offset: usize) {
        self.emit("li t0, 0");
        for byte_index in 0..8 {
            let absolute = word_index * 8 + byte_index;
            self.emit(format!("lbu t1, {}(a0)", absolute));
            if byte_index > 0 {
                self.emit(format!("slli t1, t1, {}", byte_index * 8));
            }
            self.emit("or t0, t0, t1");
        }
        self.emit_stack_store("t0", stack_offset);
    }

    fn emit_blake2b_rotr(&mut self, register: &str, bits: usize) {
        self.emit(format!("srli t1, {}, {}", register, bits));
        self.emit(format!("slli {}, {}, {}", register, register, 64 - bits));
        self.emit(format!("or {}, {}, t1", register, register));
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_blake2b_g(&mut self, v_base: usize, m_base: usize, a: usize, b: usize, c: usize, d: usize, mx: usize, my: usize) {
        let va = v_base + a * 8;
        let vb = v_base + b * 8;
        let vc = v_base + c * 8;
        let vd = v_base + d * 8;
        let vmx = m_base + mx * 8;
        let vmy = m_base + my * 8;

        self.emit_stack_load("t0", va);
        self.emit_stack_load("t1", vb);
        self.emit("add t0, t0, t1");
        self.emit_stack_load("t1", vmx);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("t0", va);
        self.emit_stack_load("t0", vd);
        self.emit_stack_load("t1", va);
        self.emit("xor t0, t0, t1");
        self.emit_blake2b_rotr("t0", 32);
        self.emit_stack_store("t0", vd);

        self.emit_stack_load("t0", vc);
        self.emit_stack_load("t1", vd);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("t0", vc);
        self.emit_stack_load("t0", vb);
        self.emit_stack_load("t1", vc);
        self.emit("xor t0, t0, t1");
        self.emit_blake2b_rotr("t0", 24);
        self.emit_stack_store("t0", vb);

        self.emit_stack_load("t0", va);
        self.emit_stack_load("t1", vb);
        self.emit("add t0, t0, t1");
        self.emit_stack_load("t1", vmy);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("t0", va);
        self.emit_stack_load("t0", vd);
        self.emit_stack_load("t1", va);
        self.emit("xor t0, t0, t1");
        self.emit_blake2b_rotr("t0", 16);
        self.emit_stack_store("t0", vd);

        self.emit_stack_load("t0", vc);
        self.emit_stack_load("t1", vd);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("t0", vc);
        self.emit_stack_load("t0", vb);
        self.emit_stack_load("t1", vc);
        self.emit("xor t0, t0, t1");
        self.emit_blake2b_rotr("t0", 63);
        self.emit_stack_store("t0", vb);
    }

    fn emit_runtime_memcmp_fixed(&mut self) {
        self.emit_global("__cellscript_memcmp_fixed");
        self.emit_label("__cellscript_memcmp_fixed");
        self.emit("# cellscript abi: fixed-byte helper compares a0/a1 for a2 bytes; returns a0=0 when equal");
        let loop_label = ".L__cellscript_memcmp_fixed_loop";
        let mismatch_label = ".L__cellscript_memcmp_fixed_mismatch";
        let equal_label = ".L__cellscript_memcmp_fixed_equal";
        self.emit(format!("beqz a2, {}", equal_label));
        self.emit_label(loop_label);
        self.emit("lbu t0, 0(a0)");
        self.emit("lbu t1, 0(a1)");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", mismatch_label));
        self.emit("addi a0, a0, 1");
        self.emit("addi a1, a1, 1");
        self.emit("addi a2, a2, -1");
        self.emit(format!("bnez a2, {}", loop_label));
        self.emit_label(equal_label);
        self.emit("li a0, 0");
        self.emit("ret");
        self.emit_label(mismatch_label);
        self.emit("li a0, 1");
        self.emit("ret");
    }

    fn emit_runtime_memzero_fixed(&mut self) {
        self.emit_global("__cellscript_memzero_fixed");
        self.emit_label("__cellscript_memzero_fixed");
        self.emit("# cellscript abi: fixed-byte helper checks a0 for a1 zero bytes; returns a0=0 when all zero");
        let loop_label = ".L__cellscript_memzero_fixed_loop";
        let mismatch_label = ".L__cellscript_memzero_fixed_mismatch";
        let equal_label = ".L__cellscript_memzero_fixed_equal";
        self.emit(format!("beqz a1, {}", equal_label));
        self.emit_label(loop_label);
        self.emit("lbu t0, 0(a0)");
        self.emit(format!("bnez t0, {}", mismatch_label));
        self.emit("addi a0, a0, 1");
        self.emit("addi a1, a1, -1");
        self.emit(format!("bnez a1, {}", loop_label));
        self.emit_label(equal_label);
        self.emit("li a0, 0");
        self.emit("ret");
        self.emit_label(mismatch_label);
        self.emit("li a0, 1");
        self.emit("ret");
    }

    fn emit_runtime_size_guards(&mut self) {
        self.emit_global("__cellscript_require_min_size");
        self.emit_label("__cellscript_require_min_size");
        self.emit("# cellscript abi: returns a0=0 when actual size a0 is at least required size a1");
        self.emit("sltu a0, a0, a1");
        self.emit("ret");

        self.emit_global("__cellscript_require_exact_size");
        self.emit_label("__cellscript_require_exact_size");
        self.emit("# cellscript abi: returns a0=0 when actual size a0 equals expected size a1");
        self.emit("sub a0, a0, a1");
        self.emit("ret");
    }

    fn emit_runtime_molecule_table_offset_guard(&mut self) {
        self.emit_global("__cellscript_validate_molecule_table_offsets");
        self.emit_label("__cellscript_validate_molecule_table_offsets");
        self.emit("# cellscript abi: validate Molecule table offsets a0=base a1=total_size a2=field_count a3=header_size");
        self.emit("addi t0, a0, 4");
        self.emit("mv t1, a2");
        self.emit("mv t2, a3");
        self.emit("mv a5, a2");
        let loop_label = ".L__cellscript_molecule_offsets_loop";
        let not_first_label = ".L__cellscript_molecule_offsets_not_first";
        let lower_bound_ok_label = ".L__cellscript_molecule_offsets_lower_bound_ok";
        let ok_label = ".L__cellscript_molecule_offsets_ok";
        let fail_label = ".L__cellscript_molecule_offsets_fail";
        self.emit_label(loop_label);
        self.emit(format!("beqz t1, {}", ok_label));
        self.emit("li t3, 0");
        self.emit("lbu a4, 0(t0)");
        self.emit("or t3, t3, a4");
        self.emit("lbu a4, 1(t0)");
        self.emit("slli a4, a4, 8");
        self.emit("or t3, t3, a4");
        self.emit("lbu a4, 2(t0)");
        self.emit("slli a4, a4, 16");
        self.emit("or t3, t3, a4");
        self.emit("lbu a4, 3(t0)");
        self.emit("slli a4, a4, 24");
        self.emit("or t3, t3, a4");
        self.emit(format!("bne t1, a5, {}", not_first_label));
        self.emit("sub a4, t3, a3");
        self.emit(format!("bnez a4, {}", fail_label));
        self.emit(format!("j {}", lower_bound_ok_label));
        self.emit_label(not_first_label);
        self.emit("sltu a4, t3, t2");
        self.emit(format!("bnez a4, {}", fail_label));
        self.emit_label(lower_bound_ok_label);
        self.emit("sltu a4, a1, t3");
        self.emit(format!("bnez a4, {}", fail_label));
        self.emit("mv t2, t3");
        self.emit("addi t0, t0, 4");
        self.emit("addi t1, t1, -1");
        self.emit(format!("j {}", loop_label));
        self.emit_label(ok_label);
        self.emit("li a0, 0");
        self.emit("ret");
        self.emit_label(fail_label);
        self.emit_runtime_error_comment(CellScriptRuntimeError::BoundsCheckFailed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::BoundsCheckFailed.code()));
        self.emit("ret");
    }

    fn emit_runtime_header_field_u64(&mut self, symbol: &str, field_name: &str, field_id: u64, enabled: bool, disabled_reason: &str) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        if !enabled {
            self.emit(format!("# cellscript abi: {}", disabled_reason));
            self.emit_runtime_error_comment(CellScriptRuntimeError::ConsumeInvalidOperand);
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::ConsumeInvalidOperand.code()));
            self.emit("ret");
            return;
        }

        let abi = self.runtime_abi();
        self.emit_large_addi("sp", "sp", -32);
        self.emit_stack_store("ra", 24);
        self.emit(format!("# cellscript abi: LOAD_HEADER_BY_FIELD field={} source=HeaderDep index=0", field_name));
        self.emit("li t0, 8");
        self.emit_stack_store("t0", 8);
        self.emit_sp_addi("a0", 16);
        self.emit_sp_addi("a1", 8);
        self.emit("li a2, 0");
        self.emit("li a3, 0");
        self.emit(format!("li a4, {}", abi.source_header_dep));
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_header_by_field));
        self.emit("ecall");
        let fail = self.fresh_label("runtime_header_field_fail");
        self.emit(format!("bnez a0, {}", fail));
        self.emit_stack_load("t0", 8);
        self.emit("li t1, 8");
        self.emit(format!("bne t0, t1, {}", fail));
        self.emit_stack_load("a0", 16);
        self.emit("li a1, 0");
        self.emit_stack_load("ra", 24);
        self.emit_large_addi("sp", "sp", 32);
        self.emit("ret");
        self.emit_label(&fail);
        self.emit_runtime_error_comment(CellScriptRuntimeError::SyscallFailed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit_stack_load("ra", 24);
        self.emit_large_addi("sp", "sp", 32);
        self.emit("ret");
    }

    fn emit_runtime_input_field_u64(&mut self, symbol: &str, field_name: &str, field_id: u64, enabled: bool, disabled_reason: &str) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        if !enabled {
            self.emit(format!("# cellscript abi: {}", disabled_reason));
            self.emit_runtime_error_comment(CellScriptRuntimeError::ConsumeInvalidOperand);
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::ConsumeInvalidOperand.code()));
            self.emit("ret");
            return;
        }

        let abi = self.runtime_abi();
        self.emit_large_addi("sp", "sp", -32);
        self.emit_stack_store("ra", 24);
        self.emit(format!("# cellscript abi: LOAD_INPUT_BY_FIELD field={} source=GroupInput index=0", field_name));
        self.emit("li t0, 8");
        self.emit_stack_store("t0", 8);
        self.emit_sp_addi("a0", 16);
        self.emit_sp_addi("a1", 8);
        self.emit("li a2, 0");
        self.emit("li a3, 0");
        self.emit(format!("li a4, {}", abi.source_group_input));
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        let fail = self.fresh_label("runtime_input_field_fail");
        self.emit(format!("bnez a0, {}", fail));
        self.emit_stack_load("t0", 8);
        self.emit("li t1, 8");
        self.emit(format!("bne t0, t1, {}", fail));
        self.emit_stack_load("a0", 16);
        self.emit("li a1, 0");
        self.emit_stack_load("ra", 24);
        self.emit_large_addi("sp", "sp", 32);
        self.emit("ret");
        self.emit_label(&fail);
        self.emit_runtime_error_comment(CellScriptRuntimeError::SyscallFailed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit_stack_load("ra", 24);
        self.emit_large_addi("sp", "sp", 32);
        self.emit("ret");
    }
}
