use super::super::{align_frame, CKB_CELL_FIELD_LOCK, CKB_CELL_FIELD_TYPE, CKB_CELL_FIELD_TYPE_HASH, CKB_ITEM_MISSING};
use super::super::{
    cell_scan, CellScriptRuntimeError, CodeGenerator, LoadOp, StoreOp, CKB_CELL_FIELD_DATA_HASH, CKB_CELL_FIELD_OCCUPIED_CAPACITY,
    CKB_INDEX_OUT_OF_BOUND, CKB_LENGTH_NOT_ENOUGH, CKB_ROLE_UNKNOWN, CKB_SOURCE_CELL_DEP, CKB_SOURCE_GROUP_FLAG, CKB_SOURCE_INPUT,
    CKB_SOURCE_OUTPUT, CKB_SOURCE_VIEW_GROUP_OUTPUT, CKB_SOURCE_VIEW_OUTPUT, CKB_SOURCE_VIEW_SHIFT,
};

impl CodeGenerator {
    pub(super) fn emit_runtime_current_role_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_current_role");
        self.emit_label("__ckb_current_role");
        self.emit("# cellscript abi: current role helper; normal lowering folds role to a compile-time lock/type constant");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit_add_immediate("a1", "a0", 0);
        } else {
            self.emit(format!("li a0, {}", CKB_ROLE_UNKNOWN));
            self.emit("li a1, 0");
        }
        self.emit("ret");
    }

    pub(super) fn emit_runtime_cell_occupied_capacity_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(48) as i64;
        self.emit_global("__ckb_cell_occupied_capacity");
        self.emit_label("__ckb_cell_occupied_capacity");
        self.emit("# cellscript abi: CKB occupied capacity via LOAD_CELL_BY_FIELD CellField::OccupiedCapacity");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit_add_immediate("a1", "a0", 0);
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("occupied_capacity_source_invalid");
        let failed = self.fresh_label("occupied_capacity_load_failed");
        let malformed = self.fresh_label("occupied_capacity_field_malformed");
        let done = self.fresh_label("occupied_capacity_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_store(StoreOp::Double, "ra", "sp", 40);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit_store(StoreOp::Double, "t1", "sp", 0);
        self.emit_store(StoreOp::Double, "t2", "sp", 8);
        self.emit("li t0, 8");
        self.emit_store(StoreOp::Double, "t0", "sp", 16);
        self.emit_add_immediate("a0", "sp", 24);
        self.emit_add_immediate("a1", "sp", 16);
        self.emit("li a2, 0");
        self.emit_load(LoadOp::Double, "a3", "sp", 0);
        self.emit_load(LoadOp::Double, "a4", "sp", 8);
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_OCCUPIED_CAPACITY));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit_load(LoadOp::Double, "t0", "sp", 16);
        self.emit(format!("bnez a0, {}", failed));
        self.emit("li t1, 8");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit_load(LoadOp::Double, "a0", "sp", 24);
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit_add_immediate("a1", "a0", 0);
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit_add_immediate("a1", "a0", 0);
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptFieldMalformed.code()));
        self.emit_add_immediate("a1", "a0", 0);
        self.emit_label(&done);
        self.emit_load(LoadOp::Double, "ra", "sp", 40);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_cell_unoccupied_capacity_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(32) as i64;
        self.emit_global("__ckb_cell_unoccupied_capacity");
        self.emit_label("__ckb_cell_unoccupied_capacity");
        self.emit("# cellscript abi: SourceView unoccupied capacity = capacity - occupied_capacity");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit_add_immediate("a1", "a0", 0);
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
        self.emit_add_immediate("a0", "a1", 0);
        self.emit(format!("bnez a0, {}", failed_status_ok));
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit_label(&failed_status_ok);
        self.emit_add_immediate("a1", "a0", 0);
        self.emit(format!("j {}", done));
        self.emit_label(&underflow);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::NumericOrDiscriminantInvalid.code()));
        self.emit_add_immediate("a1", "a0", 0);
        self.emit_label(&done);
        self.emit_stack_load("ra", 24);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_cell_output_index_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_cell_output_index");
        self.emit_label("__ckb_cell_output_index");
        self.emit("# cellscript abi: SourceView output index extractor");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit_add_immediate("a1", "a0", 0);
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
        self.emit_add_immediate("a0", "t1", 0);
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit_add_immediate("a1", "a0", 0);
        self.emit_label(&done);
        self.emit("ret");
    }

    /// Generic cell probes. Results use a0=value, a1=error; a missing Type is
    /// false, but an out-of-range cell or syscall failure is never false.
    pub(super) fn emit_runtime_cell_probe_helper(&mut self, symbol: &str, count: bool, enabled: bool) {
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
        self.emit_add_immediate("a0", "sp", BUFFER);
        self.emit_add_immediate("a1", "sp", LENGTH);
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

    pub(super) fn emit_runtime_cell_data_size_helper(&mut self, enabled: bool) {
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
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
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

    pub(super) fn emit_runtime_cell_data_equal_helper(&mut self, enabled: bool) {
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

    pub(super) fn emit_runtime_source_bytes_equal_helper(&mut self, enabled: bool) {
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

    pub(super) fn emit_runtime_source_bytes_equal_memory_helper(&mut self, enabled: bool) {
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

    pub(super) fn emit_runtime_source_bytes_zero_helper(&mut self, enabled: bool) {
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

    pub(super) fn emit_runtime_bounded_cell_dep_data_hash_requirement_helper(&mut self, enabled: bool) {
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
        self.emit_store(StoreOp::Double, "ra", "sp", RA_OFFSET);
        self.emit_store(StoreOp::Double, "a1", "sp", EXPECTED_PTR_OFFSET);
        self.emit_store(StoreOp::Double, "a0", "sp", LIMIT_OFFSET);
        self.emit(format!("beqz a1, {}", invalid));
        self.emit(format!("beqz a0, {}", invalid));
        self.emit("li t0, 64");
        self.emit(format!("bltu t0, a0, {}", invalid));
        self.emit_store(StoreOp::Double, "zero", "sp", INDEX_OFFSET);

        self.emit_label(&scan);
        self.emit_load(LoadOp::Double, "t0", "sp", INDEX_OFFSET);
        self.emit_load(LoadOp::Double, "t1", "sp", LIMIT_OFFSET);
        self.emit(format!("bgeu t0, t1, {}", not_found));
        self.emit("li t1, 32");
        self.emit_store(StoreOp::Double, "t1", "sp", SIZE_OFFSET);
        self.emit_add_immediate("a0", "sp", BUFFER_OFFSET);
        self.emit_add_immediate("a1", "sp", SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit_load(LoadOp::Double, "a3", "sp", INDEX_OFFSET);
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
        self.emit_load(LoadOp::Double, "t0", "sp", SIZE_OFFSET);
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit_add_immediate("a0", "sp", BUFFER_OFFSET);
        self.emit_load(LoadOp::Double, "a1", "sp", EXPECTED_PTR_OFFSET);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&mismatch);
        self.emit_load(LoadOp::Double, "t0", "sp", INDEX_OFFSET);
        self.emit_add_immediate("t0", "t0", 1);
        self.emit_store(StoreOp::Double, "t0", "sp", INDEX_OFFSET);
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
        self.emit_load(LoadOp::Double, "ra", "sp", RA_OFFSET);
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_cell_data_bytes32_helper(&mut self, symbol: &str, detail: &str, enabled: bool) {
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
        self.emit_store(StoreOp::Double, "t0", "a3", 0);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit_stack_load("a0", 8);
        self.emit_stack_load("a1", 16);
        self.emit_stack_load("a2", 0);
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("bnez t1, {}", failed));
        self.emit_label(&loaded);
        self.emit("# cellscript abi: normalize fixed 32-byte slice length after LOAD_CELL_DATA");
        self.emit_stack_load("t0", 16);
        self.emit_load(LoadOp::Double, "t2", "t0", 0);
        self.emit("li t1, 32");
        self.emit(format!("bltu t2, t1, {}", failed));
        self.emit_store(StoreOp::Double, "t1", "t0", 0);
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
}
