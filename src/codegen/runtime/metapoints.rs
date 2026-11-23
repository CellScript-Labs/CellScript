use super::super::align_frame;
use super::super::{
    CellScriptRuntimeError, CodeGenerator, LoadOp, StoreOp, CKB_CELL_FIELD_LOCK_HASH, CKB_CELL_FIELD_TYPE_HASH,
    CKB_INDEX_OUT_OF_BOUND, CKB_INPUT_FIELD_OUT_POINT, CKB_ITEM_MISSING, CKB_LENGTH_NOT_ENOUGH, CKB_SOURCE_GROUP_FLAG,
    CKB_SOURCE_INPUT, CKB_SOURCE_OUTPUT, CKB_SOURCE_VIEW_GROUP_INPUT, CKB_SOURCE_VIEW_GROUP_OUTPUT, CKB_SOURCE_VIEW_INPUT,
    CKB_SOURCE_VIEW_OUTPUT, CKB_SOURCE_VIEW_SHIFT,
};

impl CodeGenerator {
    pub(super) fn emit_runtime_metapoint_relative_requirement_helper(&mut self, enabled: bool) {
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

    pub(super) fn emit_runtime_current_script_role_at_helper(&mut self, enabled: bool) {
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
        self.emit_store(StoreOp::Double, "ra", "sp", 88);
        self.emit_store(StoreOp::Double, "a0", "sp", SOURCE_OFFSET);
        self.emit_store(StoreOp::Double, "a1", "sp", INDEX_OFFSET);
        self.emit_store(StoreOp::Double, "a2", "sp", SCRIPT_HASH_PTR_OFFSET);
        self.emit_store(StoreOp::Double, "zero", "sp", LOCK_MATCH_OFFSET);
        self.emit_store(StoreOp::Double, "zero", "sp", TYPE_MATCH_OFFSET);
        self.emit(format!("beqz a2, {}", bad_args));

        self.emit("li t0, 32");
        self.emit_store(StoreOp::Double, "t0", "sp", SIZE_OFFSET);
        self.emit_add_immediate("a0", "sp", HASH_BUFFER_OFFSET);
        self.emit_add_immediate("a1", "sp", SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit_load(LoadOp::Double, "a3", "sp", INDEX_OFFSET);
        self.emit_load(LoadOp::Double, "a4", "sp", SOURCE_OFFSET);
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_LOCK_HASH));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", lock_loaded));
        self.emit(format!("li t0, {}", CKB_INDEX_OUT_OF_BOUND));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", out_of_bound));
        self.emit(format!("j {}", failed));

        self.emit_label(&lock_loaded);
        self.emit_load(LoadOp::Double, "t0", "sp", SIZE_OFFSET);
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit_add_immediate("a0", "sp", HASH_BUFFER_OFFSET);
        self.emit_load(LoadOp::Double, "a1", "sp", SCRIPT_HASH_PTR_OFFSET);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", lock_not_match));
        self.emit("li t0, 1");
        self.emit_store(StoreOp::Double, "t0", "sp", LOCK_MATCH_OFFSET);
        self.emit_label(&lock_not_match);

        self.emit("li t0, 32");
        self.emit_store(StoreOp::Double, "t0", "sp", SIZE_OFFSET);
        self.emit_add_immediate("a0", "sp", HASH_BUFFER_OFFSET);
        self.emit_add_immediate("a1", "sp", SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit_load(LoadOp::Double, "a3", "sp", INDEX_OFFSET);
        self.emit_load(LoadOp::Double, "a4", "sp", SOURCE_OFFSET);
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
        self.emit_load(LoadOp::Double, "t0", "sp", SIZE_OFFSET);
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit_add_immediate("a0", "sp", HASH_BUFFER_OFFSET);
        self.emit_load(LoadOp::Double, "a1", "sp", SCRIPT_HASH_PTR_OFFSET);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", type_not_match));
        self.emit("li t0, 1");
        self.emit_store(StoreOp::Double, "t0", "sp", TYPE_MATCH_OFFSET);
        self.emit_label(&type_not_match);
        self.emit(format!("j {}", build_role));

        self.emit_label(&type_missing);
        self.emit(format!("j {}", build_role));

        self.emit_label(&build_role);
        self.emit_load(LoadOp::Double, "t0", "sp", LOCK_MATCH_OFFSET);
        self.emit_load(LoadOp::Double, "t1", "sp", TYPE_MATCH_OFFSET);
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
        self.emit_load(LoadOp::Double, "ra", "sp", 88);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_metapoint_pair_cardinality_helper(
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
        self.emit_add_immediate("t0", "a0", 0);
        self.emit_add_immediate("t1", "a1", 0);
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
        self.emit_add_immediate("t0", "t0", 1);
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
        self.emit_add_immediate("t0", "a0", 0);
        self.emit_add_immediate("t1", "a1", 0);
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
        self.emit_add_immediate("t0", "t0", 1);
        self.emit_stack_store("t0", RELATED_INDEX_OFFSET);
        self.emit(format!("j {}", inner_loop));

        self.emit_label(&relation_matched);
        self.emit_stack_load("t0", MATCH_COUNT_OFFSET);
        self.emit_add_immediate("t0", "t0", 1);
        self.emit_stack_store("t0", MATCH_COUNT_OFFSET);
        self.emit_stack_load("t1", RELATED_INDEX_OFFSET);
        self.emit_add_immediate("t1", "t1", 1);
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
        self.emit_add_immediate("t4", "t4", 1);
        self.emit_stack_store("t4", RELATED_COUNT_OFFSET);

        self.emit_label(&increment_outer);
        self.emit_stack_load("t0", BASE_INDEX_OFFSET);
        self.emit_add_immediate("t0", "t0", 1);
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
        self.emit_add_immediate("a0", "t1", 0);
        self.emit(format!("j {}", done));
        self.emit_label(&relation_failed);
        self.emit_add_immediate("t1", "a0", 0);
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
        self.emit_store(StoreOp::Double, "t0", "sp", size_offset);
        self.emit_add_immediate("a0", "sp", type_hash_buffer_offset);
        self.emit_add_immediate("a1", "sp", size_offset);
        self.emit("li a2, 0");
        self.emit_load(LoadOp::Double, "a3", "sp", related_index_offset);
        self.emit_load(LoadOp::Double, "a4", "sp", source_offset);
        self.emit(format!("li a5, {}", CKB_CELL_FIELD_TYPE_HASH));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", type_loaded));
        self.emit_add_immediate("t1", "a0", 0);
        self.emit(format!("j {}", status_failed));
        self.emit_label(&type_loaded);
        self.emit_load(LoadOp::Double, "t0", "sp", size_offset);
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", type_size_ok));
        self.emit(format!("j {}", type_mismatch));
        self.emit_label(&type_size_ok);
        self.emit_add_immediate("a0", "sp", type_hash_buffer_offset);
        self.emit_load(LoadOp::Double, "a1", "sp", expected_hash_ptr_offset);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", type_mismatch));

        self.emit_load(LoadOp::Double, "t0", "sp", filter_rule_offset);
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
            self.emit_store(StoreOp::Double, "t0", "sp", size_offset);
            self.emit_add_immediate("a0", "sp", data_buffer_offset);
            self.emit_add_immediate("a1", "sp", size_offset);
            self.emit("li a2, 0");
            self.emit_load(LoadOp::Double, "a3", "sp", related_index_offset);
            self.emit_load(LoadOp::Double, "a4", "sp", source_offset);
            self.emit(format!("li a7, {}", abi.load_cell_data));
            self.emit("ecall");
            self.emit(format!("beqz a0, {}", data_loaded));
            self.emit(format!("j {}", data_mismatch));
            self.emit_label(&data_loaded);
            self.emit_load(LoadOp::Double, "t0", "sp", size_offset);
            self.emit("li t1, 8");
            self.emit("sub t2, t0, t1");
            self.emit(format!("beqz t2, {}", data_size_ok));
            self.emit(format!("j {}", data_mismatch));
            self.emit_label(&data_size_ok);
            self.emit_load(LoadOp::Double, "t0", "sp", data_buffer_offset);
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
}
