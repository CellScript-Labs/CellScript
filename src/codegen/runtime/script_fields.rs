use super::super::{align_frame, align_up};
use super::super::{
    CellScriptRuntimeError, CodeGenerator, LoadOp, ScriptArgsBytes32RequirementMode, ScriptHashFieldRead, ScriptScalarFieldRead,
    StoreOp, CKB_CELL_FIELD_LOCK, CKB_INDEX_OUT_OF_BOUND, CKB_LENGTH_NOT_ENOUGH, CKB_SOURCE_OUTPUT,
};

impl CodeGenerator {
    pub(super) fn emit_runtime_cell_hash_requirement_helper(
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
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
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

    pub(super) fn emit_runtime_cell_script_hash_field_helper(
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
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
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
        self.emit_load(LoadOp::ByteUnsigned, "t5", "t6", 0);
        self.emit_stack_load("t6", out_ptr_offset);
        self.emit("add t6, t6, t1");
        self.emit_store(StoreOp::Byte, "t5", "t6", 0);
        self.emit_add_immediate("t1", "t1", 1);
        self.emit(format!("j {}", copy_loop));
        self.emit_label(&copy_done);
        self.emit_stack_load("t6", size_ptr_offset);
        self.emit("li t0, 32");
        self.emit_store(StoreOp::Double, "t0", "t6", 0);
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

    pub(super) fn emit_runtime_cell_script_scalar_field_helper(
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
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
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

    pub(super) fn emit_runtime_cell_script_args_empty_requirement_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        field_id: u64,
        enabled: bool,
    ) {
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
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
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

    pub(super) fn emit_runtime_cell_script_args_exact_requirement_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        field_id: u64,
        enabled: bool,
    ) {
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
        self.emit_store(StoreOp::Double, "ra", "sp", RA_OFFSET);
        self.emit_store(StoreOp::Double, "a1", "sp", EXPECTED_PTR_OFFSET);
        self.emit_store(StoreOp::Double, "a2", "sp", EXPECTED_LEN_OFFSET);
        self.emit(format!("beqz a2, {}", bad_expected));
        self.emit(format!("beqz a1, {}", bad_expected));

        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit_store(StoreOp::Double, "t1", "sp", SOURCE_INDEX_OFFSET);
        self.emit_store(StoreOp::Double, "t2", "sp", SOURCE_KIND_OFFSET);

        self.emit(format!("li t0, {}", SCRIPT_PREFIX_SIZE));
        self.emit_store(StoreOp::Double, "t0", "sp", SCRIPT_SIZE_OFFSET);
        self.emit_add_immediate("a0", "sp", SCRIPT_BUFFER_OFFSET);
        self.emit_add_immediate("a1", "sp", SCRIPT_SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit_load(LoadOp::Double, "a3", "sp", SOURCE_INDEX_OFFSET);
        self.emit_load(LoadOp::Double, "a4", "sp", SOURCE_KIND_OFFSET);
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", prefix_loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", prefix_loaded));
        self.emit(format!("j {}", load_failed));

        self.emit_label(&prefix_loaded);
        self.emit_load(LoadOp::Double, "t3", "sp", SCRIPT_SIZE_OFFSET);
        self.emit(format!("li t1, {}", SCRIPT_PREFIX_SIZE));
        self.emit("sltu t2, t3, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit_stack_u32_le_to("t0", SCRIPT_BUFFER_OFFSET);
        self.emit_load(LoadOp::Double, "t1", "sp", EXPECTED_LEN_OFFSET);
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
        self.emit_load(LoadOp::Double, "t1", "sp", EXPECTED_LEN_OFFSET);
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", mismatch));
        self.emit_store(StoreOp::Double, "zero", "sp", ARGS_OFFSET_OFFSET);

        self.emit_label(&chunk_loop);
        self.emit_load(LoadOp::Double, "t0", "sp", ARGS_OFFSET_OFFSET);
        self.emit_load(LoadOp::Double, "t1", "sp", EXPECTED_LEN_OFFSET);
        self.emit("sub t2, t1, t0");
        self.emit(format!("beqz t2, {}", success));
        self.emit(format!("li t3, {}", CHUNK_SIZE));
        self.emit("sltu t4, t2, t3");
        self.emit(format!("bnez t4, {}", chunk_tail));
        self.emit(format!("li t2, {}", CHUNK_SIZE));
        self.emit_label(&chunk_tail);
        self.emit_store(StoreOp::Double, "t2", "sp", SCRIPT_SIZE_OFFSET);
        self.emit_store(StoreOp::Double, "t2", "sp", CHUNK_LEN_OFFSET);
        self.emit_add_immediate("a0", "sp", SCRIPT_BUFFER_OFFSET);
        self.emit_add_immediate("a1", "sp", SCRIPT_SIZE_OFFSET);
        self.emit(format!("li a2, {}", SCRIPT_PREFIX_SIZE));
        self.emit("add a2, a2, t0");
        self.emit_load(LoadOp::Double, "a3", "sp", SOURCE_INDEX_OFFSET);
        self.emit_load(LoadOp::Double, "a4", "sp", SOURCE_KIND_OFFSET);
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", chunk_loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", chunk_loaded));
        self.emit(format!("j {}", load_failed));
        self.emit_label(&chunk_loaded);
        self.emit_load(LoadOp::Double, "t2", "sp", CHUNK_LEN_OFFSET);
        self.emit_load(LoadOp::Double, "t0", "sp", ARGS_OFFSET_OFFSET);
        self.emit_load(LoadOp::Double, "t1", "sp", EXPECTED_PTR_OFFSET);
        self.emit("add a1, t1, t0");
        self.emit_add_immediate("a0", "sp", SCRIPT_BUFFER_OFFSET);
        self.emit_add_immediate("a2", "t2", 0);
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));
        self.emit_load(LoadOp::Double, "t0", "sp", ARGS_OFFSET_OFFSET);
        self.emit_load(LoadOp::Double, "t2", "sp", CHUNK_LEN_OFFSET);
        self.emit("add t0, t0, t2");
        self.emit_store(StoreOp::Double, "t0", "sp", ARGS_OFFSET_OFFSET);
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
        self.emit_load(LoadOp::Double, "ra", "sp", RA_OFFSET);
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_current_script_args_empty_requirement_helper(&mut self, enabled: bool) {
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
        self.emit_add_immediate("t0", "t0", 1);
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

    pub(super) fn emit_runtime_cell_script_args_bytes32_requirement_helper(
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
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
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
                self.emit_add_immediate("t1", "t1", -32);
                self.emit("li t0, 32");
                self.emit_stack_store("t0", SCRIPT_SIZE_OFFSET);
                self.emit_sp_addi("a0", SCRIPT_BUFFER_OFFSET);
                self.emit_sp_addi("a1", SCRIPT_SIZE_OFFSET);
                self.emit_add_immediate("a2", "t1", 0);
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

    pub(super) fn emit_runtime_cell_script_hash_type_requirement_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        field_id: u64,
        enabled: bool,
    ) {
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
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
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
}
