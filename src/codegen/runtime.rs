use super::*;

mod dispatch;
mod handles;
mod hashes;

#[cfg(test)]
mod stack_audit;

/// Raw byte sources for the exact-width read ABI. This does not parse a
/// protocol envelope or change the legacy fixed-buffer field getters.
#[derive(Clone, Copy)]
enum RuntimeByteSource {
    CellData,
    Witness,
    CellLock,
    CellType,
}

/// Keep the legacy in-memory artifact unchanged; both transaction byte
/// sources share one streaming compressor with a compiler-selected syscall.
#[derive(Clone, Copy)]
enum RuntimeHashInput {
    Memory,
    Transaction,
    PrefixedTransaction,
    Segments,
}

#[derive(Clone, Copy)]
struct OrderMasterDataOffsets {
    source: usize,
    cell_index: usize,
    action_offset: usize,
    tx_hash_offset: usize,
    index_offset: usize,
    tx_dest: usize,
    index_dest: usize,
    data_buffer: usize,
    size: usize,
}

#[derive(Clone, Copy)]
struct OrderMasterFailureLabels<'a> {
    invalid_action: &'a str,
    malformed: &'a str,
    out_point_failed: &'a str,
}

impl CodeGenerator {
    pub(super) fn generate_runtime_support(&mut self, ir: &IrModule) {
        self.emit_section(".text");
        self.emit_runtime_memcmp_fixed();
        self.emit_runtime_memzero_fixed();
        self.emit_runtime_memcpy_fixed();
        self.emit_runtime_size_guards();
        if ir.items.iter().any(|item| {
            let body = match item {
                IrItem::Action(entry) => &entry.body,
                IrItem::Lock(entry) => &entry.body,
                IrItem::Function(entry) => &entry.body,
                _ => return false,
            };
            body.cell_bindings.iter().any(|binding| binding.membership != IrCellMembership::Unproven)
        }) {
            self.emit_runtime_cell_membership();
        }
        // These scalar getters have no runtime-to-runtime callers. Emit their
        // complete checked implementation only when a retained IR call needs
        // it, rather than adding unused failure paths to every artifact.
        let scalar_helpers = ir
            .items
            .iter()
            .filter_map(|item| match item {
                IrItem::Action(entry) => Some(&entry.body),
                IrItem::Lock(entry) => Some(&entry.body),
                IrItem::Function(entry) => Some(&entry.body),
                _ => None,
            })
            .flat_map(|body| &body.blocks)
            .flat_map(|block| &block.instructions)
            .filter_map(|instruction| match instruction {
                IrInstruction::Call { func, .. } if is_runtime_header_u64_call(func) => Some(func.as_str()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        // CKB exposes epoch-number based timepoints here, not Unix timestamps.
        for (symbol, field_name, field_id, enabled, disabled_reason) in [
            (
                "__env_current_timepoint",
                "ckb_epoch_number",
                CKB_HEADER_FIELD_EPOCH_NUMBER,
                true,
                "env::current_timepoint is required for CKB profile",
            ),
            (
                "__ckb_header_epoch_number",
                "ckb_epoch_number",
                CKB_HEADER_FIELD_EPOCH_NUMBER,
                self.options.target_profile.is_ckb(),
                "ckb::header_epoch_number is rejected outside the ckb target profile",
            ),
            (
                "__ckb_header_epoch_start_block_number",
                "ckb_epoch_start_block_number",
                CKB_HEADER_FIELD_EPOCH_START_BLOCK_NUMBER,
                self.options.target_profile.is_ckb(),
                "ckb::header_epoch_start_block_number is rejected outside the ckb target profile",
            ),
            (
                "__ckb_header_epoch_length",
                "ckb_epoch_length",
                CKB_HEADER_FIELD_EPOCH_LENGTH,
                self.options.target_profile.is_ckb(),
                "ckb::header_epoch_length is rejected outside the ckb target profile",
            ),
            (
                "__ckb_header_dep_epoch_number",
                "ckb_epoch_number",
                CKB_HEADER_FIELD_EPOCH_NUMBER,
                self.options.target_profile.is_ckb(),
                "HeaderDepView.epoch_number is rejected outside the ckb target profile",
            ),
            (
                "__ckb_header_dep_epoch_start_block_number",
                "ckb_epoch_start_block_number",
                CKB_HEADER_FIELD_EPOCH_START_BLOCK_NUMBER,
                self.options.target_profile.is_ckb(),
                "HeaderDepView.epoch_start_block_number is rejected outside the ckb target profile",
            ),
            (
                "__ckb_header_dep_epoch_length",
                "ckb_epoch_length",
                CKB_HEADER_FIELD_EPOCH_LENGTH,
                self.options.target_profile.is_ckb(),
                "HeaderDepView.epoch_length is rejected outside the ckb target profile",
            ),
        ] {
            if scalar_helpers.contains(symbol) {
                if symbol.starts_with("__ckb_header_dep_") {
                    self.emit_runtime_header_dep_field_u64(symbol, field_name, field_id, enabled, disabled_reason);
                } else {
                    self.emit_runtime_header_field_u64(symbol, field_name, field_id, enabled, disabled_reason);
                }
            }
        }
        for (symbol, field_name, field_offset, disabled_reason) in [
            (
                "__ckb_header_dep_block_number",
                "number",
                ckb_abi::header::NUMBER_OFFSET,
                "HeaderDepView.block_number is rejected outside the ckb target profile",
            ),
            (
                "__ckb_header_dep_timestamp_millis",
                "timestamp",
                ckb_abi::header::TIMESTAMP_OFFSET,
                "HeaderDepView.timestamp is rejected outside the ckb target profile",
            ),
        ] {
            if scalar_helpers.contains(symbol) {
                self.emit_runtime_header_dep_full_u64(
                    symbol,
                    field_name,
                    field_offset,
                    self.options.target_profile.is_ckb(),
                    disabled_reason,
                );
            }
        }
        if scalar_helpers.contains("__ckb_input_since") {
            self.emit_runtime_input_field_u64(
                "__ckb_input_since",
                "ckb_input_since",
                CKB_INPUT_FIELD_SINCE,
                self.options.target_profile.is_ckb(),
                "ckb::input_since is rejected outside the ckb target profile",
            );
        }
        let mut runtime_helpers = referenced_runtime_helpers(ir);
        if runtime_helpers.contains("__ckb_require_instance_script_participant_from_args") {
            runtime_helpers.insert("__ckb_require_cell_dep_open_script_handle".into());
        }
        self.emit_runtime_helpers(&runtime_helpers);
    }

    /// Internal ABI: a0=Cell source, a1=ordinal, a2=expected Script hash field.
    /// Returns 0, ScriptRoleMismatch, or ExactSizeMismatch. Only caller-saved
    /// registers are touched; the 96-byte private frame holds both aligned
    /// hashes and syscall arguments. No nested calls or caller buffers are used.
    fn emit_runtime_cell_membership(&mut self) {
        const STACK_FRAME_BYTES: i64 = align_frame(96) as i64;
        self.entry_frame_sizes.insert("__cellscript_require_cell_membership".to_string(), 96);
        self.emit_global("__cellscript_require_cell_membership");
        self.emit_label("__cellscript_require_cell_membership");
        let failed = self.fresh_label("membership_role_failed");
        let malformed = self.fresh_label("membership_hash_malformed");
        let loaded = self.fresh_label("membership_opposite_loaded");
        let success = self.fresh_label("membership_valid");
        let done = self.fresh_label("membership_done");
        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("a0", 72);
        self.emit_stack_store("a1", 80);
        self.emit_stack_store("a2", 88);
        self.emit("li t0, 32");
        self.emit_stack_store("t0", 0);
        self.emit_sp_addi("a0", 8);
        self.emit_sp_addi("a1", 0);
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", self.runtime_abi().load_script_hash));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit_stack_load("t0", 0);
        self.emit("li t1, 32");
        self.emit(format!("bne t0, t1, {}", malformed));

        for opposite in [false, true] {
            self.emit("li t0, 32");
            self.emit_stack_store("t0", 0);
            self.emit_sp_addi("a0", 40);
            self.emit_sp_addi("a1", 0);
            self.emit("li a2, 0");
            self.emit_stack_load("a3", 80);
            self.emit_stack_load("a4", 72);
            self.emit_stack_load("a5", 88);
            if opposite {
                // Both allowed field numbers are compiler-selected constants.
                // XOR with their difference swaps LockHash and TypeHash.
                self.emit(format!("xori a5, a5, {}", CKB_CELL_FIELD_LOCK_HASH ^ CKB_CELL_FIELD_TYPE_HASH));
            }
            self.emit(format!("li a7, {}", self.runtime_abi().load_cell_by_field));
            self.emit("ecall");
            if opposite {
                self.emit(format!("beqz a0, {}", loaded));
                self.emit_stack_load("t0", 88);
                self.emit(format!("li t1, {}", CKB_CELL_FIELD_LOCK_HASH));
                self.emit(format!("bne t0, t1, {}", failed));
                self.emit(format!("li t0, {}", CKB_ITEM_MISSING));
                self.emit(format!("beq a0, t0, {}", success));
                self.emit(format!("j {}", failed));
                self.emit_label(&loaded);
            } else {
                self.emit(format!("bnez a0, {}", failed));
            }
            self.emit_stack_load("t0", 0);
            self.emit("li t1, 32");
            self.emit(format!("bne t0, t1, {}", malformed));
            for word in 0..4 {
                self.emit_stack_load("t0", 8 + word * 8);
                self.emit_stack_load("t1", 40 + word * 8);
                if word == 0 {
                    self.emit("xor t2, t0, t1");
                } else {
                    self.emit("xor t0, t0, t1");
                    self.emit("or t2, t2, t0");
                }
            }
            self.emit(format!("{} t2, {}", if opposite { "beqz" } else { "bnez" }, failed));
        }
        self.emit_label(&success);
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ExactSizeMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptRoleMismatch.code()));
        self.emit_label(&done);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_witness_count_helper(&mut self, enabled: bool) {
        const SIZE: usize = 0;
        const INDEX: usize = 8;
        const BUFFER: usize = 16;
        const RA: usize = 24;
        const FRAME: usize = 32;
        self.emit_global("__ckb_witness_count");
        self.emit_label("__ckb_witness_count");
        self.emit("# cellscript abi: no arguments; a0=complete Input witness count, a1=error");
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }
        let scan = self.fresh_label("witness_count_scan");
        let success = self.fresh_label("witness_count_success");
        let failed = self.fresh_label("witness_count_failed");
        let done = self.fresh_label("witness_count_done");
        let abi = self.runtime_abi();
        self.emit_stack_adjustment(-(FRAME as i64));
        self.emit_stack_store("ra", RA);
        self.emit_stack_store("zero", INDEX);
        self.emit_label(&scan);
        self.emit_stack_store("zero", SIZE);
        self.emit(format!("addi a0, sp, {BUFFER}"));
        self.emit(format!("addi a1, sp, {SIZE}"));
        self.emit("li a2, 0");
        self.emit_stack_load("a3", INDEX);
        self.emit(format!("li a4, {CKB_SOURCE_INPUT}"));
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {success}"));
        self.emit(format!("bnez a0, {failed}"));
        self.emit_stack_load("t0", INDEX);
        self.emit("addi t0, t0, 1");
        self.emit(format!("beqz t0, {failed}"));
        self.emit_stack_store("t0", INDEX);
        self.emit(format!("j {scan}"));
        self.emit_label(&success);
        self.emit_stack_load("a0", INDEX);
        self.emit("li a1, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&failed);
        self.emit_process_failure(CellScriptRuntimeError::SyscallFailed);
        self.emit_label(&done);
        self.emit_stack_load("ra", RA);
        self.emit_stack_adjustment(FRAME as i64);
        self.emit("ret");
    }

    fn emit_runtime_exact_byte_word_helper(&mut self, symbol: &str, source: RuntimeByteSource, width: usize, enabled: bool) {
        debug_assert!(matches!(width, 1 | 4 | 8));
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!(
            "# cellscript abi: a0=SourceView, a1=offset, a2=function-local read cache; exactly {width} little-endian bytes"
        ));
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }
        let kind = match source {
            RuntimeByteSource::CellData => 0,
            RuntimeByteSource::Witness => 1,
            RuntimeByteSource::CellLock => 2,
            RuntimeByteSource::CellType => 3,
        };
        // The entry function keeps the most-recent validated source window in
        // callee-saved registers. Small scalar reads can therefore avoid both
        // the four-way metadata scan and its generic width dispatch.
        if self.module_uses_exact_read_hot_cache && width <= 4 {
            let slow = self.fresh_label("exact_read_scalar_slow");
            self.emit(format!("beqz s10, {slow}"));
            self.emit(format!("bne s9, a0, {slow}"));
            self.emit(format!("li t0, {kind}"));
            self.emit(format!("bne s8, t0, {slow}"));
            self.emit(format!("bltu a1, s7, {slow}"));
            self.emit("sub t4, a1, s7");
            self.emit(format!("bltu s6, t4, {slow}"));
            self.emit("sub t1, s6, t4");
            self.emit(format!("li t2, {width}"));
            self.emit(format!("bltu t1, t2, {slow}"));
            self.emit("add t0, s5, t4");
            self.emit("lbu a0, 0(t0)");
            for byte in 1..width {
                self.emit(format!("lbu t1, {byte}(t0)"));
                self.emit(format!("slli t1, t1, {}", byte * 8));
                self.emit("or a0, a0, t1");
            }
            self.emit("li a1, 0");
            self.emit("ret");
            self.emit_label(&slow);
        }
        self.emit(format!("li a3, {width}"));
        self.emit(format!("li a4, {kind}"));
        self.emit("j __cellscript_exact_read_cached");
    }

    fn emit_runtime_exact_read_cached(&mut self) {
        const VIEW: usize = 0;
        const START: usize = 8;
        const LEN: usize = 16;
        const VALID: usize = 24;
        const KIND: usize = 32;
        const WIDTH: usize = 40;
        const CACHE_BASE: usize = 48;
        const BUFFER: usize = 56;
        const CAPACITY: usize = RUNTIME_EXACT_READ_CACHE_CAPACITY;
        const LAST: usize = 8;
        let scan = self.fresh_label("exact_read_cache_scan");
        let miss = self.fresh_label("exact_read_cache_miss");
        let source_ready = self.fresh_label("exact_read_source_ready");
        let cell_data_syscall = self.fresh_label("exact_read_cell_data_syscall");
        let witness_syscall = self.fresh_label("exact_read_witness_syscall");
        let lock_syscall = self.fresh_label("exact_read_lock_syscall");
        let type_syscall = self.fresh_label("exact_read_type_syscall");
        let syscall_ready = self.fresh_label("exact_read_syscall_ready");
        let syscall_status_ok = self.fresh_label("exact_read_syscall_status_ok");
        let len_ready = self.fresh_label("exact_read_cache_len_ready");
        let load = self.fresh_label("exact_read_cache_load");
        let load1 = self.fresh_label("exact_read_u8");
        let load4 = self.fresh_label("exact_read_u32");
        let load8 = self.fresh_label("exact_read_u64");
        let success = self.fresh_label("exact_read_success");
        let invalid = self.fresh_label("exact_read_invalid_source");
        let failed = self.fresh_label("exact_read_failed");
        let failed_witness = self.fresh_label("exact_read_failed_witness");
        let abi = self.runtime_abi();

        self.emit_global("__cellscript_exact_read_cached");
        self.emit_label("__cellscript_exact_read_cached");
        self.emit(format!(
            "# cellscript abi: four-way source-bound {CAPACITY}-byte exact-read windows; a0=view,a1=offset,a2=cache,a3=width,a4=kind"
        ));
        if self.module_uses_exact_read_hot_cache {
            self.emit(format!("beqz s10, {scan}"));
            self.emit(format!("bne s9, a0, {scan}"));
            self.emit(format!("bne s8, a4, {scan}"));
            self.emit(format!("bltu a1, s7, {scan}"));
            self.emit("sub t4, a1, s7");
            self.emit(format!("bltu s6, t4, {scan}"));
            self.emit("sub t1, s6, t4");
            self.emit(format!("bltu t1, a3, {scan}"));
            self.emit("mv a6, s10");
            self.emit(format!("j {load}"));
        } else {
            self.emit(format!("ld a6, {LAST}(a2)"));
            self.emit(format!("beqz a6, {scan}"));
            self.emit(format!("ld t0, {VALID}(a6)"));
            self.emit(format!("beqz t0, {scan}"));
            self.emit(format!("ld t0, {VIEW}(a6)"));
            self.emit(format!("bne t0, a0, {scan}"));
            self.emit(format!("ld t0, {KIND}(a6)"));
            self.emit(format!("bne t0, a4, {scan}"));
            self.emit(format!("ld t0, {START}(a6)"));
            self.emit(format!("bltu a1, t0, {scan}"));
            self.emit("sub t4, a1, t0");
            self.emit(format!("ld t1, {LEN}(a6)"));
            self.emit(format!("bltu t1, t4, {scan}"));
            self.emit("sub t1, t1, t4");
            self.emit(format!("bltu t1, a3, {scan}"));
            self.emit(format!("j {load}"));
        }

        self.emit_label(&scan);
        for way in 0..RUNTIME_EXACT_READ_CACHE_WAYS {
            let next = self.fresh_label("exact_read_cache_next_way");
            self.emit(format!("addi a6, a2, {}", self.exact_read_cache_header_size() + way * RUNTIME_EXACT_READ_CACHE_ENTRY_SIZE));
            self.emit(format!("ld t0, {VALID}(a6)"));
            self.emit(format!("beqz t0, {next}"));
            self.emit(format!("ld t0, {VIEW}(a6)"));
            self.emit(format!("bne t0, a0, {next}"));
            self.emit(format!("ld t0, {KIND}(a6)"));
            self.emit(format!("bne t0, a4, {next}"));
            self.emit(format!("ld t0, {START}(a6)"));
            self.emit(format!("bltu a1, t0, {next}"));
            self.emit("sub t4, a1, t0");
            self.emit(format!("ld t1, {LEN}(a6)"));
            self.emit(format!("bltu t1, t4, {next}"));
            self.emit("sub t1, t1, t4");
            self.emit(format!("bltu t1, a3, {next}"));
            self.emit(format!("sd a6, {LAST}(a2)"));
            if self.module_uses_exact_read_hot_cache {
                self.emit("mv s10, a6");
                self.emit("mv s9, a0");
                self.emit("mv s8, a4");
                self.emit(format!("ld s7, {START}(a6)"));
                self.emit(format!("ld s6, {LEN}(a6)"));
                self.emit(format!("addi s5, a6, {BUFFER}"));
            }
            self.emit(format!("j {load}"));
            self.emit_label(&next);
        }
        self.emit(format!("j {miss}"));

        self.emit_label(&miss);
        self.emit("ld t0, 0(a2)");
        self.emit("addi t1, t0, 1");
        self.emit(format!("li t2, {}", RUNTIME_EXACT_READ_CACHE_WAYS - 1));
        self.emit("and t1, t1, t2");
        self.emit("sd t1, 0(a2)");
        self.emit(format!("li t2, {RUNTIME_EXACT_READ_CACHE_ENTRY_SIZE}"));
        self.emit("mul t1, t0, t2");
        self.emit("add a6, a2, t1");
        self.emit(format!("addi a6, a6, {}", self.exact_read_cache_header_size()));
        self.emit(format!("sd zero, {VALID}(a6)"));
        self.emit(format!("sd a0, {VIEW}(a6)"));
        self.emit(format!("sd a1, {START}(a6)"));
        self.emit(format!("sd a4, {KIND}(a6)"));
        self.emit(format!("sd a3, {WIDTH}(a6)"));
        self.emit(format!("sd a2, {CACHE_BASE}(a6)"));
        self.emit_decode_source_view_to_t1_t2(&invalid);
        for allowed in
            [CKB_SOURCE_INPUT, CKB_SOURCE_OUTPUT, CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT, CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT]
        {
            self.emit(format!("li t0, {allowed}"));
            self.emit(format!("beq t0, t2, {source_ready}"));
        }
        self.emit(format!("li t0, {CKB_SOURCE_CELL_DEP}"));
        self.emit(format!("bne t0, t2, {invalid}"));
        self.emit(format!("ld t0, {KIND}(a6)"));
        self.emit("li t3, 1");
        self.emit(format!("beq t0, t3, {invalid}"));

        self.emit_label(&source_ready);
        self.emit(format!("li t0, {CAPACITY}"));
        self.emit(format!("sd t0, {LEN}(a6)"));
        self.emit(format!("addi a0, a6, {BUFFER}"));
        self.emit(format!("addi a1, a6, {LEN}"));
        self.emit(format!("ld a2, {START}(a6)"));
        self.emit("mv a3, t1");
        self.emit("mv a4, t2");
        self.emit(format!("ld t0, {KIND}(a6)"));
        self.emit(format!("beqz t0, {cell_data_syscall}"));
        self.emit("li t3, 1");
        self.emit(format!("beq t0, t3, {witness_syscall}"));
        self.emit("li t3, 2");
        self.emit(format!("beq t0, t3, {lock_syscall}"));
        self.emit("li t3, 3");
        self.emit(format!("beq t0, t3, {type_syscall}"));
        self.emit(format!("j {invalid}"));
        self.emit_label(&cell_data_syscall);
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit(format!("j {syscall_ready}"));
        self.emit_label(&witness_syscall);
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit(format!("j {syscall_ready}"));
        self.emit_label(&lock_syscall);
        self.emit(format!("li a5, {CKB_CELL_FIELD_LOCK}"));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit(format!("j {syscall_ready}"));
        self.emit_label(&type_syscall);
        self.emit(format!("li a5, {CKB_CELL_FIELD_TYPE}"));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit_label(&syscall_ready);
        self.emit("ecall");
        self.emit(format!("beqz a0, {syscall_status_ok}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("bne a0, t0, {failed}"));
        self.emit_label(&syscall_status_ok);
        self.emit(format!("ld t0, {LEN}(a6)"));
        self.emit(format!("li t1, {CAPACITY}"));
        self.emit(format!("bltu t0, t1, {len_ready}"));
        self.emit("mv t0, t1");
        self.emit_label(&len_ready);
        self.emit(format!("sd t0, {LEN}(a6)"));
        self.emit(format!("ld a3, {WIDTH}(a6)"));
        self.emit(format!("bltu t0, a3, {failed}"));
        self.emit("li t1, 1");
        self.emit(format!("sd t1, {VALID}(a6)"));
        self.emit(format!("ld t2, {CACHE_BASE}(a6)"));
        self.emit(format!("sd a6, {LAST}(t2)"));
        if self.module_uses_exact_read_hot_cache {
            self.emit("mv s10, a6");
            self.emit(format!("ld s9, {VIEW}(a6)"));
            self.emit(format!("ld s8, {KIND}(a6)"));
            self.emit(format!("ld s7, {START}(a6)"));
            self.emit(format!("ld s6, {LEN}(a6)"));
            self.emit(format!("addi s5, a6, {BUFFER}"));
        }
        self.emit("li t4, 0");

        self.emit_label(&load);
        self.emit(format!("addi t0, a6, {BUFFER}"));
        self.emit("add t0, t0, t4");
        self.emit("li t1, 1");
        self.emit(format!("beq a3, t1, {load1}"));
        self.emit("li t1, 4");
        self.emit(format!("beq a3, t1, {load4}"));
        self.emit(format!("j {load8}"));
        self.emit_label(&load1);
        self.emit("lbu a0, 0(t0)");
        self.emit(format!("j {success}"));
        for (label, width) in [(&load4, 4usize), (&load8, 8usize)] {
            self.emit_label(label);
            self.emit("li a0, 0");
            for byte in 0..width {
                self.emit(format!("lbu t1, {byte}(t0)"));
                if byte != 0 {
                    self.emit(format!("slli t1, t1, {}", byte * 8));
                }
                self.emit("or a0, a0, t1");
            }
            self.emit(format!("j {success}"));
        }
        self.emit_label(&success);
        self.emit("li a1, 0");
        self.emit("ret");

        self.emit_label(&invalid);
        self.emit_process_failure(CellScriptRuntimeError::CkbSourceViewInvalid);
        self.emit_label(&failed);
        self.emit(format!("ld t0, {KIND}(a6)"));
        self.emit(format!("bnez t0, {failed_witness}"));
        self.emit_process_failure(CellScriptRuntimeError::CellLoadFailed);
        self.emit_label(&failed_witness);
        self.emit_process_failure(CellScriptRuntimeError::SyscallFailed);
    }

    /// a0=absolute CellDep index, a1=argc (0..=4), a2..a5=bytes (0..=255).
    /// Each byte becomes a NUL-terminated C string (zero means the empty string).
    /// EXEC replaces this process on success. Every return is an unconditional
    /// failure through the standard a0=status requirement ABI; it must never
    /// allow the parent to continue after a failed or unexpectedly returning EXEC.
    /// Only caller-saved registers are used. The private 64-byte frame holds four
    /// two-byte strings, five argv pointers including a null trailer, and ra.
    fn emit_runtime_exec_cell_dep_u8_args(&mut self, enabled: bool) {
        const ARGV: usize = 8;
        const RA: usize = 56;
        const FRAME: usize = 64;
        self.emit_global("__ckb_exec_cell_dep_u8_args");
        self.emit_label("__ckb_exec_cell_dep_u8_args");
        self.emit("# cellscript abi: process replacement; no successful return; a0=error on failure");
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }
        let failed = self.fresh_label("exec_cell_dep_failed");
        self.emit_stack_adjustment(-(FRAME as i64));
        self.emit_stack_store("ra", RA);
        self.emit("li t0, 4");
        self.emit(format!("bltu t0, a1, {failed}"));
        self.emit("li t0, 255");
        for register in ["a2", "a3", "a4", "a5"] {
            self.emit(format!("bltu t0, {register}, {failed}"));
        }
        self.emit_stack_store("zero", 0);
        for (index, register) in ["a2", "a3", "a4", "a5"].iter().enumerate() {
            self.emit_stack_store_byte(register, index * 2);
            self.emit(format!("addi t0, sp, {}", index * 2));
            self.emit_stack_store("t0", ARGV + index * 8);
        }
        self.emit_stack_store("zero", ARGV + 4 * 8);
        // argv[argc] is null even when fewer than four arguments are supplied.
        self.emit("slli t0, a1, 3");
        self.emit(format!("addi t1, sp, {ARGV}"));
        self.emit("add t0, t0, t1");
        self.emit("sd zero, 0(t0)");
        self.emit("addi a4, a1, 0");
        self.emit(format!("addi a5, sp, {ARGV}"));
        self.emit(format!("li a1, {CKB_SOURCE_CELL_DEP}"));
        self.emit("li a2, 0");
        self.emit("li a3, 0");
        self.emit(format!("li a7, {}", crate::ckb_abi::syscall::EXEC));
        self.emit("ecall");
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit_stack_load("ra", RA);
        self.emit_stack_adjustment(FRAME as i64);
        self.emit("ret");
    }

    /// a0=absolute CellDep, a1=proven local Vec<u8> data, a2=actual count,
    /// a3..a6=four segment lengths. Only caller-saved registers are clobbered.
    /// Maximum 256 input bytes become 512 ASCII bytes and four terminators;
    /// the disjoint argv region contains a fifth, null pointer.
    fn emit_runtime_cell_dep_hex4(&mut self, enabled: bool, returning: bool) {
        const ARGV: usize = 520;
        const RA: usize = 560;
        const DEP: usize = 568;
        const SPAWN_ARGS: usize = 576;
        const PID: usize = 608;
        const EXIT: usize = 616;
        let frame: i64 = if returning { 624 } else { 576 };
        let symbol = if returning { "__ckb_spawn_wait_cell_dep_hex4" } else { "__ckb_exec_cell_dep_hex4" };
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(if returning {
            "# cellscript abi: four hex argv; SPAWN+WAIT returns only after successful child exit"
        } else {
            "# cellscript abi: four hex argv; non-returning EXEC; any return is failure"
        });
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }
        let failed = self.fresh_label("exec_hex4_failed");
        self.emit_large_addi("sp", "sp", -frame);
        self.emit_stack_store("ra", RA);
        self.emit_stack_store("a0", DEP);
        self.emit("li t0, 256");
        for register in ["a2", "a3", "a4", "a5", "a6"] {
            self.emit(format!("bltu t0, {register}, {failed}"));
        }
        self.emit("add t0, a3, a4");
        self.emit("add t0, t0, a5");
        self.emit("add t0, t0, a6");
        self.emit(format!("bne t0, a2, {failed}"));
        self.emit("addi t1, a1, 0"); // input cursor
        self.emit("addi t2, sp, 0"); // encoded output cursor
        for (index, register) in ["a3", "a4", "a5", "a6"].iter().enumerate() {
            let scan = self.fresh_label("exec_hex4_encode");
            let done = self.fresh_label("exec_hex4_encoded");
            self.emit_stack_store("t2", ARGV + index * 8);
            self.emit(format!("addi t3, {register}, 0"));
            self.emit_label(&scan);
            self.emit(format!("beqz t3, {done}"));
            self.emit("lbu t4, 0(t1)");
            for (shift, offset) in [(4, 0), (0, 1)] {
                let digit = self.fresh_label("exec_hex4_digit");
                let ready = self.fresh_label("exec_hex4_char");
                self.emit(format!("srli t5, t4, {shift}"));
                self.emit("li t6, 15");
                self.emit("and t5, t5, t6");
                self.emit("li t6, 10");
                self.emit(format!("bltu t5, t6, {digit}"));
                self.emit("addi t5, t5, 87");
                self.emit(format!("j {ready}"));
                self.emit_label(&digit);
                self.emit("addi t5, t5, 48");
                self.emit_label(&ready);
                self.emit(format!("sb t5, {offset}(t2)"));
            }
            self.emit("addi t1, t1, 1");
            self.emit("addi t2, t2, 2");
            self.emit("addi t3, t3, -1");
            self.emit(format!("j {scan}"));
            self.emit_label(&done);
            self.emit("sb zero, 0(t2)");
            self.emit("addi t2, t2, 1");
        }
        self.emit_stack_store("zero", ARGV + 32);
        self.emit_stack_load("a0", DEP);
        self.emit(format!("li a1, {CKB_SOURCE_CELL_DEP}"));
        self.emit("li a2, 0");
        self.emit("li a3, 0");
        let done = returning.then(|| self.fresh_label("spawn_hex4_done"));
        if returning {
            // SpawnArgs is four u64 fields: argc, argv, process_id pointer,
            // inherited_fds pointer. No FDs are inherited. PID and exit byte
            // have disjoint storage and survive both scheduler transitions.
            self.emit("li t0, 4");
            self.emit_stack_store("t0", SPAWN_ARGS);
            self.emit_sp_addi("t0", ARGV);
            self.emit_stack_store("t0", SPAWN_ARGS + 8);
            self.emit_sp_addi("t0", PID);
            self.emit_stack_store("t0", SPAWN_ARGS + 16);
            self.emit_stack_store("zero", SPAWN_ARGS + 24);
            self.emit_stack_store("zero", PID);
            self.emit_stack_store("zero", EXIT);
            self.emit_sp_addi("a4", SPAWN_ARGS);
            self.emit("li a5, 0");
            self.emit(format!("li a7, {}", ckb_abi::syscall::SPAWN));
            self.emit("ecall");
            self.emit(format!("bnez a0, {failed}"));
            self.emit_stack_load("a0", PID);
            self.emit_sp_addi("a1", EXIT);
            self.emit(format!("li a7, {}", ckb_abi::syscall::WAIT));
            self.emit("ecall");
            self.emit(format!("bnez a0, {failed}"));
            self.emit_stack_load_byte("t0", EXIT);
            self.emit(format!("bnez t0, {failed}"));
            self.emit("li a0, 0");
            self.emit(format!("j {}", done.as_ref().expect("returning label")));
        } else {
            self.emit("li a4, 4");
            self.emit_sp_addi("a5", ARGV);
            self.emit(format!("li a7, {}", ckb_abi::syscall::EXEC));
            self.emit("ecall");
        }
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        if let Some(done) = done {
            self.emit_label(&done);
        }
        self.emit_stack_load("ra", RA);
        self.emit_large_addi("sp", "sp", frame);
        self.emit("ret");
    }

