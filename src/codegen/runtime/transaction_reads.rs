use super::super::align_frame;
use super::super::{
    ckb_abi, CellScriptRuntimeError, CodeGenerator, LoadOp, StoreOp, CKB_SOURCE_CELL_DEP, CKB_SOURCE_GROUP_FLAG,
    CKB_SOURCE_HEADER_DEP, CKB_SOURCE_INPUT, CKB_SOURCE_OUTPUT, CKB_SOURCE_VIEW_CELL_DEP, CKB_SOURCE_VIEW_GROUP_INPUT,
    CKB_SOURCE_VIEW_GROUP_OUTPUT, CKB_SOURCE_VIEW_HEADER_DEP, CKB_SOURCE_VIEW_INPUT, CKB_SOURCE_VIEW_OUTPUT, CKB_SOURCE_VIEW_SHIFT,
};

impl CodeGenerator {
    pub(super) fn emit_runtime_current_script_hash_helper(&mut self, enabled: bool) {
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
        self.emit_store(StoreOp::Double, "t0", "a1", 0);
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", abi.load_script_hash));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit_stack_load("t6", 0);
        self.emit_load(LoadOp::Double, "t0", "t6", 0);
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

    pub(super) fn emit_runtime_transaction_hash_helper(&mut self, enabled: bool) {
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
        self.emit_store(StoreOp::Double, "t0", "a1", 0);
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_TX_HASH));
        self.emit("ecall");
        self.emit(format!("bnez a0, {failed}"));
        self.emit_stack_load("t6", 0);
        self.emit_load(LoadOp::Double, "t0", "t6", 0);
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

    pub(super) fn emit_runtime_source_view_helper(&mut self, symbol: &str, source_view: u64, detail: &str, enabled: bool) {
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

    pub(in crate::codegen) fn emit_decode_source_view_to_t1_t2(&mut self, invalid_label: &str) {
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
        self.emit_add_immediate("t2", "t0", -4);
        self.emit(format!("li t5, {}", CKB_SOURCE_GROUP_FLAG));
        self.emit("or t2, t2, t5");
        self.emit(format!("j {}", done));
        self.emit_label(&direct);
        self.emit("mv t2, t0");
        self.emit_label(&done);
    }

    pub(super) fn emit_decode_input_source_view_to_t1_t2(&mut self, invalid_label: &str) {
        let done = self.fresh_label("input_source_view_decoded");
        self.emit_decode_source_view_to_t1_t2(invalid_label);
        self.emit(format!("li t0, {}", CKB_SOURCE_INPUT));
        self.emit(format!("beq t2, t0, {}", done));
        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit(format!("beq t2, t0, {}", done));
        self.emit(format!("j {}", invalid_label));
        self.emit_label(&done);
    }

    pub(super) fn emit_runtime_cell_field_u64_helper(&mut self, symbol: &str, detail: &str, field_id: u64, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(32) as i64;
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: CKB SourceView LOAD_CELL_BY_FIELD ({})", detail));
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit_add_immediate("a1", "a0", 0);
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
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit_stack_load("a0", 16);
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit_add_immediate("a1", "a0", 0);
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit_add_immediate("a1", "a0", 0);
        self.emit_label(&done);
        self.emit_stack_load("ra", 24);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_cell_field_low_word_helper(&mut self, symbol: &str, detail: &str, field_id: u64, enabled: bool) {
        self.emit_runtime_cell_field_u64_helper(symbol, detail, field_id, enabled);
    }

    pub(super) fn emit_runtime_cell_hash_field_helper(&mut self, symbol: &str, detail: &str, field_id: u64, enabled: bool) {
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
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
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
        self.emit_load(LoadOp::ByteUnsigned, "t5", "t6", 0);
        self.emit_stack_load("t6", OUT_PTR_OFFSET);
        self.emit("add t6, t6, t1");
        self.emit_store(StoreOp::Byte, "t5", "t6", 0);
        self.emit_add_immediate("t1", "t1", 1);
        self.emit(format!("j {}", copy_loop));

        self.emit_label(&copy_done);
        self.emit_stack_load("t6", SIZE_PTR_OFFSET);
        self.emit("li t0, 32");
        self.emit_store(StoreOp::Double, "t0", "t6", 0);
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
}
