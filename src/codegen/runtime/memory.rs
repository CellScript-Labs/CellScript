use super::super::{CodeGenerator, LoadOp, StoreOp};

impl CodeGenerator {
    pub(super) fn emit_runtime_memcmp_fixed(&mut self) {
        self.emit_global("__cellscript_memcmp_fixed");
        self.emit_label("__cellscript_memcmp_fixed");
        self.emit("# cellscript abi: fixed-byte helper compares a0/a1 for a2 bytes; returns a0=0 when equal");
        let loop_label = ".L__cellscript_memcmp_fixed_loop";
        let mismatch_label = ".L__cellscript_memcmp_fixed_mismatch";
        let equal_label = ".L__cellscript_memcmp_fixed_equal";
        self.emit(format!("beqz a2, {}", equal_label));
        self.emit_label(loop_label);
        self.emit_load(LoadOp::ByteUnsigned, "t0", "a0", 0);
        self.emit_load(LoadOp::ByteUnsigned, "t1", "a1", 0);
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {}", mismatch_label));
        self.emit_add_immediate("a0", "a0", 1);
        self.emit_add_immediate("a1", "a1", 1);
        self.emit_add_immediate("a2", "a2", -1);
        self.emit(format!("bnez a2, {}", loop_label));
        self.emit_label(equal_label);
        self.emit("li a0, 0");
        self.emit("ret");
        self.emit_label(mismatch_label);
        self.emit("li a0, 1");
        self.emit("ret");
    }

    pub(super) fn emit_runtime_memzero_fixed(&mut self) {
        self.emit_global("__cellscript_memzero_fixed");
        self.emit_label("__cellscript_memzero_fixed");
        self.emit("# cellscript abi: fixed-byte helper checks a0 for a1 zero bytes; returns a0=0 when all zero");
        let loop_label = ".L__cellscript_memzero_fixed_loop";
        let mismatch_label = ".L__cellscript_memzero_fixed_mismatch";
        let equal_label = ".L__cellscript_memzero_fixed_equal";
        self.emit(format!("beqz a1, {}", equal_label));
        self.emit_label(loop_label);
        self.emit_load(LoadOp::ByteUnsigned, "t0", "a0", 0);
        self.emit(format!("bnez t0, {}", mismatch_label));
        self.emit_add_immediate("a0", "a0", 1);
        self.emit_add_immediate("a1", "a1", -1);
        self.emit(format!("bnez a1, {}", loop_label));
        self.emit_label(equal_label);
        self.emit("li a0, 0");
        self.emit("ret");
        self.emit_label(mismatch_label);
        self.emit("li a0, 1");
        self.emit("ret");
    }

    pub(super) fn emit_runtime_memcpy_fixed(&mut self) {
        self.emit_global("__cellscript_memcpy_fixed");
        self.emit_label("__cellscript_memcpy_fixed");
        self.emit("# cellscript abi: fixed-byte helper copies a0 to a1 for a2 bytes; returns a0=0");
        let loop_label = ".L__cellscript_memcpy_fixed_loop";
        let done_label = ".L__cellscript_memcpy_fixed_done";
        self.emit(format!("beqz a2, {}", done_label));
        self.emit_label(loop_label);
        self.emit_load(LoadOp::ByteUnsigned, "t0", "a0", 0);
        self.emit_store(StoreOp::Byte, "t0", "a1", 0);
        self.emit_add_immediate("a0", "a0", 1);
        self.emit_add_immediate("a1", "a1", 1);
        self.emit_add_immediate("a2", "a2", -1);
        self.emit(format!("bnez a2, {}", loop_label));
        self.emit_label(done_label);
        self.emit("li a0, 0");
        self.emit("ret");
    }

    pub(super) fn emit_runtime_size_guards(&mut self) {
        self.emit_global("__cellscript_require_min_size");
        self.emit_label("__cellscript_require_min_size");
        self.emit("# cellscript abi: returns a0=0 when actual size a0 is at least required size a1");
        self.emit("slt a0, a0, a1");
        self.emit("ret");

        self.emit_global("__cellscript_require_exact_size");
        self.emit_label("__cellscript_require_exact_size");
        self.emit("# cellscript abi: returns a0=0 when actual size a0 equals expected size a1");
        self.emit("sub a0, a0, a1");
        self.emit("ret");
    }
}
