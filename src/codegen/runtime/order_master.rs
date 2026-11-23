use super::super::align_frame;
use super::super::{
    CellScriptRuntimeError, CodeGenerator, LoadOp, StoreOp, CKB_INDEX_OUT_OF_BOUND, CKB_INPUT_FIELD_OUT_POINT, CKB_LENGTH_NOT_ENOUGH,
    CKB_SOURCE_GROUP_FLAG, CKB_SOURCE_INPUT, CKB_SOURCE_OUTPUT,
};
use super::{OrderMasterDataOffsets, OrderMasterFailureLabels};

impl CodeGenerator {
    pub(super) fn emit_runtime_lock_match_master_out_point_pairs_from_data_helper(&mut self, enabled: bool) {
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
        self.emit_add_immediate("t0", "a0", 0);
        self.emit_add_immediate("t1", "a1", 0);
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
        self.emit_add_immediate("t0", "t0", 1);
        self.emit_stack_store("t0", OUTPUT_COUNT_OFFSET);
        self.emit_label(&output_count_advance);
        self.emit_stack_load("t0", OUTPUT_INDEX_OFFSET);
        self.emit_add_immediate("t0", "t0", 1);
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
        self.emit_add_immediate("t0", "a0", 0);
        self.emit_add_immediate("t1", "a1", 0);
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
        self.emit_add_immediate("t0", "t0", 1);
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
        self.emit_add_immediate("t0", "a0", 0);
        self.emit_add_immediate("t1", "a1", 0);
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
        self.emit_add_immediate("t0", "t0", 1);
        self.emit_stack_store("t0", MATCH_COUNT_OFFSET);

        self.emit_label(&output_match_advance);
        self.emit_stack_load("t0", OUTPUT_INDEX_OFFSET);
        self.emit_add_immediate("t0", "t0", 1);
        self.emit_stack_store("t0", OUTPUT_INDEX_OFFSET);
        self.emit(format!("j {}", output_match_loop));

        self.emit_label(&output_match_done);
        self.emit_stack_load("t0", MATCH_COUNT_OFFSET);
        self.emit("li t1, 1");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", cardinality));

        self.emit_label(&input_advance);
        self.emit_stack_load("t0", INPUT_INDEX_OFFSET);
        self.emit_add_immediate("t0", "t0", 1);
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
        self.emit_add_immediate("a0", "t1", 0);
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
        self.emit_load(LoadOp::Double, "t0", "sp", index_offset_offset);
        self.emit("li t1, 37");
        self.emit("add t0, t0, t1");
        self.emit_store(StoreOp::Double, "t0", "sp", data_buffer_offset);
        self.emit("li t1, 0");
        self.emit_store(StoreOp::Double, "t1", "sp", size_offset);
        self.emit_add_immediate("a0", "sp", data_buffer_offset + 8);
        self.emit_add_immediate("a1", "sp", size_offset);
        self.emit("li a2, 0");
        self.emit_load(LoadOp::Double, "a3", "sp", cell_index_offset);
        self.emit_load(LoadOp::Double, "a4", "sp", source_offset);
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", size_status_ok));
        self.emit(format!("beqz a0, {}", size_status_ok));
        self.emit(format!("j {}", malformed));
        self.emit_label(&size_status_ok);
        self.emit_load(LoadOp::Double, "t0", "sp", data_buffer_offset);
        self.emit_load(LoadOp::Double, "t1", "sp", size_offset);
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
        self.emit_store(StoreOp::Double, "t0", "sp", index_dest_offset);
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
                self.emit_load(LoadOp::Double, "t0", "sp", tx_dest_offset + word * 8);
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
            self.emit_store(StoreOp::Double, "t3", "sp", data_buffer_offset);
            self.emit_load_input_out_point_to_stack(
                source_offset,
                cell_index_offset,
                tx_dest_offset,
                index_dest_offset,
                size_offset,
                out_point_failed,
            );
            self.emit_load(LoadOp::Double, "t3", "sp", data_buffer_offset);
            self.emit_load(LoadOp::Double, "t0", "sp", index_dest_offset);
            self.emit("add t0, t0, t3");
            self.emit("slt t1, t0, zero");
            self.emit(format!("bnez t1, {}", out_point_failed));
            self.emit_store(StoreOp::Double, "t0", "sp", index_dest_offset);
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
        self.emit_store(StoreOp::Double, "t0", "sp", size_offset);
        self.emit_add_immediate("a0", "sp", dest_offset);
        self.emit_add_immediate("a1", "sp", size_offset);
        self.emit_load(LoadOp::Double, "a2", "sp", data_offset_offset);
        self.emit_load(LoadOp::Double, "a3", "sp", cell_index_offset);
        self.emit_load(LoadOp::Double, "a4", "sp", source_offset);
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("beqz a0, {}", loaded));
        self.emit(format!("li t0, {}", CKB_LENGTH_NOT_ENOUGH));
        self.emit("sub t1, a0, t0");
        self.emit(format!("beqz t1, {}", len_enough));
        self.emit(format!("j {}", malformed));
        self.emit_label(&loaded);
        self.emit_load(LoadOp::Double, "t0", "sp", size_offset);
        self.emit(format!("li t1, {}", width));
        self.emit("sltu t2, t0, t1");
        self.emit(format!("bnez t2, {}", malformed));
        self.emit(format!("j {}", ready));
        self.emit_label(&len_enough);
        self.emit_load(LoadOp::Double, "t0", "sp", size_offset);
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
        self.emit_store(StoreOp::Double, "t0", "sp", size_offset);
        self.emit_add_immediate("a0", "sp", tx_dest_offset);
        self.emit_add_immediate("a1", "sp", size_offset);
        self.emit("li a2, 0");
        self.emit_load(LoadOp::Double, "a3", "sp", cell_index_offset);
        self.emit_load(LoadOp::Double, "a4", "sp", source_offset);
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_OUT_POINT));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit_load(LoadOp::Double, "t0", "sp", size_offset);
        self.emit("li t1, 36");
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", failed));
        self.emit_stack_u32_le_to("t0", tx_dest_offset + 32);
        self.emit_store(StoreOp::Double, "t0", "sp", index_dest_offset);
    }
}
