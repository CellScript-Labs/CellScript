//! Exhaustive runtime helper dispatch, including the VM2 syscall transport.
use super::*;

impl CodeGenerator {
    pub(super) fn emit_runtime_helpers(&mut self, referenced_helpers: &BTreeSet<String>) {
        let enabled = self.options.target_profile.is_ckb();
        if referenced_helpers.contains("__ckb_require_instance_script_participant_from_args") {
            self.emit_runtime_instance_participant(enabled);
        }
        use runtime_contract::IpcHelper as Ipc;
        for &helper in Ipc::ALL {
            let name = helper.symbol();
            let (syscall, detail) = match helper {
                Ipc::CkbWait => (ckb_abi::syscall::WAIT, "wait for bounded verifier child"),
                Ipc::CkbProcessId => (ckb_abi::syscall::PROCESS_ID, "current process id"),
                Ipc::CkbPipe => (ckb_abi::syscall::PIPE, "create IPC pipe; returns status in a0, read fd in a1 and write fd in a2"),
                Ipc::CkbPipeWrite => (ckb_abi::syscall::WRITE, "write u64 payload to IPC pipe"),
                Ipc::CkbPipeRead => (ckb_abi::syscall::READ, "read u64 payload from IPC pipe"),
                Ipc::CkbInheritedFd => (ckb_abi::syscall::INHERITED_FDS, "resolve inherited fd"),
                Ipc::CkbClose => (ckb_abi::syscall::CLOSE, "close fd"),
            };
            if !referenced_helpers.contains(name) {
                continue;
            }
            self.emit_global(name);
            self.emit_label(name);
            self.emit(format!("# cellscript abi: CKB VM v2 syscall {} ({})", syscall, detail));
            match helper {
                _ if !enabled => self.emit_fail(CellScriptRuntimeError::SyscallFailed),
                Ipc::CkbPipe => {
                    self.emit_stack_adjustment(-32);
                    self.emit_stack_store("ra", 24);
                    self.emit_sp_addi("a0", 8);
                    self.emit(format!("li a7, {}", syscall));
                    self.emit("ecall");
                    let failed = self.fresh_label("ckb_pipe_failed");
                    let done = self.fresh_label("ckb_pipe_done");
                    self.emit(format!("bnez a0, {}", failed));
                    self.emit_stack_load("a1", 8);
                    self.emit_stack_load("a2", 16);
                    self.emit("li a0, 0");
                    self.emit(format!("j {}", done));
                    self.emit_label(&failed);
                    self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
                    self.emit_label(&done);
                    self.emit_stack_load("ra", 24);
                    self.emit_stack_adjustment(32);
                    self.emit("ret");
                }
                Ipc::CkbPipeWrite => {
                    self.emit_stack_adjustment(-32);
                    self.emit_stack_store("ra", 24);
                    self.emit_stack_store("a1", 8);
                    self.emit("li t0, 8");
                    self.emit_stack_store("t0", 16);
                    self.emit_sp_addi("a1", 8);
                    self.emit_sp_addi("a2", 16);
                    self.emit(format!("li a7, {}", syscall));
                    self.emit("ecall");
                    let done = self.fresh_label("ckb_pipe_write_done");
                    self.emit(format!("beqz a0, {}", done));
                    self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
                    self.emit_label(&done);
                    self.emit_stack_load("ra", 24);
                    self.emit_stack_adjustment(32);
                    self.emit("ret");
                }
                Ipc::CkbClose => {
                    self.emit(format!("li a7, {}", syscall));
                    self.emit("ecall");
                    let done = self.fresh_label("ckb_close_done");
                    self.emit(format!("beqz a0, {}", done));
                    self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
                    self.emit_label(&done);
                    self.emit("ret");
                }
                Ipc::CkbWait => {
                    self.emit_stack_adjustment(-16);
                    self.emit_stack_store("ra", 8);
                    self.emit_stack_store("zero", 0);
                    self.emit_sp_addi("a1", 0);
                    self.emit(format!("li a7, {}", syscall));
                    self.emit("ecall");
                    let failed = self.fresh_label("ckb_wait_failed");
                    let exit_ok = self.fresh_label("ckb_wait_exit_ok");
                    let child_failed = self.fresh_label("ckb_wait_child_failed");
                    let done = self.fresh_label("ckb_wait_done");
                    self.emit(format!("bnez a0, {}", failed));
                    self.emit_stack_load_byte("t0", 0);
                    self.emit(format!("beqz t0, {}", exit_ok));
                    self.emit_label(&child_failed);
                    self.emit("addi a0, t0, 0");
                    self.emit(format!("j {}", done));
                    self.emit_label(&failed);
                    self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
                    self.emit(format!("j {}", done));
                    self.emit_label(&exit_ok);
                    self.emit("li a0, 0");
                    self.emit_label(&done);
                    self.emit_stack_load("ra", 8);
                    self.emit_stack_adjustment(16);
                    self.emit("ret");
                }
                // READ takes (fd, buffer_addr, length_addr): the scheduler
                // writes the payload to the buffer and the delivered byte
                // count back through length_addr, with SUCCESS in a0. A
                // bare ecall would pass unprepared a1/a2 and return the
                // status word as if it were the payload.
                Ipc::CkbPipeRead => {
                    self.emit_stack_adjustment(-32);
                    self.emit_stack_store("ra", 24);
                    self.emit_stack_store("a0", 0);
                    self.emit("li t0, 8");
                    self.emit_stack_store("t0", 8);
                    self.emit_stack_load("a0", 0);
                    self.emit_sp_addi("a1", 16);
                    self.emit_sp_addi("a2", 8);
                    self.emit(format!("li a7, {}", syscall));
                    self.emit("ecall");
                    let failed = self.fresh_label("ckb_pipe_read_failed");
                    let done = self.fresh_label("ckb_pipe_read_done");
                    self.emit(format!("bnez a0, {}", failed));
                    self.emit_stack_load("t0", 8);
                    self.emit("li t1, 8");
                    self.emit(format!("bne t0, t1, {}", failed));
                    self.emit_stack_load("a0", 16);
                    self.emit(format!("j {}", done));
                    self.emit_label(&failed);
                    self.emit_process_failure(CellScriptRuntimeError::SyscallFailed);
                    self.emit_label(&done);
                    self.emit_stack_load("ra", 24);
                    self.emit_stack_adjustment(32);
                    self.emit("ret");
                }
                // INHERITED_FDS takes (buffer_addr, length_addr): the
                // scheduler fills the inherited fd list into the buffer,
                // the fd count through length_addr, and SUCCESS in a0.
                // The DSL builtin selects from a bounded prefix covering
                // all 64 distinct scheduler fd slots. Spawn can repeat an
                // fd in its list, so the reported count can exceed this
                // capacity: check both bounds before reading the buffer.
                Ipc::CkbInheritedFd => {
                    const FD_CAPACITY: usize = 64;
                    const FD_BUFFER: usize = 16;
                    const FRAME_BYTES: usize = align_frame(FD_BUFFER + FD_CAPACITY * 8 + 8);
                    self.emit_stack_adjustment(-(FRAME_BYTES as i64));
                    self.emit_stack_store("ra", FRAME_BYTES - 8);
                    self.emit_stack_store("a0", 0);
                    self.emit(format!("li t0, {FD_CAPACITY}"));
                    self.emit_stack_store("t0", 8);
                    self.emit_sp_addi("a0", FD_BUFFER);
                    self.emit_sp_addi("a1", 8);
                    self.emit(format!("li a7, {}", syscall));
                    self.emit("ecall");
                    let failed = self.fresh_label("ckb_inherited_fd_failed");
                    let done = self.fresh_label("ckb_inherited_fd_done");
                    self.emit(format!("bnez a0, {}", failed));
                    self.emit_stack_load("t0", 8);
                    self.emit_stack_load("t1", 0);
                    self.emit(format!("bgeu t1, t0, {}", failed));
                    self.emit(format!("li t2, {FD_CAPACITY}"));
                    self.emit(format!("bgeu t1, t2, {}", failed));
                    self.emit("slli t1, t1, 3");
                    self.emit_sp_addi("t2", FD_BUFFER);
                    self.emit("add t2, t2, t1");
                    self.emit("ld a0, 0(t2)");
                    self.emit(format!("j {}", done));
                    self.emit_label(&failed);
                    self.emit_process_failure(CellScriptRuntimeError::SyscallFailed);
                    self.emit_label(&done);
                    self.emit_stack_load("ra", FRAME_BYTES - 8);
                    self.emit_stack_adjustment(FRAME_BYTES as i64);
                    self.emit("ret");
                }
                // PROCESS_ID takes no arguments and
                // returns the pid in a0, so the direct syscall is exact.
                Ipc::CkbProcessId => {
                    self.emit(format!("li a7, {}", syscall));
                    self.emit("ecall");
                    self.emit("ret");
                }
            }
        }
        if referenced_helpers.contains("__ckb_spawn_with_fd1") {
            self.emit_global("__ckb_spawn_with_fd1");
            self.emit_label("__ckb_spawn_with_fd1");
            self.emit("# cellscript abi: CKB VM v2 spawn CellDep index a0/code with one inherited fd from a1");
            if !enabled {
                self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            } else {
                self.emit_stack_adjustment(-80);
                self.emit_stack_store("ra", 72);
                self.emit_stack_store("a1", 8);
                self.emit_stack_store("a0", 64);
                self.emit_stack_store("zero", 16);
                self.emit_stack_store("zero", 32);
                self.emit_stack_store("zero", 40);
                self.emit_sp_addi("t0", 24);
                self.emit_stack_store("t0", 48);
                self.emit_sp_addi("t0", 8);
                self.emit_stack_store("t0", 56);
                self.emit_stack_load("a0", 64);
                self.emit(format!("li a1, {}", ckb_abi::source::CELL_DEP));
                self.emit("li a2, 0");
                self.emit(format!("li a3, {}", ckb_abi::place::CELL));
                self.emit_sp_addi("a4", 32);
                self.emit(format!("li a7, {}", ckb_abi::syscall::SPAWN));
                self.emit("ecall");
                let failed = self.fresh_label("ckb_spawn_with_fd_failed");
                let done = self.fresh_label("ckb_spawn_with_fd_done");
                self.emit(format!("bnez a0, {}", failed));
                self.emit_stack_load("a1", 24);
                self.emit("li a0, 0");
                self.emit(format!("j {}", done));
                self.emit_label(&failed);
                self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
                self.emit_label(&done);
                self.emit_stack_load("ra", 72);
                self.emit_stack_adjustment(80);
                self.emit("ret");
            }
        }

        for (name, source_view, detail) in [
            ("__ckb_source_input", CKB_SOURCE_VIEW_INPUT, "Source::Input"),
            ("__ckb_source_output", CKB_SOURCE_VIEW_OUTPUT, "Source::Output"),
            ("__ckb_source_cell_dep", CKB_SOURCE_VIEW_CELL_DEP, "Source::CellDep"),
            ("__ckb_source_header_dep", CKB_SOURCE_VIEW_HEADER_DEP, "Source::HeaderDep"),
            ("__ckb_source_group_input", CKB_SOURCE_VIEW_GROUP_INPUT, "Source::GroupInput"),
            ("__ckb_source_group_output", CKB_SOURCE_VIEW_GROUP_OUTPUT, "Source::GroupOutput"),
        ] {
            if !referenced_helpers.contains(name) {
                continue;
            }
            self.emit_runtime_source_view_helper(name, source_view, detail, enabled);
        }

        for (name, relative, detail) in [
            ("__ckb_since_epoch_absolute", false, "CKB RFC0017 absolute epoch since encoder"),
            ("__ckb_since_epoch_relative", true, "CKB RFC0017 relative epoch since encoder"),
        ] {
            if !referenced_helpers.contains(name) {
                continue;
            }
            self.emit_runtime_ckb_since_epoch_helper(name, relative, detail, enabled);
        }

        for (name, relative, metric_flag, timestamp, detail) in [
            (
                "__ckb_since_block_absolute",
                false,
                CKB_SINCE_BLOCK_NUMBER_FLAG,
                false,
                "CKB RFC0017 absolute block-number since encoder",
            ),
            (
                "__ckb_since_block_relative",
                true,
                CKB_SINCE_BLOCK_NUMBER_FLAG,
                false,
                "CKB RFC0017 relative block-number since encoder",
            ),
            (
                "__ckb_since_timestamp_absolute",
                false,
                CKB_SINCE_TIMESTAMP_FLAG,
                true,
                "CKB RFC0017 absolute timestamp-seconds since encoder",
            ),
            (
                "__ckb_since_timestamp_relative",
                true,
                CKB_SINCE_TIMESTAMP_FLAG,
                true,
                "CKB RFC0017 relative timestamp-seconds since encoder",
            ),
        ] {
            if !referenced_helpers.contains(name) {
                continue;
            }
            self.emit_runtime_ckb_since_scalar_helper(name, relative, metric_flag, timestamp, detail, enabled);
        }

        if referenced_helpers.contains("__ckb_since_decode") {
            self.emit_runtime_ckb_since_decode_helper("__ckb_since_decode", enabled);
        }
        if referenced_helpers.contains("__ckb_since_from_raw_checked") {
            self.emit_runtime_ckb_since_decode_helper("__ckb_since_from_raw_checked", enabled);
        }

        for (name, relative, metric_flag, detail) in [
            ("__ckb_since_as_absolute_block", false, CKB_SINCE_BLOCK_NUMBER_FLAG, "absolute block-number"),
            ("__ckb_since_as_relative_block", true, CKB_SINCE_BLOCK_NUMBER_FLAG, "relative block-number"),
            ("__ckb_since_as_absolute_epoch", false, CKB_SINCE_EPOCH_NUMBER_WITH_FRACTION_FLAG, "absolute epoch-fraction"),
            ("__ckb_since_as_relative_epoch", true, CKB_SINCE_EPOCH_NUMBER_WITH_FRACTION_FLAG, "relative epoch-fraction"),
            ("__ckb_since_as_absolute_timestamp", false, CKB_SINCE_TIMESTAMP_FLAG, "absolute timestamp"),
            ("__ckb_since_as_relative_timestamp", true, CKB_SINCE_TIMESTAMP_FLAG, "relative timestamp"),
        ] {
            if !referenced_helpers.contains(name) {
                continue;
            }
            self.emit_runtime_ckb_since_narrow_helper(name, relative, metric_flag, detail, enabled);
        }

        for (name, operation, detail) in [
            ("__ckb_since_is_relative", "relative", "reads the validated RFC0017 relative flag"),
            ("__ckb_since_is_disabled", "disabled", "tests the RFC0017 disabled zero encoding"),
            ("__ckb_since_metric", "metric", "returns 0=block, 1=epoch fraction, or 2=timestamp"),
            ("__ckb_since_value", "value", "returns the validated low 56-bit RFC0017 value"),
        ] {
            if !referenced_helpers.contains(name) {
                continue;
            }
            self.emit_runtime_ckb_since_projection_helper(name, operation, detail, enabled);
        }

        for (name, operation, detail) in [
            ("__ckb_epoch_duration", "duration", "checked CKB epoch duration constructor"),
            ("__ckb_epoch_add", "add", "checked EpochNumber plus EpochDuration"),
            ("__ckb_epoch_sub", "sub", "checked EpochNumber minus EpochDuration"),
        ] {
            if !referenced_helpers.contains(name) {
                continue;
            }
            self.emit_runtime_ckb_epoch_arithmetic_helper(name, operation, detail, enabled);
        }

        let needs_c256_product = referenced_helpers.contains("__c256_require_u128_product_lte")
            || referenced_helpers.contains("__c256_require_u128_product_eq")
            || referenced_helpers.contains("__c256_require_u128_sum2_products_lte")
            || referenced_helpers.contains("__c256_require_u128_sum2_products_eq");
        let needs_c256_sum = referenced_helpers.contains("__c256_require_u128_sum2_products_lte")
            || referenced_helpers.contains("__c256_require_u128_sum2_products_eq");
        if needs_c256_product {
            self.emit_runtime_load_u64_le_helper();
            self.emit_runtime_mul_u128_to_u256_helper();
            if needs_c256_sum {
                self.emit_runtime_add_u256_helper();
            }
        }
        if referenced_helpers.contains("__ckb_require_lock_type_metapoint_pairs")
            || referenced_helpers.contains("__ckb_require_type_lock_metapoint_pairs")
            || referenced_helpers.contains("__ckb_require_lock_type_metapoint_pairs_from_i32_data")
            || referenced_helpers.contains("__ckb_require_type_lock_metapoint_pairs_from_i32_data")
            || referenced_helpers.contains("__ckb_require_lock_type_metapoint_pairs_from_i32_data_filtered")
            || referenced_helpers.contains("__ckb_require_type_lock_metapoint_pairs_from_i32_data_filtered")
            || referenced_helpers.contains("__ckb_require_lock_match_master_out_point_pairs_from_data")
        {
            self.emit_runtime_current_script_role_at_helper(enabled);
        }

        use runtime_contract::SurfaceHelper as S;
        for &helper in S::ALL {
            let name = helper.symbol();
            let detail = helper.detail();
            if !referenced_helpers.contains(name) {
                continue;
            }
            match helper {
                S::CkbCurrentRole => self.emit_runtime_current_role_helper(enabled),
                S::CkbTransactionU32Le => self.emit_runtime_transaction_u32(enabled),
                S::CkbWitnessBytes32 => self.emit_runtime_witness_bytes32(enabled),
                S::CkbTransactionBlake2bGather => self.emit_runtime_gather_hash(enabled, false),
                S::CkbWitnessBlake2bSelectChunks => self.emit_runtime_gather_hash(enabled, true),
                S::CkbCurrentScriptHash => self.emit_runtime_current_script_hash_helper(enabled),
                S::CkbTransactionHash => self.emit_runtime_transaction_hash_helper(enabled),
                S::CkbScriptHash => self.emit_runtime_script_hash_helper(enabled),
                S::CkbSighashAllZeroLock => self.emit_runtime_sighash_all_zero_lock(enabled),
                S::CkbSinceToRaw
                | S::CkbEpochNumberToU64
                | S::CkbEpochDurationToU64
                | S::CkbBlockNumberToU64
                | S::CkbEpochLengthToU64
                | S::CkbTimestampMillisToU64 => self.emit_runtime_ckb_temporal_to_raw(name, detail),
                S::CkbExecCellDepU8Args => self.emit_runtime_exec_cell_dep_u8_args(enabled),
                S::CkbExecCellDepHex4 => self.emit_runtime_cell_dep_hex4(enabled, false),
                S::CkbSpawnWaitCellDepHex4 => self.emit_runtime_cell_dep_hex4(enabled, true),
                S::CkbCellCapacity => {
                    self.emit_runtime_cell_field_u64_helper(name, detail, CKB_CELL_FIELD_CAPACITY, enabled);
                }
                S::CkbCellOccupiedCapacity => self.emit_runtime_cell_occupied_capacity_helper(enabled),
                S::CkbCellUnoccupiedCapacity => self.emit_runtime_cell_unoccupied_capacity_helper(enabled),
                S::CkbCellOutputIndex => self.emit_runtime_cell_output_index_helper(enabled),
                S::CkbInputOutPointIndex => self.emit_runtime_input_out_point_word_helper(name, detail, 32, 4, enabled),
                S::CkbInputOutPointTxHashLow => self.emit_runtime_input_out_point_word_helper(name, detail, 0, 8, enabled),
                S::CkbInputOutPointTxHash => self.emit_runtime_input_out_point_tx_hash_helper(enabled),
                S::CkbRequireInputOutPointTxHash => self.emit_runtime_input_out_point_tx_hash_requirement_helper(enabled),
                S::CkbRequireInputOutPoint => self.emit_runtime_input_out_point_requirement_helper(enabled),
                S::CkbRequireMetapointRelative => self.emit_runtime_metapoint_relative_requirement_helper(enabled),
                S::CkbRequireLockTypeMetapointPairs => {
                    self.emit_runtime_metapoint_pair_cardinality_helper(name, detail, true, false, false, enabled)
                }
                S::CkbRequireTypeLockMetapointPairs => {
                    self.emit_runtime_metapoint_pair_cardinality_helper(name, detail, false, false, false, enabled)
                }
                S::CkbRequireLockTypeMetapointPairsFromI32Data => {
                    self.emit_runtime_metapoint_pair_cardinality_helper(name, detail, true, true, false, enabled)
                }
                S::CkbRequireTypeLockMetapointPairsFromI32Data => {
                    self.emit_runtime_metapoint_pair_cardinality_helper(name, detail, false, true, false, enabled)
                }
                S::CkbRequireLockTypeMetapointPairsFromI32DataFiltered => {
                    self.emit_runtime_metapoint_pair_cardinality_helper(name, detail, true, true, true, enabled)
                }
                S::CkbRequireTypeLockMetapointPairsFromI32DataFiltered => {
                    self.emit_runtime_metapoint_pair_cardinality_helper(name, detail, false, true, true, enabled)
                }
                S::CkbRequireLockMatchMasterOutPointPairsFromData => {
                    self.emit_runtime_lock_match_master_out_point_pairs_from_data_helper(enabled)
                }
                S::CkbCellLockHashLow => {
                    self.emit_runtime_cell_field_low_word_helper(name, detail, CKB_CELL_FIELD_LOCK_HASH, enabled);
                }
                S::CkbCellTypeHashLow => {
                    self.emit_runtime_cell_field_low_word_helper(name, detail, CKB_CELL_FIELD_TYPE_HASH, enabled);
                }
                S::CkbCellLockHash => {
                    self.emit_runtime_cell_hash_field_helper(name, detail, CKB_CELL_FIELD_LOCK_HASH, enabled);
                }
                S::CkbCellTypeHash => {
                    self.emit_runtime_cell_hash_field_helper(name, detail, CKB_CELL_FIELD_TYPE_HASH, enabled);
                }
                S::CkbCellDataHashField => {
                    self.emit_runtime_cell_hash_field_helper(name, detail, CKB_CELL_FIELD_DATA_HASH, enabled);
                }
                S::CkbCellDataBytes32 => {
                    self.emit_runtime_cell_data_bytes32_helper(name, detail, enabled);
                }
                S::CkbCellDataBlake2bSpan => self.emit_runtime_span_hash_helper(name, RuntimeByteSource::CellData, enabled),
                S::CkbWitnessBlake2bSpan => self.emit_runtime_span_hash_helper(name, RuntimeByteSource::Witness, enabled),
                S::CkbRawTransactionHashWithoutCellDeps => self.emit_runtime_raw_transaction_hash_without_cell_deps(enabled),
                S::CkbCellLockCodeHash => {
                    self.emit_runtime_cell_script_hash_field_helper(
                        name,
                        detail,
                        CKB_CELL_FIELD_LOCK,
                        ScriptHashFieldRead::CodeHash,
                        enabled,
                    );
                }
                S::CkbCellTypeCodeHash => {
                    self.emit_runtime_cell_script_hash_field_helper(
                        name,
                        detail,
                        CKB_CELL_FIELD_TYPE,
                        ScriptHashFieldRead::CodeHash,
                        enabled,
                    );
                }
                S::CkbCellLockArgs32 => {
                    self.emit_runtime_cell_script_hash_field_helper(
                        name,
                        detail,
                        CKB_CELL_FIELD_LOCK,
                        ScriptHashFieldRead::Args32,
                        enabled,
                    );
                }
                S::CkbCellTypeArgs32 => {
                    self.emit_runtime_cell_script_hash_field_helper(
                        name,
                        detail,
                        CKB_CELL_FIELD_TYPE,
                        ScriptHashFieldRead::Args32,
                        enabled,
                    );
                }
                S::CkbCellLockHashType => {
                    self.emit_runtime_cell_script_scalar_field_helper(
                        name,
                        detail,
                        CKB_CELL_FIELD_LOCK,
                        ScriptScalarFieldRead::HashType,
                        enabled,
                    );
                }
                S::CkbCellTypeHashType => {
                    self.emit_runtime_cell_script_scalar_field_helper(
                        name,
                        detail,
                        CKB_CELL_FIELD_TYPE,
                        ScriptScalarFieldRead::HashType,
                        enabled,
                    );
                }
                S::CkbCellLockArgsEmpty => {
                    self.emit_runtime_cell_script_scalar_field_helper(
                        name,
                        detail,
                        CKB_CELL_FIELD_LOCK,
                        ScriptScalarFieldRead::ArgsEmpty,
                        enabled,
                    );
                }
                S::CkbCellTypeArgsEmpty => {
                    self.emit_runtime_cell_script_scalar_field_helper(
                        name,
                        detail,
                        CKB_CELL_FIELD_TYPE,
                        ScriptScalarFieldRead::ArgsEmpty,
                        enabled,
                    );
                }
                S::CkbRequireCellLockHash => self.emit_runtime_cell_hash_requirement_helper(
                    name,
                    detail,
                    CKB_CELL_FIELD_LOCK_HASH,
                    CellScriptRuntimeError::ScriptRoleMismatch,
                    enabled,
                ),
                S::CkbRequireCellTypeHash => self.emit_runtime_cell_hash_requirement_helper(
                    name,
                    detail,
                    CKB_CELL_FIELD_TYPE_HASH,
                    CellScriptRuntimeError::TypeHashMismatch,
                    enabled,
                ),
                S::CkbRequireCellLockExactHandle => self.emit_runtime_exact_script_handle_requirement_helper(
                    name,
                    detail,
                    CKB_CELL_FIELD_LOCK_HASH,
                    crate::script_handle_contract::EXACT_SCRIPT_HANDLE_CLASS_SCRIPT,
                    crate::script_handle_contract::EXACT_SCRIPT_HANDLE_ROLE_LOCK,
                    crate::script_handle_contract::EXACT_SCRIPT_HANDLE_SCRIPT_HASH_OFFSET,
                    enabled,
                ),
                S::CkbRequireCellTypeExactHandle => self.emit_runtime_exact_script_handle_requirement_helper(
                    name,
                    detail,
                    CKB_CELL_FIELD_TYPE_HASH,
                    crate::script_handle_contract::EXACT_SCRIPT_HANDLE_CLASS_SCRIPT,
                    crate::script_handle_contract::EXACT_SCRIPT_HANDLE_ROLE_TYPE,
                    crate::script_handle_contract::EXACT_SCRIPT_HANDLE_SCRIPT_HASH_OFFSET,
                    enabled,
                ),
                S::CkbRequireCellDepExactVerifierHandle => self.emit_runtime_exact_script_handle_requirement_helper(
                    name,
                    detail,
                    CKB_CELL_FIELD_DATA_HASH,
                    crate::script_handle_contract::EXACT_SCRIPT_HANDLE_CLASS_VERIFIER,
                    crate::script_handle_contract::EXACT_SCRIPT_HANDLE_ROLE_SPAWNED_VERIFIER,
                    crate::script_handle_contract::EXACT_SCRIPT_HANDLE_ARTIFACT_HASH_OFFSET,
                    enabled,
                ),
                S::CkbRequireCellDepOpenScriptHandle => self.emit_runtime_open_handle_requirement_helper(name, detail, true, enabled),
                S::CkbRequireCellDepOpenVerifierHandle => {
                    self.emit_runtime_open_handle_requirement_helper(name, detail, false, enabled)
                }
                S::CkbRequireCellDepOpenScriptHandleFromArgs | S::CkbRequireOpenScriptParticipantFromArgs => {
                    self.emit_runtime_open_handle_from_args_requirement_helper(name, detail, true, enabled)
                }
                S::CkbRequireCellDepOpenVerifierHandleFromArgs => {
                    self.emit_runtime_open_handle_from_args_requirement_helper(name, detail, false, enabled)
                }
                S::CkbRequireCellLockDeploymentLineHandle => self.emit_runtime_deployment_line_handle_requirement_helper(
                    name,
                    detail,
                    Some(CKB_CELL_FIELD_LOCK_HASH),
                    crate::script_handle_contract::EXACT_SCRIPT_HANDLE_CLASS_SCRIPT,
                    crate::script_handle_contract::EXACT_SCRIPT_HANDLE_ROLE_LOCK,
                    enabled,
                ),
                S::CkbRequireCellTypeDeploymentLineHandle => self.emit_runtime_deployment_line_handle_requirement_helper(
                    name,
                    detail,
                    Some(CKB_CELL_FIELD_TYPE_HASH),
                    crate::script_handle_contract::EXACT_SCRIPT_HANDLE_CLASS_SCRIPT,
                    crate::script_handle_contract::EXACT_SCRIPT_HANDLE_ROLE_TYPE,
                    enabled,
                ),
                S::CkbRequireCellDepDeploymentLineVerifierHandle => self.emit_runtime_deployment_line_handle_requirement_helper(
                    name,
                    detail,
                    None,
                    crate::script_handle_contract::EXACT_SCRIPT_HANDLE_CLASS_VERIFIER,
                    crate::script_handle_contract::EXACT_SCRIPT_HANDLE_ROLE_SPAWNED_VERIFIER,
                    enabled,
                ),
                S::CkbRequireCellDataHash => self.emit_runtime_cell_hash_requirement_helper(
                    name,
                    detail,
                    CKB_CELL_FIELD_DATA_HASH,
                    CellScriptRuntimeError::ScriptIdentityMismatch,
                    enabled,
                ),
                S::CkbRequireBoundedCellDepDataHash => self.emit_runtime_bounded_cell_dep_data_hash_requirement_helper(enabled),
                S::CkbRequireCurrentScriptArgsEmpty => self.emit_runtime_current_script_args_empty_requirement_helper(enabled),
                S::CkbRequireCellLockArgsEmpty => {
                    self.emit_runtime_cell_script_args_empty_requirement_helper(name, detail, CKB_CELL_FIELD_LOCK, enabled)
                }
                S::CkbRequireCellTypeArgsEmpty => {
                    self.emit_runtime_cell_script_args_empty_requirement_helper(name, detail, CKB_CELL_FIELD_TYPE, enabled)
                }
                S::CkbRequireCellLockArgs32 => self.emit_runtime_cell_script_args_bytes32_requirement_helper(
                    name,
                    detail,
                    CKB_CELL_FIELD_LOCK,
                    ScriptArgsBytes32RequirementMode::Exact32,
                    enabled,
                ),
                S::CkbRequireCellTypeArgs32 => self.emit_runtime_cell_script_args_bytes32_requirement_helper(
                    name,
                    detail,
                    CKB_CELL_FIELD_TYPE,
                    ScriptArgsBytes32RequirementMode::Exact32,
                    enabled,
                ),
                S::CkbRequireCellLockArgsExact => {
                    self.emit_runtime_cell_script_args_exact_requirement_helper(name, detail, CKB_CELL_FIELD_LOCK, enabled)
                }
                S::CkbRequireCellTypeArgsExact => {
                    self.emit_runtime_cell_script_args_exact_requirement_helper(name, detail, CKB_CELL_FIELD_TYPE, enabled)
                }
                S::CkbRequireCellLockArgsPrefix32 => self.emit_runtime_cell_script_args_bytes32_requirement_helper(
                    name,
                    detail,
                    CKB_CELL_FIELD_LOCK,
                    ScriptArgsBytes32RequirementMode::Prefix32,
                    enabled,
                ),
                S::CkbRequireCellTypeArgsPrefix32 => self.emit_runtime_cell_script_args_bytes32_requirement_helper(
                    name,
                    detail,
                    CKB_CELL_FIELD_TYPE,
                    ScriptArgsBytes32RequirementMode::Prefix32,
                    enabled,
                ),
                S::CkbRequireCellLockArgsSuffix32 => self.emit_runtime_cell_script_args_bytes32_requirement_helper(
                    name,
                    detail,
                    CKB_CELL_FIELD_LOCK,
                    ScriptArgsBytes32RequirementMode::Suffix32,
                    enabled,
                ),
                S::CkbRequireCellTypeArgsSuffix32 => self.emit_runtime_cell_script_args_bytes32_requirement_helper(
                    name,
                    detail,
                    CKB_CELL_FIELD_TYPE,
                    ScriptArgsBytes32RequirementMode::Suffix32,
                    enabled,
                ),
                S::CkbRequireCellLockScriptHashType => {
                    self.emit_runtime_cell_script_hash_type_requirement_helper(name, detail, CKB_CELL_FIELD_LOCK, enabled)
                }
                S::CkbRequireCellTypeScriptHashType => {
                    self.emit_runtime_cell_script_hash_type_requirement_helper(name, detail, CKB_CELL_FIELD_TYPE, enabled)
                }
                S::C256RequireU128ProductLte => self.emit_runtime_c256_product_requirement_helper(name, detail, false),
                S::C256RequireU128ProductEq => self.emit_runtime_c256_product_requirement_helper(name, detail, true),
                S::C256RequireU128Sum2ProductsLte => self.emit_runtime_c256_sum2_product_requirement_helper(name, detail, false),
                S::C256RequireU128Sum2ProductsEq => self.emit_runtime_c256_sum2_product_requirement_helper(name, detail, true),
                S::CkbCellDataSize => self.emit_runtime_cell_data_size_helper(enabled),
                S::CkbCellDataEqual => self.emit_runtime_cell_data_equal_helper(enabled),
                S::CkbSourceBytesEqual => self.emit_runtime_source_bytes_equal_helper(enabled),
                S::CkbSourceBytesEqualMemory => self.emit_runtime_source_bytes_equal_memory_helper(enabled),
                S::CkbSourceBytesZero => self.emit_runtime_source_bytes_zero_helper(enabled),
                S::CkbCellCount => self.emit_runtime_cell_probe_helper(name, true, enabled),
                S::CkbCellHasType => self.emit_runtime_cell_probe_helper(name, false, enabled),
                S::CkbCellDataU8 => self.emit_runtime_exact_byte_word_helper(name, RuntimeByteSource::CellData, 1, enabled),
                S::CkbCellLockSize => self.emit_runtime_cell_script_read(name, CKB_CELL_FIELD_LOCK, false, enabled),
                S::CkbCellTypeSize => self.emit_runtime_cell_script_read(name, CKB_CELL_FIELD_TYPE, false, enabled),
                S::CkbCellLockU8 => self.emit_runtime_exact_byte_word_helper(name, RuntimeByteSource::CellLock, 1, enabled),
                S::CkbCellTypeU8 => self.emit_runtime_exact_byte_word_helper(name, RuntimeByteSource::CellType, 1, enabled),
                S::CkbInputSinceAt => self.emit_runtime_input_since_at(enabled),
                S::CkbCellDataU32Le => self.emit_runtime_exact_byte_word_helper(name, RuntimeByteSource::CellData, 4, enabled),
                S::CkbCellDataU64Le => self.emit_runtime_exact_byte_word_helper(name, RuntimeByteSource::CellData, 8, enabled),
                S::DaoAccumulatedRate => self.emit_runtime_dao_accumulated_rate_helper(enabled),
                S::DaoInputAccumulatedRate => self.emit_runtime_dao_input_accumulated_rate_helper(enabled),
                S::DaoHasDaoType => self.emit_runtime_dao_type_classifier_helper(enabled),
                S::DaoIsDepositData => self.emit_runtime_dao_cell_data_classifier_helper(name, detail, true, enabled),
                S::DaoIsWithdrawalRequestData => {
                    self.emit_runtime_dao_cell_data_classifier_helper(name, detail, false, enabled);
                }
                S::DaoRequireHeaderDepForInput => self.emit_runtime_dao_require_header_dep_for_input_helper(enabled),
                S::DaoRequireInputSinceAtLeast => self.emit_runtime_dao_require_input_since_at_least_helper(enabled),
                S::DaoRequireInputRelativeEpochSinceAtLeast => {
                    self.emit_runtime_dao_require_input_relative_epoch_since_at_least_helper(enabled);
                }
                S::XudtAmountLow => self.emit_runtime_xudt_amount_word_helper(name, detail, 0, enabled),
                S::XudtAmountHigh => self.emit_runtime_xudt_amount_word_helper(name, detail, 8, enabled),
                S::XudtOwnerModeInputTypeHash => {
                    self.emit_runtime_cell_field_low_word_helper(name, detail, CKB_CELL_FIELD_TYPE_HASH, enabled);
                }
                S::XudtRequireOwnerModeInputType => self.emit_runtime_xudt_require_owner_mode_input_type_helper(enabled),
                S::XudtRequireOwnerModeTypeArgs => self.emit_runtime_xudt_require_owner_mode_type_args_helper(enabled),
                S::XudtRequireOwnerModeTypeArgsCurrentScript => {
                    self.emit_runtime_xudt_require_owner_mode_type_args_current_script_helper(enabled)
                }
                S::CellscriptRequireFungibleTypeGroupV1 | S::XudtRequireGroupAmountConserved => {
                    self.emit_runtime_fungible_type_group_conservation_helper(name, detail, enabled)
                }
                S::XudtRequireGroupAmountMinted => {
                    self.emit_runtime_xudt_require_group_amount_delta_helper(name, true, enabled);
                }
                S::XudtRequireGroupAmountBurned => {
                    self.emit_runtime_xudt_require_group_amount_delta_helper(name, false, enabled);
                }
                S::CkbWitnessSize => self.emit_runtime_witness_size_helper(enabled),
                S::CkbWitnessCount => self.emit_runtime_witness_count_helper(enabled),
                S::CkbWitnessU8 => self.emit_runtime_exact_byte_word_helper(name, RuntimeByteSource::Witness, 1, enabled),
                S::CkbWitnessU32Le => self.emit_runtime_exact_byte_word_helper(name, RuntimeByteSource::Witness, 4, enabled),
                S::CkbWitnessU64Le => self.emit_runtime_exact_byte_word_helper(name, RuntimeByteSource::Witness, 8, enabled),
                S::CkbRequireWitnessSizeAtLeast => self.emit_runtime_require_witness_size_at_least_helper(enabled),
                S::CkbWitnessLockExact32 => self.emit_runtime_witness_args_exact32_helper(name, detail, 0, enabled),
                S::CkbWitnessInputTypeExact32 => self.emit_runtime_witness_args_exact32_helper(name, detail, 1, enabled),
                S::CkbWitnessOutputTypeExact32 => self.emit_runtime_witness_args_exact32_helper(name, detail, 2, enabled),
                S::CkbWitnessBoundedSize => self.emit_runtime_bounded_witness_size(enabled),
                S::CkbWitnessBoundedU8 => self.emit_runtime_bounded_witness_word(name, 1, enabled),
                S::CkbWitnessBoundedU32Le => self.emit_runtime_bounded_witness_word(name, 4, enabled),
                S::CkbWitnessBoundedU64Le => self.emit_runtime_bounded_witness_word(name, 8, enabled),
                S::CkbWitnessBoundedBlake2b => self.emit_runtime_bounded_witness_blake2b(enabled),
                S::CkbSighashAll
                | S::CkbRequireMaturity
                | S::CkbRequireTime
                | S::CkbRequireEpochAfter
                | S::CkbRequireEpochRelative
                | S::CkbOccupiedCapacity => {
                    let Some(deferred) = crate::ir::IrDeferredRuntimeFeature::from_helper(name) else {
                        self.emission_error.get_or_insert_with(|| format!("runtime helper '{name}' has no named deferred failure"));
                        continue;
                    };
                    self.emit_global(name);
                    self.emit_label(name);
                    self.emit_process_failure(deferred.runtime_error());
                }
            }
        }

        if enabled && referenced_helpers.iter().any(|helper| is_cached_exact_read_helper(helper)) {
            self.emit_runtime_exact_read_cached();
        }

        let needs_blake2b_compress = referenced_helpers.iter().any(|helper| {
            matches!(
                helper.as_str(),
                "__ckb_hash_chain"
                    | "__ckb_hash_blake2b"
                    | "__ckb_hash_pair"
                    | "__ckb_hash_data_packed"
                    | "__ckb_hash_blake2b_var"
                    | "__ckb_hash_blake2b_packed"
                    | "__ckb_script_hash"
                    | "__ckb_cell_data_blake2b_span"
                    | "__ckb_witness_blake2b_span"
                    | "__ckb_witness_bounded_blake2b"
                    | "__ckb_raw_transaction_hash_without_cell_deps"
                    | "__ckb_transaction_blake2b_gather"
                    | "__ckb_witness_blake2b_select_chunks"
                    | "__ckb_sighash_all_zero_lock"
                    | "__ckb_require_cell_lock_exact_handle"
                    | "__ckb_require_cell_type_exact_handle"
                    | "__ckb_require_cell_dep_exact_verifier_handle"
                    | "__ckb_require_cell_dep_open_script_handle"
                    | "__ckb_require_cell_dep_open_verifier_handle"
                    | "__ckb_require_cell_dep_open_script_handle_from_args"
                    | "__ckb_require_open_script_participant_from_args"
                    | "__ckb_require_cell_dep_open_verifier_handle_from_args"
                    | "__ckb_require_cell_lock_deployment_line_handle"
                    | "__ckb_require_cell_type_deployment_line_handle"
                    | "__ckb_require_cell_dep_deployment_line_verifier_handle"
            )
        });
        if enabled && needs_blake2b_compress {
            self.emit_runtime_blake2b_compress();
        }

        if referenced_helpers.contains("__ckb_hash_chain") {
            self.emit_global("__ckb_hash_chain");
            self.emit_label("__ckb_hash_chain");
            self.emit("# cellscript abi: hash_chain aliases CKB Blake2b-256 over one 32-byte Hash input");
            if !enabled {
                self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            } else {
                self.emit("j __ckb_hash_blake2b");
            }
        }
        if referenced_helpers.contains("__ckb_hash_pair") {
            self.emit_runtime_blake2b_hash_pair(enabled);
        }
        if referenced_helpers.contains("__ckb_hash_chain")
            || referenced_helpers.contains("__ckb_hash_blake2b")
            || referenced_helpers.contains("__ckb_hash_data_packed")
        {
            self.emit_runtime_blake2b_hash32(enabled);
        }
        if referenced_helpers.contains("__ckb_hash_blake2b_var")
            || referenced_helpers.contains("__ckb_script_hash")
            || referenced_helpers.contains("__ckb_hash_data_packed")
            || referenced_helpers.contains("__ckb_hash_blake2b_packed")
            || referenced_helpers.contains("__ckb_require_cell_lock_exact_handle")
            || referenced_helpers.contains("__ckb_require_cell_type_exact_handle")
            || referenced_helpers.contains("__ckb_require_cell_dep_exact_verifier_handle")
            || referenced_helpers.contains("__ckb_require_cell_dep_open_script_handle")
            || referenced_helpers.contains("__ckb_require_cell_dep_open_verifier_handle")
            || referenced_helpers.contains("__ckb_require_cell_dep_open_script_handle_from_args")
            || referenced_helpers.contains("__ckb_require_open_script_participant_from_args")
            || referenced_helpers.contains("__ckb_require_cell_dep_open_verifier_handle_from_args")
            || referenced_helpers.contains("__ckb_require_cell_lock_deployment_line_handle")
            || referenced_helpers.contains("__ckb_require_cell_type_deployment_line_handle")
            || referenced_helpers.contains("__ckb_require_cell_dep_deployment_line_verifier_handle")
        {
            self.emit_runtime_blake2b_hash_var(enabled, RuntimeHashInput::Memory);
        }
        if referenced_helpers.contains("__ckb_cell_data_blake2b_span")
            || referenced_helpers.contains("__ckb_witness_blake2b_span")
            || referenced_helpers.contains("__ckb_witness_bounded_blake2b")
        {
            self.emit_runtime_blake2b_hash_var(enabled, RuntimeHashInput::Transaction);
        }
        if referenced_helpers.iter().any(|helper| {
            helper.starts_with("__ckb_witness_bounded_")
                || matches!(
                    helper.as_str(),
                    "__ckb_witness_lock_exact32"
                        | "__ckb_witness_input_type_exact32"
                        | "__ckb_witness_output_type_exact32"
                        | "__ckb_sighash_all_zero_lock"
                )
        }) {
            self.emit_runtime_bounded_witness_resolver(enabled);
        }
        if referenced_helpers.contains("__ckb_raw_transaction_hash_without_cell_deps") {
            self.emit_runtime_blake2b_hash_var(enabled, RuntimeHashInput::PrefixedTransaction);
        }
        if referenced_helpers.contains("__ckb_transaction_blake2b_gather")
            || referenced_helpers.contains("__ckb_witness_blake2b_select_chunks")
            || referenced_helpers.contains("__ckb_sighash_all_zero_lock")
        {
            self.emit_runtime_blake2b_hash_var(enabled, RuntimeHashInput::Segments);
        }
        if referenced_helpers.iter().any(|helper| {
            matches!(
                helper.as_str(),
                "__ckb_hash_sha256"
                    | "__ckb_hash_sha256d"
                    | "__ckb_hash_sha256_pair"
                    | "__ckb_hash_sha256d_pair"
                    | "__ckb_require_sha256d_merkle_root"
            )
        }) {
            self.emit_runtime_sha256_surface(enabled);
        }
    }
}