    /// a0=SourceView; byte reads additionally take a1=offset. a0=value,
    /// a1=status. Only caller-saved registers are touched; the private frame
    /// preserves offset across SourceView decoding (which clobbers t0..t6).
    /// An absent Type Script is a read failure, not an empty Script.
    fn emit_runtime_cell_script_read(&mut self, symbol: &str, field: u64, byte: bool, enabled: bool) {
        debug_assert!(matches!(field, CKB_CELL_FIELD_LOCK | CKB_CELL_FIELD_TYPE));
        const OFFSET: usize = 0;
        const SIZE: usize = 8;
        const BUFFER: usize = 16;
        const RA: usize = 24;
        const FRAME: usize = 32;
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit("# cellscript abi: complete serialized Script size or exact byte; a0=value, a1=error");
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }
        let invalid = self.fresh_label("script_read_invalid_source");
        let ready = self.fresh_label("script_read_source_ready");
        let failed = self.fresh_label("script_read_failed");
        let done = self.fresh_label("script_read_done");
        self.emit_stack_adjustment(-(FRAME as i64));
        self.emit_stack_store("ra", RA);
        if byte {
            self.emit_stack_store("a1", OFFSET);
        }
        self.emit_decode_source_view_to_t1_t2(&invalid);
        for allowed in [
            CKB_SOURCE_INPUT,
            CKB_SOURCE_OUTPUT,
            CKB_SOURCE_CELL_DEP,
            CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT,
            CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT,
        ] {
            self.emit(format!("li t0, {allowed}"));
            self.emit(format!("beq t0, t2, {ready}"));
        }
        self.emit(format!("j {invalid}"));
        self.emit_label(&ready);
        self.emit(format!("li t0, {}", u8::from(byte)));
        self.emit_stack_store("t0", SIZE);
        self.emit_sp_addi("a0", BUFFER);
        self.emit_sp_addi("a1", SIZE);
        if byte {
            self.emit_stack_load("a2", OFFSET);
        } else {
            self.emit("li a2, 0");
        }
        self.emit("mv a3, t1");
        self.emit("mv a4, t2");
        self.emit(format!("li a5, {field}"));
        self.emit(format!("li a7, {}", self.runtime_abi().load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {failed}"));
        self.emit_stack_load("a0", SIZE);
        if byte {
            // The syscall reports full remaining length, not bytes copied.
            // Offset at/beyond EOF clamps to an empty read and must fail.
            self.emit(format!("beqz a0, {failed}"));
            self.emit_stack_load_byte("a0", BUFFER);
        }
        self.emit("li a1, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&invalid);
        self.emit_process_failure(CellScriptRuntimeError::CkbSourceViewInvalid);
        self.emit_label(&failed);
        self.emit_process_failure(CellScriptRuntimeError::CellLoadFailed);
        self.emit_label(&done);
        self.emit_stack_load("ra", RA);
        self.emit_stack_adjustment(FRAME as i64);
        self.emit("ret");
    }

    /// a0=Input/GroupInput SourceView; a0=raw u64 since, a1=status.
    /// This does not interpret or authorize a timelock. The no-argument
    /// __ckb_input_since helper retains its old GroupInput-0 contract.
    fn emit_runtime_input_since_at(&mut self, enabled: bool) {
        const SIZE: usize = 0;
        const VALUE: usize = 8;
        const RA: usize = 24;
        const FRAME: usize = 32;
        self.emit_global("__ckb_input_since_at");
        self.emit_label("__ckb_input_since_at");
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }
        let invalid = self.fresh_label("since_at_invalid_source");
        let failed = self.fresh_label("since_at_load_failed");
        let done = self.fresh_label("since_at_done");
        self.emit_stack_adjustment(-(FRAME as i64));
        self.emit_stack_store("ra", RA);
        self.emit_decode_input_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 8");
        self.emit_stack_store("t0", SIZE);
        self.emit_sp_addi("a0", VALUE);
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit("mv a3, t1");
        self.emit("mv a4, t2");
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_SINCE));
        self.emit(format!("li a7, {}", self.runtime_abi().load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {failed}"));
        self.emit_stack_load("t0", SIZE);
        self.emit("li t1, 8");
        self.emit(format!("bne t0, t1, {failed}"));
        self.emit_stack_load("a0", VALUE);
        self.emit("li a1, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&invalid);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {done}"));
        self.emit_label(&failed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", RA);
        self.emit_stack_adjustment(FRAME as i64);
        self.emit("ret");
    }

    fn emit_runtime_bounded_witness_resolver(&mut self, enabled: bool) {
        const VIEW: usize = 0;
        const OWNER: usize = 8;
        const MAXIMUM: usize = 16;
        const TOTAL_SIZE: usize = 24;
        const HEADER: usize = 32;
        const READ_SIZE: usize = 48;
        const INDEX: usize = 56;
        const SOURCE: usize = 64;
        const FIELD_START: usize = 72;
        const FIELD_END: usize = 80;
        const FRAME: usize = 96;

        self.emit_global("__cellscript_witness_bounded_resolve");
        self.emit_label("__cellscript_witness_bounded_resolve");
        self.emit("# cellscript abi: a0=SourceView,a1=owner(raw/lock/entry/output_type),a2=max<=65536");
        self.emit("# returns a0=payload_offset,a1=payload_len,a2=index,a3=source,a4=status");
        if !enabled {
            self.emit("li a0, 0");
            self.emit("li a1, 0");
            self.emit("li a2, 0");
            self.emit("li a3, 0");
            self.emit(format!("li a4, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("bounded_witness_source_invalid");
        let source_ready = self.fresh_label("bounded_witness_source_ready");
        let size_status_ok = self.fresh_label("bounded_witness_size_status_ok");
        let raw = self.fresh_label("bounded_witness_raw");
        let header_status_ok = self.fresh_label("bounded_witness_header_status_ok");
        let select_lock = self.fresh_label("bounded_witness_select_lock");
        let select_entry = self.fresh_label("bounded_witness_select_entry");
        let select_output_type = self.fresh_label("bounded_witness_select_output_type");
        let selected = self.fresh_label("bounded_witness_selected");
        let length_status_ok = self.fresh_label("bounded_witness_length_status_ok");
        let success = self.fresh_label("bounded_witness_success");
        let failed = self.fresh_label("bounded_witness_load_failed");
        let malformed = self.fresh_label("bounded_witness_malformed");
        let truncated = self.fresh_label("bounded_witness_truncated");
        let absent = self.fresh_label("bounded_witness_absent");
        let exceeded = self.fresh_label("bounded_witness_exceeded");
        let finish = self.fresh_label("bounded_witness_finish");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-(FRAME as i64));
        self.emit_stack_store("a0", VIEW);
        self.emit_stack_store("a1", OWNER);
        self.emit_stack_store("a2", MAXIMUM);
        self.emit("li t0, 65536");
        self.emit(format!("bltu t0, a2, {exceeded}"));
        self.emit(format!("li t0, {CKB_WITNESS_OWNER_OUTPUT_TYPE}"));
        self.emit(format!("bltu t0, a1, {invalid}"));
        self.emit_decode_source_view_to_t1_t2(&invalid);
        for source in
            [CKB_SOURCE_INPUT, CKB_SOURCE_OUTPUT, CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT, CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT]
        {
            self.emit(format!("li t0, {source}"));
            self.emit(format!("beq t0, t2, {source_ready}"));
        }
        self.emit(format!("j {invalid}"));
        self.emit_label(&source_ready);
        self.emit_stack_store("t1", INDEX);
        self.emit_stack_store("t2", SOURCE);

        self.emit_stack_store("zero", TOTAL_SIZE);
        self.emit("li a0, 0");
        self.emit_sp_addi("a1", TOTAL_SIZE);
        self.emit("li a2, 0");
        self.emit("mv a3, t1");
        self.emit("mv a4, t2");
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("beqz a0, {size_status_ok}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {size_status_ok}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&size_status_ok);
        self.emit_stack_load("t0", OWNER);
        self.emit(format!("beqz t0, {raw}"));

        self.emit_stack_load("t0", TOTAL_SIZE);
        self.emit("li t1, 16");
        self.emit(format!("bltu t0, t1, {malformed}"));
        self.emit_stack_store("t1", READ_SIZE);
        self.emit_sp_addi("a0", HEADER);
        self.emit_sp_addi("a1", READ_SIZE);
        self.emit("li a2, 0");
        self.emit_stack_load("a3", INDEX);
        self.emit_stack_load("a4", SOURCE);
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("beqz a0, {header_status_ok}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {header_status_ok}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&header_status_ok);
        self.emit_sp_addi("t3", HEADER);
        self.emit_u32_le_from_base_to("t4", "t3", 0, "t5");
        self.emit_stack_load("t0", TOTAL_SIZE);
        self.emit(format!("bne t4, t0, {malformed}"));
        self.emit_u32_le_from_base_to("t4", "t3", 4, "t5");
        self.emit("li t5, 16");
        self.emit(format!("bne t4, t5, {malformed}"));
        self.emit_u32_le_from_base_to("t4", "t3", 4, "t5");
        self.emit_u32_le_from_base_to("t5", "t3", 8, "t6");
        self.emit_u32_le_from_base_to("t6", "t3", 12, "t2");
        self.emit(format!("bltu t5, t4, {malformed}"));
        self.emit(format!("bltu t6, t5, {malformed}"));
        self.emit_stack_load("t0", TOTAL_SIZE);
        self.emit(format!("bltu t0, t6, {truncated}"));
        self.emit_stack_store("t4", FIELD_START);
        self.emit_stack_store("t5", FIELD_END);
        self.emit_stack_load("t0", OWNER);
        self.emit(format!("li t1, {CKB_WITNESS_OWNER_LOCK}"));
        self.emit(format!("beq t0, t1, {select_lock}"));
        self.emit(format!("li t1, {CKB_WITNESS_OWNER_ENTRY}"));
        self.emit(format!("beq t0, t1, {select_entry}"));
        self.emit(format!("j {select_output_type}"));
        self.emit_label(&select_lock);
        self.emit(format!("j {selected}"));
        self.emit_label(&select_entry);
        self.emit_stack_store("t5", FIELD_START);
        self.emit_stack_store("t6", FIELD_END);
        self.emit(format!("j {selected}"));
        self.emit_label(&select_output_type);
        self.emit_stack_store("t6", FIELD_START);
        self.emit_stack_load("t0", TOTAL_SIZE);
        self.emit_stack_store("t0", FIELD_END);
        self.emit_label(&selected);

        self.emit_stack_load("t4", FIELD_START);
        self.emit_stack_load("t5", FIELD_END);
        self.emit("sub t2, t5, t4");
        self.emit(format!("beqz t2, {absent}"));
        self.emit("li t3, 4");
        self.emit(format!("bltu t2, t3, {malformed}"));
        self.emit_stack_store("t3", READ_SIZE);
        self.emit_sp_addi("a0", HEADER);
        self.emit_sp_addi("a1", READ_SIZE);
        self.emit("mv a2, t4");
        self.emit_stack_load("a3", INDEX);
        self.emit_stack_load("a4", SOURCE);
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("beqz a0, {length_status_ok}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {length_status_ok}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&length_status_ok);
        self.emit_sp_addi("t3", HEADER);
        self.emit_u32_le_from_base_to("t1", "t3", 0, "t5");
        self.emit_stack_load("t4", FIELD_START);
        self.emit_stack_load("t5", FIELD_END);
        self.emit("sub t2, t5, t4");
        self.emit("addi t2, t2, -4");
        self.emit(format!("bne t1, t2, {malformed}"));
        self.emit_stack_load("t3", MAXIMUM);
        self.emit(format!("bltu t3, t1, {exceeded}"));
        self.emit("addi a0, t4, 4");
        self.emit("mv a1, t1");
        self.emit(format!("j {success}"));

        self.emit_label(&raw);
        self.emit_stack_load("a1", TOTAL_SIZE);
        self.emit_stack_load("t0", MAXIMUM);
        self.emit(format!("bltu t0, a1, {exceeded}"));
        self.emit("li a0, 0");
        self.emit_label(&success);
        self.emit_stack_load("a2", INDEX);
        self.emit_stack_load("a3", SOURCE);
        self.emit("li a4, 0");
        self.emit(format!("j {finish}"));

        for (label, error) in [
            (&invalid, CellScriptRuntimeError::CkbSourceViewInvalid),
            (&failed, CellScriptRuntimeError::SyscallFailed),
            (&malformed, CellScriptRuntimeError::WitnessMalformed),
            (&truncated, CellScriptRuntimeError::WitnessFieldTruncated),
            (&absent, CellScriptRuntimeError::WitnessFieldAbsent),
            (&exceeded, CellScriptRuntimeError::WitnessBoundExceeded),
        ] {
            self.emit_label(label);
            self.emit("li a0, 0");
            self.emit("li a1, 0");
            self.emit("li a2, 0");
            self.emit("li a3, 0");
            self.emit(format!("li a4, {}", error.code()));
            self.emit(format!("j {finish}"));
        }
        self.emit_label(&finish);
        self.emit_stack_adjustment(FRAME as i64);
        self.emit("ret");
    }

    /// Append one already-validated signing-message segment.
    /// t0=pointer/transaction offset, t1=length, t2=segment kind. The caller
    /// keeps the descriptor count and total length in the fixed sighash frame.
    fn emit_sighash_descriptor_append(&mut self, failed: &str) {
        const DESCRIPTOR_COUNT: usize = 40;
        const TOTAL_LENGTH: usize = 48;
        const DESCRIPTORS: usize = 1184;
        self.emit_stack_load("t3", DESCRIPTOR_COUNT);
        self.emit("li t4, 260");
        self.emit(format!("bgeu t3, t4, {failed}"));
        self.emit("li t4, 24");
        self.emit("mul t4, t3, t4");
        self.emit("add t4, sp, t4");
        self.emit(format!("addi t4, t4, {DESCRIPTORS}"));
        self.emit("sd t0, 0(t4)");
        self.emit("sd t1, 8(t4)");
        self.emit("sd t2, 16(t4)");
        self.emit("addi t3, t3, 1");
        self.emit_stack_store("t3", DESCRIPTOR_COUNT);
        self.emit_stack_load("t5", TOTAL_LENGTH);
        self.emit("add t6, t5, t1");
        self.emit(format!("bltu t6, t5, {failed}"));
        self.emit_stack_store("t6", TOTAL_LENGTH);
    }

    fn emit_runtime_sighash_all_zero_lock(&mut self, enabled: bool) {
        const MAX_GROUP_INPUTS: usize = 0;
        const MAX_INPUTS: usize = 8;
        const MAX_EXTRA_WITNESSES: usize = 16;
        const MAX_WITNESS_BYTES: usize = 24;
        const OUT: usize = 32;
        const DESCRIPTOR_COUNT: usize = 40;
        const TOTAL_LENGTH: usize = 48;
        const GROUP_COUNT: usize = 56;
        const INPUT_COUNT: usize = 64;
        const ITERATOR: usize = 72;
        const SIZE: usize = 80;
        const FIRST_SIZE: usize = 88;
        const LOCK_OFFSET: usize = 96;
        const LOCK_LENGTH: usize = 104;
        const PREFIX_COUNT: usize = 112;
        const TX_HASH: usize = 120;
        const LENGTH_PREFIXES: usize = 152;
        const DESCRIPTORS: usize = 1184;
        const RA: usize = 7424;
        const FRAME: usize = 7440;

        self.emit_global("__ckb_sighash_all_zero_lock");
        self.emit_label("__ckb_sighash_all_zero_lock");
        self.emit("# cellscript abi: a0=max_group_inputs<=64,a1=max_inputs<=256,a2=max_extra_witnesses<=64,a3=max_witness_bytes<=65536,a4=out[32]");
        self.emit("# digest = tx_hash || len(first WitnessArgs with lock payload zeroed) || first witness || later group witnesses || witnesses after inputs");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let bound = self.fresh_label("sighash_zero_lock_bound");
        let failed = self.fresh_label("sighash_zero_lock_failed");
        let resolver_failed = self.fresh_label("sighash_zero_lock_resolver_failed");
        let finish = self.fresh_label("sighash_zero_lock_finish");
        let group_scan = self.fresh_label("sighash_group_scan");
        let group_present = self.fresh_label("sighash_group_present");
        let group_limit = self.fresh_label("sighash_group_limit");
        let group_done = self.fresh_label("sighash_group_done");
        let group_limit_absent = self.fresh_label("sighash_group_limit_absent");
        let input_scan = self.fresh_label("sighash_input_scan");
        let input_present = self.fresh_label("sighash_input_present");
        let input_limit = self.fresh_label("sighash_input_limit");
        let input_done = self.fresh_label("sighash_input_done");
        let input_limit_absent = self.fresh_label("sighash_input_limit_absent");
        let first_size_ok = self.fresh_label("sighash_first_size_ok");
        let later_group_scan = self.fresh_label("sighash_later_group_scan");
        let later_group_present = self.fresh_label("sighash_later_group_present");
        let later_group_next = self.fresh_label("sighash_later_group_next");
        let later_group_done = self.fresh_label("sighash_later_group_done");
        let extra_scan = self.fresh_label("sighash_extra_scan");
        let extra_present = self.fresh_label("sighash_extra_present");
        let extra_limit = self.fresh_label("sighash_extra_limit");
        let extra_done = self.fresh_label("sighash_extra_done");
        let extra_limit_absent = self.fresh_label("sighash_extra_limit_absent");
        let abi = self.runtime_abi();

        self.emit_large_addi("sp", "sp", -(FRAME as i64));
        for (register, offset) in
            [("a0", MAX_GROUP_INPUTS), ("a1", MAX_INPUTS), ("a2", MAX_EXTRA_WITNESSES), ("a3", MAX_WITNESS_BYTES), ("a4", OUT)]
        {
            self.emit_stack_store(register, offset);
        }
        self.emit_stack_store("ra", RA);
        self.emit_stack_store("zero", DESCRIPTOR_COUNT);
        self.emit_stack_store("zero", TOTAL_LENGTH);
        self.emit_stack_store("zero", PREFIX_COUNT);

        self.emit_stack_load("t0", MAX_GROUP_INPUTS);
        self.emit(format!("beqz t0, {bound}"));
        self.emit("li t1, 64");
        self.emit(format!("bltu t1, t0, {bound}"));
        self.emit_stack_load("t2", MAX_INPUTS);
        self.emit(format!("beqz t2, {bound}"));
        self.emit("li t1, 256");
        self.emit(format!("bltu t1, t2, {bound}"));
        self.emit(format!("bltu t2, t0, {bound}"));
        self.emit_stack_load("t0", MAX_EXTRA_WITNESSES);
        self.emit("li t1, 64");
        self.emit(format!("bltu t1, t0, {bound}"));
        self.emit_stack_load("t0", MAX_WITNESS_BYTES);
        self.emit(format!("beqz t0, {bound}"));
        self.emit("li t1, 65536");
        self.emit(format!("bltu t1, t0, {bound}"));

        // Count the complete current input Script Group, proving the declared
        // bound with LOAD_INPUT rather than inferring it from witness presence.
        self.emit_stack_store("zero", ITERATOR);
        self.emit_label(&group_scan);
        self.emit_stack_load("t0", ITERATOR);
        self.emit_stack_load("t1", MAX_GROUP_INPUTS);
        self.emit(format!("bgeu t0, t1, {group_limit}"));
        self.emit_stack_store("zero", SIZE);
        self.emit("li a0, 0");
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit("mv a3, t0");
        self.emit(format!("li a4, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_INPUT));
        self.emit("ecall");
        self.emit(format!("beqz a0, {group_present}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {group_present}"));
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {group_done}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&group_present);
        self.emit_stack_load("t0", ITERATOR);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", ITERATOR);
        self.emit(format!("j {group_scan}"));
        self.emit_label(&group_limit);
        self.emit_stack_store("zero", SIZE);
        self.emit("li a0, 0");
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit_stack_load("a3", MAX_GROUP_INPUTS);
        self.emit(format!("li a4, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_INPUT));
        self.emit("ecall");
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {group_limit_absent}"));
        self.emit(format!("beqz a0, {bound}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {bound}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&group_limit_absent);
        self.emit(format!("j {group_done}"));
        self.emit_label(&group_done);
        self.emit_stack_load("t0", ITERATOR);
        self.emit(format!("beqz t0, {failed}"));
        self.emit_stack_store("t0", GROUP_COUNT);

        // Count all transaction inputs independently. The extra witness domain
        // starts exactly at this cardinality.
        self.emit_stack_store("zero", ITERATOR);
        self.emit_label(&input_scan);
        self.emit_stack_load("t0", ITERATOR);
        self.emit_stack_load("t1", MAX_INPUTS);
        self.emit(format!("bgeu t0, t1, {input_limit}"));
        self.emit_stack_store("zero", SIZE);
        self.emit("li a0, 0");
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit("mv a3, t0");
        self.emit(format!("li a4, {CKB_SOURCE_INPUT}"));
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_INPUT));
        self.emit("ecall");
        self.emit(format!("beqz a0, {input_present}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {input_present}"));
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {input_done}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&input_present);
        self.emit_stack_load("t0", ITERATOR);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", ITERATOR);
        self.emit(format!("j {input_scan}"));
        self.emit_label(&input_limit);
        self.emit_stack_store("zero", SIZE);
        self.emit("li a0, 0");
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit_stack_load("a3", MAX_INPUTS);
        self.emit(format!("li a4, {CKB_SOURCE_INPUT}"));
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_INPUT));
        self.emit("ecall");
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {input_limit_absent}"));
        self.emit(format!("beqz a0, {bound}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {bound}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&input_limit_absent);
        self.emit(format!("j {input_done}"));
        self.emit_label(&input_done);
        self.emit_stack_load("t0", ITERATOR);
        self.emit(format!("beqz t0, {failed}"));
        self.emit_stack_store("t0", INPUT_COUNT);

        // Load the canonical raw transaction hash used by ckb-sdk-rust's
        // generate_message before processing any witnesses.
        self.emit("li t0, 32");
        self.emit_stack_store("t0", SIZE);
        self.emit_sp_addi("a0", TX_HASH);
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_TX_HASH));
        self.emit("ecall");
        self.emit(format!("bnez a0, {failed}"));
        self.emit_stack_load("t0", SIZE);
        self.emit("li t1, 32");
        self.emit(format!("bne t0, t1, {failed}"));

        // The first group witness is mandatory. Bound its complete bytes, then
        // resolve the exact WitnessArgs.lock payload span for equal-length zeroing.
        self.emit_stack_store("zero", SIZE);
        self.emit("li a0, 0");
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit("li a3, 0");
        self.emit(format!("li a4, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("beqz a0, {first_size_ok}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {first_size_ok}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&first_size_ok);
        self.emit_stack_load("t0", SIZE);
        self.emit_stack_load("t1", MAX_WITNESS_BYTES);
        self.emit(format!("bltu t1, t0, {bound}"));
        self.emit_stack_store("t0", FIRST_SIZE);
        self.emit(format!("li a0, {}", CKB_SOURCE_VIEW_GROUP_INPUT * CKB_SOURCE_VIEW_SHIFT));
        self.emit(format!("li a1, {CKB_WITNESS_OWNER_LOCK}"));
        self.emit_stack_load("a2", MAX_WITNESS_BYTES);
        self.emit("call __cellscript_witness_bounded_resolve");
        self.emit(format!("bnez a4, {resolver_failed}"));
        self.emit_stack_store("a0", LOCK_OFFSET);
        self.emit_stack_store("a1", LOCK_LENGTH);

        self.emit_sp_addi("t0", TX_HASH);
        self.emit("li t1, 32");
        self.emit("li t2, 0");
        self.emit_sighash_descriptor_append(&bound);
        self.emit_stack_load("t0", FIRST_SIZE);
        self.emit_stack_store("t0", LENGTH_PREFIXES);
        self.emit_sp_addi("t0", LENGTH_PREFIXES);
        self.emit("li t1, 8");
        self.emit("li t2, 0");
        self.emit_sighash_descriptor_append(&bound);
        self.emit("li t0, 0");
        self.emit_stack_load("t1", LOCK_OFFSET);
        self.emit("li t2, 4");
        self.emit_sighash_descriptor_append(&bound);
        self.emit("li t0, 0");
        self.emit_stack_load("t1", LOCK_LENGTH);
        self.emit("li t2, 2");
        self.emit_sighash_descriptor_append(&bound);
        self.emit_stack_load("t0", LOCK_OFFSET);
        self.emit_stack_load("t1", LOCK_LENGTH);
        self.emit("add t0, t0, t1");
        self.emit_stack_load("t1", FIRST_SIZE);
        self.emit("sub t1, t1, t0");
        self.emit("li t2, 4");
        self.emit_sighash_descriptor_append(&bound);
        self.emit("li t0, 1");
        self.emit_stack_store("t0", PREFIX_COUNT);

        // Later group inputs contribute only when their absolute witness exists,
        // matching ScriptSigner::generate_message's filter_map contract.
        self.emit("li t0, 1");
        self.emit_stack_store("t0", ITERATOR);
        self.emit_label(&later_group_scan);
        self.emit_stack_load("t0", ITERATOR);
        self.emit_stack_load("t1", GROUP_COUNT);
        self.emit(format!("bgeu t0, t1, {later_group_done}"));
        self.emit_stack_store("zero", SIZE);
        self.emit("li a0, 0");
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit("mv a3, t0");
        self.emit(format!("li a4, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("beqz a0, {later_group_present}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {later_group_present}"));
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {later_group_next}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&later_group_present);
        self.emit_stack_load("t0", SIZE);
        self.emit_stack_load("t1", MAX_WITNESS_BYTES);
        self.emit(format!("bltu t1, t0, {bound}"));
        self.emit_stack_load("t2", PREFIX_COUNT);
        self.emit("li t3, 128");
        self.emit(format!("bgeu t2, t3, {bound}"));
        self.emit("slli t3, t2, 3");
        self.emit("add t3, sp, t3");
        self.emit(format!("addi t3, t3, {LENGTH_PREFIXES}"));
        self.emit("sd t0, 0(t3)");
        self.emit("mv t0, t3");
        self.emit("li t1, 8");
        self.emit("li t2, 0");
        self.emit_sighash_descriptor_append(&bound);
        self.emit("li t0, 0");
        self.emit_stack_load("t1", SIZE);
        self.emit_stack_load("t2", ITERATOR);
        self.emit("slli t2, t2, 1");
        self.emit("addi t2, t2, 4");
        self.emit_sighash_descriptor_append(&bound);
        self.emit_stack_load("t0", PREFIX_COUNT);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", PREFIX_COUNT);
        self.emit_label(&later_group_next);
        self.emit_stack_load("t0", ITERATOR);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", ITERATOR);
        self.emit(format!("j {later_group_scan}"));
        self.emit_label(&later_group_done);

        // Include every witness after the transaction input vector. A probe at
        // the declared limit proves there is no silently omitted suffix.
        self.emit_stack_store("zero", ITERATOR);
        self.emit_label(&extra_scan);
        self.emit_stack_load("t0", ITERATOR);
        self.emit_stack_load("t1", MAX_EXTRA_WITNESSES);
        self.emit(format!("bgeu t0, t1, {extra_limit}"));
        self.emit_stack_load("t2", INPUT_COUNT);
        self.emit("add t2, t2, t0");
        self.emit_stack_store("zero", SIZE);
        self.emit("li a0, 0");
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit("mv a3, t2");
        self.emit(format!("li a4, {CKB_SOURCE_INPUT}"));
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("beqz a0, {extra_present}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {extra_present}"));
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {extra_done}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&extra_present);
        self.emit_stack_load("t0", SIZE);
        self.emit_stack_load("t1", MAX_WITNESS_BYTES);
        self.emit(format!("bltu t1, t0, {bound}"));
        self.emit_stack_load("t2", PREFIX_COUNT);
        self.emit("li t3, 128");
        self.emit(format!("bgeu t2, t3, {bound}"));
        self.emit("slli t3, t2, 3");
        self.emit("add t3, sp, t3");
        self.emit(format!("addi t3, t3, {LENGTH_PREFIXES}"));
        self.emit("sd t0, 0(t3)");
        self.emit("mv t0, t3");
        self.emit("li t1, 8");
        self.emit("li t2, 0");
        self.emit_sighash_descriptor_append(&bound);
        self.emit("li t0, 0");
        self.emit_stack_load("t1", SIZE);
        self.emit_stack_load("t2", INPUT_COUNT);
        self.emit_stack_load("t3", ITERATOR);
        self.emit("add t2, t2, t3");
        self.emit("slli t2, t2, 1");
        self.emit("addi t2, t2, 3");
        self.emit_sighash_descriptor_append(&bound);
        self.emit_stack_load("t0", PREFIX_COUNT);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", PREFIX_COUNT);
        self.emit_stack_load("t0", ITERATOR);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", ITERATOR);
        self.emit(format!("j {extra_scan}"));
        self.emit_label(&extra_limit);
        self.emit_stack_load("t0", INPUT_COUNT);
        self.emit_stack_load("t1", MAX_EXTRA_WITNESSES);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("zero", SIZE);
        self.emit("li a0, 0");
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit("mv a3, t0");
        self.emit(format!("li a4, {CKB_SOURCE_INPUT}"));
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {extra_limit_absent}"));
        self.emit(format!("beqz a0, {bound}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {bound}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&extra_limit_absent);
        self.emit_label(&extra_done);

        self.emit_sp_addi("a0", DESCRIPTORS);
        self.emit_stack_load("a1", DESCRIPTOR_COUNT);
        self.emit("li a2, 0");
        self.emit("li a3, 0");
        self.emit(format!("li a4, {}", abi.load_witness));
        self.emit_stack_load("a5", OUT);
        self.emit_stack_load("a6", TOTAL_LENGTH);
        self.emit("call __cellscript_blake2b_segments");
        self.emit(format!("j {finish}"));

        self.emit_label(&resolver_failed);
        self.emit("mv a0, a4");
        self.emit(format!("j {finish}"));
        self.emit_label(&bound);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SighashBoundExceeded.code()));
        self.emit(format!("j {finish}"));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit_label(&finish);
        self.emit_stack_load("ra", RA);
        self.emit_large_addi("sp", "sp", FRAME as i64);
        self.emit("ret");
    }

    fn emit_runtime_bounded_witness_size(&mut self, enabled: bool) {
        const RA: usize = 0;
        const FRAME: usize = 16;
        self.emit_global("__ckb_witness_bounded_size");
        self.emit_label("__ckb_witness_bounded_size");
        self.emit("# cellscript abi: a0=view,a1=owner,a2=max; returns a0=size,a1=status");
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let failed = self.fresh_label("bounded_witness_size_failed");
        let done = self.fresh_label("bounded_witness_size_done");
        self.emit_stack_adjustment(-(FRAME as i64));
        self.emit_stack_store("ra", RA);
        self.emit("call __cellscript_witness_bounded_resolve");
        self.emit(format!("bnez a4, {failed}"));
        self.emit("mv a0, a1");
        self.emit("li a1, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&failed);
        self.emit("li a0, 0");
        self.emit("mv a1, a4");
        self.emit_label(&done);
        self.emit_stack_load("ra", RA);
        self.emit_stack_adjustment(FRAME as i64);
        self.emit("ret");
    }

    fn emit_runtime_bounded_witness_word(&mut self, symbol: &str, width: usize, enabled: bool) {
        debug_assert!(matches!(width, 1 | 4 | 8));
        const RELATIVE_OFFSET: usize = 0;
        const PAYLOAD_OFFSET: usize = 8;
        const PAYLOAD_LEN: usize = 16;
        const INDEX: usize = 24;
        const SOURCE: usize = 32;
        const READ_SIZE: usize = 40;
        const BUFFER: usize = 48;
        const RA: usize = 56;
        const FRAME: usize = 64;
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: a0=view,a1=owner,a2=max,a3=offset; exact {width}-byte bounded witness read"));
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let resolved = self.fresh_label("bounded_witness_word_resolved");
        let status_ok = self.fresh_label("bounded_witness_word_status_ok");
        let bounds = self.fresh_label("bounded_witness_word_bounds");
        let failed = self.fresh_label("bounded_witness_word_failed");
        let done = self.fresh_label("bounded_witness_word_done");
        let abi = self.runtime_abi();
        self.emit_stack_adjustment(-(FRAME as i64));
        self.emit_stack_store("ra", RA);
        self.emit_stack_store("a3", RELATIVE_OFFSET);
        self.emit("call __cellscript_witness_bounded_resolve");
        self.emit(format!("beqz a4, {resolved}"));
        self.emit("li a0, 0");
        self.emit("mv a1, a4");
        self.emit(format!("j {done}"));
        self.emit_label(&resolved);
        self.emit_stack_store("a0", PAYLOAD_OFFSET);
        self.emit_stack_store("a1", PAYLOAD_LEN);
        self.emit_stack_store("a2", INDEX);
        self.emit_stack_store("a3", SOURCE);
        self.emit_stack_load("t0", RELATIVE_OFFSET);
        self.emit_stack_load("t1", PAYLOAD_LEN);
        self.emit(format!("bltu t1, t0, {bounds}"));
        self.emit("sub t1, t1, t0");
        self.emit(format!("li t2, {width}"));
        self.emit(format!("bltu t1, t2, {bounds}"));
        self.emit_stack_load("t1", PAYLOAD_OFFSET);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("t2", READ_SIZE);
        self.emit_sp_addi("a0", BUFFER);
        self.emit_sp_addi("a1", READ_SIZE);
        self.emit("mv a2, t0");
        self.emit_stack_load("a3", INDEX);
        self.emit_stack_load("a4", SOURCE);
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("beqz a0, {status_ok}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {status_ok}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&status_ok);
        if width == 1 {
            self.emit_stack_load_byte("a0", BUFFER);
        } else if width == 4 {
            self.emit_sp_addi("t0", BUFFER);
            self.emit_u32_le_from_base_to("a0", "t0", 0, "t1");
        } else {
            self.emit_stack_load("a0", BUFFER);
        }
        self.emit("li a1, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&bounds);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::BoundsCheckFailed.code()));
        self.emit(format!("j {done}"));
        self.emit_label(&failed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", RA);
        self.emit_stack_adjustment(FRAME as i64);
        self.emit("ret");
    }

    fn emit_runtime_bounded_witness_blake2b(&mut self, enabled: bool) {
        const OUT: usize = 0;
        const RA: usize = 8;
        const FRAME: usize = 16;
        self.emit_global("__ckb_witness_bounded_blake2b");
        self.emit_label("__ckb_witness_bounded_blake2b");
        self.emit("# cellscript abi: a0=view,a1=owner,a2=max,a3=out[32]; returns a0=status");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let failed = self.fresh_label("bounded_witness_blake2b_failed");
        let done = self.fresh_label("bounded_witness_blake2b_done");
        self.emit_stack_adjustment(-(FRAME as i64));
        self.emit_stack_store("ra", RA);
        self.emit_stack_store("a3", OUT);
        self.emit("call __cellscript_witness_bounded_resolve");
        self.emit(format!("bnez a4, {failed}"));
        self.emit("mv t0, a0");
        self.emit("mv t1, a1");
        self.emit("mv a0, a2");
        self.emit("mv a1, a3");
        self.emit("mv a2, t0");
        self.emit("mv a3, t1");
        self.emit_stack_load("a4", OUT);
        self.emit(format!("li a5, {}", self.runtime_abi().load_witness));
        self.emit("call __cellscript_blake2b_transaction_span");
        self.emit(format!("j {done}"));
        self.emit_label(&failed);
        self.emit("mv a0, a4");
        self.emit_label(&done);
        self.emit_stack_load("ra", RA);
        self.emit_stack_adjustment(FRAME as i64);
        self.emit("ret");
    }

    fn emit_runtime_witness_size_helper(&mut self, enabled: bool) {
        const SIZE_OFFSET: usize = 0;
        const RA_OFFSET: usize = 8;
        const FRAME_SIZE: usize = align_frame(RA_OFFSET + 8);

        self.emit_global("__ckb_witness_size");
        self.emit_label("__ckb_witness_size");
        self.emit("# cellscript abi: witness byte size via LOAD_WITNESS");
        self.emit("# cellscript abi: args a0=SourceView; returns a0=size, a1=0 on success, a1=error_code on failure");
        if !enabled {
            self.emit_process_failure(CellScriptRuntimeError::SyscallFailed);
            return;
        }
        let invalid = self.fresh_label("witness_size_source_invalid");
        let failed = self.fresh_label("witness_size_load_failed");
        let status_ok = self.fresh_label("witness_size_status_ok");
        let done = self.fresh_label("witness_size_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-(FRAME_SIZE as i64));
        self.emit_stack_store("ra", RA_OFFSET);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 0");
        self.emit_stack_store("t0", SIZE_OFFSET);
        self.emit("li a0, 0");
        self.emit_sp_addi("a1", SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", status_ok));
        self.emit(format!("beqz a0, {}", status_ok));
        self.emit(format!("j {}", failed));

        self.emit_label(&status_ok);
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit_process_failure(CellScriptRuntimeError::CkbSourceViewInvalid);

        self.emit_label(&failed);
        self.emit_process_failure(CellScriptRuntimeError::SyscallFailed);

        self.emit_label(&done);
        self.emit_stack_load("a0", SIZE_OFFSET);
        self.emit("li a1, 0");
        self.emit_stack_load("ra", RA_OFFSET);
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");
    }

    fn emit_runtime_require_witness_size_at_least_helper(&mut self, enabled: bool) {
        const SIZE_OFFSET: usize = 8;
        const MIN_SIZE_OFFSET: usize = 16;
        const RA_OFFSET: usize = 24;
        const FRAME_SIZE: usize = 32;

        self.emit_global("__ckb_require_witness_size_at_least");
        self.emit_label("__ckb_require_witness_size_at_least");
        self.emit("# cellscript abi: require witness size >= min_size");
        self.emit("# cellscript abi: args a0=SourceView, a1=min_size; returns a0=status");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("witness_req_size_source_invalid");
        let failed = self.fresh_label("witness_req_size_load_failed");
        let too_small = self.fresh_label("witness_req_size_too_small");
        let status_ok = self.fresh_label("witness_req_size_status_ok");
        let done = self.fresh_label("witness_req_size_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-(FRAME_SIZE as i64));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit("# cellscript abi: preserve min_size before LOAD_WITNESS size probe");
        self.emit(format!("sd a1, {}(sp)", MIN_SIZE_OFFSET));
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 0");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit("li a0, 0");
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", status_ok));
        self.emit(format!("beqz a0, {}", status_ok));
        self.emit(format!("j {}", failed));

        self.emit_label(&status_ok);
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("ld t1, {}(sp)", MIN_SIZE_OFFSET));
        self.emit("sltu t2, t0, t1");
        self.emit(format!("beqz t2, {}", done));

        self.emit_label(&too_small);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::WitnessMalformed.code()));
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");

        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");

        self.emit_label(&done);
        self.emit("li a0, 0");
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");
    }

    fn emit_runtime_witness_args_exact32_helper(&mut self, symbol: &str, detail: &str, field_index: u64, enabled: bool) {
        const OUTPTR: usize = 0;
        const PAYLOAD_OFFSET: usize = 8;
        const INDEX: usize = 16;
        const SOURCE: usize = 24;
        const READ_SIZE: usize = 32;
        const RA: usize = 40;
        const FRAME: usize = 48;

        let owner = match field_index {
            0 => CKB_WITNESS_OWNER_LOCK,
            1 => CKB_WITNESS_OWNER_ENTRY,
            2 => CKB_WITNESS_OWNER_OUTPUT_TYPE,
            _ => unreachable!("WitnessArgs has exactly three fields"),
        };
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: stream exact 32-byte WitnessArgs field {field_index} ({detail})"));
        self.emit("# cellscript abi: args a0=SourceView, a1=out32_ptr; returns a0=status");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let resolved = self.fresh_label("witness_exact32_resolved");
        let resolver_failed = self.fresh_label("witness_exact32_resolver_failed");
        let exact_size = self.fresh_label("witness_exact32_size_mismatch");
        let syscall_status_ok = self.fresh_label("witness_exact32_syscall_status_ok");
        let syscall_failed = self.fresh_label("witness_exact32_syscall_failed");
        let done = self.fresh_label("witness_exact32_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-(FRAME as i64));
        self.emit_stack_store("ra", RA);
        self.emit_stack_store("a1", OUTPTR);
        self.emit(format!("li a1, {owner}"));
        self.emit("li a2, 32");
        self.emit("call __cellscript_witness_bounded_resolve");
        self.emit(format!("beqz a4, {resolved}"));
        self.emit(format!("j {resolver_failed}"));

        self.emit_label(&resolved);
        self.emit("li t0, 32");
        self.emit(format!("bne a1, t0, {exact_size}"));
        self.emit_stack_store("a0", PAYLOAD_OFFSET);
        self.emit_stack_store("a2", INDEX);
        self.emit_stack_store("a3", SOURCE);
        self.emit_stack_store("t0", READ_SIZE);
        self.emit_stack_load("a0", OUTPTR);
        self.emit_sp_addi("a1", READ_SIZE);
        self.emit_stack_load("a2", PAYLOAD_OFFSET);
        self.emit_stack_load("a3", INDEX);
        self.emit_stack_load("a4", SOURCE);
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("beqz a0, {syscall_status_ok}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {syscall_status_ok}"));
        self.emit(format!("j {syscall_failed}"));

        self.emit_label(&resolver_failed);
        self.emit(format!("li t0, {}", CellScriptRuntimeError::WitnessFieldAbsent.code()));
        self.emit(format!("beq a4, t0, {exact_size}"));
        self.emit(format!("li t0, {}", CellScriptRuntimeError::WitnessBoundExceeded.code()));
        self.emit(format!("beq a4, t0, {exact_size}"));
        self.emit("mv a0, a4");
        self.emit(format!("j {done}"));

        self.emit_label(&exact_size);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ExactSizeMismatch.code()));
        self.emit(format!("j {done}"));
        self.emit_label(&syscall_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit(format!("j {done}"));
        self.emit_label(&syscall_status_ok);
        self.emit("li a0, 0");
        self.emit_label(&done);
        self.emit_stack_load("ra", RA);
        self.emit_stack_adjustment(FRAME as i64);
        self.emit("ret");
    }

    fn emit_runtime_current_script_hash_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(16) as i64;
        self.emit_global("__ckb_current_script_hash");
        self.emit_label("__ckb_current_script_hash");
        self.emit("# cellscript abi: current script Hash via LOAD_SCRIPT_HASH");
        self.emit("# cellscript abi: args a0=out32_ptr, a1=size_ptr; returns a0=status");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let failed = self.fresh_label("current_script_hash_load_failed");
        let malformed = self.fresh_label("current_script_hash_malformed");
        let done = self.fresh_label("current_script_hash_done");
        let abi = self.runtime_abi();
        // Two packed spill slots preserve 16-byte stack alignment.
        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 8);
        self.emit_stack_store("a1", 0);
        self.emit("li t0, 32");
        self.emit("sd t0, 0(a1)");
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", abi.load_script_hash));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit_stack_load("t6", 0);
        self.emit("ld t0, 0(t6)");
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", 8);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_transaction_hash_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(16) as i64;
        self.emit_global("__ckb_transaction_hash");
        self.emit_label("__ckb_transaction_hash");
        self.emit("# cellscript abi: canonical raw transaction Hash via LOAD_TX_HASH");
        self.emit("# cellscript abi: args a0=out32_ptr, a1=size_ptr; returns a0=status");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let failed = self.fresh_label("transaction_hash_load_failed");
        let malformed = self.fresh_label("transaction_hash_malformed");
        let done = self.fresh_label("transaction_hash_done");
        // Two packed spill slots match the checked owned-frame alignment.
        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 8);
        self.emit_stack_store("a1", 0);
        self.emit("li t0, 32");
        self.emit("sd t0, 0(a1)");
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_TX_HASH));
        self.emit("ecall");
        self.emit(format!("bnez a0, {failed}"));
        self.emit_stack_load("t6", 0);
        self.emit("ld t0, 0(t6)");
        self.emit("li t1, 32");
        self.emit(format!("bne t0, t1, {malformed}"));
        self.emit("li a0, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit(format!("j {done}"));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", 8);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_source_view_helper(&mut self, symbol: &str, source_view: u64, detail: &str, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView helper ({})", detail));
        if !enabled {
            self.emit_process_failure(CellScriptRuntimeError::SyscallFailed);
            return;
        }
        let invalid = self.fresh_label("source_view_index_invalid");
        // Bits at or above the shift would carry into the view tag and
        // silently re-route the request to another source family.
        self.emit("srli t1, a0, 32");
        self.emit(format!("bnez t1, {}", invalid));
        self.emit(format!("li t0, {}", source_view));
        self.emit("slli t0, t0, 32");
        self.emit("add a0, a0, t0");
        self.emit("ret");
        self.emit_label(&invalid);
        self.emit_process_failure(CellScriptRuntimeError::CkbSourceViewInvalid);
    }

    fn emit_runtime_ckb_since_epoch_helper(&mut self, symbol: &str, relative: bool, detail: &str, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {}", detail));
        self.emit("# cellscript abi: args a0=number(<2^24), a1=index(<2^16), a2=length(<2^16); requires length>0 and index<length");
        self.emit("# cellscript abi: encodes CKB RFC0017 EpochNumberWithFraction as number | index<<24 | length<<40");
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let malformed = self.fresh_label("ckb_since_epoch_malformed");
        let done = self.fresh_label("ckb_since_epoch_done");
        self.emit(format!("li t0, {}", CKB_EPOCH_NUMBER_BOUND));
        self.emit("sltu t1, a0, t0");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit(format!("li t0, {}", CKB_EPOCH_FRACTION_BOUND));
        self.emit("sltu t1, a1, t0");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit("sltu t1, a2, t0");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit(format!("beqz a2, {}", malformed));
        self.emit("sltu t1, a1, a2");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit("addi t2, a0, 0");
        self.emit("slli t0, a1, 24");
        self.emit("or t2, t2, t0");
        self.emit("slli t0, a2, 40");
        self.emit("or t2, t2, t0");
        self.emit(format!("li t0, {}", CKB_SINCE_EPOCH_NUMBER_WITH_FRACTION_FLAG));
        self.emit("or t2, t2, t0");
        if relative {
            self.emit("li t0, 1");
            self.emit("slli t0, t0, 63");
            self.emit("or t2, t2, t0");
        }
        self.emit("addi a0, t2, 0");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&malformed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSinceMalformed.code()));
        self.emit_label(&done);
        self.emit("ret");
    }

    fn emit_runtime_ckb_since_scalar_helper(
        &mut self,
        symbol: &str,
        relative: bool,
        metric_flag: u64,
        timestamp: bool,
        detail: &str,
        enabled: bool,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {detail}; arg a0 is the low-bit metric value"));
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let malformed = self.fresh_label("ckb_since_scalar_malformed");
        let done = self.fresh_label("ckb_since_scalar_done");
        let bound = if timestamp { CKB_SINCE_TIMESTAMP_BOUND } else { CKB_SINCE_VALUE_MASK + 1 };
        self.emit(format!("li t0, {bound}"));
        self.emit("sltu t1, a0, t0");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit(format!("li t0, {metric_flag}"));
        self.emit("or a0, a0, t0");
        if relative {
            self.emit(format!("li t0, {CKB_SINCE_RELATIVE_FLAG}"));
            self.emit("or a0, a0, t0");
        }
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSinceMalformed.code()));
        self.emit_label(&done);
        self.emit("ret");
    }

    fn emit_runtime_ckb_since_validation(&mut self, malformed: &str) {
        let non_epoch = self.fresh_label("ckb_since_decode_non_epoch");
        let epoch_nonzero_length = self.fresh_label("ckb_since_decode_epoch_nonzero_length");
        let valid = self.fresh_label("ckb_since_decode_valid");

        self.emit(format!("li t0, {CKB_SINCE_REMAIN_FLAGS_BITS}"));
        self.emit("and t1, a0, t0");
        self.emit(format!("bnez t1, {malformed}"));
        self.emit(format!("li t0, {CKB_SINCE_METRIC_TYPE_FLAG_MASK}"));
        self.emit("and t1, a0, t0");
        self.emit(format!("beq t1, t0, {malformed}"));
        self.emit(format!("li t0, {CKB_SINCE_EPOCH_NUMBER_WITH_FRACTION_FLAG}"));
        self.emit(format!("bne t1, t0, {}", non_epoch));

        self.emit(format!("li t0, {CKB_EPOCH_FRACTION_MASK}"));
        self.emit("srli t2, a0, 24");
        self.emit("and t2, t2, t0");
        self.emit("srli t3, a0, 40");
        self.emit("and t3, t3, t0");
        self.emit(format!("bnez t3, {}", epoch_nonzero_length));
        self.emit(format!("beqz t2, {}", valid));
        self.emit(format!("j {malformed}"));
        self.emit_label(&epoch_nonzero_length);
        self.emit("sltu t4, t2, t3");
        self.emit(format!("bnez t4, {}", valid));
        self.emit(format!("j {malformed}"));

        self.emit_label(&non_epoch);
        self.emit(format!("li t0, {CKB_SINCE_TIMESTAMP_FLAG}"));
        self.emit(format!("bne t1, t0, {}", valid));
        self.emit(format!("li t0, {CKB_SINCE_VALUE_MASK}"));
        self.emit("and t2, a0, t0");
        self.emit(format!("li t0, {CKB_SINCE_TIMESTAMP_BOUND}"));
        self.emit("sltu t3, t2, t0");
        self.emit(format!("beqz t3, {malformed}"));
        self.emit_label(&valid);
    }

    fn emit_runtime_ckb_since_decode_helper(&mut self, symbol: &str, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit("# cellscript abi: validates RFC0017 reserved flags, metric, epoch fraction, and timestamp multiplication bound");
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let malformed = self.fresh_label("ckb_since_decode_malformed");
        let done = self.fresh_label("ckb_since_decode_done");
        self.emit_runtime_ckb_since_validation(&malformed);
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSinceMalformed.code()));
        self.emit_label(&done);
        self.emit("ret");
    }

    fn emit_runtime_ckb_since_narrow_helper(&mut self, symbol: &str, relative: bool, metric_flag: u64, detail: &str, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: narrows a validated DecodedSince to {detail}"));
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let malformed = self.fresh_label("ckb_since_narrow_malformed");
        let done = self.fresh_label("ckb_since_narrow_done");
        self.emit_runtime_ckb_since_validation(&malformed);
        let expected = metric_flag | if relative { CKB_SINCE_RELATIVE_FLAG } else { 0 };
        self.emit(format!("li t0, {}", CKB_SINCE_RELATIVE_FLAG | CKB_SINCE_METRIC_TYPE_FLAG_MASK));
        self.emit("and t1, a0, t0");
        self.emit(format!("li t0, {expected}"));
        self.emit(format!("bne t1, t0, {}", malformed));
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSinceMalformed.code()));
        self.emit_label(&done);
        self.emit("ret");
    }

    fn emit_runtime_ckb_since_projection_helper(&mut self, symbol: &str, operation: &str, detail: &str, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {detail}"));
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let malformed = self.fresh_label("ckb_since_projection_malformed");
        let done = self.fresh_label("ckb_since_projection_done");
        self.emit_runtime_ckb_since_validation(&malformed);
        match operation {
            "relative" => self.emit("srli a0, a0, 63"),
            "disabled" => self.emit("seqz a0, a0"),
            "metric" => {
                self.emit("srli a0, a0, 61");
                self.emit("andi a0, a0, 3");
            }
            "value" => {
                self.emit(format!("li t0, {CKB_SINCE_VALUE_MASK}"));
                self.emit("and a0, a0, t0");
            }
            _ => unreachable!("known CKB Since projection"),
        }
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSinceMalformed.code()));
        self.emit_label(&done);
        self.emit("ret");
    }

    fn emit_runtime_ckb_epoch_arithmetic_helper(&mut self, symbol: &str, operation: &str, detail: &str, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {detail}; values must remain below 2^24"));
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("ckb_epoch_arithmetic_invalid");
        let done = self.fresh_label("ckb_epoch_arithmetic_done");
        self.emit(format!("li t0, {CKB_EPOCH_NUMBER_BOUND}"));
        self.emit("sltu t1, a0, t0");
        self.emit(format!("beqz t1, {invalid}"));

        match operation {
            "duration" => {}
            "add" | "sub" => {
                self.emit("sltu t1, a1, t0");
                self.emit(format!("beqz t1, {invalid}"));
                if operation == "add" {
                    self.emit("add t2, a0, a1");
                    self.emit("sltu t1, t2, a0");
                    self.emit(format!("bnez t1, {invalid}"));
                    self.emit("sltu t1, t2, t0");
                    self.emit(format!("beqz t1, {invalid}"));
                    self.emit("addi a0, t2, 0");
                } else {
                    self.emit("sltu t1, a0, a1");
                    self.emit(format!("bnez t1, {invalid}"));
                    self.emit("sub a0, a0, a1");
                }
            }
            _ => unreachable!("known CKB epoch arithmetic operation"),
        }

        self.emit("li a1, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&invalid);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::NumericOrDiscriminantInvalid.code()));
        self.emit_label(&done);
        self.emit("ret");
    }

    fn emit_runtime_ckb_temporal_to_raw(&mut self, symbol: &str, detail: &str) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {detail}; preserves the exact 64-bit representation"));
        self.emit("li a1, 0");
        self.emit("ret");
    }

    pub(super) fn emit_decode_source_view_to_t1_t2(&mut self, invalid_label: &str) {
        let done = self.fresh_label("source_view_decoded");
        debug_assert_eq!(CKB_SOURCE_VIEW_SHIFT, 1u64 << 32);
        // SourceView is a pair of u32 values, not an arithmetic quotient.
        // Shifts avoid a wide immediate plus the VM's expensive DIV/REM path.
        self.emit("srli t0, a0, 32");
        self.emit("slli t1, a0, 32");
        self.emit("srli t1, t1, 32");
        debug_assert_eq!(
            [CKB_SOURCE_VIEW_INPUT, CKB_SOURCE_VIEW_OUTPUT, CKB_SOURCE_VIEW_CELL_DEP, CKB_SOURCE_VIEW_HEADER_DEP],
            [CKB_SOURCE_INPUT, CKB_SOURCE_OUTPUT, CKB_SOURCE_CELL_DEP, CKB_SOURCE_HEADER_DEP]
        );
        debug_assert_eq!(CKB_SOURCE_VIEW_GROUP_INPUT, 5);
        debug_assert_eq!(CKB_SOURCE_VIEW_GROUP_OUTPUT, 6);
        let direct = self.fresh_label("source_view_direct");
        self.emit(format!("beqz t0, {}", invalid_label));
        self.emit("li t5, 4");
        self.emit(format!("bgeu t5, t0, {}", direct));
        self.emit("li t5, 6");
        self.emit(format!("bltu t5, t0, {}", invalid_label));
        self.emit("addi t2, t0, -4");
        self.emit(format!("li t5, {}", CKB_SOURCE_GROUP_FLAG));
        self.emit("or t2, t2, t5");
        self.emit(format!("j {}", done));
        self.emit_label(&direct);
        self.emit("mv t2, t0");
        self.emit_label(&done);
    }

    fn emit_decode_input_source_view_to_t1_t2(&mut self, invalid_label: &str) {
        let done = self.fresh_label("input_source_view_decoded");
        self.emit_decode_source_view_to_t1_t2(invalid_label);
        self.emit(format!("li t0, {}", CKB_SOURCE_INPUT));
        self.emit(format!("beq t2, t0, {}", done));
        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit(format!("beq t2, t0, {}", done));
        self.emit(format!("j {}", invalid_label));
        self.emit_label(&done);
    }

    fn emit_runtime_cell_field_u64_helper(&mut self, symbol: &str, detail: &str, field_id: u64, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(32) as i64;
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView LOAD_CELL_BY_FIELD ({})", detail));
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("source_view_invalid");
        let done = self.fresh_label("cell_field_done");
        let failed = self.fresh_label("cell_field_failed");
        let abi = self.runtime_abi();
        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 24);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 8");
        self.emit_stack_store("t0", 8);
        self.emit_sp_addi("a0", 16);
        self.emit_sp_addi("a1", 8);
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit_stack_load("a0", 16);
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit_label(&done);
        self.emit_stack_load("ra", 24);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_cell_field_low_word_helper(&mut self, symbol: &str, detail: &str, field_id: u64, enabled: bool) {
        self.emit_runtime_cell_field_u64_helper(symbol, detail, field_id, enabled);
    }

    fn emit_runtime_cell_hash_field_helper(&mut self, symbol: &str, detail: &str, field_id: u64, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView LOAD_CELL_BY_FIELD full hash ({})", detail));
        self.emit("# cellscript abi: args a0=SourceView, a1=out32_ptr, a2=size_ptr; returns a0=status");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SIZE_OFFSET: usize = 0;
        const BUFFER_OFFSET: usize = 8;
        const OUT_PTR_OFFSET: usize = 40;
        const SIZE_PTR_OFFSET: usize = 48;
        const RA_OFFSET: usize = 56;
        const FRAME_SIZE: usize = align_frame(RA_OFFSET + 8);

        let invalid = self.fresh_label("cell_hash_source_invalid");
        let bad_output = self.fresh_label("cell_hash_output_invalid");
        let failed = self.fresh_label("cell_hash_load_failed");
        let copy_loop = self.fresh_label("cell_hash_copy");
        let copy_done = self.fresh_label("cell_hash_copy_done");
        let done = self.fresh_label("cell_hash_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-(FRAME_SIZE as i64));
        self.emit_stack_store("ra", RA_OFFSET);
        self.emit_stack_store("a1", OUT_PTR_OFFSET);
        self.emit_stack_store("a2", SIZE_PTR_OFFSET);
        self.emit(format!("beqz a1, {}", bad_output));
        self.emit(format!("beqz a2, {}", bad_output));

        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 32");
        self.emit_stack_store("t0", SIZE_OFFSET);
        self.emit_sp_addi("a0", BUFFER_OFFSET);
        self.emit_sp_addi("a1", SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit_stack_load("t0", SIZE_OFFSET);
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));

        self.emit("li t1, 0");
        self.emit_label(&copy_loop);
        self.emit("li t2, 32");
        self.emit("sltu t3, t1, t2");
        self.emit(format!("beqz t3, {}", copy_done));
        self.emit_sp_addi("t6", BUFFER_OFFSET);
        self.emit("add t6, t6, t1");
        self.emit("lbu t5, 0(t6)");
        self.emit_stack_load("t6", OUT_PTR_OFFSET);
        self.emit("add t6, t6, t1");
        self.emit("sb t5, 0(t6)");
        self.emit("addi t1, t1, 1");
        self.emit(format!("j {}", copy_loop));

        self.emit_label(&copy_done);
        self.emit_stack_load("t6", SIZE_PTR_OFFSET);
        self.emit("li t0, 32");
        self.emit("sd t0, 0(t6)");
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_output);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", RA_OFFSET);
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");
    }

    fn emit_runtime_input_out_point_word_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        out_point_offset: usize,
        width: usize,
        enabled: bool,
    ) {
        const STACK_FRAME_BYTES: i64 = align_frame(64) as i64;
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView LOAD_INPUT_BY_FIELD OutPoint ({})", detail));
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("input_out_point_source_invalid");
        let failed = self.fresh_label("input_out_point_load_failed");
        let done = self.fresh_label("input_out_point_done");
        let abi = self.runtime_abi();
        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 56);
        self.emit_decode_input_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 36");
        self.emit_stack_store("t0", 8);
        self.emit_sp_addi("a0", 16);
        self.emit_sp_addi("a1", 8);
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_OUT_POINT));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit_stack_load("t0", 8);
        self.emit("li t1, 36");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit_sp_addi("t4", 16);
        self.emit_unaligned_scalar_load("t4", "a0", "t3", out_point_offset, width);
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit_label(&done);
        self.emit_stack_load("ra", 56);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_input_out_point_tx_hash_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_input_out_point_tx_hash");
        self.emit_label("__ckb_input_out_point_tx_hash");
        self.emit("# cellscript abi: CKB SourceView LOAD_INPUT_BY_FIELD OutPoint full tx-hash read");
        self.emit("# cellscript abi: args a0=input SourceView, a1=out32_ptr, a2=size_ptr; returns a0=status");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const OUT_PTR_OFFSET: usize = 8;
        const SIZE_PTR_OFFSET: usize = 16;
        const OUT_POINT_SIZE_OFFSET: usize = 24;
        const OUT_POINT_OFFSET: usize = 32;
        const RA_OFFSET: usize = 72;
        const FRAME_SIZE: usize = 80;

        let invalid = self.fresh_label("input_out_point_hash_source_invalid");
        let bad_output = self.fresh_label("input_out_point_hash_output_invalid");
        let failed = self.fresh_label("input_out_point_hash_load_failed");
        let copy_loop = self.fresh_label("input_out_point_hash_copy");
        let copy_done = self.fresh_label("input_out_point_hash_copy_done");
        let done = self.fresh_label("input_out_point_hash_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-(FRAME_SIZE as i64));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit(format!("sd a1, {}(sp)", OUT_PTR_OFFSET));
        self.emit(format!("sd a2, {}(sp)", SIZE_PTR_OFFSET));
        self.emit(format!("beqz a1, {}", bad_output));
        self.emit(format!("beqz a2, {}", bad_output));

        self.emit_decode_input_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 36");
        self.emit(format!("sd t0, {}(sp)", OUT_POINT_SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", OUT_POINT_OFFSET));
        self.emit(format!("addi a1, sp, {}", OUT_POINT_SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_OUT_POINT));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit(format!("ld t0, {}(sp)", OUT_POINT_SIZE_OFFSET));
        self.emit("li t1, 36");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));

        self.emit("li t0, 0");
        self.emit_label(&copy_loop);
        self.emit("li t1, 32");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("beqz t2, {}", copy_done));
        self.emit(format!("addi t3, sp, {}", OUT_POINT_OFFSET));
        self.emit("add t3, t3, t0");
        self.emit("lbu t4, 0(t3)");
        self.emit(format!("ld t5, {}(sp)", OUT_PTR_OFFSET));
        self.emit("add t5, t5, t0");
        self.emit("sb t4, 0(t5)");
        self.emit("addi t0, t0, 1");
        self.emit(format!("j {}", copy_loop));
        self.emit_label(&copy_done);
        self.emit(format!("ld t0, {}(sp)", SIZE_PTR_OFFSET));
        self.emit("li t1, 32");
        self.emit("sd t1, 0(t0)");
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_output);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::OutPointMismatch.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");
    }

    fn emit_runtime_input_out_point_tx_hash_requirement_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(64) as i64;
        self.emit_global("__ckb_require_input_out_point_tx_hash");
        self.emit_label("__ckb_require_input_out_point_tx_hash");
        self.emit("# cellscript abi: CKB SourceView LOAD_INPUT_BY_FIELD OutPoint full tx-hash requirement");
        self.emit("# cellscript abi: args a0=input SourceView, a1=expected_tx_hash_ptr, a2=expected_tx_hash_len");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("input_out_point_source_invalid");
        let bad_expected = self.fresh_label("input_out_point_expected_invalid");
        let failed = self.fresh_label("input_out_point_load_failed");
        let mismatch = self.fresh_label("input_out_point_tx_hash_mismatch");
        let done = self.fresh_label("input_out_point_tx_hash_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 56);
        self.emit_stack_store("a1", 48);

        self.emit(format!("beqz a1, {}", bad_expected));
        self.emit("li t0, 32");
        self.emit("sub t1, a2, t0");
        self.emit(format!("bnez t1, {}", bad_expected));

        self.emit_decode_input_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 36");
        self.emit_stack_store("t0", 0);
        self.emit_sp_addi("a0", 8);
        self.emit_sp_addi("a1", 0);
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_OUT_POINT));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit_stack_load("t0", 0);
        self.emit("li t1, 36");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit_sp_addi("a0", 8);
        self.emit_stack_load("a1", 48);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::OutPointMismatch.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", 56);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_input_out_point_requirement_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(80) as i64;
        self.emit_global("__ckb_require_input_out_point");
        self.emit_label("__ckb_require_input_out_point");
        self.emit("# cellscript abi: CKB SourceView LOAD_INPUT_BY_FIELD OutPoint full tx-hash + index requirement");
        self.emit("# cellscript abi: args a0=input SourceView, a1=expected_tx_hash_ptr, a2=expected_tx_hash_len, a3=expected_index");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("input_out_point_source_invalid");
        let bad_expected = self.fresh_label("input_out_point_expected_invalid");
        let failed = self.fresh_label("input_out_point_load_failed");
        let mismatch = self.fresh_label("input_out_point_mismatch");
        let done = self.fresh_label("input_out_point_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 64);
        self.emit_stack_store("a1", 48);
        self.emit_stack_store("a3", 56);

        self.emit(format!("beqz a1, {}", bad_expected));
        self.emit("li t0, 32");
        self.emit("sub t1, a2, t0");
        self.emit(format!("bnez t1, {}", bad_expected));

        self.emit_decode_input_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 36");
        self.emit_stack_store("t0", 0);
        self.emit_sp_addi("a0", 8);
        self.emit_sp_addi("a1", 0);
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_OUT_POINT));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit_stack_load("t0", 0);
        self.emit("li t1, 36");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));

        self.emit_sp_addi("a0", 8);
        self.emit_stack_load("a1", 48);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));

        self.emit_sp_addi("t0", 8);
        self.emit_unaligned_scalar_load("t0", "t1", "t2", 32, 4);
        self.emit_stack_load("t3", 56);
        self.emit("sub t4, t1, t3");
        self.emit(format!("bnez t4, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::OutPointMismatch.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", 64);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_metapoint_relative_requirement_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(160) as i64;
        self.emit_global("__ckb_require_metapoint_relative");
        self.emit_label("__ckb_require_metapoint_relative");
        self.emit("# cellscript abi: CKB SourceView MetaPoint relative-distance requirement");
        self.emit("# cellscript abi: args a0=base SourceView, a1=related SourceView, a2=signed i32 distance");
        self.emit("# cellscript abi: input metapoint = input OutPoint(tx_hash,index); output metapoint = output index");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const RELATED_VIEW_OFFSET: usize = 0;
        const DISTANCE_OFFSET: usize = 8;
        const BASE_SOURCE_OFFSET: usize = 16;
        const BASE_INDEX_OFFSET: usize = 24;
        const RELATED_SOURCE_OFFSET: usize = 32;
        const RELATED_INDEX_OFFSET: usize = 40;
        const BASE_SIZE_OFFSET: usize = 48;
        const RELATED_SIZE_OFFSET: usize = 56;
        const BASE_OUT_POINT_OFFSET: usize = 72;
        const RELATED_OUT_POINT_OFFSET: usize = 112;

        let invalid = self.fresh_label("metapoint_source_invalid");
        let input_pair = self.fresh_label("metapoint_input_pair");
        let output_pair = self.fresh_label("metapoint_output_pair");
        let load_failed = self.fresh_label("metapoint_load_failed");
        let mismatch = self.fresh_label("metapoint_mismatch");
        let done = self.fresh_label("metapoint_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 64);
        self.emit_stack_store("a1", RELATED_VIEW_OFFSET);
        self.emit_sign_extend_i32("a2");
        self.emit_stack_store("a2", DISTANCE_OFFSET);

        self.emit("# cellscript abi: decode base MetaPoint SourceView");
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit_stack_store("t2", BASE_SOURCE_OFFSET);
        self.emit_stack_store("t1", BASE_INDEX_OFFSET);

        self.emit("# cellscript abi: decode related MetaPoint SourceView");
        self.emit_stack_load("a0", RELATED_VIEW_OFFSET);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit_stack_store("t2", RELATED_SOURCE_OFFSET);
        self.emit_stack_store("t1", RELATED_INDEX_OFFSET);

        self.emit("# cellscript abi: MetaPoint relation requires both views from the same source class");
        self.emit_stack_load("t0", BASE_SOURCE_OFFSET);
        self.emit_stack_load("t1", RELATED_SOURCE_OFFSET);
        self.emit("sub t3, t0, t1");
        self.emit(format!("bnez t3, {}", mismatch));

        self.emit(format!("li t4, {}", CKB_SOURCE_INPUT));
        self.emit("sub t3, t0, t4");
        self.emit(format!("beqz t3, {}", input_pair));
        self.emit(format!("li t4, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit("sub t3, t0, t4");
        self.emit(format!("beqz t3, {}", input_pair));
        self.emit(format!("li t4, {}", CKB_SOURCE_OUTPUT));
        self.emit("sub t3, t0, t4");
        self.emit(format!("beqz t3, {}", output_pair));
        self.emit(format!("li t4, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT));
        self.emit("sub t3, t0, t4");
        self.emit(format!("beqz t3, {}", output_pair));
        self.emit(format!("j {}", invalid));

        self.emit_label(&output_pair);
        self.emit("# cellscript abi: output MetaPoint compare base_output_index + distance == related_output_index");
        self.emit_stack_load("t0", BASE_INDEX_OFFSET);
        self.emit_stack_load("t1", DISTANCE_OFFSET);
        self.emit("add t0, t0, t1");
        self.emit("slt t4, t0, zero");
        self.emit(format!("bnez t4, {}", mismatch));
        self.emit_stack_load("t2", RELATED_INDEX_OFFSET);
        self.emit("sub t3, t0, t2");
        self.emit(format!("bnez t3, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&input_pair);
        self.emit("# cellscript abi: input MetaPoint compare OutPoint tx_hash and base_out_index + distance");
        self.emit("li t0, 36");
        self.emit_stack_store("t0", BASE_SIZE_OFFSET);
        self.emit_sp_addi("a0", BASE_OUT_POINT_OFFSET);
        self.emit_sp_addi("a1", BASE_SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit_stack_load("a3", BASE_INDEX_OFFSET);
        self.emit_stack_load("a4", BASE_SOURCE_OFFSET);
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_OUT_POINT));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", load_failed));
        self.emit_stack_load("t0", BASE_SIZE_OFFSET);
        self.emit("li t1, 36");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", load_failed));

        self.emit("li t0, 36");
        self.emit_stack_store("t0", RELATED_SIZE_OFFSET);
        self.emit_sp_addi("a0", RELATED_OUT_POINT_OFFSET);
        self.emit_sp_addi("a1", RELATED_SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit_stack_load("a3", RELATED_INDEX_OFFSET);
        self.emit_stack_load("a4", RELATED_SOURCE_OFFSET);
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_OUT_POINT));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", load_failed));
        self.emit_stack_load("t0", RELATED_SIZE_OFFSET);
        self.emit("li t1, 36");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", load_failed));

        self.emit_sp_addi("a0", BASE_OUT_POINT_OFFSET);
        self.emit_sp_addi("a1", RELATED_OUT_POINT_OFFSET);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));
        self.emit_sp_addi("t0", BASE_OUT_POINT_OFFSET);
        self.emit_unaligned_scalar_load("t0", "t1", "t2", 32, 4);
        self.emit_stack_load("t3", DISTANCE_OFFSET);
        self.emit("add t1, t1, t3");
        self.emit("slt t4, t1, zero");
        self.emit(format!("bnez t4, {}", mismatch));
        self.emit_sp_addi("t0", RELATED_OUT_POINT_OFFSET);
        self.emit_unaligned_scalar_load("t0", "t2", "t3", 32, 4);
        self.emit("sub t4, t1, t2");
        self.emit(format!("bnez t4, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&load_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::OutPointMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::MetaPointMismatch.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", 64);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_current_script_role_at_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(96) as i64;
        self.emit_global("__cellscript_current_script_role_at");
        self.emit_label("__cellscript_current_script_role_at");
        self.emit("# cellscript abi: classify one cell against current script hash");
        self.emit("# cellscript abi: args a0=source, a1=index, a2=current_script_hash_ptr; returns a0=role(0 none,1 lock-only,2 type-only,3 both), a1=status");
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SOURCE_OFFSET: usize = 8;
        const INDEX_OFFSET: usize = 16;
        const SCRIPT_HASH_PTR_OFFSET: usize = 24;
        const SIZE_OFFSET: usize = 32;
        const HASH_BUFFER_OFFSET: usize = 40;
        const LOCK_MATCH_OFFSET: usize = 72;
        const TYPE_MATCH_OFFSET: usize = 80;

        let bad_args = self.fresh_label("current_script_role_bad_args");
        let lock_loaded = self.fresh_label("current_script_role_lock_loaded");
        let lock_not_match = self.fresh_label("current_script_role_lock_not_match");
        let type_loaded = self.fresh_label("current_script_role_type_loaded");
        let type_missing = self.fresh_label("current_script_role_type_missing");
        let type_not_match = self.fresh_label("current_script_role_type_not_match");
        let build_role = self.fresh_label("current_script_role_build");
        let out_of_bound = self.fresh_label("current_script_role_oob");
        let failed = self.fresh_label("current_script_role_failed");
        let done = self.fresh_label("current_script_role_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit("sd ra, 88(sp)");
        self.emit(format!("sd a0, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("sd a1, {}(sp)", INDEX_OFFSET));
        self.emit(format!("sd a2, {}(sp)", SCRIPT_HASH_PTR_OFFSET));
        self.emit(format!("sd zero, {}(sp)", LOCK_MATCH_OFFSET));
        self.emit(format!("sd zero, {}(sp)", TYPE_MATCH_OFFSET));
        self.emit(format!("beqz a2, {}", bad_args));

        self.emit("li t0, 32");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", HASH_BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", INDEX_OFFSET));
        self.emit(format!("ld a4, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_LOCK_HASH));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", lock_loaded));
        self.emit(format!("li t0, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", out_of_bound));
        self.emit(format!("j {}", failed));

        self.emit_label(&lock_loaded);
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit(format!("addi a0, sp, {}", HASH_BUFFER_OFFSET));
        self.emit(format!("ld a1, {}(sp)", SCRIPT_HASH_PTR_OFFSET));
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", lock_not_match));
        self.emit("li t0, 1");
        self.emit(format!("sd t0, {}(sp)", LOCK_MATCH_OFFSET));
        self.emit_label(&lock_not_match);

        self.emit("li t0, 32");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", HASH_BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", INDEX_OFFSET));
        self.emit(format!("ld a4, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_TYPE_HASH));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", type_loaded));
        self.emit(format!("li t0, {}", CKB_ITEM_MISSING));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", type_missing));
        self.emit(format!("li t0, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", out_of_bound));
        self.emit(format!("j {}", failed));

        self.emit_label(&type_loaded);
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit(format!("addi a0, sp, {}", HASH_BUFFER_OFFSET));
        self.emit(format!("ld a1, {}(sp)", SCRIPT_HASH_PTR_OFFSET));
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", type_not_match));
        self.emit("li t0, 1");
        self.emit(format!("sd t0, {}(sp)", TYPE_MATCH_OFFSET));
        self.emit_label(&type_not_match);
        self.emit(format!("j {}", build_role));

        self.emit_label(&type_missing);
        self.emit(format!("j {}", build_role));

        self.emit_label(&build_role);
        self.emit(format!("ld t0, {}(sp)", LOCK_MATCH_OFFSET));
        self.emit(format!("ld t1, {}(sp)", TYPE_MATCH_OFFSET));
        self.emit("slli t1, t1, 1");
        self.emit("add a0, t0, t1");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&out_of_bound);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_args);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit_label(&done);
        self.emit("ld ra, 88(sp)");
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_metapoint_pair_cardinality_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        lock_to_type: bool,
        distance_from_base_data: bool,
        related_filter: bool,
        enabled: bool,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {}", detail));
        self.emit("# cellscript abi: scans current-script lock-only/type-only cells and requires one-to-one MetaPoint pairing");
        if related_filter {
            self.emit("# cellscript abi: related role cells must match expected TypeHash and generic data rule");
            self.emit("# cellscript abi: filtered data rules: 0=no data check, 1=exact 8-byte zero u64, 2=exact 8-byte nonzero u64");
        }
        if distance_from_base_data {
            self.emit("# cellscript abi: args a0=SourceView selecting Input/Output source class, a1=base-cell data offset containing signed i32 distance");
        } else {
            self.emit("# cellscript abi: args a0=SourceView selecting Input/Output source class, a1=signed i32 distance");
        }
        if related_filter {
            self.emit("# cellscript abi: filtered args a2=expected_related_type_hash_ptr, a3=hash_len, a4=related_data_rule");
        }
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SOURCE_OFFSET: usize = 0;
        const VIEW_KIND_OFFSET: usize = 8;
        const DISTANCE_OFFSET: usize = 16;
        const BASE_INDEX_OFFSET: usize = 24;
        const RELATED_INDEX_OFFSET: usize = 32;
        const BASE_COUNT_OFFSET: usize = 40;
        const RELATED_COUNT_OFFSET: usize = 48;
        const MATCH_COUNT_OFFSET: usize = 56;
        const SIZE_OFFSET: usize = 64;
        const DATA_OFFSET_OFFSET: usize = 72;
        const FILTER_RULE_OFFSET: usize = 80;
        const EXPECTED_HASH_PTR_OFFSET: usize = 88;
        const SCRIPT_HASH_OFFSET: usize = 96;
        const TYPE_HASH_BUFFER_OFFSET: usize = 128;
        const DATA_BUFFER_OFFSET: usize = 160;
        const RA_OFFSET: usize = 168;
        const STACK_SIZE: usize = align_frame(RA_OFFSET + 8);

        let invalid = self.fresh_label("metapoint_pair_source_invalid");
        let source_input = self.fresh_label("metapoint_pair_source_input");
        let source_group_input = self.fresh_label("metapoint_pair_source_group_input");
        let source_output = self.fresh_label("metapoint_pair_source_output");
        let source_group_output = self.fresh_label("metapoint_pair_source_group_output");
        let source_ready = self.fresh_label("metapoint_pair_source_ready");
        let hash_failed = self.fresh_label("metapoint_pair_hash_failed");
        let outer_loop = self.fresh_label("metapoint_pair_outer_loop");
        let outer_done = self.fresh_label("metapoint_pair_outer_done");
        let outer_role_ok = self.fresh_label("metapoint_pair_outer_role_ok");
        let maybe_related = self.fresh_label("metapoint_pair_maybe_related");
        let inner_loop = self.fresh_label("metapoint_pair_inner_loop");
        let inner_done = self.fresh_label("metapoint_pair_inner_done");
        let inner_role_candidate = self.fresh_label("metapoint_pair_inner_role_candidate");
        let relation_matched = self.fresh_label("metapoint_pair_relation_matched");
        let advance_related = self.fresh_label("metapoint_pair_advance_related");
        let increment_outer = self.fresh_label("metapoint_pair_increment_outer");
        let status_failed = self.fresh_label("metapoint_pair_status_failed");
        let relation_failed = self.fresh_label("metapoint_pair_relation_failed");
        let role_mismatch = self.fresh_label("metapoint_pair_role_mismatch");
        let bad_expected = self.fresh_label("metapoint_pair_filter_expected_invalid");
        let bad_data_rule = self.fresh_label("metapoint_pair_filter_data_rule_invalid");
        let related_type_mismatch = self.fresh_label("metapoint_pair_related_type_mismatch");
        let related_data_mismatch = self.fresh_label("metapoint_pair_related_data_mismatch");
        let data_loaded = self.fresh_label("metapoint_pair_data_loaded");
        let data_len_enough = self.fresh_label("metapoint_pair_data_len_enough");
        let data_malformed = self.fresh_label("metapoint_pair_data_malformed");
        let distance_ready = self.fresh_label("metapoint_pair_distance_ready");
        let cardinality = self.fresh_label("metapoint_pair_cardinality");
        let done = self.fresh_label("metapoint_pair_done");
        let abi = self.runtime_abi();
        let base_role = if lock_to_type { 1 } else { 2 };
        let related_role = if lock_to_type { 2 } else { 1 };

        self.emit_stack_adjustment(-(STACK_SIZE as i64));
        self.emit_stack_store("ra", RA_OFFSET);
        if distance_from_base_data {
            self.emit_stack_store("a1", DATA_OFFSET_OFFSET);
            self.emit_stack_store("zero", DISTANCE_OFFSET);
        } else {
            self.emit_sign_extend_i32("a1");
            self.emit_stack_store("a1", DISTANCE_OFFSET);
        }
        self.emit_stack_store("zero", BASE_INDEX_OFFSET);
        self.emit_stack_store("zero", BASE_COUNT_OFFSET);
        self.emit_stack_store("zero", RELATED_COUNT_OFFSET);
        if related_filter {
            self.emit_stack_store("a2", EXPECTED_HASH_PTR_OFFSET);
            self.emit_stack_store("a4", FILTER_RULE_OFFSET);
            self.emit(format!("beqz a2, {}", bad_expected));
            self.emit("li t0, 32");
            self.emit("sub t1, a3, t0");
            self.emit(format!("bnez t1, {}", bad_expected));
            self.emit("li t0, 2");
            self.emit("sltu t1, t0, a4");
            self.emit(format!("bnez t1, {}", bad_data_rule));
        }

        self.emit("# cellscript abi: decode SourceView source class; index component is ignored for group scan");
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", CKB_SOURCE_INPUT));
        self.emit("sub t3, t2, t0");
        self.emit(format!("beqz t3, {}", source_input));
        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit("sub t3, t2, t0");
        self.emit(format!("beqz t3, {}", source_group_input));
        self.emit(format!("li t0, {}", CKB_SOURCE_OUTPUT));
        self.emit("sub t3, t2, t0");
        self.emit(format!("beqz t3, {}", source_output));
        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT));
        self.emit("sub t3, t2, t0");
        self.emit(format!("beqz t3, {}", source_group_output));
        self.emit(format!("j {}", invalid));

        for (label, source, view) in [
            (&source_input, CKB_SOURCE_INPUT, CKB_SOURCE_VIEW_INPUT),
            (&source_group_input, CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT, CKB_SOURCE_VIEW_GROUP_INPUT),
            (&source_output, CKB_SOURCE_OUTPUT, CKB_SOURCE_VIEW_OUTPUT),
            (&source_group_output, CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT, CKB_SOURCE_VIEW_GROUP_OUTPUT),
        ] {
            self.emit_label(label.as_str());
            self.emit(format!("li t0, {}", source));
            self.emit_stack_store("t0", SOURCE_OFFSET);
            self.emit(format!("li t0, {}", view));
            self.emit_stack_store("t0", VIEW_KIND_OFFSET);
            self.emit(format!("j {}", source_ready));
        }

        self.emit_label(&source_ready);
        self.emit("li t0, 32");
        self.emit_stack_store("t0", SIZE_OFFSET);
        self.emit_sp_addi("a0", SCRIPT_HASH_OFFSET);
        self.emit_sp_addi("a1", SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", abi.load_script_hash));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", hash_failed));
        self.emit_stack_load("t0", SIZE_OFFSET);
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", hash_failed));

        self.emit_label(&outer_loop);
        self.emit_stack_load("a0", SOURCE_OFFSET);
        self.emit_stack_load("a1", BASE_INDEX_OFFSET);
        self.emit_sp_addi("a2", SCRIPT_HASH_OFFSET);
        self.emit("call __cellscript_current_script_role_at");
        self.emit("addi t0, a0, 0");
        self.emit("addi t1, a1, 0");
        self.emit(format!("li t2, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t3, t1, t2");
        self.emit(format!("beqz t3, {}", outer_done));
        self.emit(format!("bnez t1, {}", status_failed));
        self.emit("li t2, 3");
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", role_mismatch));
        self.emit(format!("li t2, {}", base_role));
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", outer_role_ok));
        self.emit(format!("j {}", maybe_related));

        self.emit_label(&outer_role_ok);
        self.emit_stack_load("t0", BASE_COUNT_OFFSET);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", BASE_COUNT_OFFSET);
        self.emit_stack_store("zero", MATCH_COUNT_OFFSET);
        self.emit_stack_store("zero", RELATED_INDEX_OFFSET);
        if distance_from_base_data {
            self.emit("# cellscript abi: load signed i32 MetaPoint distance from the base cell data");
            self.emit("li t0, 4");
            self.emit_stack_store("t0", SIZE_OFFSET);
            self.emit_sp_addi("a0", DATA_BUFFER_OFFSET);
            self.emit_sp_addi("a1", SIZE_OFFSET);
            self.emit_stack_load("a2", DATA_OFFSET_OFFSET);
            self.emit_stack_load("a3", BASE_INDEX_OFFSET);
            self.emit_stack_load("a4", SOURCE_OFFSET);
            self.emit(format!("li a7, {}", abi.load_cell_data));
            self.emit("ecall");
            self.emit(format!("beqz a0, {}", data_loaded));
            self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
            self.emit("sub t1, a0, t0");
            self.emit(format!("beqz t1, {}", data_len_enough));
            self.emit(format!("j {}", data_malformed));
            self.emit_label(&data_loaded);
            self.emit_stack_load("t0", SIZE_OFFSET);
            self.emit("li t1, 4");
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", data_malformed));
            self.emit(format!("j {}", distance_ready));
            self.emit_label(&data_len_enough);
            self.emit_stack_load("t0", SIZE_OFFSET);
            self.emit("li t1, 4");
            self.emit("sltu t2, t0, t1");
            self.emit(format!("bnez t2, {}", data_malformed));
            self.emit_label(&distance_ready);
            self.emit_stack_u32_le_to("t0", DATA_BUFFER_OFFSET);
            self.emit_sign_extend_i32("t0");
            self.emit_stack_store("t0", DISTANCE_OFFSET);
        }

        self.emit_label(&inner_loop);
        self.emit_stack_load("a0", SOURCE_OFFSET);
        self.emit_stack_load("a1", RELATED_INDEX_OFFSET);
        self.emit_sp_addi("a2", SCRIPT_HASH_OFFSET);
        self.emit("call __cellscript_current_script_role_at");
        self.emit("addi t0, a0, 0");
        self.emit("addi t1, a1, 0");
        self.emit(format!("li t2, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t3, t1, t2");
        self.emit(format!("beqz t3, {}", inner_done));
        self.emit(format!("bnez t1, {}", status_failed));
        self.emit("li t2, 3");
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", role_mismatch));
        self.emit(format!("li t2, {}", related_role));
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", inner_role_candidate));
        self.emit(format!("j {}", advance_related));

        self.emit_label(&inner_role_candidate);
        if related_filter {
            self.emit_metapoint_related_cell_filter_check(
                SOURCE_OFFSET,
                RELATED_INDEX_OFFSET,
                EXPECTED_HASH_PTR_OFFSET,
                FILTER_RULE_OFFSET,
                SIZE_OFFSET,
                TYPE_HASH_BUFFER_OFFSET,
                DATA_BUFFER_OFFSET,
                &status_failed,
                &related_type_mismatch,
                &related_data_mismatch,
                &bad_data_rule,
            );
        }
        self.emit_stack_load("t0", VIEW_KIND_OFFSET);
        self.emit(format!("li t1, {}", CKB_SOURCE_VIEW_SHIFT));
        self.emit("mul t0, t0, t1");
        self.emit_stack_load("a0", BASE_INDEX_OFFSET);
        self.emit("add a0, a0, t0");
        self.emit_stack_load("a1", RELATED_INDEX_OFFSET);
        self.emit("add a1, a1, t0");
        self.emit_stack_load("a2", DISTANCE_OFFSET);
        self.emit("call __ckb_require_metapoint_relative");
        self.emit(format!("beqz a0, {}", relation_matched));
        self.emit(format!("li t0, {}", CellScriptRuntimeError::MetaPointMismatch.code()));
        self.emit("sub t1, a0, t0");
        self.emit(format!("bnez t1, {}", relation_failed));
        self.emit(format!("j {}", advance_related));

        self.emit_label(&advance_related);
        self.emit_stack_load("t0", RELATED_INDEX_OFFSET);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", RELATED_INDEX_OFFSET);
        self.emit(format!("j {}", inner_loop));

        self.emit_label(&relation_matched);
        self.emit_stack_load("t0", MATCH_COUNT_OFFSET);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", MATCH_COUNT_OFFSET);
        self.emit_stack_load("t1", RELATED_INDEX_OFFSET);
        self.emit("addi t1, t1, 1");
        self.emit_stack_store("t1", RELATED_INDEX_OFFSET);
        self.emit(format!("j {}", inner_loop));

        self.emit_label(&inner_done);
        self.emit_stack_load("t0", MATCH_COUNT_OFFSET);
        self.emit("li t1, 1");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", cardinality));
        self.emit(format!("j {}", increment_outer));

        self.emit_label(&maybe_related);
        self.emit(format!("li t2, {}", related_role));
        self.emit("sub t3, t0, t2");
        self.emit(format!("bnez t3, {}", increment_outer));
        if related_filter {
            self.emit_metapoint_related_cell_filter_check(
                SOURCE_OFFSET,
                BASE_INDEX_OFFSET,
                EXPECTED_HASH_PTR_OFFSET,
                FILTER_RULE_OFFSET,
                SIZE_OFFSET,
                TYPE_HASH_BUFFER_OFFSET,
                DATA_BUFFER_OFFSET,
                &status_failed,
                &related_type_mismatch,
                &related_data_mismatch,
                &bad_data_rule,
            );
        }
        self.emit_stack_load("t4", RELATED_COUNT_OFFSET);
        self.emit("addi t4, t4, 1");
        self.emit_stack_store("t4", RELATED_COUNT_OFFSET);

        self.emit_label(&increment_outer);
        self.emit_stack_load("t0", BASE_INDEX_OFFSET);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", BASE_INDEX_OFFSET);
        self.emit(format!("j {}", outer_loop));

        self.emit_label(&outer_done);
        self.emit_stack_load("t0", BASE_COUNT_OFFSET);
        self.emit_stack_load("t1", RELATED_COUNT_OFFSET);
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", cardinality));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&hash_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&status_failed);
        self.emit("addi a0, t1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&relation_failed);
        self.emit("addi t1, a0, 0");
        self.emit(format!("j {}", status_failed));
        self.emit_label(&role_mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptRoleMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_data_rule);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&related_type_mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::TypeHashMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&related_data_mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&data_malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&cardinality);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::MetaPointCardinalityMismatch.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", RA_OFFSET);
        self.emit_stack_adjustment(STACK_SIZE as i64);
        self.emit("ret");
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_metapoint_related_cell_filter_check(
        &mut self,
        source_offset: usize,
        related_index_offset: usize,
        expected_hash_ptr_offset: usize,
        filter_rule_offset: usize,
        size_offset: usize,
        type_hash_buffer_offset: usize,
        data_buffer_offset: usize,
        status_failed: &str,
        type_mismatch: &str,
        data_mismatch: &str,
        bad_data_rule: &str,
    ) {
        let type_loaded = self.fresh_label("metapoint_filter_type_loaded");
        let type_size_ok = self.fresh_label("metapoint_filter_type_size_ok");
        let data_rule_done = self.fresh_label("metapoint_filter_data_rule_done");
        let data_rule_zero = self.fresh_label("metapoint_filter_data_rule_zero");
        let data_rule_nonzero = self.fresh_label("metapoint_filter_data_rule_nonzero");
        let abi = self.runtime_abi();

        self.emit("# cellscript abi: filtered MetaPoint related cell type hash and data-rule check");
        self.emit("li t0, 32");
        self.emit(format!("sd t0, {}(sp)", size_offset));
        self.emit(format!("addi a0, sp, {}", type_hash_buffer_offset));
        self.emit(format!("addi a1, sp, {}", size_offset));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", related_index_offset));
        self.emit(format!("ld a4, {}(sp)", source_offset));
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_TYPE_HASH));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", type_loaded));
        self.emit("addi t1, a0, 0");
        self.emit(format!("j {}", status_failed));
        self.emit_label(&type_loaded);
        self.emit(format!("ld t0, {}(sp)", size_offset));
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", type_size_ok));
        self.emit(format!("j {}", type_mismatch));
        self.emit_label(&type_size_ok);
        self.emit(format!("addi a0, sp, {}", type_hash_buffer_offset));
        self.emit(format!("ld a1, {}(sp)", expected_hash_ptr_offset));
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", type_mismatch));

        self.emit(format!("ld t0, {}(sp)", filter_rule_offset));
        self.emit(format!("beqz t0, {}", data_rule_done));
        self.emit("li t1, 1");
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", data_rule_zero));
        self.emit("li t1, 2");
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", data_rule_nonzero));
        self.emit(format!("j {}", bad_data_rule));

        for (label, require_nonzero) in [(&data_rule_zero, false), (&data_rule_nonzero, true)] {
            let data_loaded = self.fresh_label("metapoint_filter_data_loaded");
            let data_size_ok = self.fresh_label("metapoint_filter_data_size_ok");
            let data_value_ok = self.fresh_label("metapoint_filter_data_value_ok");
            self.emit_label(label);
            self.emit("li t0, 8");
            self.emit(format!("sd t0, {}(sp)", size_offset));
            self.emit(format!("addi a0, sp, {}", data_buffer_offset));
            self.emit(format!("addi a1, sp, {}", size_offset));
            self.emit("li a2, 0");
            self.emit(format!("ld a3, {}(sp)", related_index_offset));
            self.emit(format!("ld a4, {}(sp)", source_offset));
            self.emit(format!("li a7, {}", abi.load_cell_data));
            self.emit("ecall");
            self.emit(format!("beqz a0, {}", data_loaded));
            self.emit(format!("j {}", data_mismatch));
            self.emit_label(&data_loaded);
            self.emit(format!("ld t0, {}(sp)", size_offset));
            self.emit("li t1, 8");
            self.emit("sub t2, t0, t1");
            self.emit(format!("beqz t2, {}", data_size_ok));
            self.emit(format!("j {}", data_mismatch));
            self.emit_label(&data_size_ok);
            self.emit(format!("ld t0, {}(sp)", data_buffer_offset));
            if require_nonzero {
                self.emit(format!("bnez t0, {}", data_value_ok));
                self.emit(format!("j {}", data_mismatch));
            } else {
                self.emit(format!("beqz t0, {}", data_value_ok));
                self.emit(format!("j {}", data_mismatch));
            }
            self.emit_label(&data_value_ok);
            self.emit(format!("j {}", data_rule_done));
        }

        self.emit_label(&data_rule_done);
    }

    fn emit_runtime_lock_match_master_out_point_pairs_from_data_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_require_lock_match_master_out_point_pairs_from_data");
        self.emit_label("__ckb_require_lock_match_master_out_point_pairs_from_data");
        self.emit("# cellscript abi: Limit-Order-style lock-only match order master OutPoint pairing");
        self.emit(
            "# cellscript abi: args a0=input SourceView, a1=output SourceView, a2=action_offset, a3=tx_hash_offset, a4=index_offset",
        );
        self.emit("# cellscript abi: input orders may encode master as Mint(relative i32) or Match(absolute OutPoint)");
        self.emit("# cellscript abi: output orders must encode master as Match(absolute OutPoint)");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const INPUT_VIEW_OFFSET: usize = 0;
        const OUTPUT_VIEW_OFFSET: usize = 8;
        const INPUT_SOURCE_OFFSET: usize = 16;
        const OUTPUT_SOURCE_OFFSET: usize = 24;
        const ACTION_OFFSET_OFFSET: usize = 32;
        const TX_HASH_OFFSET_OFFSET: usize = 40;
        const INDEX_OFFSET_OFFSET: usize = 48;
        const INPUT_INDEX_OFFSET: usize = 56;
        const OUTPUT_INDEX_OFFSET: usize = 64;
        const INPUT_COUNT_OFFSET: usize = 72;
        const OUTPUT_COUNT_OFFSET: usize = 80;
        const MATCH_COUNT_OFFSET: usize = 88;
        const SIZE_OFFSET: usize = 96;
        const SCRIPT_HASH_OFFSET: usize = 104;
        const INPUT_MASTER_TX_OFFSET: usize = 136;
        const OUTPUT_MASTER_TX_OFFSET: usize = 176;
        const INPUT_MASTER_INDEX_OFFSET: usize = 216;
        const OUTPUT_MASTER_INDEX_OFFSET: usize = 224;
        const DATA_BUFFER_OFFSET: usize = 232;
        const FRAME_SIZE: usize = align_frame(RA_OFFSET + 8);
        const RA_OFFSET: usize = 248;

        let invalid = self.fresh_label("match_master_source_invalid");
        let input_source_ok = self.fresh_label("match_master_input_source_ok");
        let output_source_ok = self.fresh_label("match_master_output_source_ok");
        let hash_failed = self.fresh_label("match_master_hash_failed");
        let output_count_loop = self.fresh_label("match_master_output_count_loop");
        let output_count_done = self.fresh_label("match_master_output_count_done");
        let output_count_lock = self.fresh_label("match_master_output_count_lock");
        let output_count_advance = self.fresh_label("match_master_output_count_advance");
        let input_loop = self.fresh_label("match_master_input_loop");
        let input_lock = self.fresh_label("match_master_input_lock");
        let input_advance = self.fresh_label("match_master_input_advance");
        let input_done = self.fresh_label("match_master_input_done");
        let output_match_loop = self.fresh_label("match_master_output_match_loop");
        let output_match_done = self.fresh_label("match_master_output_match_done");
        let output_match_candidate = self.fresh_label("match_master_output_match_candidate");
        let output_match_advance = self.fresh_label("match_master_output_match_advance");
        let output_match_equal = self.fresh_label("match_master_output_match_equal");
        let status_failed = self.fresh_label("match_master_status_failed");
        let role_mismatch = self.fresh_label("match_master_role_mismatch");
        let invalid_action = self.fresh_label("match_master_invalid_action");
        let malformed = self.fresh_label("match_master_malformed");
        let out_point_failed = self.fresh_label("match_master_out_point_failed");
        let cardinality = self.fresh_label("match_master_cardinality");
        let done = self.fresh_label("match_master_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-(FRAME_SIZE as i64));
        self.emit_stack_store("ra", RA_OFFSET);
        self.emit_stack_store("a0", INPUT_VIEW_OFFSET);
        self.emit_stack_store("a1", OUTPUT_VIEW_OFFSET);
        self.emit_stack_store("a2", ACTION_OFFSET_OFFSET);
        self.emit_stack_store("a3", TX_HASH_OFFSET_OFFSET);
        self.emit_stack_store("a4", INDEX_OFFSET_OFFSET);

        self.emit("# cellscript abi: decode input source class for match-order scan");
        self.emit_stack_load("a0", INPUT_VIEW_OFFSET);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", CKB_SOURCE_INPUT));
        self.emit("sub t3, t2, t0");
        self.emit(format!("beqz t3, {}", input_source_ok));
        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit("sub t3, t2, t0");
        self.emit(format!("bnez t3, {}", invalid));
        self.emit_label(&input_source_ok);
        self.emit_stack_store("t2", INPUT_SOURCE_OFFSET);

        self.emit("# cellscript abi: decode output source class for match-order scan");
        self.emit_stack_load("a0", OUTPUT_VIEW_OFFSET);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", CKB_SOURCE_OUTPUT));
        self.emit("sub t3, t2, t0");
        self.emit(format!("beqz t3, {}", output_source_ok));
        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT));
        self.emit("sub t3, t2, t0");
        self.emit(format!("bnez t3, {}", invalid));
        self.emit_label(&output_source_ok);
        self.emit_stack_store("t2", OUTPUT_SOURCE_OFFSET);

        self.emit("li t0, 32");
        self.emit_stack_store("t0", SIZE_OFFSET);
        self.emit_sp_addi("a0", SCRIPT_HASH_OFFSET);
        self.emit_sp_addi("a1", SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", abi.load_script_hash));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", hash_failed));
        self.emit_stack_load("t0", SIZE_OFFSET);
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", hash_failed));

        self.emit_stack_store("zero", OUTPUT_INDEX_OFFSET);
        self.emit_stack_store("zero", OUTPUT_COUNT_OFFSET);
        self.emit_label(&output_count_loop);
        self.emit_stack_load("a0", OUTPUT_SOURCE_OFFSET);
        self.emit_stack_load("a1", OUTPUT_INDEX_OFFSET);
        self.emit_sp_addi("a2", SCRIPT_HASH_OFFSET);
        self.emit("call __cellscript_current_script_role_at");
        self.emit("addi t0, a0, 0");
        self.emit("addi t1, a1, 0");
        self.emit(format!("li t2, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t3, t1, t2");
        self.emit(format!("beqz t3, {}", output_count_done));
        self.emit(format!("bnez t1, {}", status_failed));
        self.emit("li t2, 3");
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", role_mismatch));
        self.emit("li t2, 1");
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", output_count_lock));
        self.emit(format!("j {}", output_count_advance));
        self.emit_label(&output_count_lock);
        self.emit_load_order_master_out_point_from_data(
            OrderMasterDataOffsets {
                source: OUTPUT_SOURCE_OFFSET,
                cell_index: OUTPUT_INDEX_OFFSET,
                action_offset: ACTION_OFFSET_OFFSET,
                tx_hash_offset: TX_HASH_OFFSET_OFFSET,
                index_offset: INDEX_OFFSET_OFFSET,
                tx_dest: OUTPUT_MASTER_TX_OFFSET,
                index_dest: OUTPUT_MASTER_INDEX_OFFSET,
                data_buffer: DATA_BUFFER_OFFSET,
                size: SIZE_OFFSET,
            },
            false,
            OrderMasterFailureLabels { invalid_action: &invalid_action, malformed: &malformed, out_point_failed: &out_point_failed },
        );
        self.emit_stack_load("t0", OUTPUT_COUNT_OFFSET);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", OUTPUT_COUNT_OFFSET);
        self.emit_label(&output_count_advance);
        self.emit_stack_load("t0", OUTPUT_INDEX_OFFSET);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", OUTPUT_INDEX_OFFSET);
        self.emit(format!("j {}", output_count_loop));

        self.emit_label(&output_count_done);
        self.emit_stack_store("zero", INPUT_INDEX_OFFSET);
        self.emit_stack_store("zero", INPUT_COUNT_OFFSET);
        self.emit_label(&input_loop);
        self.emit_stack_load("a0", INPUT_SOURCE_OFFSET);
        self.emit_stack_load("a1", INPUT_INDEX_OFFSET);
        self.emit_sp_addi("a2", SCRIPT_HASH_OFFSET);
        self.emit("call __cellscript_current_script_role_at");
        self.emit("addi t0, a0, 0");
        self.emit("addi t1, a1, 0");
        self.emit(format!("li t2, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t3, t1, t2");
        self.emit(format!("beqz t3, {}", input_done));
        self.emit(format!("bnez t1, {}", status_failed));
        self.emit("li t2, 3");
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", role_mismatch));
        self.emit("li t2, 1");
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", input_lock));
        self.emit(format!("j {}", input_advance));

        self.emit_label(&input_lock);
        self.emit_stack_load("t0", INPUT_COUNT_OFFSET);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", INPUT_COUNT_OFFSET);
        self.emit_stack_store("zero", MATCH_COUNT_OFFSET);
        self.emit_load_order_master_out_point_from_data(
            OrderMasterDataOffsets {
                source: INPUT_SOURCE_OFFSET,
                cell_index: INPUT_INDEX_OFFSET,
                action_offset: ACTION_OFFSET_OFFSET,
                tx_hash_offset: TX_HASH_OFFSET_OFFSET,
                index_offset: INDEX_OFFSET_OFFSET,
                tx_dest: INPUT_MASTER_TX_OFFSET,
                index_dest: INPUT_MASTER_INDEX_OFFSET,
                data_buffer: DATA_BUFFER_OFFSET,
                size: SIZE_OFFSET,
            },
            true,
            OrderMasterFailureLabels { invalid_action: &invalid_action, malformed: &malformed, out_point_failed: &out_point_failed },
        );
        self.emit_stack_store("zero", OUTPUT_INDEX_OFFSET);
        self.emit_label(&output_match_loop);
        self.emit_stack_load("a0", OUTPUT_SOURCE_OFFSET);
        self.emit_stack_load("a1", OUTPUT_INDEX_OFFSET);
        self.emit_sp_addi("a2", SCRIPT_HASH_OFFSET);
        self.emit("call __cellscript_current_script_role_at");
        self.emit("addi t0, a0, 0");
        self.emit("addi t1, a1, 0");
        self.emit(format!("li t2, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t3, t1, t2");
        self.emit(format!("beqz t3, {}", output_match_done));
        self.emit(format!("bnez t1, {}", status_failed));
        self.emit("li t2, 3");
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", role_mismatch));
        self.emit("li t2, 1");
        self.emit("sub t3, t0, t2");
        self.emit(format!("beqz t3, {}", output_match_candidate));
        self.emit(format!("j {}", output_match_advance));

        self.emit_label(&output_match_candidate);
        self.emit_load_order_master_out_point_from_data(
            OrderMasterDataOffsets {
                source: OUTPUT_SOURCE_OFFSET,
                cell_index: OUTPUT_INDEX_OFFSET,
                action_offset: ACTION_OFFSET_OFFSET,
                tx_hash_offset: TX_HASH_OFFSET_OFFSET,
                index_offset: INDEX_OFFSET_OFFSET,
                tx_dest: OUTPUT_MASTER_TX_OFFSET,
                index_dest: OUTPUT_MASTER_INDEX_OFFSET,
                data_buffer: DATA_BUFFER_OFFSET,
                size: SIZE_OFFSET,
            },
            false,
            OrderMasterFailureLabels { invalid_action: &invalid_action, malformed: &malformed, out_point_failed: &out_point_failed },
        );
        for word in 0..4 {
            self.emit_stack_load("t0", INPUT_MASTER_TX_OFFSET + word * 8);
            self.emit_stack_load("t1", OUTPUT_MASTER_TX_OFFSET + word * 8);
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", output_match_advance));
        }
        self.emit_stack_load("t0", INPUT_MASTER_INDEX_OFFSET);
        self.emit_stack_load("t1", OUTPUT_MASTER_INDEX_OFFSET);
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", output_match_equal));
        self.emit(format!("j {}", output_match_advance));
        self.emit_label(&output_match_equal);
        self.emit_stack_load("t0", MATCH_COUNT_OFFSET);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", MATCH_COUNT_OFFSET);

        self.emit_label(&output_match_advance);
        self.emit_stack_load("t0", OUTPUT_INDEX_OFFSET);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", OUTPUT_INDEX_OFFSET);
        self.emit(format!("j {}", output_match_loop));

        self.emit_label(&output_match_done);
        self.emit_stack_load("t0", MATCH_COUNT_OFFSET);
        self.emit("li t1, 1");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", cardinality));

        self.emit_label(&input_advance);
        self.emit_stack_load("t0", INPUT_INDEX_OFFSET);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", INPUT_INDEX_OFFSET);
        self.emit(format!("j {}", input_loop));

        self.emit_label(&input_done);
        self.emit_stack_load("t0", INPUT_COUNT_OFFSET);
        self.emit_stack_load("t1", OUTPUT_COUNT_OFFSET);
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", cardinality));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&hash_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&status_failed);
        self.emit("addi a0, t1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&role_mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptRoleMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&invalid_action);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&out_point_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::OutPointMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&cardinality);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::MetaPointCardinalityMismatch.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", RA_OFFSET);
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");
    }

    fn emit_load_order_master_out_point_from_data(
        &mut self,
        offsets: OrderMasterDataOffsets,
        allow_mint_relative: bool,
        failures: OrderMasterFailureLabels<'_>,
    ) {
        let OrderMasterDataOffsets {
            source: source_offset,
            cell_index: cell_index_offset,
            action_offset: action_offset_offset,
            tx_hash_offset: tx_hash_offset_offset,
            index_offset: index_offset_offset,
            tx_dest: tx_dest_offset,
            index_dest: index_dest_offset,
            data_buffer: data_buffer_offset,
            size: size_offset,
        } = offsets;
        let OrderMasterFailureLabels { invalid_action, malformed, out_point_failed } = failures;
        let action_match = self.fresh_label("order_master_action_match");
        let action_mint = self.fresh_label("order_master_action_mint");
        let size_status_ok = self.fresh_label("order_master_data_size_status_ok");
        let done = self.fresh_label("order_master_loaded");
        let abi = self.runtime_abi();

        self.emit("# cellscript abi: iCKB Limit Order data is exact-length order fields; trailing bytes are malformed");
        self.emit(format!("ld t0, {}(sp)", index_offset_offset));
        self.emit("li t1, 37");
        self.emit("add t0, t0, t1");
        self.emit(format!("sd t0, {}(sp)", data_buffer_offset));
        self.emit("li t1, 0");
        self.emit(format!("sd t1, {}(sp)", size_offset));
        self.emit(format!("addi a0, sp, {}", data_buffer_offset + 8));
        self.emit(format!("addi a1, sp, {}", size_offset));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", cell_index_offset));
        self.emit(format!("ld a4, {}(sp)", source_offset));
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", size_status_ok));
        self.emit(format!("beqz a0, {}", size_status_ok));
        self.emit(format!("j {}", malformed));
        self.emit_label(&size_status_ok);
        self.emit(format!("ld t0, {}(sp)", data_buffer_offset));
        self.emit(format!("ld t1, {}(sp)", size_offset));
        self.emit("sub t2, t1, t0");
        self.emit(format!("bnez t2, {}", malformed));

        self.emit_load_cell_data_prefix_to_stack(
            source_offset,
            cell_index_offset,
            action_offset_offset,
            data_buffer_offset,
            4,
            size_offset,
            malformed,
        );
        self.emit_stack_u32_le_to("t0", data_buffer_offset);
        self.emit("li t1, 1");
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", action_match));
        if allow_mint_relative {
            self.emit(format!("beqz t0, {}", action_mint));
        }
        self.emit(format!("j {}", invalid_action));

        self.emit_label(&action_match);
        self.emit_load_cell_data_prefix_to_stack(
            source_offset,
            cell_index_offset,
            tx_hash_offset_offset,
            tx_dest_offset,
            32,
            size_offset,
            malformed,
        );
        self.emit_load_cell_data_prefix_to_stack(
            source_offset,
            cell_index_offset,
            index_offset_offset,
            data_buffer_offset,
            4,
            size_offset,
            malformed,
        );
        self.emit_stack_u32_le_to("t0", data_buffer_offset);
        self.emit(format!("sd t0, {}(sp)", index_dest_offset));
        self.emit(format!("j {}", done));

        if allow_mint_relative {
            self.emit_label(&action_mint);
            self.emit_load_cell_data_prefix_to_stack(
                source_offset,
                cell_index_offset,
                tx_hash_offset_offset,
                tx_dest_offset,
                32,
                size_offset,
                malformed,
            );
            for word in 0..4 {
                self.emit(format!("ld t0, {}(sp)", tx_dest_offset + word * 8));
                self.emit(format!("bnez t0, {}", malformed));
            }
            self.emit_load_cell_data_prefix_to_stack(
                source_offset,
                cell_index_offset,
                index_offset_offset,
                data_buffer_offset,
                4,
                size_offset,
                malformed,
            );
            self.emit_stack_u32_le_to("t3", data_buffer_offset);
            self.emit_sign_extend_i32("t3");
            self.emit(format!("sd t3, {}(sp)", data_buffer_offset));
            self.emit_load_input_out_point_to_stack(
                source_offset,
                cell_index_offset,
                tx_dest_offset,
                index_dest_offset,
                size_offset,
                out_point_failed,
            );
            self.emit(format!("ld t3, {}(sp)", data_buffer_offset));
            self.emit(format!("ld t0, {}(sp)", index_dest_offset));
            self.emit("add t0, t0, t3");
            self.emit("slt t1, t0, zero");
            self.emit(format!("bnez t1, {}", out_point_failed));
            self.emit(format!("sd t0, {}(sp)", index_dest_offset));
        }

        self.emit_label(&done);
    }

    fn emit_load_cell_data_prefix_to_stack(
        &mut self,
        source_offset: usize,
        cell_index_offset: usize,
        data_offset_offset: usize,
        dest_offset: usize,
        width: usize,
        size_offset: usize,
        malformed: &str,
    ) {
        let loaded = self.fresh_label("cell_data_prefix_loaded");
        let len_enough = self.fresh_label("cell_data_prefix_len_enough");
        let ready = self.fresh_label("cell_data_prefix_ready");
        let abi = self.runtime_abi();

        self.emit(format!("li t0, {}", width));
        self.emit(format!("sd t0, {}(sp)", size_offset));
        self.emit(format!("addi a0, sp, {}", dest_offset));
        self.emit(format!("addi a1, sp, {}", size_offset));
        self.emit(format!("ld a2, {}(sp)", data_offset_offset));
        self.emit(format!("ld a3, {}(sp)", cell_index_offset));
        self.emit(format!("ld a4, {}(sp)", source_offset));
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", len_enough));
        self.emit(format!("j {}", malformed));
        self.emit_label(&loaded);
        self.emit(format!("ld t0, {}(sp)", size_offset));
        self.emit(format!("li t1, {}", width));
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit(format!("j {}", ready));
        self.emit_label(&len_enough);
        self.emit(format!("ld t0, {}(sp)", size_offset));
        self.emit(format!("li t1, {}", width));
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit_label(&ready);
    }

    fn emit_load_input_out_point_to_stack(
        &mut self,
        source_offset: usize,
        cell_index_offset: usize,
        tx_dest_offset: usize,
        index_dest_offset: usize,
        size_offset: usize,
        failed: &str,
    ) {
        let abi = self.runtime_abi();

        self.emit("li t0, 36");
        self.emit(format!("sd t0, {}(sp)", size_offset));
        self.emit(format!("addi a0, sp, {}", tx_dest_offset));
        self.emit(format!("addi a1, sp, {}", size_offset));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", cell_index_offset));
        self.emit(format!("ld a4, {}(sp)", source_offset));
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_OUT_POINT));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit(format!("ld t0, {}(sp)", size_offset));
        self.emit("li t1, 36");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit_stack_u32_le_to("t0", tx_dest_offset + 32);
        self.emit(format!("sd t0, {}(sp)", index_dest_offset));
    }

    fn emit_runtime_cell_hash_requirement_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        field_id: u64,
        mismatch_error: CellScriptRuntimeError,
        enabled: bool,
    ) {
        const STACK_FRAME_BYTES: i64 = align_frame(64) as i64;
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView full-hash requirement ({})", detail));
        self.emit("# cellscript abi: args a0=SourceView, a1=expected_hash_ptr, a2=expected_hash_len");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("source_view_invalid");
        let bad_expected = self.fresh_label("expected_hash_invalid");
        let failed = self.fresh_label("cell_hash_load_failed");
        let mismatch = self.fresh_label("cell_hash_mismatch");
        let done = self.fresh_label("cell_hash_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 56);
        self.emit_stack_store("a1", 48);

        self.emit(format!("beqz a1, {}", bad_expected));
        self.emit("li t0, 32");
        self.emit("sub t1, a2, t0");
        self.emit(format!("bnez t1, {}", bad_expected));

        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 32");
        self.emit_stack_store("t0", 8);
        self.emit_sp_addi("a0", 16);
        self.emit_sp_addi("a1", 8);
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit_stack_load("t0", 8);
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit_sp_addi("a0", 16);
        self.emit_stack_load("a1", 48);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", mismatch_error.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", 56);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_cell_script_hash_field_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        field_id: u64,
        read: ScriptHashFieldRead,
        enabled: bool,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView read-only ScriptRef Hash field ({})", detail));
        self.emit("# cellscript abi: args a0=SourceView, a1=out32_ptr, a2=size_ptr; returns a0=status");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SCRIPT_SIZE_OFFSET: usize = 0;
        const SCRIPT_BUFFER_OFFSET: usize = 8;
        let requested_size = match read {
            ScriptHashFieldRead::CodeHash => 53usize,
            ScriptHashFieldRead::Args32 => 128usize,
        };
        // Round only the following word slots; the byte buffer itself needs
        // no 16-byte alignment. Allocation and restoration share this layout.
        let out_ptr_offset = align_up(SCRIPT_BUFFER_OFFSET + requested_size, 8);
        let size_ptr_offset = out_ptr_offset + 8;
        let ra_offset = size_ptr_offset + 8;
        let frame_size = align_frame(ra_offset + 8);
        let payload_offset = match read {
            ScriptHashFieldRead::CodeHash => SCRIPT_BUFFER_OFFSET + 16,
            ScriptHashFieldRead::Args32 => SCRIPT_BUFFER_OFFSET + 53,
        };
        let invalid = self.fresh_label("script_ref_hash_source_invalid");
        let failed = self.fresh_label("script_ref_hash_load_failed");
        let loaded = self.fresh_label("script_ref_hash_loaded");
        let malformed = self.fresh_label("script_ref_hash_malformed");
        let args_mismatch = self.fresh_label("script_ref_hash_args_mismatch");
        let copy_loop = self.fresh_label("script_ref_hash_copy");
        let copy_done = self.fresh_label("script_ref_hash_copy_done");
        let done = self.fresh_label("script_ref_hash_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-(frame_size as i64));
        self.emit_stack_store("ra", ra_offset);
        self.emit_stack_store("a1", out_ptr_offset);
        self.emit_stack_store("a2", size_ptr_offset);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", requested_size));
        self.emit_stack_store("t0", SCRIPT_SIZE_OFFSET);
        self.emit_sp_addi("a0", SCRIPT_BUFFER_OFFSET);
        self.emit_sp_addi("a1", SCRIPT_SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        if matches!(read, ScriptHashFieldRead::CodeHash) {
            self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
            self.emit("sub t1, a0, t0");
            self.emit(format!("beqz t1, {}", loaded));
        }
        self.emit(format!("j {}", failed));

        self.emit_label(&loaded);
        self.emit_stack_load("t3", SCRIPT_SIZE_OFFSET);
        self.emit("li t1, 49");
        self.emit("sltu t2, t3, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET);
        self.emit("li t1, 53");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        for (offset, expected) in [(4usize, 16u64), (8, 48), (12, 49)] {
            self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + offset);
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }
        if matches!(read, ScriptHashFieldRead::Args32) {
            self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + 49);
            self.emit("li t1, 32");
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", args_mismatch));
            self.emit("li t1, 85");
            self.emit("sub t2, t3, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }

        self.emit("li t1, 0");
        self.emit_label(&copy_loop);
        self.emit("li t2, 32");
        self.emit("sltu t3, t1, t2");
        self.emit(format!("beqz t3, {}", copy_done));
        self.emit_sp_addi("t6", payload_offset);
        self.emit("add t6, t6, t1");
        self.emit("lbu t5, 0(t6)");
        self.emit_stack_load("t6", out_ptr_offset);
        self.emit("add t6, t6, t1");
        self.emit("sb t5, 0(t6)");
        self.emit("addi t1, t1, 1");
        self.emit(format!("j {}", copy_loop));
        self.emit_label(&copy_done);
        self.emit_stack_load("t6", size_ptr_offset);
        self.emit("li t0, 32");
        self.emit("sd t0, 0(t6)");
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&args_mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptArgsMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", ra_offset);
        self.emit_stack_adjustment(frame_size as i64);
        self.emit("ret");
    }

    fn emit_runtime_cell_script_scalar_field_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        field_id: u64,
        read: ScriptScalarFieldRead,
        enabled: bool,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView read-only ScriptRef scalar field ({})", detail));
        self.emit("# cellscript abi: args a0=SourceView; returns a0=value, a1=status");
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SCRIPT_SIZE_OFFSET: usize = 0;
        const SCRIPT_BUFFER_OFFSET: usize = 8;
        let requested_size = match read {
            ScriptScalarFieldRead::HashType => 53usize,
            ScriptScalarFieldRead::ArgsEmpty => 128usize,
        };
        let ra_offset = align_up(SCRIPT_BUFFER_OFFSET + requested_size, 8);
        let frame_size = align_frame(ra_offset + 8);
        let invalid = self.fresh_label("script_ref_scalar_source_invalid");
        let failed = self.fresh_label("script_ref_scalar_load_failed");
        let loaded = self.fresh_label("script_ref_scalar_loaded");
        let malformed = self.fresh_label("script_ref_scalar_malformed");
        let nonempty = self.fresh_label("script_ref_scalar_nonempty");
        let done = self.fresh_label("script_ref_scalar_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-(frame_size as i64));
        self.emit_stack_store("ra", ra_offset);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", requested_size));
        self.emit_stack_store("t0", SCRIPT_SIZE_OFFSET);
        self.emit_sp_addi("a0", SCRIPT_BUFFER_OFFSET);
        self.emit_sp_addi("a1", SCRIPT_SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", loaded));
        self.emit(format!("j {}", failed));

        self.emit_label(&loaded);
        self.emit_stack_load("t3", SCRIPT_SIZE_OFFSET);
        self.emit("li t1, 49");
        self.emit("sltu t2, t3, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET);
        if matches!(read, ScriptScalarFieldRead::HashType) {
            self.emit("li t1, 53");
            self.emit("sltu t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        } else {
            self.emit("sub t2, t0, t3");
            self.emit(format!("bnez t2, {}", malformed));
        }
        for (offset, expected) in [(4usize, 16u64), (8, 48), (12, 49)] {
            self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + offset);
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }
        match read {
            ScriptScalarFieldRead::HashType => {
                self.emit_stack_load_byte("a0", SCRIPT_BUFFER_OFFSET + 48);
                self.emit("li a1, 0");
                self.emit(format!("j {}", done));
            }
            ScriptScalarFieldRead::ArgsEmpty => {
                self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + 49);
                self.emit(format!("bnez t0, {}", nonempty));
                self.emit("li t1, 53");
                self.emit("sub t2, t3, t1");
                self.emit(format!("bnez t2, {}", malformed));
                self.emit("li a0, 1");
                self.emit("li a1, 0");
                self.emit(format!("j {}", done));
                self.emit_label(&nonempty);
                self.emit("li a0, 0");
                self.emit("li a1, 0");
                self.emit(format!("j {}", done));
            }
        }

        self.emit_label(&invalid);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", ra_offset);
        self.emit_stack_adjustment(frame_size as i64);
        self.emit("ret");
    }

    fn emit_runtime_cell_script_args_empty_requirement_helper(&mut self, symbol: &str, detail: &str, field_id: u64, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(144) as i64;
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView Script empty-args requirement ({})", detail));
        self.emit("# cellscript abi: args a0=SourceView; expects Molecule Script args Bytes length == 0");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SCRIPT_SIZE_OFFSET: usize = 0;
        const SCRIPT_BUFFER_OFFSET: usize = 8;
        const EMPTY_SCRIPT_SIZE: u64 = 53;

        let invalid = self.fresh_label("script_args_source_invalid");
        let failed = self.fresh_label("script_args_load_failed");
        let nonempty = self.fresh_label("script_args_nonempty");
        let malformed = self.fresh_label("script_args_malformed");
        let done = self.fresh_label("script_args_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 136);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 128");
        self.emit_stack_store("t0", SCRIPT_SIZE_OFFSET);
        self.emit_sp_addi("a0", SCRIPT_BUFFER_OFFSET);
        self.emit_sp_addi("a1", SCRIPT_SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));

        self.emit_stack_load("t0", SCRIPT_SIZE_OFFSET);
        self.emit(format!("li t1, {}", EMPTY_SCRIPT_SIZE));
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", nonempty));

        for (offset, expected) in [(0usize, EMPTY_SCRIPT_SIZE), (4, 16), (8, 48), (12, 49), (49, 0)] {
            self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + offset);
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&nonempty);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptArgsMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", 136);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_cell_script_args_exact_requirement_helper(&mut self, symbol: &str, detail: &str, field_id: u64, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView Script arbitrary exact args requirement ({})", detail));
        self.emit("# cellscript abi: args a0=SourceView, a1=expected_args_ptr, a2=expected_args_len");
        self.emit("# cellscript abi: validates Molecule packed::Script args Bytes exactly, not only 32-byte hash args");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SCRIPT_SIZE_OFFSET: usize = 8;
        const SCRIPT_BUFFER_OFFSET: usize = 16;
        const EXPECTED_PTR_OFFSET: usize = 72;
        const EXPECTED_LEN_OFFSET: usize = 80;
        const ARGS_OFFSET_OFFSET: usize = 88;
        const CHUNK_LEN_OFFSET: usize = 96;
        const SOURCE_INDEX_OFFSET: usize = 104;
        const SOURCE_KIND_OFFSET: usize = 112;
        const RA_OFFSET: usize = 120;
        const FRAME_SIZE: usize = 128;
        const SCRIPT_PREFIX_SIZE: u64 = 53;
        const CHUNK_SIZE: u64 = 32;

        let invalid = self.fresh_label("script_args_exact_source_invalid");
        let bad_expected = self.fresh_label("script_args_exact_expected_invalid");
        let prefix_loaded = self.fresh_label("script_args_exact_prefix_loaded");
        let load_failed = self.fresh_label("script_args_exact_load_failed");
        let malformed = self.fresh_label("script_args_exact_malformed");
        let mismatch = self.fresh_label("script_args_exact_mismatch");
        let chunk_loop = self.fresh_label("script_args_exact_chunk_loop");
        let chunk_tail = self.fresh_label("script_args_exact_chunk_tail");
        let chunk_loaded = self.fresh_label("script_args_exact_chunk_loaded");
        let success = self.fresh_label("script_args_exact_success");
        let done = self.fresh_label("script_args_exact_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-(FRAME_SIZE as i64));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit(format!("sd a1, {}(sp)", EXPECTED_PTR_OFFSET));
        self.emit(format!("sd a2, {}(sp)", EXPECTED_LEN_OFFSET));
        self.emit(format!("beqz a2, {}", bad_expected));
        self.emit(format!("beqz a1, {}", bad_expected));

        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("sd t1, {}(sp)", SOURCE_INDEX_OFFSET));
        self.emit(format!("sd t2, {}(sp)", SOURCE_KIND_OFFSET));

        self.emit(format!("li t0, {}", SCRIPT_PREFIX_SIZE));
        self.emit(format!("sd t0, {}(sp)", SCRIPT_SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", SCRIPT_BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SCRIPT_SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", SOURCE_INDEX_OFFSET));
        self.emit(format!("ld a4, {}(sp)", SOURCE_KIND_OFFSET));
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", prefix_loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", prefix_loaded));
        self.emit(format!("j {}", load_failed));

        self.emit_label(&prefix_loaded);
        self.emit(format!("ld t3, {}(sp)", SCRIPT_SIZE_OFFSET));
        self.emit(format!("li t1, {}", SCRIPT_PREFIX_SIZE));
        self.emit("sltu t2, t3, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET);
        self.emit(format!("ld t1, {}(sp)", EXPECTED_LEN_OFFSET));
        self.emit(format!("li t2, {}", SCRIPT_PREFIX_SIZE));
        self.emit("add t1, t1, t2");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        for (offset, expected) in [(4usize, 16u64), (8, 48), (12, 49)] {
            self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + offset);
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }
        self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + 49);
        self.emit(format!("ld t1, {}(sp)", EXPECTED_LEN_OFFSET));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", mismatch));
        self.emit(format!("sd zero, {}(sp)", ARGS_OFFSET_OFFSET));

        self.emit_label(&chunk_loop);
        self.emit(format!("ld t0, {}(sp)", ARGS_OFFSET_OFFSET));
        self.emit(format!("ld t1, {}(sp)", EXPECTED_LEN_OFFSET));
        self.emit("sub t2, t1, t0");
        self.emit(format!("beqz t2, {}", success));
        self.emit(format!("li t3, {}", CHUNK_SIZE));
        self.emit("sltu t4, t2, t3");
        self.emit(format!("bnez t4, {}", chunk_tail));
        self.emit(format!("li t2, {}", CHUNK_SIZE));
        self.emit_label(&chunk_tail);
        self.emit(format!("sd t2, {}(sp)", SCRIPT_SIZE_OFFSET));
        self.emit(format!("sd t2, {}(sp)", CHUNK_LEN_OFFSET));
        self.emit(format!("addi a0, sp, {}", SCRIPT_BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SCRIPT_SIZE_OFFSET));
        self.emit(format!("li a2, {}", SCRIPT_PREFIX_SIZE));
        self.emit("add a2, a2, t0");
        self.emit(format!("ld a3, {}(sp)", SOURCE_INDEX_OFFSET));
        self.emit(format!("ld a4, {}(sp)", SOURCE_KIND_OFFSET));
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", chunk_loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", chunk_loaded));
        self.emit(format!("j {}", load_failed));
        self.emit_label(&chunk_loaded);
        self.emit(format!("ld t2, {}(sp)", CHUNK_LEN_OFFSET));
        self.emit(format!("ld t0, {}(sp)", ARGS_OFFSET_OFFSET));
        self.emit(format!("ld t1, {}(sp)", EXPECTED_PTR_OFFSET));
        self.emit("add a1, t1, t0");
        self.emit(format!("addi a0, sp, {}", SCRIPT_BUFFER_OFFSET));
        self.emit("addi a2, t2, 0");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));
        self.emit(format!("ld t0, {}(sp)", ARGS_OFFSET_OFFSET));
        self.emit(format!("ld t2, {}(sp)", CHUNK_LEN_OFFSET));
        self.emit("add t0, t0, t2");
        self.emit(format!("sd t0, {}(sp)", ARGS_OFFSET_OFFSET));
        self.emit(format!("j {}", chunk_loop));

        self.emit_label(&success);
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&load_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptArgsMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");
    }

    fn emit_runtime_current_script_args_empty_requirement_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_require_current_script_args_empty");
        self.emit_label("__ckb_require_current_script_args_empty");
        self.emit("# cellscript abi: current-script empty-args requirement via LOAD_SCRIPT plus output lock scan");
        self.emit("# cellscript abi: expects current Script args empty and same-code/hash-type Output locks args empty");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const CURRENT_SIZE_OFFSET: usize = 8;
        const CURRENT_BUFFER_OFFSET: usize = 16;
        const OUTPUT_INDEX_OFFSET: usize = 144;
        const OUTPUT_SIZE_OFFSET: usize = 152;
        const OUTPUT_BUFFER_OFFSET: usize = 160;
        const OUTPUT_TRUNCATED_OFFSET: usize = 288;
        const EMPTY_SCRIPT_SIZE: u64 = 53;
        const FRAME_SIZE: usize = align_frame(RA_OFFSET + 8);
        const RA_OFFSET: usize = 296;

        let failed = self.fresh_label("current_script_args_load_failed");
        let current_loaded = self.fresh_label("current_script_args_loaded");
        let nonempty = self.fresh_label("current_script_args_nonempty");
        let malformed = self.fresh_label("current_script_args_malformed");
        let output_loop = self.fresh_label("current_script_args_output_loop");
        let output_loaded = self.fresh_label("current_script_args_output_loaded");
        let output_prefix_loaded = self.fresh_label("current_script_args_output_prefix_loaded");
        let output_advance = self.fresh_label("current_script_args_output_advance");
        let output_done = self.fresh_label("current_script_args_output_done");
        let output_same_hash = self.fresh_label("current_script_args_output_same_hash");
        let output_same_script = self.fresh_label("current_script_args_output_same_script");
        let output_failed = self.fresh_label("current_script_args_output_failed");
        let done = self.fresh_label("current_script_args_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-(FRAME_SIZE as i64));
        self.emit_stack_store("ra", RA_OFFSET);
        self.emit("li t0, 128");
        self.emit_stack_store("t0", CURRENT_SIZE_OFFSET);
        self.emit_sp_addi("a0", CURRENT_BUFFER_OFFSET);
        self.emit_sp_addi("a1", CURRENT_SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", abi.load_script));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", current_loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", nonempty));
        self.emit(format!("j {}", failed));

        self.emit_label(&current_loaded);
        self.emit_stack_load("t0", CURRENT_SIZE_OFFSET);
        self.emit(format!("li t1, {}", EMPTY_SCRIPT_SIZE));
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", nonempty));

        for (offset, expected) in [(0usize, EMPTY_SCRIPT_SIZE), (4, 16), (8, 48), (12, 49), (49, 0)] {
            self.emit_stack_u32_le_to("t0", CURRENT_BUFFER_OFFSET + offset);
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }

        self.emit("# cellscript abi: require matching output lock scripts to keep empty args");
        self.emit_stack_store("zero", OUTPUT_INDEX_OFFSET);
        self.emit_label(&output_loop);
        self.emit("li t0, 128");
        self.emit_stack_store("t0", OUTPUT_SIZE_OFFSET);
        self.emit_stack_store("zero", OUTPUT_TRUNCATED_OFFSET);
        self.emit_sp_addi("a0", OUTPUT_BUFFER_OFFSET);
        self.emit_sp_addi("a1", OUTPUT_SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit_stack_load("a3", OUTPUT_INDEX_OFFSET);
        self.emit(format!("li a4, {}", CKB_SOURCE_OUTPUT));
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_LOCK));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", output_loaded));
        self.emit(format!("li t0, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", output_done));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", output_prefix_loaded));
        self.emit(format!("j {}", output_failed));

        self.emit_label(&output_prefix_loaded);
        self.emit("li t0, 1");
        self.emit_stack_store("t0", OUTPUT_TRUNCATED_OFFSET);
        self.emit_label(&output_loaded);
        self.emit_stack_load("t0", OUTPUT_SIZE_OFFSET);
        self.emit("li t1, 49");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit_sp_addi("a0", CURRENT_BUFFER_OFFSET + 16);
        self.emit_sp_addi("a1", OUTPUT_BUFFER_OFFSET + 16);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("beqz a0, {}", output_same_hash));
        self.emit(format!("j {}", output_advance));

        self.emit_label(&output_same_hash);
        self.emit_stack_load_byte("t0", CURRENT_BUFFER_OFFSET + 48);
        self.emit_stack_load_byte("t1", OUTPUT_BUFFER_OFFSET + 48);
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", output_same_script));
        self.emit(format!("j {}", output_advance));

        self.emit_label(&output_same_script);
        self.emit_stack_load("t0", OUTPUT_TRUNCATED_OFFSET);
        self.emit(format!("bnez t0, {}", nonempty));
        self.emit_stack_load("t0", OUTPUT_SIZE_OFFSET);
        self.emit(format!("li t1, {}", EMPTY_SCRIPT_SIZE));
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", nonempty));
        for (offset, expected) in [(0usize, EMPTY_SCRIPT_SIZE), (4, 16), (8, 48), (12, 49), (49, 0)] {
            self.emit_stack_u32_le_to("t0", OUTPUT_BUFFER_OFFSET + offset);
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }

        self.emit_label(&output_advance);
        self.emit_stack_load("t0", OUTPUT_INDEX_OFFSET);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", OUTPUT_INDEX_OFFSET);
        self.emit(format!("j {}", output_loop));

        self.emit_label(&output_done);
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&output_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&nonempty);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptArgsMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", RA_OFFSET);
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");
    }

    fn emit_runtime_cell_script_args_bytes32_requirement_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        field_id: u64,
        mode: ScriptArgsBytes32RequirementMode,
        enabled: bool,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView Script 32-byte args requirement ({})", detail));
        self.emit("# cellscript abi: args a0=SourceView, a1=expected_args_bytes_ptr, a2=expected_args_bytes_len");
        match mode {
            ScriptArgsBytes32RequirementMode::Exact32 => {
                self.emit("# cellscript abi: expects Molecule Script args Bytes length == 32 and payload == expected hash");
            }
            ScriptArgsBytes32RequirementMode::Prefix32 => {
                self.emit("# cellscript abi: expects Molecule Script args Bytes length >= 32 and first 32 bytes == expected hash");
            }
            ScriptArgsBytes32RequirementMode::Suffix32 => {
                self.emit("# cellscript abi: expects Molecule Script args Bytes length >= 32 and last 32 bytes == expected hash");
            }
        }
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SCRIPT_SIZE_OFFSET: usize = 0;
        const SCRIPT_BUFFER_OFFSET: usize = 8;
        const ARGS_PAYLOAD_OFFSET: usize = SCRIPT_BUFFER_OFFSET + 53;
        const SOURCE_INDEX_OFFSET: usize = 136;
        const SOURCE_KIND_OFFSET: usize = 144;
        const EXPECTED_HASH_PTR_OFFSET: usize = 152;
        const RA_OFFSET: usize = 160;
        const FRAME_SIZE: usize = align_frame(RA_OFFSET + 8);
        const SCRIPT_PREFIX_SIZE: u64 = 53;
        const HASH_ARGS_SCRIPT_SIZE: u64 = 85;

        let invalid = self.fresh_label("script_args32_source_invalid");
        let bad_expected = self.fresh_label("script_args32_expected_invalid");
        let loaded = self.fresh_label("script_args32_loaded");
        let suffix_loaded = self.fresh_label("script_args32_suffix_loaded");
        let failed = self.fresh_label("script_args32_load_failed");
        let mismatch = self.fresh_label("script_args32_mismatch");
        let malformed = self.fresh_label("script_args32_malformed");
        let done = self.fresh_label("script_args32_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-(FRAME_SIZE as i64));
        self.emit_stack_store("ra", RA_OFFSET);
        self.emit_stack_store("a1", EXPECTED_HASH_PTR_OFFSET);

        self.emit(format!("beqz a1, {}", bad_expected));
        self.emit("li t0, 32");
        self.emit("sub t1, a2, t0");
        self.emit(format!("bnez t1, {}", bad_expected));

        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit_stack_store("t1", SOURCE_INDEX_OFFSET);
        self.emit_stack_store("t2", SOURCE_KIND_OFFSET);
        let requested_size = match mode {
            ScriptArgsBytes32RequirementMode::Exact32 | ScriptArgsBytes32RequirementMode::Prefix32 => 128u64,
            ScriptArgsBytes32RequirementMode::Suffix32 => SCRIPT_PREFIX_SIZE,
        };
        self.emit(format!("li t0, {}", requested_size));
        self.emit_stack_store("t0", SCRIPT_SIZE_OFFSET);
        self.emit_sp_addi("a0", SCRIPT_BUFFER_OFFSET);
        self.emit_sp_addi("a1", SCRIPT_SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", loaded));
        self.emit(format!("j {}", failed));

        self.emit_label(&loaded);
        self.emit_stack_load("t3", SCRIPT_SIZE_OFFSET);
        self.emit("li t1, 53");
        self.emit("sltu t2, t3, t1");
        self.emit(format!("bnez t2, {}", malformed));

        self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET);
        self.emit("sub t2, t0, t3");
        self.emit(format!("bnez t2, {}", malformed));
        for (offset, expected) in [(4usize, 16u64), (8, 48), (12, 49)] {
            self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + offset);
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }

        self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + 49);
        match mode {
            ScriptArgsBytes32RequirementMode::Exact32 => {
                self.emit("li t1, 32");
                self.emit("sub t2, t0, t1");
                self.emit(format!("bnez t2, {}", mismatch));
                self.emit(format!("li t1, {}", HASH_ARGS_SCRIPT_SIZE));
                self.emit("sub t2, t3, t1");
                self.emit(format!("bnez t2, {}", malformed));
                self.emit_sp_addi("a0", ARGS_PAYLOAD_OFFSET);
            }
            ScriptArgsBytes32RequirementMode::Prefix32 => {
                self.emit("li t1, 32");
                self.emit("sltu t2, t0, t1");
                self.emit(format!("bnez t2, {}", mismatch));
                self.emit(format!("li t1, {}", SCRIPT_PREFIX_SIZE));
                self.emit("add t1, t1, t0");
                self.emit("sub t2, t3, t1");
                self.emit(format!("bnez t2, {}", malformed));
                self.emit_sp_addi("a0", ARGS_PAYLOAD_OFFSET);
            }
            ScriptArgsBytes32RequirementMode::Suffix32 => {
                self.emit("li t1, 32");
                self.emit("sltu t2, t0, t1");
                self.emit(format!("bnez t2, {}", mismatch));
                self.emit(format!("li t1, {}", SCRIPT_PREFIX_SIZE));
                self.emit("add t1, t1, t0");
                self.emit("sub t2, t3, t1");
                self.emit(format!("bnez t2, {}", malformed));
                self.emit("addi t1, t1, -32");
                self.emit("li t0, 32");
                self.emit_stack_store("t0", SCRIPT_SIZE_OFFSET);
                self.emit_sp_addi("a0", SCRIPT_BUFFER_OFFSET);
                self.emit_sp_addi("a1", SCRIPT_SIZE_OFFSET);
                self.emit("addi a2, t1, 0");
                self.emit_stack_load("a3", SOURCE_INDEX_OFFSET);
                self.emit_stack_load("a4", SOURCE_KIND_OFFSET);
                self.emit(format!("li a5, {}", field_id));
                self.emit(format!("li a7, {}", abi.load_cell_by_field));
                self.emit("ecall");
                self.emit(format!("beqz a0, {}", suffix_loaded));
                self.emit(format!("j {}", failed));
                self.emit_label(&suffix_loaded);
                self.emit_stack_load("t0", SCRIPT_SIZE_OFFSET);
                self.emit("li t1, 32");
                self.emit("sub t2, t0, t1");
                self.emit(format!("bnez t2, {}", malformed));
                self.emit_sp_addi("a0", SCRIPT_BUFFER_OFFSET);
            }
        }
        self.emit_stack_load("a1", EXPECTED_HASH_PTR_OFFSET);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptArgsMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", RA_OFFSET);
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");
    }

    fn emit_runtime_cell_script_hash_type_requirement_helper(&mut self, symbol: &str, detail: &str, field_id: u64, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView Script code_hash/hash_type requirement ({})", detail));
        self.emit("# cellscript abi: args a0=SourceView, a1=expected_code_hash_ptr, a2=expected_code_hash_len, a3=expected_hash_type");
        self.emit("# cellscript abi: validates Molecule Script table prefix without constraining args length");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SCRIPT_SIZE_OFFSET: usize = 0;
        const SCRIPT_BUFFER_OFFSET: usize = 8;
        const EXPECTED_CODE_HASH_PTR_OFFSET: usize = 64;
        const EXPECTED_HASH_TYPE_OFFSET: usize = 72;
        const RA_OFFSET: usize = 80;
        const FRAME_SIZE: usize = align_frame(RA_OFFSET + 8);
        const SCRIPT_PREFIX_SIZE: u64 = 53;

        let invalid = self.fresh_label("script_identity_source_invalid");
        let bad_expected = self.fresh_label("script_identity_expected_invalid");
        let bad_hash_type = self.fresh_label("script_identity_hash_type_invalid");
        let loaded = self.fresh_label("script_identity_loaded");
        let prefix_loaded = self.fresh_label("script_identity_prefix_loaded");
        let failed = self.fresh_label("script_identity_load_failed");
        let malformed = self.fresh_label("script_identity_malformed");
        let mismatch = self.fresh_label("script_identity_mismatch");
        let done = self.fresh_label("script_identity_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-(FRAME_SIZE as i64));
        self.emit_stack_store("ra", RA_OFFSET);
        self.emit_stack_store("a1", EXPECTED_CODE_HASH_PTR_OFFSET);
        self.emit_stack_store("a3", EXPECTED_HASH_TYPE_OFFSET);

        self.emit(format!("beqz a1, {}", bad_expected));
        self.emit("li t0, 32");
        self.emit("sub t1, a2, t0");
        self.emit(format!("bnez t1, {}", bad_expected));
        self.emit("li t0, 256");
        self.emit("sltu t1, a3, t0");
        self.emit(format!("beqz t1, {}", bad_hash_type));

        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", SCRIPT_PREFIX_SIZE));
        self.emit_stack_store("t0", SCRIPT_SIZE_OFFSET);
        self.emit_sp_addi("a0", SCRIPT_BUFFER_OFFSET);
        self.emit_sp_addi("a1", SCRIPT_SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", prefix_loaded));
        self.emit(format!("j {}", failed));

        self.emit_label(&loaded);
        self.emit(format!("j {}", prefix_loaded));
        self.emit_label(&prefix_loaded);
        self.emit_stack_load("t3", SCRIPT_SIZE_OFFSET);
        self.emit("li t1, 49");
        self.emit("sltu t2, t3, t1");
        self.emit(format!("bnez t2, {}", malformed));

        self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET);
        self.emit(format!("li t1, {}", SCRIPT_PREFIX_SIZE));
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        for (offset, expected) in [(4usize, 16u64), (8, 48), (12, 49)] {
            self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + offset);
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }

        self.emit_sp_addi("a0", SCRIPT_BUFFER_OFFSET + 16);
        self.emit_stack_load("a1", EXPECTED_CODE_HASH_PTR_OFFSET);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));

        self.emit_stack_load_byte("t0", SCRIPT_BUFFER_OFFSET + 48);
        self.emit_stack_load("t1", EXPECTED_HASH_TYPE_OFFSET);
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_hash_type);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::NumericOrDiscriminantInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptIdentityMismatch.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", RA_OFFSET);
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");
    }

    fn emit_runtime_load_u64_le_helper(&mut self) {
        self.emit_global("__cellscript_load_u64_le");
        self.emit_label("__cellscript_load_u64_le");
        self.emit("# cellscript abi: load unaligned little-endian u64 from pointer a0");
        self.emit("li a1, 0");
        for byte_index in 0..8 {
            self.emit(format!("lbu t0, {}(a0)", byte_index));
            if byte_index != 0 {
                self.emit(format!("slli t0, t0, {}", byte_index * 8));
            }
            self.emit("or a1, a1, t0");
        }
        self.emit("addi a0, a1, 0");
        self.emit("ret");
    }

    fn emit_runtime_mul_u128_to_u256_helper(&mut self) {
        const STACK_FRAME_BYTES: i64 = align_frame(96) as i64;
        self.emit_global("__cellscript_mul_u128_to_u256");
        self.emit_label("__cellscript_mul_u128_to_u256");
        self.emit("# cellscript abi: u128*u128 -> u256 limbs; args a0=left_ptr a1=right_ptr a2=out32_ptr");
        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit("sd ra, 88(sp)");
        self.emit("sd a0, 0(sp)");
        self.emit("sd a1, 8(sp)");
        self.emit("sd a2, 16(sp)");

        self.emit("ld a0, 0(sp)");
        self.emit("call __cellscript_load_u64_le");
        self.emit("sd a0, 24(sp)");
        self.emit("ld a0, 0(sp)");
        self.emit("addi a0, a0, 8");
        self.emit("call __cellscript_load_u64_le");
        self.emit("sd a0, 32(sp)");
        self.emit("ld a0, 8(sp)");
        self.emit("call __cellscript_load_u64_le");
        self.emit("sd a0, 40(sp)");
        self.emit("ld a0, 8(sp)");
        self.emit("addi a0, a0, 8");
        self.emit("call __cellscript_load_u64_le");
        self.emit("sd a0, 48(sp)");

        self.emit("ld t0, 24(sp)");
        self.emit("ld t1, 40(sp)");
        self.emit("mul t2, t0, t1");
        self.emit("mulhu t3, t0, t1");
        self.emit("sd t2, 56(sp)");

        self.emit("ld t0, 24(sp)");
        self.emit("ld t1, 48(sp)");
        self.emit("mul t4, t0, t1");
        self.emit("mulhu t5, t0, t1");

        self.emit("ld t0, 32(sp)");
        self.emit("ld t1, 40(sp)");
        self.emit("mul t6, t0, t1");
        self.emit("mulhu a3, t0, t1");

        self.emit("add t0, t3, t4");
        self.emit("sltu a4, t0, t3");
        self.emit("add t1, t0, t6");
        self.emit("sltu a5, t1, t0");
        self.emit("add a4, a4, a5");
        self.emit("sd t1, 64(sp)");

        self.emit("ld t0, 32(sp)");
        self.emit("ld t1, 48(sp)");
        self.emit("mul a5, t0, t1");
        self.emit("mulhu a6, t0, t1");

        self.emit("add t2, t5, a3");
        self.emit("sltu a7, t2, t5");
        self.emit("add t3, t2, a5");
        self.emit("sltu t4, t3, t2");
        self.emit("add t5, t3, a4");
        self.emit("sltu t6, t5, t3");
        self.emit("sd t5, 72(sp)");
        self.emit("add t0, a6, a7");
        self.emit("add t0, t0, t4");
        self.emit("add t0, t0, t6");
        self.emit("sd t0, 80(sp)");

        self.emit("ld t0, 16(sp)");
        self.emit("ld t1, 56(sp)");
        self.emit("sd t1, 0(t0)");
        self.emit("ld t1, 64(sp)");
        self.emit("sd t1, 8(t0)");
        self.emit("ld t1, 72(sp)");
        self.emit("sd t1, 16(t0)");
        self.emit("ld t1, 80(sp)");
        self.emit("sd t1, 24(t0)");
        self.emit("ld ra, 88(sp)");
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_add_u256_helper(&mut self) {
        self.emit_global("__cellscript_add_u256");
        self.emit_label("__cellscript_add_u256");
        self.emit("# cellscript abi: checked u256 addition; args a0=left32_ptr a1=right32_ptr a2=out32_ptr, returns carry in a0");
        self.emit("li a3, 0");
        for limb_offset in [0, 8, 16, 24] {
            self.emit(format!("ld t0, {}(a0)", limb_offset));
            self.emit(format!("ld t1, {}(a1)", limb_offset));
            self.emit("add t2, t0, t1");
            self.emit("sltu t3, t2, t0");
            self.emit("add t2, t2, a3");
            self.emit("sltu t4, t2, a3");
            self.emit(format!("sd t2, {}(a2)", limb_offset));
            self.emit("add a3, t3, t4");
        }
        self.emit("addi a0, a3, 0");
        self.emit("ret");
    }

    fn emit_runtime_c256_product_requirement_helper(&mut self, symbol: &str, detail: &str, equality: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(112) as i64;
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {} with overflow-safe C256 product comparison", detail));
        self.emit("# cellscript abi: args a0..a3 are u128 little-endian pointers");
        let bad_expected = self.fresh_label("c256_operand_invalid");
        let mismatch = self.fresh_label("c256_product_mismatch");
        let success = self.fresh_label("c256_product_ok");
        let done = self.fresh_label("c256_product_done");

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 104);
        self.emit_stack_store("a0", 0);
        self.emit_stack_store("a1", 8);
        self.emit_stack_store("a2", 16);
        self.emit_stack_store("a3", 24);
        self.emit(format!("beqz a0, {}", bad_expected));
        self.emit(format!("beqz a1, {}", bad_expected));
        self.emit(format!("beqz a2, {}", bad_expected));
        self.emit(format!("beqz a3, {}", bad_expected));

        self.emit_stack_load("a0", 0);
        self.emit_stack_load("a1", 8);
        self.emit_sp_addi("a2", 32);
        self.emit("call __cellscript_mul_u128_to_u256");
        self.emit_stack_load("a0", 16);
        self.emit_stack_load("a1", 24);
        self.emit_sp_addi("a2", 64);
        self.emit("call __cellscript_mul_u128_to_u256");

        for limb_offset in [24, 16, 8, 0] {
            self.emit_stack_load("t0", 32 + limb_offset);
            self.emit_stack_load("t1", 64 + limb_offset);
            if equality {
                self.emit("sub t2, t0, t1");
                self.emit(format!("bnez t2, {}", mismatch));
            } else {
                self.emit("sltu t2, t0, t1");
                self.emit(format!("bnez t2, {}", success));
                self.emit("sltu t2, t1, t0");
                self.emit(format!("bnez t2, {}", mismatch));
            }
        }

        self.emit_label(&success);
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::AggregateAmountMismatch.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", 104);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_c256_sum2_product_requirement_helper(&mut self, symbol: &str, detail: &str, equality: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(272) as i64;
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {} with checked u256 product sums", detail));
        self.emit("# cellscript abi: args a0..a7 are u128 little-endian pointers; compares a0*a1+a2*a3 with a4*a5+a6*a7");
        let bad_expected = self.fresh_label("c256_sum_operand_invalid");
        let mismatch = self.fresh_label("c256_sum_mismatch");
        let success = self.fresh_label("c256_sum_ok");
        let done = self.fresh_label("c256_sum_done");

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 264);
        for (index, register) in ["a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7"].into_iter().enumerate() {
            self.emit(format!("sd {}, {}(sp)", register, index * 8));
            self.emit(format!("beqz {}, {}", register, bad_expected));
        }

        self.emit_stack_load("a0", 0);
        self.emit_stack_load("a1", 8);
        self.emit_sp_addi("a2", 64);
        self.emit("call __cellscript_mul_u128_to_u256");
        self.emit_stack_load("a0", 16);
        self.emit_stack_load("a1", 24);
        self.emit_sp_addi("a2", 96);
        self.emit("call __cellscript_mul_u128_to_u256");
        self.emit_sp_addi("a0", 64);
        self.emit_sp_addi("a1", 96);
        self.emit_sp_addi("a2", 128);
        self.emit("call __cellscript_add_u256");
        self.emit(format!("bnez a0, {}", mismatch));

        self.emit_stack_load("a0", 32);
        self.emit_stack_load("a1", 40);
        self.emit_sp_addi("a2", 160);
        self.emit("call __cellscript_mul_u128_to_u256");
        self.emit_stack_load("a0", 48);
        self.emit_stack_load("a1", 56);
        self.emit_sp_addi("a2", 192);
        self.emit("call __cellscript_mul_u128_to_u256");
        self.emit_sp_addi("a0", 160);
        self.emit_sp_addi("a1", 192);
        self.emit_sp_addi("a2", 224);
        self.emit("call __cellscript_add_u256");
        self.emit(format!("bnez a0, {}", mismatch));

        for limb_offset in [24, 16, 8, 0] {
            self.emit_stack_load("t0", 128 + limb_offset);
            self.emit_stack_load("t1", 224 + limb_offset);
            if equality {
                self.emit("sub t2, t0, t1");
                self.emit(format!("bnez t2, {}", mismatch));
            } else {
                self.emit("sltu t2, t0, t1");
                self.emit(format!("bnez t2, {}", success));
                self.emit("sltu t2, t1, t0");
                self.emit(format!("bnez t2, {}", mismatch));
            }
        }

        self.emit_label(&success);
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::AggregateAmountMismatch.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", 264);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_current_role_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_current_role");
        self.emit_label("__ckb_current_role");
        self.emit("# cellscript abi: current role helper; normal lowering folds role to a compile-time lock/type constant");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
        } else {
            self.emit(format!("li a0, {}", CKB_ROLE_UNKNOWN));
            self.emit("li a1, 0");
        }
        self.emit("ret");
    }

    fn emit_runtime_cell_occupied_capacity_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(48) as i64;
        self.emit_global("__ckb_cell_occupied_capacity");
        self.emit_label("__ckb_cell_occupied_capacity");
        self.emit("# cellscript abi: CKB occupied capacity via LOAD_CELL_BY_FIELD CellField::OccupiedCapacity");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("occupied_capacity_source_invalid");
        let failed = self.fresh_label("occupied_capacity_load_failed");
        let malformed = self.fresh_label("occupied_capacity_field_malformed");
        let done = self.fresh_label("occupied_capacity_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit("sd ra, 40(sp)");
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("sd t1, 0(sp)");
        self.emit("sd t2, 8(sp)");
        self.emit("li t0, 8");
        self.emit("sd t0, 16(sp)");
        self.emit("addi a0, sp, 24");
        self.emit("addi a1, sp, 16");
        self.emit("li a2, 0");
        self.emit("ld a3, 0(sp)");
        self.emit("ld a4, 8(sp)");
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_OCCUPIED_CAPACITY));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit("ld t0, 16(sp)");
        self.emit(format!("bnez a0, {}", failed));
        self.emit("li t1, 8");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit("ld a0, 24(sp)");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit("addi a1, a0, 0");
        self.emit_label(&done);
        self.emit("ld ra, 40(sp)");
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_cell_unoccupied_capacity_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(32) as i64;
        self.emit_global("__ckb_cell_unoccupied_capacity");
        self.emit_label("__ckb_cell_unoccupied_capacity");
        self.emit("# cellscript abi: SourceView unoccupied capacity = capacity - occupied_capacity");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }

        let failed = self.fresh_label("unoccupied_capacity_failed");
        let failed_status_ok = self.fresh_label("unoccupied_capacity_failed_status_ok");
        let underflow = self.fresh_label("unoccupied_capacity_underflow");
        let done = self.fresh_label("unoccupied_capacity_done");

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 24);
        self.emit_stack_store("a0", 8);
        self.emit("call __ckb_cell_capacity");
        self.emit(format!("bnez a1, {}", failed));
        self.emit_stack_store("a0", 0);
        self.emit_stack_load("a0", 8);
        self.emit("call __ckb_cell_occupied_capacity");
        self.emit(format!("bnez a1, {}", failed));
        self.emit_stack_load("t0", 0);
        self.emit("sltu t1, t0, a0");
        self.emit(format!("bnez t1, {}", underflow));
        self.emit("sub a0, t0, a0");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit("addi a0, a1, 0");
        self.emit(format!("bnez a0, {}", failed_status_ok));
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit_label(&failed_status_ok);
        self.emit("addi a1, a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&underflow);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::NumericOrDiscriminantInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit_label(&done);
        self.emit_stack_load("ra", 24);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_cell_output_index_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_cell_output_index");
        self.emit_label("__ckb_cell_output_index");
        self.emit("# cellscript abi: SourceView output index extractor");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("source_view_invalid");
        let output = self.fresh_label("source_view_output");
        let done = self.fresh_label("source_view_output_index_done");
        self.emit(format!("li t6, {}", CKB_SOURCE_VIEW_SHIFT));
        self.emit("div t0, a0, t6");
        self.emit("rem t1, a0, t6");
        self.emit(format!("li t5, {}", CKB_SOURCE_VIEW_OUTPUT));
        self.emit("sub t4, t0, t5");
        self.emit(format!("beqz t4, {}", output));
        self.emit(format!("li t5, {}", CKB_SOURCE_VIEW_GROUP_OUTPUT));
        self.emit("sub t4, t0, t5");
        self.emit(format!("beqz t4, {}", output));
        self.emit(format!("j {}", invalid));
        self.emit_label(&output);
        self.emit("addi a0, t1, 0");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit_label(&done);
        self.emit("ret");
    }

    /// Generic cell probes. Results use a0=value, a1=error; a missing Type is
    /// false, but an out-of-range cell or syscall failure is never false.
    fn emit_runtime_cell_probe_helper(&mut self, symbol: &str, count: bool, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit("# cellscript abi: a0=SourceView; returns a0=value, a1=error");
        if count {
            self.emit("# cellscript abi: count the complete cell source from index zero; supplied view index is ignored");
        }
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }
        const SOURCE: usize = 0;
        const INDEX: usize = 8;
        const LENGTH: usize = 16;
        const BUFFER: usize = 24;
        const RA: usize = 56;
        const FRAME: usize = 64;
        let invalid = self.fresh_label("cell_probe_invalid");
        let ready = self.fresh_label("cell_probe_ready");
        // Keep the existing Type-presence helper's evidence labels stable.
        let scan = self.fresh_label("cell_probe_scan");
        let absent = self.fresh_label("cell_probe_absent");
        let success = self.fresh_label("cell_probe_success");
        let done = self.fresh_label("cell_probe_done");
        let abi = self.runtime_abi();
        self.emit_stack_adjustment(-(FRAME as i64));
        self.emit_stack_store("ra", RA);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        // This API counts Cells, not headers. Reject any decoded non-cell
        // source rather than interpreting its first missing item as an empty set.
        for source in [
            CKB_SOURCE_INPUT,
            CKB_SOURCE_OUTPUT,
            CKB_SOURCE_CELL_DEP,
            CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT,
            CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT,
        ] {
            self.emit(format!("li t0, {source}"));
            self.emit(format!("beq t0, t2, {ready}"));
        }
        self.emit(format!("j {invalid}"));
        self.emit_label(&ready);
        self.emit_stack_store("t2", SOURCE);
        if count {
            self.emit_cell_source_count_scan(
                cell_scan::CellScanFrame { source: SOURCE, index: INDEX, length: LENGTH, buffer: BUFFER },
                None,
                &invalid,
                &invalid,
            );
            self.emit("li a1, 0");
            self.emit(format!("j {done}"));
            self.emit_label(&invalid);
            self.emit_process_failure(CellScriptRuntimeError::CkbSourceViewInvalid);
            self.emit_label(&done);
            self.emit_stack_load("ra", RA);
            self.emit_stack_adjustment(FRAME as i64);
            self.emit("ret");
            return;
        }
        self.emit_stack_store("t1", INDEX);
        self.emit_label(&scan);
        let size = 32;
        self.emit(format!("li t0, {size}"));
        self.emit_stack_store("t0", LENGTH);
        self.emit(format!("addi a0, sp, {BUFFER}"));
        self.emit(format!("addi a1, sp, {LENGTH}"));
        self.emit("li a2, 0");
        self.emit_stack_load("a3", INDEX);
        self.emit_stack_load("a4", SOURCE);
        self.emit(format!("li a5, {CKB_CELL_FIELD_TYPE_HASH}"));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("li t0, {CKB_ITEM_MISSING}"));
        self.emit(format!("beq a0, t0, {absent}"));
        self.emit(format!("bnez a0, {invalid}"));
        self.emit_stack_load("t0", LENGTH);
        self.emit(format!("li t1, {size}"));
        self.emit(format!("bne t0, t1, {invalid}"));
        self.emit("li a0, 1");
        self.emit(format!("j {success}"));
        self.emit_label(&absent);
        self.emit("li a0, 0");
        self.emit_label(&success);
        self.emit("li a1, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&invalid);
        self.emit_process_failure(CellScriptRuntimeError::CkbSourceViewInvalid);
        self.emit_label(&done);
        self.emit_stack_load("ra", RA);
        self.emit_stack_adjustment(FRAME as i64);
        self.emit("ret");
    }

    fn emit_runtime_cell_data_size_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(32) as i64;
        self.emit_global("__ckb_cell_data_size");
        self.emit_label("__ckb_cell_data_size");
        self.emit("# cellscript abi: CKB SourceView LOAD_CELL_DATA size probe");
        if !enabled {
            self.emit_process_failure(CellScriptRuntimeError::SyscallFailed);
            return;
        }
        let invalid = self.fresh_label("source_view_invalid");
        let done = self.fresh_label("cell_data_size_done");
        let failed = self.fresh_label("cell_data_size_failed");
        let status_ok = self.fresh_label("cell_data_size_status_ok");
        let abi = self.runtime_abi();
        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 24);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 0");
        self.emit_stack_store("t0", 8);
        self.emit_sp_addi("a0", 16);
        self.emit_sp_addi("a1", 8);
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", status_ok));
        self.emit(format!("beqz a0, {}", status_ok));
        self.emit(format!("j {}", failed));
        self.emit_label(&status_ok);
        self.emit_stack_load("a0", 8);
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit_process_failure(CellScriptRuntimeError::CkbSourceViewInvalid);
        self.emit_label(&failed);
        self.emit_process_failure(CellScriptRuntimeError::CellLoadFailed);
        self.emit_label(&done);
        self.emit_stack_load("ra", 24);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_cell_data_equal_helper(&mut self, enabled: bool) {
        const VIEW_A: usize = 0;
        const VIEW_B: usize = 8;
        const LENGTH: usize = 16;
        const INDEX_A: usize = 24;
        const SOURCE_A: usize = 32;
        const INDEX_B: usize = 40;
        const SOURCE_B: usize = 48;
        const OFFSET: usize = 56;
        const CHUNK: usize = 64;
        const LOAD_LENGTH_A: usize = 72;
        const LOAD_LENGTH_B: usize = 80;
        const BUFFER_A: usize = 88;
        const BUFFER_B: usize = 344;
        const RA: usize = 600;
        const FRAME: i64 = 608;
        const CAPACITY: u64 = 256;

        self.emit_global("__ckb_cell_data_equal");
        self.emit_label("__ckb_cell_data_equal");
        self.emit("# cellscript abi: a0/a1=SourceView pair; exact complete Cell-data equality; returns a0=bool,a1=status");
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let size_a_ok = self.fresh_label("cell_data_equal_size_a_ok");
        let size_b_ok = self.fresh_label("cell_data_equal_size_b_ok");
        let mismatch = self.fresh_label("cell_data_equal_mismatch");
        let invalid = self.fresh_label("cell_data_equal_invalid_source");
        let loop_label = self.fresh_label("cell_data_equal_loop");
        let chunk_ready = self.fresh_label("cell_data_equal_chunk_ready");
        let load_a_status_ok = self.fresh_label("cell_data_equal_load_a_status_ok");
        let load_b_status_ok = self.fresh_label("cell_data_equal_load_b_status_ok");
        let success = self.fresh_label("cell_data_equal_success");
        let failed = self.fresh_label("cell_data_equal_failed");
        let propagated = self.fresh_label("cell_data_equal_propagated_status");
        let done = self.fresh_label("cell_data_equal_done");
        let abi = self.runtime_abi();

        self.emit_large_addi("sp", "sp", -FRAME);
        self.emit_stack_store("a0", VIEW_A);
        self.emit_stack_store("a1", VIEW_B);
        self.emit_stack_store("ra", RA);

        self.emit_stack_load("a0", VIEW_A);
        self.emit("call __ckb_cell_data_size");
        self.emit(format!("beqz a1, {size_a_ok}"));
        self.emit(format!("j {propagated}"));
        self.emit_label(&size_a_ok);
        self.emit_stack_store("a0", LENGTH);
        self.emit_stack_load("a0", VIEW_B);
        self.emit("call __ckb_cell_data_size");
        self.emit(format!("beqz a1, {size_b_ok}"));
        self.emit(format!("j {propagated}"));
        self.emit_label(&size_b_ok);
        self.emit_stack_load("t0", LENGTH);
        self.emit(format!("bne t0, a0, {mismatch}"));

        self.emit_stack_load("a0", VIEW_A);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit_stack_store("t1", INDEX_A);
        self.emit_stack_store("t2", SOURCE_A);
        self.emit_stack_load("a0", VIEW_B);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit_stack_store("t1", INDEX_B);
        self.emit_stack_store("t2", SOURCE_B);
        self.emit_stack_store("zero", OFFSET);

        self.emit_label(&loop_label);
        self.emit_stack_load("t0", OFFSET);
        self.emit_stack_load("t1", LENGTH);
        self.emit(format!("beq t0, t1, {success}"));
        self.emit("sub t1, t1, t0");
        self.emit(format!("li t2, {CAPACITY}"));
        self.emit(format!("bltu t1, t2, {chunk_ready}"));
        self.emit("mv t1, t2");
        self.emit_label(&chunk_ready);
        self.emit_stack_store("t1", CHUNK);

        self.emit_stack_store("t1", LOAD_LENGTH_A);
        self.emit_sp_addi("a0", BUFFER_A);
        self.emit_sp_addi("a1", LOAD_LENGTH_A);
        self.emit_stack_load("a2", OFFSET);
        self.emit_stack_load("a3", INDEX_A);
        self.emit_stack_load("a4", SOURCE_A);
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("beqz a0, {load_a_status_ok}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {load_a_status_ok}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&load_a_status_ok);
        self.emit_stack_load("t0", LOAD_LENGTH_A);
        self.emit_stack_load("t1", CHUNK);
        self.emit(format!("bltu t0, t1, {failed}"));

        self.emit_stack_store("t1", LOAD_LENGTH_B);
        self.emit_sp_addi("a0", BUFFER_B);
        self.emit_sp_addi("a1", LOAD_LENGTH_B);
        self.emit_stack_load("a2", OFFSET);
        self.emit_stack_load("a3", INDEX_B);
        self.emit_stack_load("a4", SOURCE_B);
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("beqz a0, {load_b_status_ok}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {load_b_status_ok}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&load_b_status_ok);
        self.emit_stack_load("t0", LOAD_LENGTH_B);
        self.emit_stack_load("t1", CHUNK);
        self.emit(format!("bltu t0, t1, {failed}"));

        self.emit_sp_addi("a0", BUFFER_A);
        self.emit_sp_addi("a1", BUFFER_B);
        self.emit_stack_load("a2", CHUNK);
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {mismatch}"));
        self.emit_stack_load("t0", OFFSET);
        self.emit_stack_load("t1", CHUNK);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("t0", OFFSET);
        self.emit(format!("j {loop_label}"));

        self.emit_label(&success);
        self.emit("li a0, 1");
        self.emit("li a1, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&mismatch);
        self.emit("li a0, 0");
        self.emit("li a1, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&invalid);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {done}"));
        self.emit_label(&failed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CellLoadFailed.code()));
        self.emit(format!("j {done}"));
        self.emit_label(&propagated);
        self.emit("li a0, 0");
        self.emit_label(&done);
        self.emit_stack_load("ra", RA);
        self.emit_large_addi("sp", "sp", FRAME);
        self.emit("ret");
    }

    fn emit_runtime_source_bytes_equal_helper(&mut self, enabled: bool) {
        const LEFT_VIEW: usize = 0;
        const LEFT_BASE: usize = 8;
        const RIGHT_VIEW: usize = 16;
        const RIGHT_BASE: usize = 24;
        const LENGTH: usize = 32;
        const LEFT_KIND: usize = 40;
        const RIGHT_KIND: usize = 48;
        const LEFT_INDEX: usize = 56;
        const LEFT_SOURCE: usize = 64;
        const RIGHT_INDEX: usize = 72;
        const RIGHT_SOURCE: usize = 80;
        const CURSOR: usize = 88;
        const CHUNK: usize = 96;
        const LEFT_LOAD_LENGTH: usize = 104;
        const RIGHT_LOAD_LENGTH: usize = 112;
        const LEFT_BUFFER: usize = 120;
        const RIGHT_BUFFER: usize = 376;
        const RA: usize = 632;
        const FRAME: i64 = 640;
        const CAPACITY: u64 = 256;

        self.emit_global("__ckb_source_bytes_equal");
        self.emit_label("__ckb_source_bytes_equal");
        self.emit(
            "# cellscript abi: a0/a1=left SourceView/base, a2/a3=right SourceView/base, a4=len, a5/a6=source kinds; returns a0=bool,a1=status",
        );
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("source_bytes_equal_invalid");
        let loop_label = self.fresh_label("source_bytes_equal_loop");
        let chunk_ready = self.fresh_label("source_bytes_equal_chunk_ready");
        let mismatch = self.fresh_label("source_bytes_equal_mismatch");
        let failed = self.fresh_label("source_bytes_equal_failed");
        let success = self.fresh_label("source_bytes_equal_success");
        let done = self.fresh_label("source_bytes_equal_done");

        self.emit_large_addi("sp", "sp", -FRAME);
        for (register, offset) in [
            ("a0", LEFT_VIEW),
            ("a1", LEFT_BASE),
            ("a2", RIGHT_VIEW),
            ("a3", RIGHT_BASE),
            ("a4", LENGTH),
            ("a5", LEFT_KIND),
            ("a6", RIGHT_KIND),
            ("ra", RA),
        ] {
            self.emit_stack_store(register, offset);
        }

        self.emit_stack_load("a0", LEFT_VIEW);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit_stack_store("t1", LEFT_INDEX);
        self.emit_stack_store("t2", LEFT_SOURCE);
        self.emit_stack_load("a0", RIGHT_VIEW);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit_stack_store("t1", RIGHT_INDEX);
        self.emit_stack_store("t2", RIGHT_SOURCE);
        self.emit_stack_store("zero", CURSOR);

        self.emit_label(&loop_label);
        self.emit_stack_load("t0", CURSOR);
        self.emit_stack_load("t1", LENGTH);
        self.emit(format!("beq t0, t1, {success}"));
        self.emit("sub t1, t1, t0");
        self.emit(format!("li t2, {CAPACITY}"));
        self.emit(format!("bltu t1, t2, {chunk_ready}"));
        self.emit("mv t1, t2");
        self.emit_label(&chunk_ready);
        self.emit_stack_store("t1", CHUNK);

        self.emit_runtime_source_range_load(
            LEFT_KIND,
            LEFT_INDEX,
            LEFT_SOURCE,
            LEFT_BASE,
            CURSOR,
            CHUNK,
            LEFT_LOAD_LENGTH,
            LEFT_BUFFER,
            &invalid,
            &failed,
        );
        self.emit_runtime_source_range_load(
            RIGHT_KIND,
            RIGHT_INDEX,
            RIGHT_SOURCE,
            RIGHT_BASE,
            CURSOR,
            CHUNK,
            RIGHT_LOAD_LENGTH,
            RIGHT_BUFFER,
            &invalid,
            &failed,
        );

        self.emit_sp_addi("a0", LEFT_BUFFER);
        self.emit_sp_addi("a1", RIGHT_BUFFER);
        self.emit_stack_load("a2", CHUNK);
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {mismatch}"));
        self.emit_stack_load("t0", CURSOR);
        self.emit_stack_load("t1", CHUNK);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("t0", CURSOR);
        self.emit(format!("j {loop_label}"));

        self.emit_label(&success);
        self.emit("li a0, 1");
        self.emit("li a1, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&mismatch);
        self.emit("li a0, 0");
        self.emit("li a1, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&invalid);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {done}"));
        self.emit_label(&failed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CellLoadFailed.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", RA);
        self.emit_large_addi("sp", "sp", FRAME);
        self.emit("ret");
    }

    fn emit_runtime_source_bytes_equal_memory_helper(&mut self, enabled: bool) {
        const VIEW: usize = 0;
        const BASE: usize = 8;
        const MEMORY: usize = 16;
        const LENGTH: usize = 24;
        const KIND: usize = 32;
        const INDEX: usize = 40;
        const SOURCE: usize = 48;
        const CURSOR: usize = 56;
        const CHUNK: usize = 64;
        const LOAD_LENGTH: usize = 72;
        const BUFFER: usize = 80;
        const RA: usize = 336;
        const FRAME: i64 = 352;
        const CAPACITY: u64 = 256;

        self.emit_global("__ckb_source_bytes_equal_memory");
        self.emit_label("__ckb_source_bytes_equal_memory");
        self.emit("# cellscript abi: a0/a1=SourceView/base, a2=trusted fixed-byte pointer, a3=len, a4=source kind; returns a0=bool,a1=status");
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("source_memory_equal_invalid");
        let failed = self.fresh_label("source_memory_equal_failed");
        let loop_label = self.fresh_label("source_memory_equal_loop");
        let chunk_ready = self.fresh_label("source_memory_equal_chunk_ready");
        let mismatch = self.fresh_label("source_memory_equal_mismatch");
        let success = self.fresh_label("source_memory_equal_success");
        let done = self.fresh_label("source_memory_equal_done");

        self.emit_large_addi("sp", "sp", -FRAME);
        for (register, offset) in [("a0", VIEW), ("a1", BASE), ("a2", MEMORY), ("a3", LENGTH), ("a4", KIND), ("ra", RA)] {
            self.emit_stack_store(register, offset);
        }
        self.emit_stack_load("a0", VIEW);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit_stack_store("t1", INDEX);
        self.emit_stack_store("t2", SOURCE);
        self.emit_stack_store("zero", CURSOR);

        self.emit_label(&loop_label);
        self.emit_stack_load("t0", CURSOR);
        self.emit_stack_load("t1", LENGTH);
        self.emit(format!("beq t0, t1, {success}"));
        self.emit("sub t1, t1, t0");
        self.emit(format!("li t2, {CAPACITY}"));
        self.emit(format!("bltu t1, t2, {chunk_ready}"));
        self.emit("mv t1, t2");
        self.emit_label(&chunk_ready);
        self.emit_stack_store("t1", CHUNK);
        self.emit_runtime_source_range_load(KIND, INDEX, SOURCE, BASE, CURSOR, CHUNK, LOAD_LENGTH, BUFFER, &invalid, &failed);
        self.emit_sp_addi("a0", BUFFER);
        self.emit_stack_load("a1", MEMORY);
        self.emit_stack_load("t0", CURSOR);
        self.emit("add a1, a1, t0");
        self.emit_stack_load("a2", CHUNK);
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {mismatch}"));
        self.emit_stack_load("t0", CURSOR);
        self.emit_stack_load("t1", CHUNK);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("t0", CURSOR);
        self.emit(format!("j {loop_label}"));

        self.emit_label(&success);
        self.emit("li a0, 1");
        self.emit("li a1, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&mismatch);
        self.emit("li a0, 0");
        self.emit("li a1, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&invalid);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {done}"));
        self.emit_label(&failed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CellLoadFailed.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", RA);
        self.emit_large_addi("sp", "sp", FRAME);
        self.emit("ret");
    }

    fn emit_runtime_source_bytes_zero_helper(&mut self, enabled: bool) {
        const VIEW: usize = 0;
        const BASE: usize = 8;
        const LENGTH: usize = 16;
        const KIND: usize = 24;
        const INDEX: usize = 32;
        const SOURCE: usize = 40;
        const CURSOR: usize = 48;
        const CHUNK: usize = 56;
        const LOAD_LENGTH: usize = 64;
        const BUFFER: usize = 72;
        const RA: usize = 328;
        const FRAME: i64 = 336;
        const CAPACITY: u64 = 256;

        self.emit_global("__ckb_source_bytes_zero");
        self.emit_label("__ckb_source_bytes_zero");
        self.emit("# cellscript abi: a0/a1=SourceView/base, a2=len, a3=source kind; returns a0=bool,a1=status");
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("source_zero_invalid");
        let failed = self.fresh_label("source_zero_failed");
        let loop_label = self.fresh_label("source_zero_loop");
        let chunk_ready = self.fresh_label("source_zero_chunk_ready");
        let mismatch = self.fresh_label("source_zero_mismatch");
        let success = self.fresh_label("source_zero_success");
        let done = self.fresh_label("source_zero_done");

        self.emit_large_addi("sp", "sp", -FRAME);
        for (register, offset) in [("a0", VIEW), ("a1", BASE), ("a2", LENGTH), ("a3", KIND), ("ra", RA)] {
            self.emit_stack_store(register, offset);
        }
        self.emit_stack_load("a0", VIEW);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit_stack_store("t1", INDEX);
        self.emit_stack_store("t2", SOURCE);
        self.emit_stack_store("zero", CURSOR);

        self.emit_label(&loop_label);
        self.emit_stack_load("t0", CURSOR);
        self.emit_stack_load("t1", LENGTH);
        self.emit(format!("beq t0, t1, {success}"));
        self.emit("sub t1, t1, t0");
        self.emit(format!("li t2, {CAPACITY}"));
        self.emit(format!("bltu t1, t2, {chunk_ready}"));
        self.emit("mv t1, t2");
        self.emit_label(&chunk_ready);
        self.emit_stack_store("t1", CHUNK);
        self.emit_runtime_source_range_load(KIND, INDEX, SOURCE, BASE, CURSOR, CHUNK, LOAD_LENGTH, BUFFER, &invalid, &failed);
        self.emit_sp_addi("a0", BUFFER);
        self.emit_stack_load("a1", CHUNK);
        self.emit("call __cellscript_memzero_fixed");
        self.emit(format!("bnez a0, {mismatch}"));
        self.emit_stack_load("t0", CURSOR);
        self.emit_stack_load("t1", CHUNK);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("t0", CURSOR);
        self.emit(format!("j {loop_label}"));

        self.emit_label(&success);
        self.emit("li a0, 1");
        self.emit("li a1, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&mismatch);
        self.emit("li a0, 0");
        self.emit("li a1, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&invalid);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {done}"));
        self.emit_label(&failed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CellLoadFailed.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", RA);
        self.emit_large_addi("sp", "sp", FRAME);
        self.emit("ret");
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_runtime_source_range_load(
        &mut self,
        kind_offset: usize,
        index_offset: usize,
        source_offset: usize,
        base_offset: usize,
        cursor_offset: usize,
        chunk_offset: usize,
        load_length_offset: usize,
        buffer_offset: usize,
        invalid: &str,
        failed: &str,
    ) {
        let data = self.fresh_label("source_range_data");
        let witness = self.fresh_label("source_range_witness");
        let lock = self.fresh_label("source_range_lock");
        let script = self.fresh_label("source_range_script");
        let status = self.fresh_label("source_range_status");
        let status_ok = self.fresh_label("source_range_status_ok");
        let abi = self.runtime_abi();

        self.emit_stack_load("t0", kind_offset);
        self.emit(format!("beqz t0, {data}"));
        self.emit("li t1, 1");
        self.emit(format!("beq t0, t1, {witness}"));
        self.emit("li t1, 2");
        self.emit(format!("beq t0, t1, {lock}"));
        self.emit("li t1, 3");
        self.emit(format!("bne t0, t1, {invalid}"));
        self.emit(format!("li a5, {CKB_CELL_FIELD_TYPE}"));
        self.emit(format!("j {script}"));

        self.emit_label(&lock);
        self.emit(format!("li a5, {CKB_CELL_FIELD_LOCK}"));
        self.emit_label(&script);
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit(format!("j {status}"));
        self.emit_label(&witness);
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit(format!("j {status}"));
        self.emit_label(&data);
        self.emit(format!("li a7, {}", abi.load_cell_data));

        self.emit_label(&status);
        self.emit_stack_load("t0", chunk_offset);
        self.emit_stack_store("t0", load_length_offset);
        self.emit_sp_addi("a0", buffer_offset);
        self.emit_sp_addi("a1", load_length_offset);
        self.emit_stack_load("a2", base_offset);
        self.emit_stack_load("t0", cursor_offset);
        self.emit("add a2, a2, t0");
        self.emit_stack_load("a3", index_offset);
        self.emit_stack_load("a4", source_offset);
        self.emit("ecall");
        self.emit(format!("beqz a0, {status_ok}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("bne a0, t0, {failed}"));
        self.emit_label(&status_ok);
        self.emit_stack_load("t0", load_length_offset);
        self.emit_stack_load("t1", chunk_offset);
        self.emit(format!("bltu t0, t1, {failed}"));
    }

    fn emit_runtime_bounded_cell_dep_data_hash_requirement_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_require_bounded_cell_dep_data_hash");
        self.emit_label("__ckb_require_bounded_cell_dep_data_hash");
        self.emit("# cellscript abi: a0=max_deps(1..=64), a1=expected_data_hash[32]; scan resolved CellDeps with LOAD_CELL_BY_FIELD");
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }

        const EXPECTED_PTR_OFFSET: usize = 8;
        const LIMIT_OFFSET: usize = 16;
        const INDEX_OFFSET: usize = 24;
        const SIZE_OFFSET: usize = 32;
        const BUFFER_OFFSET: usize = 40;
        const RA_OFFSET: usize = 72;
        const FRAME_SIZE: usize = 80;

        let invalid = self.fresh_label("bounded_cell_dep_invalid");
        let scan = self.fresh_label("bounded_cell_dep_scan");
        let not_found = self.fresh_label("bounded_cell_dep_not_found");
        let loaded = self.fresh_label("bounded_cell_dep_loaded");
        let mismatch = self.fresh_label("bounded_cell_dep_mismatch");
        let failed = self.fresh_label("bounded_cell_dep_load_failed");
        let done = self.fresh_label("bounded_cell_dep_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-(FRAME_SIZE as i64));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit(format!("sd a1, {}(sp)", EXPECTED_PTR_OFFSET));
        self.emit(format!("sd a0, {}(sp)", LIMIT_OFFSET));
        self.emit(format!("beqz a1, {}", invalid));
        self.emit(format!("beqz a0, {}", invalid));
        self.emit("li t0, 64");
        self.emit(format!("bltu t0, a0, {}", invalid));
        self.emit(format!("sd zero, {}(sp)", INDEX_OFFSET));

        self.emit_label(&scan);
        self.emit(format!("ld t0, {}(sp)", INDEX_OFFSET));
        self.emit(format!("ld t1, {}(sp)", LIMIT_OFFSET));
        self.emit(format!("bgeu t0, t1, {}", not_found));
        self.emit("li t1, 32");
        self.emit(format!("sd t1, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", INDEX_OFFSET));
        self.emit(format!("li a4, {}", CKB_SOURCE_CELL_DEP));
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_DATA_HASH));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        self.emit(format!("li t0, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", not_found));
        self.emit(format!("j {}", failed));

        self.emit_label(&loaded);
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit(format!("addi a0, sp, {}", BUFFER_OFFSET));
        self.emit(format!("ld a1, {}(sp)", EXPECTED_PTR_OFFSET));
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&mismatch);
        self.emit(format!("ld t0, {}(sp)", INDEX_OFFSET));
        self.emit("addi t0, t0, 1");
        self.emit(format!("sd t0, {}(sp)", INDEX_OFFSET));
        self.emit(format!("j {}", scan));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::BoundsCheckFailed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&not_found);
        self.emit("# cellscript runtime error 63 bounded-cell-dep-not-found");
        self.emit(format!("li a0, {}", CellScriptRuntimeError::BoundedCellDepNotFound.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CellLoadFailed.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");
    }

    fn emit_runtime_cell_data_bytes32_helper(&mut self, symbol: &str, detail: &str, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(32) as i64;
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {}; a0=source_view, a1=offset, a2=out[32], a3=size_ptr", detail));
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("cell_data_bytes32_source_invalid");
        let failed = self.fresh_label("cell_data_bytes32_failed");
        let loaded = self.fresh_label("cell_data_bytes32_loaded");
        let done = self.fresh_label("cell_data_bytes32_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 24);
        self.emit_stack_store("a1", 0);
        self.emit_stack_store("a2", 8);
        self.emit_stack_store("a3", 16);
        self.emit(format!("beqz a2, {}", failed));
        self.emit(format!("beqz a3, {}", failed));
        // Own the requested capacity; a caller-provided length cannot enlarge
        // the fixed result buffer's write extent.
        self.emit("li t0, 32");
        self.emit("sd t0, 0(a3)");
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit_stack_load("a0", 8);
        self.emit_stack_load("a1", 16);
        self.emit_stack_load("a2", 0);
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("bnez t1, {}", failed));
        self.emit_label(&loaded);
        self.emit("# cellscript abi: normalize fixed 32-byte slice length after LOAD_CELL_DATA");
        self.emit_stack_load("t0", 16);
        self.emit("ld t2, 0(t0)");
        self.emit("li t1, 32");
        self.emit(format!("bltu t2, t1, {}", failed));
        self.emit("sd t1, 0(t0)");
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", 24);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_dao_accumulated_rate_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(32) as i64;
        self.emit_global("__dao_accumulated_rate");
        self.emit_label("__dao_accumulated_rate");
        self.emit(
            "# cellscript abi: DAO accumulated-rate HeaderDep SourceView helper via LOAD_HEADER at absolute header offset 160+8",
        );
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("dao_header_source_invalid");
        let done = self.fresh_label("dao_accumulated_rate_done");
        let failed = self.fresh_label("dao_accumulated_rate_failed");
        let loaded = self.fresh_label("dao_accumulated_rate_loaded");
        let abi = self.runtime_abi();
        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 24);
        self.emit(format!("li t6, {}", CKB_SOURCE_VIEW_SHIFT));
        self.emit("div t0, a0, t6");
        self.emit("rem t1, a0, t6");
        self.emit(format!("li t5, {}", CKB_SOURCE_VIEW_HEADER_DEP));
        self.emit("sub t4, t0, t5");
        self.emit(format!("bnez t4, {}", invalid));
        self.emit("li t0, 8");
        self.emit_stack_store("t0", 8);
        self.emit_sp_addi("a0", 16);
        self.emit_sp_addi("a1", 8);
        self.emit(format!("li a2, {}", CKB_DAO_HEADER_ACCUMULATED_RATE_ABSOLUTE_OFFSET));
        self.emit("addi a3, t1, 0");
        self.emit(format!("li a4, {}", abi.source_header_dep));
        self.emit(format!("li a7, {}", abi.load_header));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("bnez t1, {}", failed));
        self.emit_label(&loaded);
        self.emit_stack_load("t0", 8);
        self.emit("li t1, 8");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit_stack_load("a0", 16);
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::HeaderDepMissing.code()));
        self.emit("addi a1, a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DaoFieldMalformed.code()));
        self.emit("addi a1, a0, 0");
        self.emit_label(&done);
        self.emit_stack_load("ra", 24);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_dao_input_accumulated_rate_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(32) as i64;
        self.emit_global("__dao_input_accumulated_rate");
        self.emit_label("__dao_input_accumulated_rate");
        self.emit(
            "# cellscript abi: DAO accumulated-rate from Input/GroupInput committed header via LOAD_HEADER at absolute header offset 160+8",
        );
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("dao_input_header_source_invalid");
        let done = self.fresh_label("dao_input_accumulated_rate_done");
        let failed = self.fresh_label("dao_input_accumulated_rate_failed");
        let loaded = self.fresh_label("dao_input_accumulated_rate_loaded");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 24);
        self.emit_decode_input_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 8");
        self.emit_stack_store("t0", 8);
        self.emit_sp_addi("a0", 16);
        self.emit_sp_addi("a1", 8);
        self.emit(format!("li a2, {}", CKB_DAO_HEADER_ACCUMULATED_RATE_ABSOLUTE_OFFSET));
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a7, {}", abi.load_header));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("bnez t1, {}", failed));
        self.emit_label(&loaded);
        self.emit_stack_load("t0", 8);
        self.emit("li t1, 8");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit_stack_load("a0", 16);
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::HeaderDepMissing.code()));
        self.emit("addi a1, a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DaoFieldMalformed.code()));
        self.emit("addi a1, a0, 0");
        self.emit_label(&done);
        self.emit_stack_load("ra", 24);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_dao_type_classifier_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(64) as i64;
        self.emit_global("__dao_has_dao_type");
        self.emit_label("__dao_has_dao_type");
        self.emit("# cellscript abi: NervosDAO type-hash classifier");
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("dao_type_source_invalid");
        let false_label = self.fresh_label("dao_type_false");
        let done = self.fresh_label("dao_type_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit("sd ra, 56(sp)");
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 32");
        self.emit("sd t0, 8(sp)");
        self.emit("addi a0, sp, 16");
        self.emit("addi a1, sp, 8");
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_TYPE_HASH));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", false_label));
        self.emit("ld t0, 8(sp)");
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", false_label));
        for (word_index, expected) in CKB_DAO_TYPE_HASH_WORDS_LE.iter().enumerate() {
            self.emit(format!("ld t0, {}(sp)", 16 + word_index * 8));
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", false_label));
        }
        self.emit("li a0, 1");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&false_label);
        self.emit("li a0, 0");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit_label(&done);
        self.emit("ld ra, 56(sp)");
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_dao_cell_data_classifier_helper(&mut self, symbol: &str, detail: &str, deposit: bool, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(32) as i64;
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {} via LOAD_CELL_DATA exact 8-byte DAO data", detail));
        self.emit("# cellscript abi: matches NervosDAO deposit/withdrawal-request 8-byte data convention");
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("dao_data_source_invalid");
        let false_label = self.fresh_label("dao_data_false");
        let true_label = self.fresh_label("dao_data_true");
        let done = self.fresh_label("dao_data_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 24);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 8");
        self.emit_stack_store("t0", 8);
        self.emit_sp_addi("a0", 16);
        self.emit_sp_addi("a1", 8);
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", false_label));
        self.emit_stack_load("t0", 8);
        self.emit("li t1, 8");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", false_label));
        self.emit_stack_load("t0", 16);
        if deposit {
            self.emit(format!("beqz t0, {}", true_label));
            self.emit(format!("j {}", false_label));
        } else {
            self.emit(format!("bnez t0, {}", true_label));
            self.emit(format!("j {}", false_label));
        }

        self.emit_label(&true_label);
        self.emit("li a0, 1");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&false_label);
        self.emit("li a0, 0");
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", 24);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_dao_require_header_dep_for_input_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(128) as i64;
        self.emit_global("__dao_require_header_dep_for_input");
        self.emit_label("__dao_require_header_dep_for_input");
        self.emit("# cellscript abi: DAO input header to HeaderDep lineage requirement");
        self.emit("# cellscript abi: args a0=input SourceView, a1=HeaderDep SourceView; compares full 32-byte DAO fields");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SIZE_OFFSET: usize = 8;
        const INPUT_INDEX_OFFSET: usize = 16;
        const INPUT_SOURCE_OFFSET: usize = 24;
        const HEADER_INDEX_OFFSET: usize = 32;
        const INPUT_DAO_OFFSET: usize = 40;
        const HEADER_DAO_OFFSET: usize = 72;
        const HEADER_VIEW_OFFSET: usize = 104;

        let invalid_input = self.fresh_label("dao_lineage_input_source_invalid");
        let invalid_header = self.fresh_label("dao_lineage_header_source_invalid");
        let input_failed = self.fresh_label("dao_lineage_input_header_missing");
        let header_failed = self.fresh_label("dao_lineage_header_dep_missing");
        let malformed = self.fresh_label("dao_lineage_dao_field_malformed");
        let mismatch = self.fresh_label("dao_lineage_mismatch");
        let done = self.fresh_label("dao_lineage_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit("sd ra, 120(sp)");
        self.emit(format!("sd a1, {}(sp)", HEADER_VIEW_OFFSET));

        self.emit_decode_input_source_view_to_t1_t2(&invalid_input);
        self.emit(format!("sd t1, {}(sp)", INPUT_INDEX_OFFSET));
        self.emit(format!("sd t2, {}(sp)", INPUT_SOURCE_OFFSET));

        self.emit(format!("ld a0, {}(sp)", HEADER_VIEW_OFFSET));
        self.emit(format!("li t6, {}", CKB_SOURCE_VIEW_SHIFT));
        self.emit("div t0, a0, t6");
        self.emit("rem t1, a0, t6");
        self.emit(format!("li t5, {}", CKB_SOURCE_VIEW_HEADER_DEP));
        self.emit("sub t4, t0, t5");
        self.emit(format!("bnez t4, {}", invalid_header));
        self.emit(format!("sd t1, {}(sp)", HEADER_INDEX_OFFSET));

        self.emit("li t0, 32");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", INPUT_DAO_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit(format!("li a2, {}", CKB_DAO_HEADER_FIELD_ABSOLUTE_OFFSET));
        self.emit(format!("ld a3, {}(sp)", INPUT_INDEX_OFFSET));
        self.emit(format!("ld a4, {}(sp)", INPUT_SOURCE_OFFSET));
        self.emit(format!("li a7, {}", abi.load_header));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", input_failed));
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 32");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));

        self.emit("li t0, 32");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", HEADER_DAO_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit(format!("li a2, {}", CKB_DAO_HEADER_FIELD_ABSOLUTE_OFFSET));
        self.emit(format!("ld a3, {}(sp)", HEADER_INDEX_OFFSET));
        self.emit(format!("li a4, {}", CKB_SOURCE_HEADER_DEP));
        self.emit(format!("li a7, {}", abi.load_header));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", header_failed));
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 32");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));

        self.emit(format!("addi a0, sp, {}", INPUT_DAO_OFFSET));
        self.emit(format!("addi a1, sp, {}", HEADER_DAO_OFFSET));
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid_input);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&invalid_header);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::HeaderDepMissing.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&input_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::HeaderDepMissing.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&header_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::HeaderDepMissing.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DaoFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DaoHeaderLineageMismatch.code()));
        self.emit_label(&done);
        self.emit("ld ra, 120(sp)");
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_dao_require_input_since_at_least_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(32) as i64;
        self.emit_global("__dao_require_input_since_at_least");
        self.emit_label("__dao_require_input_since_at_least");
        self.emit("# cellscript abi: DAO input since lower-bound requirement");
        self.emit("# cellscript abi: args a0=input SourceView, a1=required_since; enforces loaded_since >= required_since");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SIZE_OFFSET: usize = 0;
        const REQUIRED_SINCE_OFFSET: usize = 8;
        const SINCE_OFFSET: usize = 16;

        let invalid = self.fresh_label("dao_since_input_source_invalid");
        let failed = self.fresh_label("dao_since_load_failed");
        let malformed = self.fresh_label("dao_since_field_malformed");
        let immature = self.fresh_label("dao_since_immature");
        let done = self.fresh_label("dao_since_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 24);
        self.emit_stack_store("a1", REQUIRED_SINCE_OFFSET);

        self.emit_decode_input_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 8");
        self.emit_stack_store("t0", SIZE_OFFSET);
        self.emit_sp_addi("a0", SINCE_OFFSET);
        self.emit_sp_addi("a1", SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_SINCE));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit_stack_load("t0", SIZE_OFFSET);
        self.emit("li t1, 8");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));

        self.emit_stack_load("t0", SINCE_OFFSET);
        self.emit_stack_load("t1", REQUIRED_SINCE_OFFSET);
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", immature));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CellLoadFailed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DaoFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&immature);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DaoMaturityViolation.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", 24);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_dao_require_input_relative_epoch_since_at_least_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(80) as i64;
        self.emit_global("__dao_require_input_relative_epoch_since_at_least");
        self.emit_label("__dao_require_input_relative_epoch_since_at_least");
        self.emit("# cellscript abi: DAO relative epoch since maturity requirement");
        self.emit("# cellscript abi: args a0=input SourceView, a1=epoch_number, a2=epoch_index, a3=epoch_length");
        self.emit("# cellscript abi: loads input since, requires RFC0017 relative epoch flags, and compares epoch fractions");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SIZE_OFFSET: usize = 8;
        const REQUIRED_NUMBER_OFFSET: usize = 16;
        const REQUIRED_INDEX_OFFSET: usize = 24;
        const REQUIRED_LENGTH_OFFSET: usize = 32;
        const SINCE_OFFSET: usize = 40;
        const LOADED_NUMBER_OFFSET: usize = 48;
        const LOADED_INDEX_OFFSET: usize = 56;
        const LOADED_LENGTH_OFFSET: usize = 64;

        let invalid = self.fresh_label("dao_epoch_since_input_source_invalid");
        let failed = self.fresh_label("dao_epoch_since_load_failed");
        let malformed = self.fresh_label("dao_epoch_since_malformed");
        let immature = self.fresh_label("dao_epoch_since_immature");
        let success = self.fresh_label("dao_epoch_since_success");
        let done = self.fresh_label("dao_epoch_since_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit("sd ra, 72(sp)");
        self.emit(format!("sd a1, {}(sp)", REQUIRED_NUMBER_OFFSET));
        self.emit(format!("sd a2, {}(sp)", REQUIRED_INDEX_OFFSET));
        self.emit(format!("sd a3, {}(sp)", REQUIRED_LENGTH_OFFSET));

        self.emit(format!("li t0, {}", CKB_EPOCH_NUMBER_BOUND));
        self.emit("sltu t1, a1, t0");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit(format!("li t0, {}", CKB_EPOCH_FRACTION_BOUND));
        self.emit("sltu t1, a2, t0");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit("sltu t1, a3, t0");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit(format!("beqz a3, {}", malformed));
        self.emit("sltu t1, a2, a3");
        self.emit(format!("beqz t1, {}", malformed));

        self.emit_decode_input_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 8");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", SINCE_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_SINCE));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 8");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));

        self.emit(format!("ld t0, {}(sp)", SINCE_OFFSET));
        self.emit("li t1, 1");
        self.emit("slli t1, t1, 63");
        self.emit("and t2, t0, t1");
        self.emit(format!("beqz t2, {}", malformed));
        self.emit(format!("li t1, {}", CKB_SINCE_REMAIN_FLAGS_BITS));
        self.emit("and t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit(format!("li t1, {}", CKB_SINCE_METRIC_TYPE_FLAG_MASK));
        self.emit("and t2, t0, t1");
        self.emit(format!("li t3, {}", CKB_SINCE_EPOCH_NUMBER_WITH_FRACTION_FLAG));
        self.emit("sub t4, t2, t3");
        self.emit(format!("bnez t4, {}", malformed));

        self.emit(format!("li t1, {}", CKB_SINCE_VALUE_MASK));
        self.emit("and t0, t0, t1");
        self.emit(format!("li t1, {}", CKB_EPOCH_NUMBER_MASK));
        self.emit("and t2, t0, t1");
        self.emit("srai t3, t0, 24");
        self.emit(format!("li t1, {}", CKB_EPOCH_FRACTION_MASK));
        self.emit("and t3, t3, t1");
        self.emit("srai t4, t0, 40");
        self.emit("and t4, t4, t1");
        self.emit(format!("beqz t4, {}", malformed));
        self.emit("sltu t5, t3, t4");
        self.emit(format!("beqz t5, {}", malformed));
        self.emit(format!("sd t2, {}(sp)", LOADED_NUMBER_OFFSET));
        self.emit(format!("sd t3, {}(sp)", LOADED_INDEX_OFFSET));
        self.emit(format!("sd t4, {}(sp)", LOADED_LENGTH_OFFSET));

        self.emit(format!("ld t0, {}(sp)", REQUIRED_NUMBER_OFFSET));
        self.emit("sltu t1, t0, t2");
        self.emit(format!("bnez t1, {}", success));
        self.emit("sltu t1, t2, t0");
        self.emit(format!("bnez t1, {}", immature));
        self.emit(format!("ld t0, {}(sp)", LOADED_INDEX_OFFSET));
        self.emit(format!("ld t1, {}(sp)", REQUIRED_LENGTH_OFFSET));
        self.emit("mul t2, t0, t1");
        self.emit(format!("ld t0, {}(sp)", REQUIRED_INDEX_OFFSET));
        self.emit(format!("ld t1, {}(sp)", LOADED_LENGTH_OFFSET));
        self.emit("mul t3, t0, t1");
        self.emit("sltu t4, t2, t3");
        self.emit(format!("bnez t4, {}", immature));

        self.emit_label(&success);
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CellLoadFailed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSinceMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&immature);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DaoMaturityViolation.code()));
        self.emit_label(&done);
        self.emit("ld ra, 72(sp)");
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_xudt_amount_word_helper(&mut self, symbol: &str, detail: &str, offset: u64, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(32) as i64;
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {} via LOAD_CELL_DATA offset={}", detail, offset));
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("addi a1, a0, 0");
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("source_view_invalid");
        let done = self.fresh_label("xudt_amount_done");
        let failed = self.fresh_label("xudt_amount_failed");
        let abi = self.runtime_abi();
        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 24);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 8");
        self.emit_stack_store("t0", 8);
        self.emit_sp_addi("a0", 16);
        self.emit_sp_addi("a1", 8);
        self.emit(format!("li a2, {}", offset));
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit_stack_load("t0", 8);
        self.emit("li t1, 8");
        self.emit(format!("bltu t0, t1, {}", failed));
        self.emit_stack_load("a0", 16);
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit("addi a1, a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::XudtBindingMismatch.code()));
        self.emit("addi a1, a0, 0");
        self.emit_label(&done);
        self.emit_stack_load("ra", 24);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_xudt_require_owner_mode_input_type_helper(&mut self, enabled: bool) {
        self.emit_runtime_cell_hash_requirement_helper(
            "__xudt_require_owner_mode_input_type",
            "xUDT owner-mode input-type full 32-byte binding check",
            CKB_CELL_FIELD_TYPE_HASH,
            CellScriptRuntimeError::XudtBindingMismatch,
            enabled,
        );
    }

    fn emit_stack_u32_le_to(&mut self, dest: &str, stack_offset: usize) {
        self.emit(format!("lbu {}, {}(sp)", dest, stack_offset));
        self.emit(format!("lbu t4, {}(sp)", stack_offset + 1));
        self.emit("slli t4, t4, 8");
        self.emit(format!("or {}, {}, t4", dest, dest));
        self.emit(format!("lbu t4, {}(sp)", stack_offset + 2));
        self.emit("slli t4, t4, 16");
        self.emit(format!("or {}, {}, t4", dest, dest));
        self.emit(format!("lbu t4, {}(sp)", stack_offset + 3));
        self.emit("slli t4, t4, 24");
        self.emit(format!("or {}, {}, t4", dest, dest));
    }

    pub(super) fn emit_u32_le_from_base_to(&mut self, dest: &str, base: &str, offset: usize, scratch: &str) {
        self.emit(format!("lbu {}, {}({})", dest, offset, base));
        self.emit(format!("lbu {}, {}({})", scratch, offset + 1, base));
        self.emit(format!("slli {}, {}, 8", scratch, scratch));
        self.emit(format!("or {}, {}, {}", dest, dest, scratch));
        self.emit(format!("lbu {}, {}({})", scratch, offset + 2, base));
        self.emit(format!("slli {}, {}, 16", scratch, scratch));
        self.emit(format!("or {}, {}, {}", dest, dest, scratch));
        self.emit(format!("lbu {}, {}({})", scratch, offset + 3, base));
        self.emit(format!("slli {}, {}, 24", scratch, scratch));
        self.emit(format!("or {}, {}, {}", dest, dest, scratch));
    }

    fn emit_runtime_xudt_require_owner_mode_type_args_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(160) as i64;
        self.emit_global("__xudt_require_owner_mode_type_args");
        self.emit_label("__xudt_require_owner_mode_type_args");
        self.emit("# cellscript abi: xUDT owner-mode Type Script args requirement");
        self.emit("# cellscript abi: args a0=SourceView, a1=owner_hash_ptr, a2=owner_hash_len, a3=flags_u32");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SCRIPT_BUFFER_OFFSET: usize = 8;
        const SCRIPT_SIZE_OFFSET: usize = 0;
        const OWNER_ARGS_OFFSET: usize = SCRIPT_BUFFER_OFFSET + 53;
        const FLAGS_ARGS_OFFSET: usize = OWNER_ARGS_OFFSET + 32;

        let invalid = self.fresh_label("xudt_args_source_invalid");
        let bad_expected = self.fresh_label("xudt_args_expected_invalid");
        let malformed = self.fresh_label("xudt_script_malformed");
        let failed = self.fresh_label("xudt_script_load_failed");
        let mismatch = self.fresh_label("xudt_args_mismatch");
        let done = self.fresh_label("xudt_args_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 152);
        self.emit_stack_store("a1", 144);
        self.emit_stack_store("a3", 136);

        self.emit(format!("beqz a1, {}", bad_expected));
        self.emit("li t0, 32");
        self.emit("sub t1, a2, t0");
        self.emit(format!("bnez t1, {}", bad_expected));
        self.emit("li t0, 4294967296");
        self.emit("sltu t1, a3, t0");
        self.emit(format!("beqz t1, {}", mismatch));

        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 128");
        self.emit_stack_store("t0", SCRIPT_SIZE_OFFSET);
        self.emit_sp_addi("a0", SCRIPT_BUFFER_OFFSET);
        self.emit_sp_addi("a1", SCRIPT_SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit("addi a3, t1, 0");
        self.emit("addi a4, t2, 0");
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_TYPE));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));

        self.emit_stack_load("t0", SCRIPT_SIZE_OFFSET);
        self.emit("li t1, 89");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));

        for (offset, expected) in [(0usize, 89u64), (4, 16), (8, 48), (12, 49), (49, 36)] {
            self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET + offset);
            self.emit(format!("li t1, {}", expected));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", malformed));
        }

        self.emit_sp_addi("a0", OWNER_ARGS_OFFSET);
        self.emit_stack_load("a1", 144);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));

        self.emit_stack_u32_le_to("t0", FLAGS_ARGS_OFFSET);
        self.emit_stack_load("t1", 136);
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::XudtBindingMismatch.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", 152);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_xudt_require_owner_mode_type_args_current_script_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(80) as i64;
        self.emit_global("__xudt_require_owner_mode_type_args_current_script");
        self.emit_label("__xudt_require_owner_mode_type_args_current_script");
        self.emit("# cellscript abi: xUDT owner-mode Type Script args requirement bound to current script hash");
        self.emit("# cellscript abi: args a0=SourceView, a1=flags_u32; owner hash is LOAD_SCRIPT_HASH(current script)");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SIZE_OFFSET: usize = 8;
        const SOURCE_VIEW_OFFSET: usize = 16;
        const FLAGS_OFFSET: usize = 24;
        const SCRIPT_HASH_OFFSET: usize = 32;

        let hash_failed = self.fresh_label("xudt_current_script_hash_load_failed");
        let hash_malformed = self.fresh_label("xudt_current_script_hash_malformed");
        let done = self.fresh_label("xudt_current_script_args_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit("sd ra, 72(sp)");
        self.emit(format!("sd a0, {}(sp)", SOURCE_VIEW_OFFSET));
        self.emit(format!("sd a1, {}(sp)", FLAGS_OFFSET));

        self.emit("li t0, 32");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", SCRIPT_HASH_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", abi.load_script_hash));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", hash_failed));
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", hash_malformed));

        self.emit(format!("ld a0, {}(sp)", SOURCE_VIEW_OFFSET));
        self.emit(format!("addi a1, sp, {}", SCRIPT_HASH_OFFSET));
        self.emit("li a2, 32");
        self.emit(format!("ld a3, {}(sp)", FLAGS_OFFSET));
        self.emit("call __xudt_require_owner_mode_type_args");
        self.emit(format!("j {}", done));

        self.emit_label(&hash_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&hash_malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit_label(&done);
        self.emit("ld ra, 72(sp)");
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    fn emit_runtime_fungible_type_group_conservation_helper(&mut self, symbol: &str, detail: &str, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        let owner_mode = symbol == FUNGIBLE_TYPE_GROUP_V1_CODEGEN_HELPER;
        if owner_mode {
            self.emit(format!(
                "# cellscript abi: {detail}; owner-authorized issuance or non-empty input/output checked-u128 conservation"
            ));
            self.emit("# cellscript abi: supply authorization: 32-byte input lock hash or 0x01-tagged 32-byte input Type Script hash");
        } else {
            self.emit(format!("# cellscript abi: {detail}; requires non-empty input/output groups and conserves checked u128 sums"));
        }
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SIZE_OFFSET: usize = 8;
        const BUFFER_OFFSET: usize = 16;
        const INPUT_LOW_OFFSET: usize = 32;
        const INPUT_HIGH_OFFSET: usize = 40;
        const OUTPUT_LOW_OFFSET: usize = 48;
        const OUTPUT_HIGH_OFFSET: usize = 56;
        const INDEX_OFFSET: usize = 64;
        const SOURCE_OFFSET: usize = 72;
        const SUM_LOW_OFFSET: usize = 80;
        const SUM_HIGH_OFFSET: usize = 88;
        const CURRENT_SCRIPT_BUFFER_OFFSET: usize = 96;
        const CURRENT_SCRIPT_SIZE_OFFSET: usize = 240;
        const OWNER_LOCK_BUFFER_OFFSET: usize = 192;
        const OWNER_LOCK_SIZE_OFFSET: usize = 224;
        const OWNER_INPUT_INDEX_OFFSET: usize = 232;
        const OWNER_AUTHORIZED_OFFSET: usize = 248;
        const OWNER_AUTHORITY_FIELD_OFFSET: usize = 256;
        const LEGACY_OWNER_SCRIPT_SIZE: u64 = 85;
        const TAGGED_TYPE_OWNER_SCRIPT_SIZE: u64 = 86;
        const TAGGED_TYPE_AUTHORITY: u64 = 1;

        let frame_size = if owner_mode { 272usize } else { 112usize };
        let ra_offset = frame_size - 8;

        let conservation_start = self.fresh_label("fungible_group_conservation_start");
        let owner_script_loaded = self.fresh_label("fungible_group_owner_script_loaded");
        let owner_legacy_lock_mode = self.fresh_label("fungible_group_owner_legacy_lock_mode");
        let owner_tagged_type_mode = self.fresh_label("fungible_group_owner_tagged_type_mode");
        let owner_authority_mode_ready = self.fresh_label("fungible_group_owner_authority_mode_ready");
        let owner_scan_loop = self.fresh_label("fungible_group_owner_scan_loop");
        let owner_lock_loaded = self.fresh_label("fungible_group_owner_lock_loaded");
        let owner_not_matched = self.fresh_label("fungible_group_owner_not_matched");
        let owner_matched = self.fresh_label("fungible_group_owner_matched");
        let owner_expected_type_hash = self.fresh_label("fungible_group_owner_expected_type_hash");
        let owner_expected_hash_ready = self.fresh_label("fungible_group_owner_expected_hash_ready");
        let owner_authorized = self.fresh_label("fungible_group_owner_authorized");
        let owner_script_failed = self.fresh_label("fungible_group_owner_script_failed");
        let owner_script_malformed = self.fresh_label("fungible_group_owner_script_malformed");
        let owner_scan_failed = self.fresh_label("fungible_group_owner_scan_failed");
        let scan_source = self.fresh_label("xudt_group_scan_source");
        let scan_loop = self.fresh_label("xudt_group_scan_loop");
        let scan_done = self.fresh_label("xudt_group_scan_done");
        let scan_failed = self.fresh_label("xudt_group_scan_failed");
        let scan_malformed = self.fresh_label("xudt_group_scan_malformed");
        let overflow = self.fresh_label("xudt_group_sum_overflow");
        let output_phase = self.fresh_label("xudt_group_output_phase");
        let compare = self.fresh_label("xudt_group_compare");
        let mismatch = self.fresh_label("xudt_group_mismatch");
        let done = self.fresh_label("xudt_group_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-(frame_size as i64));
        self.emit(format!("sd ra, {}(sp)", ra_offset));
        for offset in [INPUT_LOW_OFFSET, INPUT_HIGH_OFFSET, OUTPUT_LOW_OFFSET, OUTPUT_HIGH_OFFSET] {
            self.emit(format!("sd zero, {}(sp)", offset));
        }

        if owner_mode {
            self.emit("# cellscript abi: authority args are legacy 32-byte lock hash or 0x01 plus 32-byte policy Type Script hash");
            self.emit("li t0, 96");
            self.emit(format!("sd t0, {}(sp)", CURRENT_SCRIPT_SIZE_OFFSET));
            self.emit(format!("addi a0, sp, {}", CURRENT_SCRIPT_BUFFER_OFFSET));
            self.emit(format!("addi a1, sp, {}", CURRENT_SCRIPT_SIZE_OFFSET));
            self.emit("li a2, 0");
            self.emit(format!("li a7, {}", abi.load_script));
            self.emit("ecall");
            self.emit(format!("beqz a0, {}", owner_script_loaded));
            self.emit(format!("j {}", owner_script_failed));

            self.emit_label(&owner_script_loaded);
            self.emit(format!("ld t0, {}(sp)", CURRENT_SCRIPT_SIZE_OFFSET));
            self.emit(format!("li t1, {}", LEGACY_OWNER_SCRIPT_SIZE));
            self.emit("sub t2, t0, t1");
            self.emit(format!("beqz t2, {}", owner_legacy_lock_mode));
            self.emit(format!("li t1, {}", TAGGED_TYPE_OWNER_SCRIPT_SIZE));
            self.emit("sub t2, t0, t1");
            self.emit(format!("beqz t2, {}", owner_tagged_type_mode));
            self.emit(format!("j {}", owner_script_malformed));

            self.emit_label(&owner_legacy_lock_mode);
            for (offset, expected) in [(0usize, LEGACY_OWNER_SCRIPT_SIZE), (4, 16), (8, 48), (12, 49), (49, 32)] {
                self.emit_stack_u32_le_to("t0", CURRENT_SCRIPT_BUFFER_OFFSET + offset);
                self.emit(format!("li t1, {}", expected));
                self.emit("sub t2, t0, t1");
                self.emit(format!("bnez t2, {}", owner_script_malformed));
            }
            self.emit(format!("li t0, {}", CKB_CELL_FIELD_LOCK_HASH));
            self.emit(format!("sd t0, {}(sp)", OWNER_AUTHORITY_FIELD_OFFSET));
            self.emit(format!("j {}", owner_authority_mode_ready));

            self.emit_label(&owner_tagged_type_mode);
            for (offset, expected) in [(0usize, TAGGED_TYPE_OWNER_SCRIPT_SIZE), (4, 16), (8, 48), (12, 49), (49, 33)] {
                self.emit_stack_u32_le_to("t0", CURRENT_SCRIPT_BUFFER_OFFSET + offset);
                self.emit(format!("li t1, {}", expected));
                self.emit("sub t2, t0, t1");
                self.emit(format!("bnez t2, {}", owner_script_malformed));
            }
            self.emit(format!("lbu t0, {}(sp)", CURRENT_SCRIPT_BUFFER_OFFSET + 53));
            self.emit(format!("li t1, {}", TAGGED_TYPE_AUTHORITY));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", owner_script_malformed));
            self.emit(format!("li t0, {}", CKB_CELL_FIELD_TYPE_HASH));
            self.emit(format!("sd t0, {}(sp)", OWNER_AUTHORITY_FIELD_OFFSET));

            self.emit_label(&owner_authority_mode_ready);

            self.emit("# cellscript abi: supply authority succeeds only when an absolute Input lock/type hash equals Script args");
            self.emit(format!("sd zero, {}(sp)", OWNER_INPUT_INDEX_OFFSET));
            self.emit(format!("sd zero, {}(sp)", OWNER_AUTHORIZED_OFFSET));
            self.emit_label(&owner_scan_loop);
            self.emit("li t0, 32");
            self.emit(format!("sd t0, {}(sp)", OWNER_LOCK_SIZE_OFFSET));
            self.emit(format!("addi a0, sp, {}", OWNER_LOCK_BUFFER_OFFSET));
            self.emit(format!("addi a1, sp, {}", OWNER_LOCK_SIZE_OFFSET));
            self.emit("li a2, 0");
            self.emit(format!("ld a3, {}(sp)", OWNER_INPUT_INDEX_OFFSET));
            self.emit(format!("li a4, {}", CKB_SOURCE_INPUT));
            self.emit(format!("ld a5, {}(sp)", OWNER_AUTHORITY_FIELD_OFFSET));
            self.emit(format!("li a7, {}", abi.load_cell_by_field));
            self.emit("ecall");
            self.emit(format!("beqz a0, {}", owner_lock_loaded));
            self.emit(format!("li t0, {}", CKB_INDEX_OUT_OF_BOUND));
            self.emit("sub t1, a0, t0");
            self.emit(format!("beqz t1, {}", conservation_start));
            self.emit(format!("li t0, {}", CKB_ITEM_MISSING));
            self.emit("sub t1, a0, t0");
            self.emit(format!("beqz t1, {}", owner_not_matched));
            self.emit(format!("j {}", owner_scan_failed));

            self.emit_label(&owner_lock_loaded);
            self.emit(format!("ld t0, {}(sp)", OWNER_LOCK_SIZE_OFFSET));
            self.emit("li t1, 32");
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", owner_scan_failed));
            self.emit(format!("addi a0, sp, {}", OWNER_LOCK_BUFFER_OFFSET));
            self.emit(format!("ld t0, {}(sp)", OWNER_AUTHORITY_FIELD_OFFSET));
            self.emit(format!("li t1, {}", CKB_CELL_FIELD_TYPE_HASH));
            self.emit("sub t2, t0, t1");
            self.emit(format!("beqz t2, {}", owner_expected_type_hash));
            self.emit(format!("addi a1, sp, {}", CURRENT_SCRIPT_BUFFER_OFFSET + 53));
            self.emit(format!("j {}", owner_expected_hash_ready));
            self.emit_label(&owner_expected_type_hash);
            self.emit(format!("addi a1, sp, {}", CURRENT_SCRIPT_BUFFER_OFFSET + 54));
            self.emit_label(&owner_expected_hash_ready);
            self.emit("li a2, 32");
            self.emit("call __cellscript_memcmp_fixed");
            self.emit(format!("beqz a0, {}", owner_matched));
            self.emit(format!("j {}", owner_not_matched));

            self.emit_label(&owner_not_matched);
            self.emit(format!("ld t0, {}(sp)", OWNER_INPUT_INDEX_OFFSET));
            self.emit("addi t0, t0, 1");
            self.emit(format!("sd t0, {}(sp)", OWNER_INPUT_INDEX_OFFSET));
            self.emit(format!("j {}", owner_scan_loop));

            self.emit_label(&owner_matched);
            self.emit("li t0, 1");
            self.emit(format!("sd t0, {}(sp)", OWNER_AUTHORIZED_OFFSET));
            self.emit(format!("j {}", conservation_start));

            self.emit_label(&owner_script_failed);
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit(format!("j {}", done));
            self.emit_label(&owner_script_malformed);
            self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
            self.emit(format!("j {}", done));
            self.emit_label(&owner_scan_failed);
            self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
            self.emit(format!("j {}", done));

            self.emit_label(&conservation_start);
        }

        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit(format!("sd t0, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("addi t0, sp, {}", INPUT_LOW_OFFSET));
        self.emit(format!("addi t1, sp, {}", INPUT_HIGH_OFFSET));
        self.emit(format!("j {}", scan_source));

        self.emit_label(&output_phase);
        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT));
        self.emit(format!("sd t0, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("addi t0, sp, {}", OUTPUT_LOW_OFFSET));
        self.emit(format!("addi t1, sp, {}", OUTPUT_HIGH_OFFSET));

        self.emit_label(&scan_source);
        self.emit(format!("sd t0, {}(sp)", SUM_LOW_OFFSET));
        self.emit(format!("sd t1, {}(sp)", SUM_HIGH_OFFSET));
        self.emit(format!("sd zero, {}(sp)", INDEX_OFFSET));

        self.emit_label(&scan_loop);
        self.emit("li t0, 16");
        self.emit(format!("sd t0, {}(sp)", SIZE_OFFSET));
        self.emit(format!("addi a0, sp, {}", BUFFER_OFFSET));
        self.emit(format!("addi a1, sp, {}", SIZE_OFFSET));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {}(sp)", INDEX_OFFSET));
        self.emit(format!("ld a4, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", scan_done));
        self.emit(format!("li t0, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", compare));
        self.emit(format!("j {}", scan_failed));

        self.emit_label(&scan_done);
        self.emit(format!("ld t0, {}(sp)", SIZE_OFFSET));
        self.emit("li t1, 16");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", scan_malformed));
        self.emit(format!("ld t0, {}(sp)", SUM_LOW_OFFSET));
        self.emit(format!("ld t1, {}(sp)", SUM_HIGH_OFFSET));
        self.emit("ld t2, 16(sp)");
        self.emit("ld t3, 24(sp)");
        self.emit("ld t4, 0(t0)");
        self.emit("ld t5, 0(t1)");
        self.emit("add t6, t4, t2");
        self.emit("sltu t4, t6, t4");
        self.emit("add t5, t5, t3");
        self.emit("sltu t3, t5, t3");
        self.emit(format!("bnez t3, {}", overflow));
        self.emit("add t5, t5, t4");
        self.emit("sltu t4, t5, t4");
        self.emit(format!("bnez t4, {}", overflow));
        self.emit("sd t6, 0(t0)");
        self.emit("sd t5, 0(t1)");
        self.emit(format!("ld t0, {}(sp)", INDEX_OFFSET));
        self.emit("addi t0, t0, 1");
        self.emit(format!("sd t0, {}(sp)", INDEX_OFFSET));
        self.emit(format!("j {}", scan_loop));

        self.emit_label(&compare);
        let non_empty = self.fresh_label("fungible_group_non_empty");
        if owner_mode {
            self.emit(format!("ld t4, {}(sp)", OWNER_AUTHORIZED_OFFSET));
            self.emit(format!("bnez t4, {}", non_empty));
        }
        self.emit(format!("ld t3, {}(sp)", INDEX_OFFSET));
        self.emit(format!("beqz t3, {}", mismatch));
        self.emit_label(&non_empty);
        self.emit(format!("ld t0, {}(sp)", SOURCE_OFFSET));
        self.emit(format!("li t1, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", output_phase));
        if owner_mode {
            self.emit(format!("ld t0, {}(sp)", OWNER_AUTHORIZED_OFFSET));
            self.emit(format!("bnez t0, {}", owner_authorized));
        }
        self.emit(format!("ld t0, {}(sp)", INPUT_LOW_OFFSET));
        self.emit(format!("ld t1, {}(sp)", OUTPUT_LOW_OFFSET));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", mismatch));
        self.emit(format!("ld t0, {}(sp)", INPUT_HIGH_OFFSET));
        self.emit(format!("ld t1, {}(sp)", OUTPUT_HIGH_OFFSET));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));
        if owner_mode {
            self.emit_label(&owner_authorized);
            self.emit("li a0, 0");
            self.emit(format!("j {}", done));
        }

        self.emit_label(&scan_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::XudtBindingMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&scan_malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::XudtBindingMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&overflow);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::AggregateAmountMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::AggregateAmountMismatch.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {}(sp)", ra_offset));
        self.emit_stack_adjustment(frame_size as i64);
        self.emit("ret");
    }

    fn emit_runtime_xudt_require_group_amount_delta_helper(&mut self, symbol: &str, minted: bool, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(112) as i64;
        self.emit_global(symbol);
        self.emit_label(symbol);
        if minted {
            self.emit(
                "# cellscript abi: scans current xUDT type group and requires sum(outputs.amount) == sum(inputs.amount) + delta",
            );
        } else {
            self.emit(
                "# cellscript abi: scans current xUDT type group and requires sum(inputs.amount) == sum(outputs.amount) + delta",
            );
        }
        self.emit("# cellscript abi: args a0=delta_u128_le_ptr");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const SIZE_OFFSET: usize = 8;
        const BUFFER_OFFSET: usize = 16;
        const INPUT_LOW_OFFSET: usize = 32;
        const INPUT_HIGH_OFFSET: usize = 40;
        const OUTPUT_LOW_OFFSET: usize = 48;
        const OUTPUT_HIGH_OFFSET: usize = 56;
        const INDEX_OFFSET: usize = 64;
        const SOURCE_OFFSET: usize = 72;
        const SUM_LOW_OFFSET: usize = 80;
        const SUM_HIGH_OFFSET: usize = 88;
        const DELTA_PTR_OFFSET: usize = 96;

        let bad_delta = self.fresh_label("xudt_group_delta_bad");
        let scan_source = self.fresh_label("xudt_group_delta_scan_source");
        let scan_loop = self.fresh_label("xudt_group_delta_scan_loop");
        let scan_done = self.fresh_label("xudt_group_delta_scan_done");
        let scan_failed = self.fresh_label("xudt_group_delta_scan_failed");
        let scan_malformed = self.fresh_label("xudt_group_delta_scan_malformed");
        let overflow = self.fresh_label("xudt_group_delta_overflow");
        let output_phase = self.fresh_label("xudt_group_delta_output_phase");
        let compare = self.fresh_label("xudt_group_delta_compare");
        let mismatch = self.fresh_label("xudt_group_delta_mismatch");
        let done = self.fresh_label("xudt_group_delta_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 104);
        self.emit(format!("beqz a0, {}", bad_delta));
        self.emit_stack_store("a0", DELTA_PTR_OFFSET);
        for offset in [INPUT_LOW_OFFSET, INPUT_HIGH_OFFSET, OUTPUT_LOW_OFFSET, OUTPUT_HIGH_OFFSET] {
            self.emit_stack_store("zero", offset);
        }

        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit_stack_store("t0", SOURCE_OFFSET);
        self.emit_sp_addi("t0", INPUT_LOW_OFFSET);
        self.emit_sp_addi("t1", INPUT_HIGH_OFFSET);
        self.emit(format!("j {}", scan_source));

        self.emit_label(&output_phase);
        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT));
        self.emit_stack_store("t0", SOURCE_OFFSET);
        self.emit_sp_addi("t0", OUTPUT_LOW_OFFSET);
        self.emit_sp_addi("t1", OUTPUT_HIGH_OFFSET);

        self.emit_label(&scan_source);
        self.emit_stack_store("t0", SUM_LOW_OFFSET);
        self.emit_stack_store("t1", SUM_HIGH_OFFSET);
        self.emit_stack_store("zero", INDEX_OFFSET);

        self.emit_label(&scan_loop);
        self.emit("li t0, 16");
        self.emit_stack_store("t0", SIZE_OFFSET);
        self.emit_sp_addi("a0", BUFFER_OFFSET);
        self.emit_sp_addi("a1", SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit_stack_load("a3", INDEX_OFFSET);
        self.emit_stack_load("a4", SOURCE_OFFSET);
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", scan_done));
        self.emit(format!("li t0, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", compare));
        self.emit(format!("j {}", scan_failed));

        self.emit_label(&scan_done);
        self.emit_stack_load("t0", SIZE_OFFSET);
        self.emit("li t1, 16");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", scan_malformed));
        self.emit_stack_load("t0", SUM_LOW_OFFSET);
        self.emit_stack_load("t1", SUM_HIGH_OFFSET);
        self.emit_stack_load("t2", 16);
        self.emit_stack_load("t3", 24);
        self.emit("ld t4, 0(t0)");
        self.emit("ld t5, 0(t1)");
        self.emit("add t6, t4, t2");
        self.emit("sltu t4, t6, t4");
        self.emit("add t5, t5, t3");
        self.emit("sltu t3, t5, t3");
        self.emit(format!("bnez t3, {}", overflow));
        self.emit("add t5, t5, t4");
        self.emit("sltu t4, t5, t4");
        self.emit(format!("bnez t4, {}", overflow));
        self.emit("sd t6, 0(t0)");
        self.emit("sd t5, 0(t1)");
        self.emit_stack_load("t0", INDEX_OFFSET);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", INDEX_OFFSET);
        self.emit(format!("j {}", scan_loop));

        self.emit_label(&compare);
        self.emit_stack_load("t0", SOURCE_OFFSET);
        self.emit(format!("li t1, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", output_phase));

        self.emit_stack_load("a0", DELTA_PTR_OFFSET);
        self.emit("ld t2, 0(a0)");
        self.emit("ld t3, 8(a0)");
        if minted {
            self.emit_stack_load("t0", INPUT_LOW_OFFSET);
            self.emit_stack_load("t1", INPUT_HIGH_OFFSET);
            self.emit_stack_load("t4", OUTPUT_LOW_OFFSET);
            self.emit_stack_load("t5", OUTPUT_HIGH_OFFSET);
        } else {
            self.emit_stack_load("t0", OUTPUT_LOW_OFFSET);
            self.emit_stack_load("t1", OUTPUT_HIGH_OFFSET);
            self.emit_stack_load("t4", INPUT_LOW_OFFSET);
            self.emit_stack_load("t5", INPUT_HIGH_OFFSET);
        }
        self.emit("add t6, t0, t2");
        self.emit("sltu t0, t6, t0");
        self.emit("add t1, t1, t3");
        self.emit("sltu t3, t1, t3");
        self.emit(format!("bnez t3, {}", overflow));
        self.emit("add t1, t1, t0");
        self.emit("sltu t0, t1, t0");
        self.emit(format!("bnez t0, {}", overflow));
        self.emit("sub t0, t6, t4");
        self.emit(format!("bnez t0, {}", mismatch));
        self.emit("sub t0, t1, t5");
        self.emit(format!("bnez t0, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&bad_delta);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&scan_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::XudtBindingMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&scan_malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::XudtBindingMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&overflow);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::AggregateAmountMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::AggregateAmountMismatch.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", 104);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
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

    fn emit_runtime_memcpy_fixed(&mut self) {
        self.emit_global("__cellscript_memcpy_fixed");
        self.emit_label("__cellscript_memcpy_fixed");
        self.emit("# cellscript abi: fixed-byte helper copies a0 to a1 for a2 bytes; returns a0=0");
        let loop_label = ".L__cellscript_memcpy_fixed_loop";
        let done_label = ".L__cellscript_memcpy_fixed_done";
        self.emit(format!("beqz a2, {}", done_label));
        self.emit_label(loop_label);
        self.emit("lbu t0, 0(a0)");
        self.emit("sb t0, 0(a1)");
        self.emit("addi a0, a0, 1");
        self.emit("addi a1, a1, 1");
        self.emit("addi a2, a2, -1");
        self.emit(format!("bnez a2, {}", loop_label));
        self.emit_label(done_label);
        self.emit("li a0, 0");
        self.emit("ret");
    }

    fn emit_runtime_size_guards(&mut self) {
        self.emit_global("__cellscript_require_min_size");
        self.emit_label("__cellscript_require_min_size");
        self.emit("# cellscript abi: returns a0=0 when actual size a0 is at least required size a1");
        self.emit("slt a0, a0, a1");
        self.emit("ret");

        self.emit_global("__cellscript_require_exact_size");
        self.emit_label("__cellscript_require_exact_size");
        self.emit("# cellscript abi: returns a0=0 when actual size a0 equals expected size a1");
        self.emit("sub a0, a0, a1");
        self.emit("ret");
    }

    fn emit_runtime_header_field_u64(&mut self, symbol: &str, field_name: &str, field_id: u64, enabled: bool, disabled_reason: &str) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        if !enabled {
            self.emit(format!("# cellscript abi: {}", disabled_reason));
            self.emit_process_failure(CellScriptRuntimeError::ConsumeInvalidOperand);
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
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_loaded_schema_exact_size_check(8, 8, "header scalar return");
        self.emit_stack_load("a0", 16);
        self.emit_stack_load("ra", 24);
        self.emit_large_addi("sp", "sp", 32);
        self.emit("ret");
    }

    fn emit_runtime_header_dep_field_u64(
        &mut self,
        symbol: &str,
        field_name: &str,
        field_id: u64,
        enabled: bool,
        disabled_reason: &str,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        if !enabled {
            self.emit(format!("# cellscript abi: {}", disabled_reason));
            self.emit_process_failure(CellScriptRuntimeError::ConsumeInvalidOperand);
            return;
        }

        let invalid = self.fresh_label("header_dep_view_invalid");
        let abi = self.runtime_abi();
        self.emit_large_addi("sp", "sp", -32);
        self.emit_stack_store("ra", 24);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", abi.source_header_dep));
        self.emit(format!("bne t2, t0, {}", invalid));
        self.emit(format!("# cellscript abi: LOAD_HEADER_BY_FIELD field={} source=HeaderDep index=SourceView", field_name));
        self.emit("li t0, 8");
        self.emit_stack_store("t0", 8);
        self.emit_sp_addi("a0", 16);
        self.emit_sp_addi("a1", 8);
        self.emit("li a2, 0");
        self.emit("mv a3, t1");
        self.emit("mv a4, t2");
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_header_by_field));
        self.emit("ecall");
        self.emit_return_on_syscall_error(CellScriptRuntimeError::HeaderDepMissing);
        self.emit_loaded_schema_exact_size_check(8, 8, "HeaderDep scalar return");
        self.emit_stack_load("a0", 16);
        self.emit_stack_load("ra", 24);
        self.emit_large_addi("sp", "sp", 32);
        self.emit("ret");

        self.emit_label(&invalid);
        self.emit_process_failure(CellScriptRuntimeError::CkbSourceViewInvalid);
    }

    fn emit_runtime_header_dep_full_u64(
        &mut self,
        symbol: &str,
        field_name: &str,
        field_offset: usize,
        enabled: bool,
        disabled_reason: &str,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        if !enabled {
            self.emit(format!("# cellscript abi: {disabled_reason}"));
            self.emit_process_failure(CellScriptRuntimeError::ConsumeInvalidOperand);
            return;
        }

        const SIZE_OFFSET: usize = 0;
        const HEADER_OFFSET: usize = 16;
        const FRAME_SIZE: usize = HEADER_OFFSET + ckb_abi::header::SERIALIZED_SIZE;
        const _: () = assert!(FRAME_SIZE.is_multiple_of(16));

        let invalid = self.fresh_label("header_dep_full_view_invalid");
        let malformed = self.fresh_label("header_dep_full_malformed");
        let abi = self.runtime_abi();
        self.emit_large_addi("sp", "sp", -(FRAME_SIZE as i64));
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", abi.source_header_dep));
        self.emit(format!("bne t2, t0, {invalid}"));
        self.emit(format!(
            "# cellscript abi: LOAD_HEADER exact Molecule Header size={} source=HeaderDep index=SourceView; {} offset={} width={}",
            ckb_abi::header::SERIALIZED_SIZE,
            field_name,
            field_offset,
            ckb_abi::header::SCALAR_WIDTH
        ));
        self.emit(format!("li t0, {}", ckb_abi::header::SERIALIZED_SIZE));
        self.emit_stack_store("t0", SIZE_OFFSET);
        self.emit_sp_addi("a0", HEADER_OFFSET);
        self.emit_sp_addi("a1", SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit("mv a3, t1");
        self.emit("mv a4, t2");
        self.emit(format!("li a7, {}", abi.load_header));
        self.emit("ecall");
        self.emit_return_on_syscall_error(CellScriptRuntimeError::HeaderDepMissing);
        self.emit_stack_load("t0", SIZE_OFFSET);
        self.emit(format!("li t1, {}", ckb_abi::header::SERIALIZED_SIZE));
        self.emit(format!("bne t0, t1, {malformed}"));
        self.emit_stack_load("a0", HEADER_OFFSET + field_offset);
        self.emit_large_addi("sp", "sp", FRAME_SIZE as i64);
        self.emit("ret");

        self.emit_label(&malformed);
        self.emit_process_failure(CellScriptRuntimeError::ExactSizeMismatch);
        self.emit_label(&invalid);
        self.emit_process_failure(CellScriptRuntimeError::CkbSourceViewInvalid);
    }

    fn emit_runtime_input_field_u64(&mut self, symbol: &str, field_name: &str, field_id: u64, enabled: bool, disabled_reason: &str) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        if !enabled {
            self.emit(format!("# cellscript abi: {}", disabled_reason));
            self.emit_process_failure(CellScriptRuntimeError::ConsumeInvalidOperand);
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
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_loaded_schema_exact_size_check(8, 8, "input scalar return");
        self.emit_stack_load("a0", 16);
        self.emit_stack_load("ra", 24);
        self.emit_large_addi("sp", "sp", 32);
        self.emit("ret");
    }
}

pub fn generate(ir: &IrModule, options: &CodegenOptions, format: ArtifactFormat) -> Result<Vec<u8>> {
    let generator = CodeGenerator::new(options.clone());
    generator.generate(ir, format)
}

pub fn generate_with_evidence(ir: &IrModule, options: &CodegenOptions, format: ArtifactFormat) -> Result<GeneratedArtifact> {
    let generator = CodeGenerator::new(options.clone());
    generator.generate_with_evidence(ir, format)
}

#[derive(Debug, Clone)]
pub struct GeneratedArtifact {
    pub bytes: Vec<u8>,
    pub machine_layout: Option<MachineLayoutEvidence>,
}

#[derive(Debug, Clone)]
pub struct MachineLayoutEvidence {
    pub text_start: u64,
    pub text_end: u64,
    pub entry_label: String,
    pub blocks: Vec<MachineBlockEvidence>,
    pub edges: Vec<MachineEdgeEvidence>,
    pub symbols: BTreeMap<String, u64>,
    /// Text labels in emission order, including labels sharing an address.
    /// A preceding empty IR block must not own the next block's instructions.
    pub symbol_order: Vec<String>,
    pub globals: BTreeSet<String>,
    pub entry_frame_sizes: BTreeMap<String, u32>,
    pub fixed_result_frames: BTreeMap<String, FixedResultFrameEvidence>,
}

#[derive(Debug, Clone)]
pub struct FixedResultFrameEvidence {
    pub scalar_region_bytes: u32,
    pub buffer_region_end: u32,
    pub buffers: Vec<cellscript_artifact_checker::FixedResultBuffer>,
}

#[derive(Debug, Clone)]
pub struct MachineBlockEvidence {
    pub index: usize,
    pub label: Option<String>,
    pub start: u64,
    pub end: u64,
    pub terminator: MachineTerminatorEvidence,
    pub runtime_error_codes: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineTerminatorEvidence {
    Fallthrough,
    Jump,
    ConditionalBranch,
    Return,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineEdgeEvidence {
    pub from: usize,
    pub to: usize,
    pub kind: MachineEdgeKindEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineEdgeKindEvidence {
    Fallthrough,
    Jump,
    ConditionalTaken,
    ConditionalFallthrough,
    Call,
}
