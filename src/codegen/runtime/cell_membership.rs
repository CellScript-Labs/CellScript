use super::super::align_frame;
use super::super::{CellScriptRuntimeError, CodeGenerator, CKB_CELL_FIELD_LOCK_HASH, CKB_CELL_FIELD_TYPE_HASH, CKB_ITEM_MISSING};

impl CodeGenerator {
    /// Internal ABI: a0=Cell source, a1=ordinal, a2=expected Script hash field.
    /// Returns 0, ScriptRoleMismatch, or ExactSizeMismatch. Only caller-saved
    /// registers are touched; the 96-byte private frame holds both aligned
    /// hashes and syscall arguments. No nested calls or caller buffers are used.
    pub(super) fn emit_runtime_cell_membership(&mut self) {
        const STACK_FRAME_BYTES: i64 = align_frame(96) as i64;
        self.entry_frame_sizes.insert("__cellscript_require_cell_membership".to_string(), 96);
        self.emit_global("__cellscript_require_cell_membership");
        self.emit_label("__cellscript_require_cell_membership");
        let failed = self.fresh_label("membership_role_failed");
        let malformed = self.fresh_label("membership_hash_malformed");
        let loaded = self.fresh_label("membership_opposite_loaded");
        let success = self.fresh_label("membership_valid");
        let done = self.fresh_label("membership_done");
        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("a0", 72);
        self.emit_stack_store("a1", 80);
        self.emit_stack_store("a2", 88);
        self.emit("li t0, 32");
        self.emit_stack_store("t0", 0);
        self.emit_sp_addi("a0", 8);
        self.emit_sp_addi("a1", 0);
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", self.runtime_abi().load_script_hash));
        self.emit("ecall");
        self.emit(format!("bnez a0, {}", failed));
        self.emit_stack_load("t0", 0);
        self.emit("li t1, 32");
        self.emit(format!("bne t0, t1, {}", malformed));

        for opposite in [false, true] {
            self.emit("li t0, 32");
            self.emit_stack_store("t0", 0);
            self.emit_sp_addi("a0", 40);
            self.emit_sp_addi("a1", 0);
            self.emit("li a2, 0");
            self.emit_stack_load("a3", 80);
            self.emit_stack_load("a4", 72);
            self.emit_stack_load("a5", 88);
            if opposite {
                // Both allowed field numbers are compiler-selected constants.
                // XOR with their difference swaps LockHash and TypeHash.
                self.emit(format!("xori a5, a5, {}", CKB_CELL_FIELD_LOCK_HASH ^ CKB_CELL_FIELD_TYPE_HASH));
            }
            self.emit(format!("li a7, {}", self.runtime_abi().load_cell_by_field));
            self.emit("ecall");
            if opposite {
                self.emit(format!("beqz a0, {}", loaded));
                self.emit_stack_load("t0", 88);
                self.emit(format!("li t1, {}", CKB_CELL_FIELD_LOCK_HASH));
                self.emit(format!("bne t0, t1, {}", failed));
                self.emit(format!("li t0, {}", CKB_ITEM_MISSING));
                self.emit(format!("beq a0, t0, {}", success));
                self.emit(format!("j {}", failed));
                self.emit_label(&loaded);
            } else {
                self.emit(format!("bnez a0, {}", failed));
            }
            self.emit_stack_load("t0", 0);
            self.emit("li t1, 32");
            self.emit(format!("bne t0, t1, {}", malformed));
            for word in 0..4 {
                self.emit_stack_load("t0", 8 + word * 8);
                self.emit_stack_load("t1", 40 + word * 8);
                if word == 0 {
                    self.emit("xor t2, t0, t1");
                } else {
                    self.emit("xor t0, t0, t1");
                    self.emit("or t2, t2, t0");
                }
            }
            self.emit(format!("{} t2, {}", if opposite { "beqz" } else { "bnez" }, failed));
        }
        self.emit_label(&success);
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ExactSizeMismatch.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptRoleMismatch.code()));
        self.emit_label(&done);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }
}
