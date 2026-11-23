use super::super::{
    ckb_abi, CellScriptRuntimeError, CodeGenerator, StoreOp, CKB_SOURCE_GROUP_FLAG, CKB_SOURCE_INPUT, CKB_SOURCE_VIEW_GROUP_INPUT,
    CKB_SOURCE_VIEW_SHIFT,
};
use super::super::{CKB_INDEX_OUT_OF_BOUND, CKB_LENGTH_NOT_ENOUGH, CKB_WITNESS_OWNER_LOCK};

impl CodeGenerator {
    /// Append one already-validated signing-message segment.
    /// t0=pointer/transaction offset, t1=length, t2=segment kind. The caller
    /// keeps the descriptor count and total length in the fixed sighash frame.
    fn emit_sighash_descriptor_append(&mut self, failed: &str) {
        const DESCRIPTOR_COUNT: usize = 40;
        const TOTAL_LENGTH: usize = 48;
        const DESCRIPTORS: usize = 1184;
        self.emit_stack_load("t3", DESCRIPTOR_COUNT);
        self.emit("li t4, 260");
        self.emit(format!("bgeu t3, t4, {failed}"));
        self.emit("li t4, 24");
        self.emit("mul t4, t3, t4");
        self.emit("add t4, sp, t4");
        self.emit_add_immediate("t4", "t4", DESCRIPTORS);
        self.emit_store(StoreOp::Double, "t0", "t4", 0);
        self.emit_store(StoreOp::Double, "t1", "t4", 8);
        self.emit_store(StoreOp::Double, "t2", "t4", 16);
        self.emit_add_immediate("t3", "t3", 1);
        self.emit_stack_store("t3", DESCRIPTOR_COUNT);
        self.emit_stack_load("t5", TOTAL_LENGTH);
        self.emit("add t6, t5, t1");
        self.emit(format!("bltu t6, t5, {failed}"));
        self.emit_stack_store("t6", TOTAL_LENGTH);
    }

