use super::super::align_frame;
use super::super::{CellScriptRuntimeError, CodeGenerator, LoadOp, StoreOp, CKB_INPUT_FIELD_OUT_POINT};

impl CodeGenerator {
    pub(super) fn emit_runtime_input_out_point_word_helper(
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
            self.emit_add_immediate("a1", "a0", 0);
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
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
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
        self.emit_add_immediate("a1", "a0", 0);
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit_add_immediate("a1", "a0", 0);
        self.emit_label(&done);
        self.emit_stack_load("ra", 56);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_input_out_point_tx_hash_helper(&mut self, enabled: bool) {
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
        self.emit_store(StoreOp::Double, "ra", "sp", RA_OFFSET);
        self.emit_store(StoreOp::Double, "a1", "sp", OUT_PTR_OFFSET);
        self.emit_store(StoreOp::Double, "a2", "sp", SIZE_PTR_OFFSET);
        self.emit(format!("beqz a1, {}", bad_output));
        self.emit(format!("beqz a2, {}", bad_output));

        self.emit_decode_input_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 36");
        self.emit_store(StoreOp::Double, "t0", "sp", OUT_POINT_SIZE_OFFSET);
        self.emit_add_immediate("a0", "sp", OUT_POINT_OFFSET);
        self.emit_add_immediate("a1", "sp", OUT_POINT_SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_OUT_POINT));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit_load(LoadOp::Double, "t0", "sp", OUT_POINT_SIZE_OFFSET);
        self.emit("li t1, 36");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));

        self.emit("li t0, 0");
        self.emit_label(&copy_loop);
        self.emit("li t1, 32");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("beqz t2, {}", copy_done));
        self.emit_add_immediate("t3", "sp", OUT_POINT_OFFSET);
        self.emit("add t3, t3, t0");
        self.emit_load(LoadOp::ByteUnsigned, "t4", "t3", 0);
        self.emit_load(LoadOp::Double, "t5", "sp", OUT_PTR_OFFSET);
        self.emit("add t5, t5, t0");
        self.emit_store(StoreOp::Byte, "t4", "t5", 0);
        self.emit_add_immediate("t0", "t0", 1);
        self.emit(format!("j {}", copy_loop));
        self.emit_label(&copy_done);
        self.emit_load(LoadOp::Double, "t0", "sp", SIZE_PTR_OFFSET);
        self.emit("li t1, 32");
        self.emit_store(StoreOp::Double, "t1", "t0", 0);
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
        self.emit_load(LoadOp::Double, "ra", "sp", RA_OFFSET);
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_input_out_point_tx_hash_requirement_helper(&mut self, enabled: bool) {
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
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
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

    pub(super) fn emit_runtime_input_out_point_requirement_helper(&mut self, enabled: bool) {
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
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
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
}
