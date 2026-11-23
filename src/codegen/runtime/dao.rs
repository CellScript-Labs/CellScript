use super::super::align_frame;
use super::super::{
    CellScriptRuntimeError, CodeGenerator, LoadOp, StoreOp, CKB_CELL_FIELD_TYPE_HASH, CKB_DAO_HEADER_ACCUMULATED_RATE_ABSOLUTE_OFFSET,
    CKB_DAO_HEADER_FIELD_ABSOLUTE_OFFSET, CKB_DAO_TYPE_HASH_WORDS_LE, CKB_EPOCH_FRACTION_BOUND, CKB_EPOCH_FRACTION_MASK,
    CKB_EPOCH_NUMBER_BOUND, CKB_EPOCH_NUMBER_MASK, CKB_INPUT_FIELD_SINCE, CKB_LENGTH_NOT_ENOUGH,
    CKB_SINCE_EPOCH_NUMBER_WITH_FRACTION_FLAG, CKB_SINCE_METRIC_TYPE_FLAG_MASK, CKB_SINCE_REMAIN_FLAGS_BITS, CKB_SINCE_VALUE_MASK,
    CKB_SOURCE_HEADER_DEP, CKB_SOURCE_VIEW_HEADER_DEP, CKB_SOURCE_VIEW_SHIFT,
};