    pub(super) fn emit_runtime_sighash_all_zero_lock(&mut self, enabled: bool) {
        const MAX_GROUP_INPUTS: usize = 0;
        const MAX_INPUTS: usize = 8;
        const MAX_EXTRA_WITNESSES: usize = 16;
        const MAX_WITNESS_BYTES: usize = 24;
        const OUT: usize = 32;
        const DESCRIPTOR_COUNT: usize = 40;
        const TOTAL_LENGTH: usize = 48;
        const GROUP_COUNT: usize = 56;
        const INPUT_COUNT: usize = 64;
        const ITERATOR: usize = 72;
        const SIZE: usize = 80;
        const FIRST_SIZE: usize = 88;
        const LOCK_OFFSET: usize = 96;
        const LOCK_LENGTH: usize = 104;
        const PREFIX_COUNT: usize = 112;
        const TX_HASH: usize = 120;
        const LENGTH_PREFIXES: usize = 152;
        const DESCRIPTORS: usize = 1184;
        const RA: usize = 7424;
        const FRAME: usize = 7440;

        self.emit_global("__ckb_sighash_all_zero_lock");
        self.emit_label("__ckb_sighash_all_zero_lock");
        self.emit("# cellscript abi: a0=max_group_inputs<=64,a1=max_inputs<=256,a2=max_extra_witnesses<=64,a3=max_witness_bytes<=65536,a4=out[32]");
        self.emit("# digest = tx_hash || len(first WitnessArgs with lock payload zeroed) || first witness || later group witnesses || witnesses after inputs");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let bound = self.fresh_label("sighash_zero_lock_bound");
        let failed = self.fresh_label("sighash_zero_lock_failed");
        let resolver_failed = self.fresh_label("sighash_zero_lock_resolver_failed");
        let finish = self.fresh_label("sighash_zero_lock_finish");
        let group_scan = self.fresh_label("sighash_group_scan");
        let group_present = self.fresh_label("sighash_group_present");
        let group_limit = self.fresh_label("sighash_group_limit");
        let group_done = self.fresh_label("sighash_group_done");
        let group_limit_absent = self.fresh_label("sighash_group_limit_absent");
        let input_scan = self.fresh_label("sighash_input_scan");
        let input_present = self.fresh_label("sighash_input_present");
        let input_limit = self.fresh_label("sighash_input_limit");
        let input_done = self.fresh_label("sighash_input_done");
        let input_limit_absent = self.fresh_label("sighash_input_limit_absent");
        let first_size_ok = self.fresh_label("sighash_first_size_ok");
        let later_group_scan = self.fresh_label("sighash_later_group_scan");
        let later_group_present = self.fresh_label("sighash_later_group_present");
        let later_group_next = self.fresh_label("sighash_later_group_next");
        let later_group_done = self.fresh_label("sighash_later_group_done");
        let extra_scan = self.fresh_label("sighash_extra_scan");
        let extra_present = self.fresh_label("sighash_extra_present");
        let extra_limit = self.fresh_label("sighash_extra_limit");
        let extra_done = self.fresh_label("sighash_extra_done");
        let extra_limit_absent = self.fresh_label("sighash_extra_limit_absent");
        let abi = self.runtime_abi();

        self.emit_large_addi("sp", "sp", -(FRAME as i64));
        for (register, offset) in
            [("a0", MAX_GROUP_INPUTS), ("a1", MAX_INPUTS), ("a2", MAX_EXTRA_WITNESSES), ("a3", MAX_WITNESS_BYTES), ("a4", OUT)]
        {
            self.emit_stack_store(register, offset);
        }
        self.emit_stack_store("ra", RA);
        self.emit_stack_store("zero", DESCRIPTOR_COUNT);
        self.emit_stack_store("zero", TOTAL_LENGTH);
        self.emit_stack_store("zero", PREFIX_COUNT);

        self.emit_stack_load("t0", MAX_GROUP_INPUTS);
        self.emit(format!("beqz t0, {bound}"));
        self.emit("li t1, 64");
        self.emit(format!("bltu t1, t0, {bound}"));
        self.emit_stack_load("t2", MAX_INPUTS);
        self.emit(format!("beqz t2, {bound}"));
        self.emit("li t1, 256");
        self.emit(format!("bltu t1, t2, {bound}"));
        self.emit(format!("bltu t2, t0, {bound}"));
        self.emit_stack_load("t0", MAX_EXTRA_WITNESSES);
        self.emit("li t1, 64");
        self.emit(format!("bltu t1, t0, {bound}"));
        self.emit_stack_load("t0", MAX_WITNESS_BYTES);
        self.emit(format!("beqz t0, {bound}"));
        self.emit("li t1, 65536");
        self.emit(format!("bltu t1, t0, {bound}"));

        // Count the complete current input Script Group, proving the declared
        // bound with LOAD_INPUT rather than inferring it from witness presence.
        self.emit_stack_store("zero", ITERATOR);
        self.emit_label(&group_scan);
        self.emit_stack_load("t0", ITERATOR);
        self.emit_stack_load("t1", MAX_GROUP_INPUTS);
        self.emit(format!("bgeu t0, t1, {group_limit}"));
        self.emit_stack_store("zero", SIZE);
        self.emit("li a0, 0");
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit("mv a3, t0");
        self.emit(format!("li a4, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_INPUT));
        self.emit("ecall");
        self.emit(format!("beqz a0, {group_present}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {group_present}"));
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {group_done}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&group_present);
        self.emit_stack_load("t0", ITERATOR);
        self.emit_add_immediate("t0", "t0", 1);
        self.emit_stack_store("t0", ITERATOR);
        self.emit(format!("j {group_scan}"));
        self.emit_label(&group_limit);
        self.emit_stack_store("zero", SIZE);
        self.emit("li a0, 0");
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit_stack_load("a3", MAX_GROUP_INPUTS);
        self.emit(format!("li a4, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_INPUT));
        self.emit("ecall");
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {group_limit_absent}"));
        self.emit(format!("beqz a0, {bound}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {bound}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&group_limit_absent);
        self.emit(format!("j {group_done}"));
        self.emit_label(&group_done);
        self.emit_stack_load("t0", ITERATOR);
        self.emit(format!("beqz t0, {failed}"));
        self.emit_stack_store("t0", GROUP_COUNT);

        // Count all transaction inputs independently. The extra witness domain
        // starts exactly at this cardinality.
        self.emit_stack_store("zero", ITERATOR);
        self.emit_label(&input_scan);
        self.emit_stack_load("t0", ITERATOR);
        self.emit_stack_load("t1", MAX_INPUTS);
        self.emit(format!("bgeu t0, t1, {input_limit}"));
        self.emit_stack_store("zero", SIZE);
        self.emit("li a0, 0");
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit("mv a3, t0");
        self.emit(format!("li a4, {CKB_SOURCE_INPUT}"));
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_INPUT));
        self.emit("ecall");
        self.emit(format!("beqz a0, {input_present}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {input_present}"));
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {input_done}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&input_present);
        self.emit_stack_load("t0", ITERATOR);
        self.emit_add_immediate("t0", "t0", 1);
        self.emit_stack_store("t0", ITERATOR);
        self.emit(format!("j {input_scan}"));
        self.emit_label(&input_limit);
        self.emit_stack_store("zero", SIZE);
        self.emit("li a0, 0");
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit_stack_load("a3", MAX_INPUTS);
        self.emit(format!("li a4, {CKB_SOURCE_INPUT}"));
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_INPUT));
        self.emit("ecall");
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {input_limit_absent}"));
        self.emit(format!("beqz a0, {bound}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {bound}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&input_limit_absent);
        self.emit(format!("j {input_done}"));
        self.emit_label(&input_done);
        self.emit_stack_load("t0", ITERATOR);
        self.emit(format!("beqz t0, {failed}"));
        self.emit_stack_store("t0", INPUT_COUNT);

        // Load the canonical raw transaction hash used by ckb-sdk-rust's
        // generate_message before processing any witnesses.
        self.emit("li t0, 32");
        self.emit_stack_store("t0", SIZE);
        self.emit_sp_addi("a0", TX_HASH);
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_TX_HASH));
        self.emit("ecall");
        self.emit(format!("bnez a0, {failed}"));
        self.emit_stack_load("t0", SIZE);
        self.emit("li t1, 32");
        self.emit(format!("bne t0, t1, {failed}"));

        // The first group witness is mandatory. Bound its complete bytes, then
        // resolve the exact WitnessArgs.lock payload span for equal-length zeroing.
        self.emit_stack_store("zero", SIZE);
        self.emit("li a0, 0");
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit("li a3, 0");
        self.emit(format!("li a4, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("beqz a0, {first_size_ok}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {first_size_ok}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&first_size_ok);
        self.emit_stack_load("t0", SIZE);
        self.emit_stack_load("t1", MAX_WITNESS_BYTES);
        self.emit(format!("bltu t1, t0, {bound}"));
        self.emit_stack_store("t0", FIRST_SIZE);
        self.emit(format!("li a0, {}", CKB_SOURCE_VIEW_GROUP_INPUT * CKB_SOURCE_VIEW_SHIFT));
        self.emit(format!("li a1, {CKB_WITNESS_OWNER_LOCK}"));
        self.emit_stack_load("a2", MAX_WITNESS_BYTES);
        self.emit("call __cellscript_witness_bounded_resolve");
        self.emit(format!("bnez a4, {resolver_failed}"));
        self.emit_stack_store("a0", LOCK_OFFSET);
        self.emit_stack_store("a1", LOCK_LENGTH);

        self.emit_sp_addi("t0", TX_HASH);
        self.emit("li t1, 32");
        self.emit("li t2, 0");
        self.emit_sighash_descriptor_append(&bound);
        self.emit_stack_load("t0", FIRST_SIZE);
        self.emit_stack_store("t0", LENGTH_PREFIXES);
        self.emit_sp_addi("t0", LENGTH_PREFIXES);
        self.emit("li t1, 8");
        self.emit("li t2, 0");
        self.emit_sighash_descriptor_append(&bound);
        self.emit("li t0, 0");
        self.emit_stack_load("t1", LOCK_OFFSET);
        self.emit("li t2, 4");
        self.emit_sighash_descriptor_append(&bound);
        self.emit("li t0, 0");
        self.emit_stack_load("t1", LOCK_LENGTH);
        self.emit("li t2, 2");
        self.emit_sighash_descriptor_append(&bound);
        self.emit_stack_load("t0", LOCK_OFFSET);
        self.emit_stack_load("t1", LOCK_LENGTH);
        self.emit("add t0, t0, t1");
        self.emit_stack_load("t1", FIRST_SIZE);
        self.emit("sub t1, t1, t0");
        self.emit("li t2, 4");
        self.emit_sighash_descriptor_append(&bound);
        self.emit("li t0, 1");
        self.emit_stack_store("t0", PREFIX_COUNT);

        // Later group inputs contribute only when their absolute witness exists,
        // matching ScriptSigner::generate_message's filter_map contract.
        self.emit("li t0, 1");
        self.emit_stack_store("t0", ITERATOR);
        self.emit_label(&later_group_scan);
        self.emit_stack_load("t0", ITERATOR);
        self.emit_stack_load("t1", GROUP_COUNT);
        self.emit(format!("bgeu t0, t1, {later_group_done}"));
        self.emit_stack_store("zero", SIZE);
        self.emit("li a0, 0");
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit("mv a3, t0");
        self.emit(format!("li a4, {}", CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT));
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("beqz a0, {later_group_present}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {later_group_present}"));
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {later_group_next}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&later_group_present);
        self.emit_stack_load("t0", SIZE);
        self.emit_stack_load("t1", MAX_WITNESS_BYTES);
        self.emit(format!("bltu t1, t0, {bound}"));
        self.emit_stack_load("t2", PREFIX_COUNT);
        self.emit("li t3, 128");
        self.emit(format!("bgeu t2, t3, {bound}"));
        self.emit("slli t3, t2, 3");
        self.emit("add t3, sp, t3");
        self.emit_add_immediate("t3", "t3", LENGTH_PREFIXES);
        self.emit_store(StoreOp::Double, "t0", "t3", 0);
        self.emit("mv t0, t3");
        self.emit("li t1, 8");
        self.emit("li t2, 0");
        self.emit_sighash_descriptor_append(&bound);
        self.emit("li t0, 0");
        self.emit_stack_load("t1", SIZE);
        self.emit_stack_load("t2", ITERATOR);
        self.emit("slli t2, t2, 1");
        self.emit_add_immediate("t2", "t2", 4);
        self.emit_sighash_descriptor_append(&bound);
        self.emit_stack_load("t0", PREFIX_COUNT);
        self.emit_add_immediate("t0", "t0", 1);
        self.emit_stack_store("t0", PREFIX_COUNT);
        self.emit_label(&later_group_next);
        self.emit_stack_load("t0", ITERATOR);
        self.emit_add_immediate("t0", "t0", 1);
        self.emit_stack_store("t0", ITERATOR);
        self.emit(format!("j {later_group_scan}"));
        self.emit_label(&later_group_done);

