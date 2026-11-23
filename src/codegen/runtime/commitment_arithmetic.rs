use super::super::align_frame;
use super::super::{CellScriptRuntimeError, CodeGenerator, LoadOp, StoreOp};

impl CodeGenerator {
    pub(super) fn emit_runtime_load_u64_le_helper(&mut self) {
        self.emit_global("__cellscript_load_u64_le");
        self.emit_label("__cellscript_load_u64_le");
        self.emit("# cellscript abi: load unaligned little-endian u64 from pointer a0");
        self.emit("li a1, 0");
        for byte_index in 0..8 {
            self.emit_load(LoadOp::ByteUnsigned, "t0", "a0", byte_index);
            if byte_index != 0 {
                self.emit(format!("slli t0, t0, {}", byte_index * 8));
            }
            self.emit("or a1, a1, t0");
        }
        self.emit_add_immediate("a0", "a1", 0);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_mul_u128_to_u256_helper(&mut self) {
        const STACK_FRAME_BYTES: i64 = align_frame(96) as i64;
        self.emit_global("__cellscript_mul_u128_to_u256");
        self.emit_label("__cellscript_mul_u128_to_u256");
        self.emit("# cellscript abi: u128*u128 -> u256 limbs; args a0=left_ptr a1=right_ptr a2=out32_ptr");
        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_store(StoreOp::Double, "ra", "sp", 88);
        self.emit_store(StoreOp::Double, "a0", "sp", 0);
        self.emit_store(StoreOp::Double, "a1", "sp", 8);
        self.emit_store(StoreOp::Double, "a2", "sp", 16);

        self.emit_load(LoadOp::Double, "a0", "sp", 0);
        self.emit("call __cellscript_load_u64_le");
        self.emit_store(StoreOp::Double, "a0", "sp", 24);
        self.emit_load(LoadOp::Double, "a0", "sp", 0);
        self.emit_add_immediate("a0", "a0", 8);
        self.emit("call __cellscript_load_u64_le");
        self.emit_store(StoreOp::Double, "a0", "sp", 32);
        self.emit_load(LoadOp::Double, "a0", "sp", 8);
        self.emit("call __cellscript_load_u64_le");
        self.emit_store(StoreOp::Double, "a0", "sp", 40);
        self.emit_load(LoadOp::Double, "a0", "sp", 8);
        self.emit_add_immediate("a0", "a0", 8);
        self.emit("call __cellscript_load_u64_le");
        self.emit_store(StoreOp::Double, "a0", "sp", 48);

        self.emit_load(LoadOp::Double, "t0", "sp", 24);
        self.emit_load(LoadOp::Double, "t1", "sp", 40);
        self.emit("mul t2, t0, t1");
        self.emit("mulhu t3, t0, t1");
        self.emit_store(StoreOp::Double, "t2", "sp", 56);

        self.emit_load(LoadOp::Double, "t0", "sp", 24);
        self.emit_load(LoadOp::Double, "t1", "sp", 48);
        self.emit("mul t4, t0, t1");
        self.emit("mulhu t5, t0, t1");

        self.emit_load(LoadOp::Double, "t0", "sp", 32);
        self.emit_load(LoadOp::Double, "t1", "sp", 40);
        self.emit("mul t6, t0, t1");
        self.emit("mulhu a3, t0, t1");

        self.emit("add t0, t3, t4");
        self.emit("sltu a4, t0, t3");
        self.emit("add t1, t0, t6");
        self.emit("sltu a5, t1, t0");
        self.emit("add a4, a4, a5");
        self.emit_store(StoreOp::Double, "t1", "sp", 64);

        self.emit_load(LoadOp::Double, "t0", "sp", 32);
        self.emit_load(LoadOp::Double, "t1", "sp", 48);
        self.emit("mul a5, t0, t1");
        self.emit("mulhu a6, t0, t1");

        self.emit("add t2, t5, a3");
        self.emit("sltu a7, t2, t5");
        self.emit("add t3, t2, a5");
        self.emit("sltu t4, t3, t2");
        self.emit("add t5, t3, a4");
        self.emit("sltu t6, t5, t3");
        self.emit_store(StoreOp::Double, "t5", "sp", 72);
        self.emit("add t0, a6, a7");
        self.emit("add t0, t0, t4");
        self.emit("add t0, t0, t6");
        self.emit_store(StoreOp::Double, "t0", "sp", 80);

        self.emit_load(LoadOp::Double, "t0", "sp", 16);
        self.emit_load(LoadOp::Double, "t1", "sp", 56);
        self.emit_store(StoreOp::Double, "t1", "t0", 0);
        self.emit_load(LoadOp::Double, "t1", "sp", 64);
        self.emit_store(StoreOp::Double, "t1", "t0", 8);
        self.emit_load(LoadOp::Double, "t1", "sp", 72);
        self.emit_store(StoreOp::Double, "t1", "t0", 16);
        self.emit_load(LoadOp::Double, "t1", "sp", 80);
        self.emit_store(StoreOp::Double, "t1", "t0", 24);
        self.emit_load(LoadOp::Double, "ra", "sp", 88);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_add_u256_helper(&mut self) {
        self.emit_global("__cellscript_add_u256");
        self.emit_label("__cellscript_add_u256");
        self.emit("# cellscript abi: checked u256 addition; args a0=left32_ptr a1=right32_ptr a2=out32_ptr, returns carry in a0");
        self.emit("li a3, 0");
        for limb_offset in [0, 8, 16, 24] {
            self.emit_load(LoadOp::Double, "t0", "a0", limb_offset);
            self.emit_load(LoadOp::Double, "t1", "a1", limb_offset);
            self.emit("add t2, t0, t1");
            self.emit("sltu t3, t2, t0");
            self.emit("add t2, t2, a3");
            self.emit("sltu t4, t2, a3");
            self.emit_store(StoreOp::Double, "t2", "a2", limb_offset);
            self.emit("add a3, t3, t4");
        }
        self.emit_add_immediate("a0", "a3", 0);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_c256_product_requirement_helper(&mut self, symbol: &str, detail: &str, equality: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(112) as i64;
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {} with overflow-safe C256 product comparison", detail));
        self.emit("# cellscript abi: args a0..a3 are u128 little-endian pointers");
        let bad_expected = self.fresh_label("c256_operand_invalid");
        let mismatch = self.fresh_label("c256_product_mismatch");
        let success = self.fresh_label("c256_product_ok");
        let done = self.fresh_label("c256_product_done");

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 104);
        self.emit_stack_store("a0", 0);
        self.emit_stack_store("a1", 8);
        self.emit_stack_store("a2", 16);
        self.emit_stack_store("a3", 24);
        self.emit(format!("beqz a0, {}", bad_expected));
        self.emit(format!("beqz a1, {}", bad_expected));
        self.emit(format!("beqz a2, {}", bad_expected));
        self.emit(format!("beqz a3, {}", bad_expected));

        self.emit_stack_load("a0", 0);
        self.emit_stack_load("a1", 8);
        self.emit_sp_addi("a2", 32);
        self.emit("call __cellscript_mul_u128_to_u256");
        self.emit_stack_load("a0", 16);
        self.emit_stack_load("a1", 24);
        self.emit_sp_addi("a2", 64);
        self.emit("call __cellscript_mul_u128_to_u256");

        for limb_offset in [24, 16, 8, 0] {
            self.emit_stack_load("t0", 32 + limb_offset);
            self.emit_stack_load("t1", 64 + limb_offset);
            if equality {
                self.emit("sub t2, t0, t1");
                self.emit(format!("bnez t2, {}", mismatch));
            } else {
                self.emit("sltu t2, t0, t1");
                self.emit(format!("bnez t2, {}", success));
                self.emit("sltu t2, t1, t0");
                self.emit(format!("bnez t2, {}", mismatch));
            }
        }

        self.emit_label(&success);
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::AggregateAmountMismatch.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", 104);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_c256_sum2_product_requirement_helper(&mut self, symbol: &str, detail: &str, equality: bool) {
        const STACK_FRAME_BYTES: i64 = align_frame(272) as i64;
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {} with checked u256 product sums", detail));
        self.emit("# cellscript abi: args a0..a7 are u128 little-endian pointers; compares a0*a1+a2*a3 with a4*a5+a6*a7");
        let bad_expected = self.fresh_label("c256_sum_operand_invalid");
        let mismatch = self.fresh_label("c256_sum_mismatch");
        let success = self.fresh_label("c256_sum_ok");
        let done = self.fresh_label("c256_sum_done");

        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 264);
        for (index, register) in ["a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7"].into_iter().enumerate() {
            self.emit_store(StoreOp::Double, register, "sp", index * 8);
            self.emit(format!("beqz {}, {}", register, bad_expected));
        }

        self.emit_stack_load("a0", 0);
        self.emit_stack_load("a1", 8);
        self.emit_sp_addi("a2", 64);
        self.emit("call __cellscript_mul_u128_to_u256");
        self.emit_stack_load("a0", 16);
        self.emit_stack_load("a1", 24);
        self.emit_sp_addi("a2", 96);
        self.emit("call __cellscript_mul_u128_to_u256");
        self.emit_sp_addi("a0", 64);
        self.emit_sp_addi("a1", 96);
        self.emit_sp_addi("a2", 128);
        self.emit("call __cellscript_add_u256");
        self.emit(format!("bnez a0, {}", mismatch));

        self.emit_stack_load("a0", 32);
        self.emit_stack_load("a1", 40);
        self.emit_sp_addi("a2", 160);
        self.emit("call __cellscript_mul_u128_to_u256");
        self.emit_stack_load("a0", 48);
        self.emit_stack_load("a1", 56);
        self.emit_sp_addi("a2", 192);
        self.emit("call __cellscript_mul_u128_to_u256");
        self.emit_sp_addi("a0", 160);
        self.emit_sp_addi("a1", 192);
        self.emit_sp_addi("a2", 224);
        self.emit("call __cellscript_add_u256");
        self.emit(format!("bnez a0, {}", mismatch));

        for limb_offset in [24, 16, 8, 0] {
            self.emit_stack_load("t0", 128 + limb_offset);
            self.emit_stack_load("t1", 224 + limb_offset);
            if equality {
                self.emit("sub t2, t0, t1");
                self.emit(format!("bnez t2, {}", mismatch));
            } else {
                self.emit("sltu t2, t0, t1");
                self.emit(format!("bnez t2, {}", success));
                self.emit("sltu t2, t1, t0");
                self.emit(format!("bnez t2, {}", mismatch));
            }
        }

        self.emit_label(&success);
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&bad_expected);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::FixedByteComparisonUnresolved.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::AggregateAmountMismatch.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", 264);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }
}
