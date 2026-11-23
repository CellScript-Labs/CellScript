use super::super::align_frame;
use super::super::{
    CellScriptRuntimeError, CodeGenerator, LoadOp, StoreOp, CKB_CELL_FIELD_LOCK_HASH, CKB_CELL_FIELD_TYPE, CKB_CELL_FIELD_TYPE_HASH,
    CKB_INDEX_OUT_OF_BOUND, CKB_ITEM_MISSING, CKB_SOURCE_GROUP_FLAG, CKB_SOURCE_INPUT, CKB_SOURCE_OUTPUT,
    FUNGIBLE_TYPE_GROUP_V1_CODEGEN_HELPER,
};

impl CodeGenerator {
    pub(super) fn emit_runtime_xudt_amount_word_helper(&mut self, symbol: &str, detail: &str, offset: u64, enabled: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(32) as i64;
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {} via LOAD_CELL_DATA offset={}", detail, offset));
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit_add_immediate("a1", "a0", 0);
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
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
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
        self.emit_add_immediate("a1", "a0", 0);
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::XudtBindingMismatch.code()));
        self.emit_add_immediate("a1", "a0", 0);
        self.emit_label(&done);
        self.emit_stack_load("ra", 24);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_xudt_require_owner_mode_input_type_helper(&mut self, enabled: bool) {
        self.emit_runtime_cell_hash_requirement_helper(
            "__xudt_require_owner_mode_input_type",
            "xUDT owner-mode input-type full 32-byte binding check",
            CKB_CELL_FIELD_TYPE_HASH,
            CellScriptRuntimeError::XudtBindingMismatch,
            enabled,
        );
    }

    pub(super) fn emit_stack_u32_le_to(&mut self, dest: &str, stack_offset: usize) {
        self.emit_load(LoadOp::ByteUnsigned, dest, "sp", stack_offset);
        self.emit_load(LoadOp::ByteUnsigned, "t4", "sp", stack_offset + 1);
        self.emit("slli t4, t4, 8");
        self.emit(format!("or {}, {}, t4", dest, dest));
        self.emit_load(LoadOp::ByteUnsigned, "t4", "sp", stack_offset + 2);
        self.emit("slli t4, t4, 16");
        self.emit(format!("or {}, {}, t4", dest, dest));
        self.emit_load(LoadOp::ByteUnsigned, "t4", "sp", stack_offset + 3);
        self.emit("slli t4, t4, 24");
        self.emit(format!("or {}, {}, t4", dest, dest));
    }

    pub(in crate::codegen) fn emit_u32_le_from_base_to(&mut self, dest: &str, base: &str, offset: usize, scratch: &str) {
        self.emit_load(LoadOp::ByteUnsigned, dest, base, offset);
        self.emit_load(LoadOp::ByteUnsigned, scratch, base, offset + 1);
        self.emit(format!("slli {}, {}, 8", scratch, scratch));
        self.emit(format!("or {}, {}, {}", dest, dest, scratch));
        self.emit_load(LoadOp::ByteUnsigned, scratch, base, offset + 2);
        self.emit(format!("slli {}, {}, 16", scratch, scratch));
        self.emit(format!("or {}, {}, {}", dest, dest, scratch));
        self.emit_load(LoadOp::ByteUnsigned, scratch, base, offset + 3);
        self.emit(format!("slli {}, {}, 24", scratch, scratch));
        self.emit(format!("or {}, {}, {}", dest, dest, scratch));
    }

    pub(super) fn emit_runtime_xudt_require_owner_mode_type_args_helper(&mut self, enabled: bool) {
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
        self.emit_add_immediate("a3", "t1", 0);
        self.emit_add_immediate("a4", "t2", 0);
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

    pub(super) fn emit_runtime_xudt_require_owner_mode_type_args_current_script_helper(&mut self, enabled: bool) {
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
        self.emit_store(StoreOp::Double, "ra", "sp", 72);
        self.emit_store(StoreOp::Double, "a0", "sp", SOURCE_VIEW_OFFSET);
        self.emit_store(StoreOp::Double, "a1", "sp", FLAGS_OFFSET);

        self.emit("li t0, 32");
        self.emit_store(StoreOp::Double, "t0", "sp", SIZE_OFFSET);
        self.emit_add_immediate("a0", "sp", SCRIPT_HASH_OFFSET);
        self.emit_add_immediate("a1", "sp", SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", abi.load_script_hash));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", hash_failed));
        self.emit_load(LoadOp::Double, "t0", "sp", SIZE_OFFSET);
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", hash_malformed));

        self.emit_load(LoadOp::Double, "a0", "sp", SOURCE_VIEW_OFFSET);
        self.emit_add_immediate("a1", "sp", SCRIPT_HASH_OFFSET);
        self.emit("li a2, 32");
        self.emit_load(LoadOp::Double, "a3", "sp", FLAGS_OFFSET);
        self.emit("call __xudt_require_owner_mode_type_args");
        self.emit(format!("j {}", done));

        self.emit_label(&hash_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&hash_malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit_label(&done);
        self.emit_load(LoadOp::Double, "ra", "sp", 72);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_fungible_type_group_conservation_helper(&mut self, symbol: &str, detail: &str, enabled: bool) {
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
        self.emit_store(StoreOp::Double, "ra", "sp", ra_offset);
        for offset in [INPUT_LOW_OFFSET, INPUT_HIGH_OFFSET, OUTPUT_LOW_OFFSET, OUTPUT_HIGH_OFFSET] {
            self.emit_store(StoreOp::Double, "zero", "sp", offset);
        }

        if owner_mode {
            self.emit("# cellscript abi: authority args are legacy 32-byte lock hash or 0x01 plus 32-byte policy Type Script hash");
            self.emit("li t0, 96");
            self.emit_store(StoreOp::Double, "t0", "sp", CURRENT_SCRIPT_SIZE_OFFSET);
            self.emit_add_immediate("a0", "sp", CURRENT_SCRIPT_BUFFER_OFFSET);
            self.emit_add_immediate("a1", "sp", CURRENT_SCRIPT_SIZE_OFFSET);
            self.emit("li a2, 0");
            self.emit(format!("li a7, {}", abi.load_script));
            self.emit("ecall");
            self.emit(format!("beqz a0, {}", owner_script_loaded));
            self.emit(format!("j {}", owner_script_failed));

            self.emit_label(&owner_script_loaded);
            self.emit_load(LoadOp::Double, "t0", "sp", CURRENT_SCRIPT_SIZE_OFFSET);
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
            self.emit_store(StoreOp::Double, "t0", "sp", OWNER_AUTHORITY_FIELD_OFFSET);
            self.emit(format!("j {}", owner_authority_mode_ready));

            self.emit_label(&owner_tagged_type_mode);
            for (offset, expected) in [(0usize, TAGGED_TYPE_OWNER_SCRIPT_SIZE), (4, 16), (8, 48), (12, 49), (49, 33)] {
                self.emit_stack_u32_le_to("t0", CURRENT_SCRIPT_BUFFER_OFFSET + offset);
                self.emit(format!("li t1, {}", expected));
                self.emit("sub t2, t0, t1");
                self.emit(format!("bnez t2, {}", owner_script_malformed));
            }
            self.emit_load(LoadOp::ByteUnsigned, "t0", "sp", CURRENT_SCRIPT_BUFFER_OFFSET + 53);
            self.emit(format!("li t1, {}", TAGGED_TYPE_AUTHORITY));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", owner_script_malformed));
            self.emit(format!("li t0, {}", CKB_CELL_FIELD_TYPE_HASH));
            self.emit_store(StoreOp::Double, "t0", "sp", OWNER_AUTHORITY_FIELD_OFFSET);

            self.emit_label(&owner_authority_mode_ready);

            self.emit("# cellscript abi: supply authority succeeds only when an absolute Input lock/type hash equals Script args");
            self.emit_store(StoreOp::Double, "zero", "sp", OWNER_INPUT_INDEX_OFFSET);
            self.emit_store(StoreOp::Double, "zero", "sp", OWNER_AUTHORIZED_OFFSET);
            self.emit_label(&owner_scan_loop);
            self.emit("li t0, 32");
            self.emit_store(StoreOp::Double, "t0", "sp", OWNER_LOCK_SIZE_OFFSET);
            self.emit_add_immediate("a0", "sp", OWNER_LOCK_BUFFER_OFFSET);
            self.emit_add_immediate("a1", "sp", OWNER_LOCK_SIZE_OFFSET);
            self.emit("li a2, 0");
            self.emit_load(LoadOp::Double, "a3", "sp", OWNER_INPUT_INDEX_OFFSET);
            self.emit(format!("li a4, {}", CKB_SOURCE_INPUT));
            self.emit_load(LoadOp::Double, "a5", "sp", OWNER_AUTHORITY_FIELD_OFFSET);
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
            self.emit_load(LoadOp::Double, "t0", "sp", OWNER_LOCK_SIZE_OFFSET);
            self.emit("li t1, 32");
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {}", owner_scan_failed));
            self.emit_add_immediate("a0", "sp", OWNER_LOCK_BUFFER_OFFSET);
            self.emit_load(LoadOp::Double, "t0", "sp", OWNER_AUTHORITY_FIELD_OFFSET);
            self.emit(format!("li t1, {}", CKB_CELL_FIELD_TYPE_HASH));
            self.emit("sub t2, t0, t1");
            self.emit(format!("beqz t2, {}", owner_expected_type_hash));
            self.emit_add_immediate("a1", "sp", CURRENT_SCRIPT_BUFFER_OFFSET + 53);
            self.emit(format!("j {}", owner_expected_hash_ready));
            self.emit_label(&owner_expected_type_hash);
            self.emit_add_immediate("a1", "sp", CURRENT_SCRIPT_BUFFER_OFFSET + 54);
            self.emit_label(&owner_expected_hash_ready);
            self.emit("li a2, 32");
            self.emit("call __cellscript_memcmp_fixed");
            self.emit(format!("beqz a0, {}", owner_matched));
            self.emit(format!("j {}", owner_not_matched));

            self.emit_label(&owner_not_matched);
            self.emit_load(LoadOp::Double, "t0", "sp", OWNER_INPUT_INDEX_OFFSET);
            self.emit_add_immediate("t0", "t0", 1);
            self.emit_store(StoreOp::Double, "t0", "sp", OWNER_INPUT_INDEX_OFFSET);
            self.emit(format!("j {}", owner_scan_loop));

            self.emit_label(&owner_matched);
            self.emit("li t0, 1");
            self.emit_store(StoreOp::Double, "t0", "sp", OWNER_AUTHORIZED_OFFSET);
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
        self.emit_store(StoreOp::Double, "t0", "sp", SOURCE_OFFSET);
        self.emit_add_immediate("t0", "sp", INPUT_LOW_OFFSET);
        self.emit_add_immediate("t1", "sp", INPUT_HIGH_OFFSET);
        self.emit(format!("j {}", scan_source));

        self.emit_label(&output_phase);
        self.emit(format!("li t0, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT));
        self.emit_store(StoreOp::Double, "t0", "sp", SOURCE_OFFSET);
        self.emit_add_immediate("t0", "sp", OUTPUT_LOW_OFFSET);
        self.emit_add_immediate("t1", "sp", OUTPUT_HIGH_OFFSET);

        self.emit_label(&scan_source);
        self.emit_store(StoreOp::Double, "t0", "sp", SUM_LOW_OFFSET);
        self.emit_store(StoreOp::Double, "t1", "sp", SUM_HIGH_OFFSET);
        self.emit_store(StoreOp::Double, "zero", "sp", INDEX_OFFSET);

        self.emit_label(&scan_loop);
        self.emit("li t0, 16");
        self.emit_store(StoreOp::Double, "t0", "sp", SIZE_OFFSET);
        self.emit_add_immediate("a0", "sp", BUFFER_OFFSET);
        self.emit_add_immediate("a1", "sp", SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit_load(LoadOp::Double, "a3", "sp", INDEX_OFFSET);
        self.emit_load(LoadOp::Double, "a4", "sp", SOURCE_OFFSET);
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", scan_done));
        self.emit(format!("li t0, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", compare));
        self.emit(format!("j {}", scan_failed));

        self.emit_label(&scan_done);
        self.emit_load(LoadOp::Double, "t0", "sp", SIZE_OFFSET);
        self.emit("li t1, 16");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", scan_malformed));
        self.emit_load(LoadOp::Double, "t0", "sp", SUM_LOW_OFFSET);
        self.emit_load(LoadOp::Double, "t1", "sp", SUM_HIGH_OFFSET);
        self.emit_load(LoadOp::Double, "t2", "sp", 16);
        self.emit_load(LoadOp::Double, "t3", "sp", 24);
        self.emit_load(LoadOp::Double, "t4", "t0", 0);
        self.emit_load(LoadOp::Double, "t5", "t1", 0);
        self.emit("add t6, t4, t2");
        self.emit("sltu t4, t6, t4");
        self.emit("add t5, t5, t3");
        self.emit("sltu t3, t5, t3");
        self.emit(format!("bnez t3, {}", overflow));
        self.emit("add t5, t5, t4");
        self.emit("sltu t4, t5, t4");
        self.emit(format!("bnez t4, {}", overflow));
        self.emit_store(StoreOp::Double, "t6", "t0", 0);
        self.emit_store(StoreOp::Double, "t5", "t1", 0);
        self.emit_load(LoadOp::Double, "t0", "sp", INDEX_OFFSET);
        self.emit_add_immediate("t0", "t0", 1);
        self.emit_store(StoreOp::Double, "t0", "sp", INDEX_OFFSET);
        self.emit(format!("j {}", scan_loop));

        self.emit_label(&compare);
        let non_empty = self.fresh_label("fungible_group_non_empty");
        if owner_mode {
            self.emit_load(LoadOp::Double, "t4", "sp", OWNER_AUTHORIZED_OFFSET);
            self.emit(format!("bnez t4, {}", non_empty));
        }
        self.emit_load(LoadOp::Double, "t3", "sp", INDEX_OFFSET);
        self.emit(format!("beqz t3, {}", mismatch));
        self.emit_label(&non_empty);
        self.emit_load(LoadOp::Double, "t0", "sp", SOURCE_OFFSET);
        self.emit(format!("li t1, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", output_phase));
        if owner_mode {
            self.emit_load(LoadOp::Double, "t0", "sp", OWNER_AUTHORIZED_OFFSET);
            self.emit(format!("bnez t0, {}", owner_authorized));
        }
        self.emit_load(LoadOp::Double, "t0", "sp", INPUT_LOW_OFFSET);
        self.emit_load(LoadOp::Double, "t1", "sp", OUTPUT_LOW_OFFSET);
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", mismatch));
        self.emit_load(LoadOp::Double, "t0", "sp", INPUT_HIGH_OFFSET);
        self.emit_load(LoadOp::Double, "t1", "sp", OUTPUT_HIGH_OFFSET);
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
        self.emit_load(LoadOp::Double, "ra", "sp", ra_offset);
        self.emit_stack_adjustment(frame_size as i64);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_xudt_require_group_amount_delta_helper(&mut self, symbol: &str, minted: bool, enabled: bool) {
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
        self.emit_load(LoadOp::Double, "t4", "t0", 0);
        self.emit_load(LoadOp::Double, "t5", "t1", 0);
        self.emit("add t6, t4, t2");
        self.emit("sltu t4, t6, t4");
        self.emit("add t5, t5, t3");
        self.emit("sltu t3, t5, t3");
        self.emit(format!("bnez t3, {}", overflow));
        self.emit("add t5, t5, t4");
        self.emit("sltu t4, t5, t4");
        self.emit(format!("bnez t4, {}", overflow));
        self.emit_store(StoreOp::Double, "t6", "t0", 0);
        self.emit_store(StoreOp::Double, "t5", "t1", 0);
        self.emit_stack_load("t0", INDEX_OFFSET);
        self.emit_add_immediate("t0", "t0", 1);
        self.emit_stack_store("t0", INDEX_OFFSET);
        self.emit(format!("j {}", scan_loop));

        self.emit_label(&compare);
        self.emit_stack_load("t0", SOURCE_OFFSET);
        self.emit(format!("li t1, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", output_phase));

        self.emit_stack_load("a0", DELTA_PTR_OFFSET);
        self.emit_load(LoadOp::Double, "t2", "a0", 0);
        self.emit_load(LoadOp::Double, "t3", "a0", 8);
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
}