        // Include every witness after the transaction input vector. A probe at
        // the declared limit proves there is no silently omitted suffix.
        self.emit_stack_store("zero", ITERATOR);
        self.emit_label(&extra_scan);
        self.emit_stack_load("t0", ITERATOR);
        self.emit_stack_load("t1", MAX_EXTRA_WITNESSES);
        self.emit(format!("bgeu t0, t1, {extra_limit}"));
        self.emit_stack_load("t2", INPUT_COUNT);
        self.emit("add t2, t2, t0");
        self.emit_stack_store("zero", SIZE);
        self.emit("li a0, 0");
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit("mv a3, t2");
        self.emit(format!("li a4, {CKB_SOURCE_INPUT}"));
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("beqz a0, {extra_present}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {extra_present}"));
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {extra_done}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&extra_present);
        self.emit_stack_load("t0", SIZE);
        self.emit_stack_load("t1", MAX_WITNESS_BYTES);
        self.emit(format!("bltu t1, t0, {bound}"));
        self.emit_stack_load("t2", PREFIX_COUNT);
        self.emit("li t3, 128");
        self.emit(format!("bgeu t2, t3, {bound}"));
        self.emit("slli t3, t2, 3");
        self.emit("add t3, sp, t3");
        self.emit_add_immediate("t3", "t3", LENGTH_PREFIXES);
        self.emit_store(StoreOp::Double, "t0", "t3", 0);
        self.emit("mv t0, t3");
        self.emit("li t1, 8");
        self.emit("li t2, 0");
        self.emit_sighash_descriptor_append(&bound);
        self.emit("li t0, 0");
        self.emit_stack_load("t1", SIZE);
        self.emit_stack_load("t2", INPUT_COUNT);
        self.emit_stack_load("t3", ITERATOR);
        self.emit("add t2, t2, t3");
        self.emit("slli t2, t2, 1");
        self.emit_add_immediate("t2", "t2", 3);
        self.emit_sighash_descriptor_append(&bound);
        self.emit_stack_load("t0", PREFIX_COUNT);
        self.emit_add_immediate("t0", "t0", 1);
        self.emit_stack_store("t0", PREFIX_COUNT);
        self.emit_stack_load("t0", ITERATOR);
        self.emit_add_immediate("t0", "t0", 1);
        self.emit_stack_store("t0", ITERATOR);
        self.emit(format!("j {extra_scan}"));
        self.emit_label(&extra_limit);
        self.emit_stack_load("t0", INPUT_COUNT);
        self.emit_stack_load("t1", MAX_EXTRA_WITNESSES);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("zero", SIZE);
        self.emit("li a0, 0");
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit("mv a3, t0");
        self.emit(format!("li a4, {CKB_SOURCE_INPUT}"));
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit("ecall");
        self.emit(format!("li t0, {CKB_INDEX_OUT_OF_BOUND}"));
        self.emit(format!("beq a0, t0, {extra_limit_absent}"));
        self.emit(format!("beqz a0, {bound}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("beq a0, t0, {bound}"));
        self.emit(format!("j {failed}"));
        self.emit_label(&extra_limit_absent);
        self.emit_label(&extra_done);

        self.emit_sp_addi("a0", DESCRIPTORS);
        self.emit_stack_load("a1", DESCRIPTOR_COUNT);
        self.emit("li a2, 0");
        self.emit("li a3, 0");
        self.emit(format!("li a4, {}", abi.load_witness));
        self.emit_stack_load("a5", OUT);
        self.emit_stack_load("a6", TOTAL_LENGTH);
        self.emit("call __cellscript_blake2b_segments");
        self.emit(format!("j {finish}"));

        self.emit_label(&resolver_failed);
        self.emit("mv a0, a4");
        self.emit(format!("j {finish}"));
        self.emit_label(&bound);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SighashBoundExceeded.code()));
        self.emit(format!("j {finish}"));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit_label(&finish);
        self.emit_stack_load("ra", RA);
        self.emit_large_addi("sp", "sp", FRAME as i64);
        self.emit("ret");
    }
}
