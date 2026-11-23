use super::super::{align_frame, CKB_INDEX_OUT_OF_BOUND};
use super::super::{
    CellScriptRuntimeError, CodeGenerator, LoadOp, StoreOp, CKB_LENGTH_NOT_ENOUGH, CKB_SOURCE_GROUP_FLAG, CKB_SOURCE_INPUT,
    CKB_SOURCE_OUTPUT, CKB_WITNESS_OWNER_ENTRY, CKB_WITNESS_OWNER_LOCK, CKB_WITNESS_OWNER_OUTPUT_TYPE,
};

impl CodeGenerator {
    pub(super) fn emit_runtime_witness_count_helper(&mut self, enabled: bool) {
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
        self.emit_add_immediate("a0", "sp", BUFFER);
        self.emit_add_immediate("a1", "sp", SIZE);
        self.emit("li a2, 0");
        self.emit_stack_load("a3", INDEX);
        self.emit(format!("li a4, {CKB_SOURCE_INPUT}"));
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {success}"));
        self.emit(format!("bnez a0, {failed}"));
        self.emit_stack_load("t0", INDEX);
        self.emit_add_immediate("t0", "t0", 1);
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

    pub(super) fn emit_runtime_bounded_witness_resolver(&mut self, enabled: bool) {
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
        self.emit_add_immediate("t2", "t2", -4);
        self.emit(format!("bne t1, t2, {malformed}"));
        self.emit_stack_load("t3", MAXIMUM);
        self.emit(format!("bltu t3, t1, {exceeded}"));
        self.emit_add_immediate("a0", "t4", 4);
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

    pub(super) fn emit_runtime_bounded_witness_size(&mut self, enabled: bool) {
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

    pub(super) fn emit_runtime_bounded_witness_word(&mut self, symbol: &str, width: usize, enabled: bool) {
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

    pub(super) fn emit_runtime_bounded_witness_blake2b(&mut self, enabled: bool) {
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

    pub(super) fn emit_runtime_witness_size_helper(&mut self, enabled: bool) {
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
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
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

    pub(super) fn emit_runtime_require_witness_size_at_least_helper(&mut self, enabled: bool) {
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
        self.emit_store(StoreOp::Double, "ra", "sp", RA_OFFSET);
        self.emit("# cellscript abi: preserve min_size before LOAD_WITNESS size probe");
        self.emit_store(StoreOp::Double, "a1", "sp", MIN_SIZE_OFFSET);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 0");
        self.emit_store(StoreOp::Double, "t0", "sp", SIZE_OFFSET);
        self.emit("li a0, 0");
        self.emit_add_immediate("a1", "sp", SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", status_ok));
        self.emit(format!("beqz a0, {}", status_ok));
        self.emit(format!("j {}", failed));

        self.emit_label(&status_ok);
        self.emit_load(LoadOp::Double, "t0", "sp", SIZE_OFFSET);
        self.emit_load(LoadOp::Double, "t1", "sp", MIN_SIZE_OFFSET);
        self.emit("sltu t2, t0, t1");
        self.emit(format!("beqz t2, {}", done));

        self.emit_label(&too_small);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::WitnessMalformed.code()));
        self.emit_load(LoadOp::Double, "ra", "sp", RA_OFFSET);
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit_load(LoadOp::Double, "ra", "sp", RA_OFFSET);
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");

        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit_load(LoadOp::Double, "ra", "sp", RA_OFFSET);
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");

        self.emit_label(&done);
        self.emit("li a0, 0");
        self.emit_load(LoadOp::Double, "ra", "sp", RA_OFFSET);
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_witness_args_exact32_helper(&mut self, symbol: &str, detail: &str, field_index: u64, enabled: bool) {
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
}