impl CodeGenerator {
    pub(super) fn emit_runtime_dao_accumulated_rate_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(32) as i64;
        self.emit_global("__dao_accumulated_rate");
        self.emit_label("__dao_accumulated_rate");
        self.emit(
            "# cellscript abi: DAO accumulated-rate HeaderDep SourceView helper via LOAD_HEADER at absolute header offset 160+8",
        );
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit_add_immediate("a1", "a0", 0);
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
        self.emit_add_immediate("a3", "t1", 0);
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
        self.emit_add_immediate("a1", "a0", 0);
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DaoFieldMalformed.code()));
        self.emit_add_immediate("a1", "a0", 0);
        self.emit_label(&done);
        self.emit_stack_load("ra", 24);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_dao_input_accumulated_rate_helper(&mut self, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(32) as i64;
        self.emit_global("__dao_input_accumulated_rate");
        self.emit_label("__dao_input_accumulated_rate");
        self.emit(
            "# cellscript abi: DAO accumulated-rate from Input/GroupInput committed header via LOAD_HEADER at absolute header offset 160+8",
        );
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit_add_immediate("a1", "a0", 0);
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
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
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
        self.emit_add_immediate("a1", "a0", 0);
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DaoFieldMalformed.code()));
        self.emit_add_immediate("a1", "a0", 0);
        self.emit_label(&done);
        self.emit_stack_load("ra", 24);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_dao_type_classifier_helper(&mut self, enabled: bool) {
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
        self.emit_store(StoreOp::Double, "ra", "sp", 56);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 32");
        self.emit_store(StoreOp::Double, "t0", "sp", 8);
        self.emit_add_immediate("a0", "sp", 16);
        self.emit_add_immediate("a1", "sp", 8);
        self.emit("li a2, 0");
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_TYPE_HASH));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", false_label));
        self.emit_load(LoadOp::Double, "t0", "sp", 8);
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", false_label));
        for (word_index, expected) in CKB_DAO_TYPE_HASH_WORDS_LE.iter().enumerate() {
            self.emit_load(LoadOp::Double, "t0", "sp", 16 + word_index * 8);
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
        self.emit_load(LoadOp::Double, "ra", "sp", 56);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_dao_cell_data_classifier_helper(&mut self, symbol: &str, detail: &str, deposit: bool, enabled: bool) {
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
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
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

    pub(super) fn emit_runtime_dao_require_header_dep_for_input_helper(&mut self, enabled: bool) {
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
        self.emit_store(StoreOp::Double, "ra", "sp", 120);
        self.emit_store(StoreOp::Double, "a1", "sp", HEADER_VIEW_OFFSET);

        self.emit_decode_input_source_view_to_t1_t2(&invalid_input);
        self.emit_store(StoreOp::Double, "t1", "sp", INPUT_INDEX_OFFSET);
        self.emit_store(StoreOp::Double, "t2", "sp", INPUT_SOURCE_OFFSET);

        self.emit_load(LoadOp::Double, "a0", "sp", HEADER_VIEW_OFFSET);
        self.emit(format!("li t6, {}", CKB_SOURCE_VIEW_SHIFT));
        self.emit("div t0, a0, t6");
        self.emit("rem t1, a0, t6");
        self.emit(format!("li t5, {}", CKB_SOURCE_VIEW_HEADER_DEP));
        self.emit("sub t4, t0, t5");
        self.emit(format!("bnez t4, {}", invalid_header));
        self.emit_store(StoreOp::Double, "t1", "sp", HEADER_INDEX_OFFSET);

        self.emit("li t0, 32");
        self.emit_store(StoreOp::Double, "t0", "sp", SIZE_OFFSET);
        self.emit_add_immediate("a0", "sp", INPUT_DAO_OFFSET);
        self.emit_add_immediate("a1", "sp", SIZE_OFFSET);
        self.emit(format!("li a2, {}", CKB_DAO_HEADER_FIELD_ABSOLUTE_OFFSET));
        self.emit_load(LoadOp::Double, "a3", "sp", INPUT_INDEX_OFFSET);
        self.emit_load(LoadOp::Double, "a4", "sp", INPUT_SOURCE_OFFSET);
        self.emit(format!("li a7, {}", abi.load_header));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", input_failed));
        self.emit_load(LoadOp::Double, "t0", "sp", SIZE_OFFSET);
        self.emit("li t1, 32");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));

        self.emit("li t0, 32");
        self.emit_store(StoreOp::Double, "t0", "sp", SIZE_OFFSET);
        self.emit_add_immediate("a0", "sp", HEADER_DAO_OFFSET);
        self.emit_add_immediate("a1", "sp", SIZE_OFFSET);
        self.emit(format!("li a2, {}", CKB_DAO_HEADER_FIELD_ABSOLUTE_OFFSET));
        self.emit_load(LoadOp::Double, "a3", "sp", HEADER_INDEX_OFFSET);
        self.emit(format!("li a4, {}", CKB_SOURCE_HEADER_DEP));
        self.emit(format!("li a7, {}", abi.load_header));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", header_failed));
        self.emit_load(LoadOp::Double, "t0", "sp", SIZE_OFFSET);
        self.emit("li t1, 32");
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));

        self.emit_add_immediate("a0", "sp", INPUT_DAO_OFFSET);
        self.emit_add_immediate("a1", "sp", HEADER_DAO_OFFSET);
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
        self.emit_load(LoadOp::Double, "ra", "sp", 120);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_dao_require_input_since_at_least_helper(&mut self, enabled: bool) {
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
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
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

    pub(super) fn emit_runtime_dao_require_input_relative_epoch_since_at_least_helper(&mut self, enabled: bool) {
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
        self.emit_store(StoreOp::Double, "ra", "sp", 72);
        self.emit_store(StoreOp::Double, "a1", "sp", REQUIRED_NUMBER_OFFSET);
        self.emit_store(StoreOp::Double, "a2", "sp", REQUIRED_INDEX_OFFSET);
        self.emit_store(StoreOp::Double, "a3", "sp", REQUIRED_LENGTH_OFFSET);

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
        self.emit_store(StoreOp::Double, "t0", "sp", SIZE_OFFSET);
        self.emit_add_immediate("a0", "sp", SINCE_OFFSET);
        self.emit_add_immediate("a1", "sp", SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_SINCE));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit_load(LoadOp::Double, "t0", "sp", SIZE_OFFSET);
        self.emit("li t1, 8");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));

        self.emit_load(LoadOp::Double, "t0", "sp", SINCE_OFFSET);
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
        self.emit_store(StoreOp::Double, "t2", "sp", LOADED_NUMBER_OFFSET);
        self.emit_store(StoreOp::Double, "t3", "sp", LOADED_INDEX_OFFSET);
        self.emit_store(StoreOp::Double, "t4", "sp", LOADED_LENGTH_OFFSET);

        self.emit_load(LoadOp::Double, "t0", "sp", REQUIRED_NUMBER_OFFSET);
        self.emit("sltu t1, t0, t2");
        self.emit(format!("bnez t1, {}", success));
        self.emit("sltu t1, t2, t0");
        self.emit(format!("bnez t1, {}", immature));
        self.emit_load(LoadOp::Double, "t0", "sp", LOADED_INDEX_OFFSET);
        self.emit_load(LoadOp::Double, "t1", "sp", REQUIRED_LENGTH_OFFSET);
        self.emit("mul t2, t0, t1");
        self.emit_load(LoadOp::Double, "t0", "sp", REQUIRED_INDEX_OFFSET);
        self.emit_load(LoadOp::Double, "t1", "sp", LOADED_LENGTH_OFFSET);
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
        self.emit_load(LoadOp::Double, "ra", "sp", 72);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }
}
