//! Bounded instance admission over the existing implementation-membership
//! helper. Every expected constraint comes from the current Script commitment;
//! the participant is read from an actual transaction Cell.
use super::*;
use cellscript_artifact_checker::instance_policy as wire;

const HANDLE: usize = 0;
const DEP: usize = 8;
const SUBJECT: usize = 16;
const RA: usize = 24;
const SIZE: usize = 32;
const ARGS_LEN: usize = 40;
const CONFIG_LEN: usize = 48;
const COUNT: usize = 56;
const CURSOR: usize = 64;
const CONSUMED: usize = 72;
const ROW: usize = 80;
const CONSUMER: usize = 96;
const CONFIG: usize = CONSUMER + 53 + 80;
const PARTICIPANT: usize = 368;
const HASH: usize = 688;
const SCHEMA: usize = 720;
const SCHEMA_BYTES: usize = wire::ARGS_SCHEMA_DOMAIN.len() + 8 + wire::MAX_CONSTRAINTS * 8;
const STAGING: usize = 912;
const FRAME: usize = 1808;

impl CodeGenerator {
    /// Read an unaligned LE field. Only `destination` and `scratch` are
    /// clobbered; callers explicitly choose registers distinct from `base`.
    fn emit_instance_le(&mut self, destination: &str, base: &str, offset: usize, width: usize, scratch: &str) {
        self.emit(format!("li {destination}, 0"));
        for byte in (0..width).rev() {
            self.emit(format!("slli {destination}, {destination}, 8"));
            self.emit(format!("lbu {scratch}, {}({base})", offset + byte));
            self.emit(format!("or {destination}, {destination}, {scratch}"));
        }
    }

    /// Reject nonzero bytes at a0 for a1 bytes. Clobbers a0/a1/t0 only;
    /// every caller bounds both address and count before entering this loop.
    fn emit_instance_zero_check(&mut self, invalid: &str) {
        let scan = self.fresh_label("instance_zero_scan");
        let done = self.fresh_label("instance_zero_done");
        self.emit_label(&scan);
        self.emit_near_cond("beqz a1", &done);
        self.emit("lbu t0, 0(a0)");
        self.emit_near_cond("bnez t0", invalid);
        self.emit("addi a0, a0, 1");
        self.emit("addi a1, a1, -1");
        self.emit(format!("j {scan}"));
        self.emit_label(&done);
    }

    fn emit_instance_magic(&mut self, base: &str, offset: usize, expected: &[u8], invalid: &str) {
        // t0/t1 are scratch, base must be a different live register.
        for (index, byte) in expected.iter().enumerate() {
            self.emit(format!("lbu t0, {}({base})", offset + index));
            self.emit(format!("li t1, {byte}"));
            self.emit_near_cond("bne t0,t1", invalid);
        }
    }

    pub(super) fn emit_runtime_instance_participant(&mut self, enabled: bool) {
        let symbol = "__ckb_require_instance_script_participant_from_args";
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit("# cellscript abi: instance policy v1; a0=dep a1=selection a2=length a3=subject view");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::OpenHandleInvalid.code()));
            self.emit("ret");
            return;
        }
        let invalid = self.fresh_label("instance_invalid");
        let done = self.fresh_label("instance_done");
        self.emit(format!("addi sp, sp, -{FRAME}"));
        for (reg, slot) in [("ra", RA), ("a0", DEP), ("a1", HANDLE), ("a3", SUBJECT)] {
            self.emit_stack_store(reg, slot);
        }
        self.emit_near_cond("beqz a1", &invalid);
        self.emit(format!("li t0, {}", wire::SELECTION_BYTES));
        self.emit_near_cond("bne a2,t0", &invalid);
        self.emit("mv t3, a1");
        self.emit_instance_magic("t3", 0, wire::SELECTION_MAGIC, &invalid);
        self.emit_instance_magic("t3", 8, wire::POLICY_MAGIC, &invalid);
        self.emit("lbu t0, 16(t3)");
        self.emit("li t1, 1");
        self.emit_near_cond("bltu t1,t0", &invalid);
        self.emit("lbu t0, 17(t3)");
        self.emit(format!("li t1, {}", wire::MAX_CONSTRAINTS));
        self.emit_near_cond("bltu t1,t0", &invalid);
        self.emit_stack_store("t0", COUNT);
        self.emit_instance_le("t0", "t3", 18, 2, "t1");
        self.emit(format!("li t1, {}", wire::MAX_ARGS));
        self.emit_near_cond("bltu t1,t0", &invalid);
        self.emit_stack_store("t0", ARGS_LEN);
        self.emit_instance_le("t0", "t3", 20, 2, "t1");
        self.emit(format!("li t1, {}", wire::MAX_CONFIG));
        self.emit_near_cond("bltu t1,t0", &invalid);
        self.emit_stack_store("t0", CONFIG_LEN);
        self.emit("addi a0, t3, 22");
        self.emit("li a1, 2");
        self.emit_instance_zero_check(&invalid);

        // LOAD_SCRIPT returns the actual canonical current Script. Exact
        // returned size rejects old root-only args and truncated/long layouts.
        self.emit(format!("li t0, {}", 53 + wire::CONSUMER_ARGS_BYTES));
        self.emit_stack_store("t0", SIZE);
        self.emit_sp_addi("a0", CONSUMER);
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_SCRIPT));
        self.emit("ecall");
        self.emit_near_cond("bnez a0", &invalid);
        self.emit_stack_load("t0", SIZE);
        self.emit(format!("li t1, {}", 53 + wire::CONSUMER_ARGS_BYTES));
        self.emit_near_cond("bne t0,t1", &invalid);
        self.emit_sp_addi("t3", CONSUMER + 53);
        self.emit_instance_magic("t3", 0, wire::CONSUMER_MAGIC, &invalid);
        self.emit_instance_le("t0", "t3", 72, 2, "t1");
        self.emit_stack_load("t1", CONFIG_LEN);
        self.emit_near_cond("bne t0,t1", &invalid);
        self.emit("addi a0, t3, 74");
        self.emit("li a1, 6");
        self.emit_instance_zero_check(&invalid);
        self.emit_sp_addi("a0", CONFIG);
        self.emit_stack_load("t1", CONFIG_LEN);
        self.emit("add a0, a0, t1");
        self.emit(format!("li a1, {}", wire::MAX_CONFIG));
        self.emit("sub a1, a1, t1");
        self.emit_instance_zero_check(&invalid);

        // Authenticate the entire canonical policy before accepting its
        // implementation root, layout or permission predicates.
        self.emit_sp_addi("t3", STAGING);
        for (index, byte) in wire::POLICY_DOMAIN.iter().enumerate() {
            self.emit(format!("li t0, {byte}"));
            self.emit(format!("sb t0, {index}(t3)"));
        }
        self.emit_stack_load("a0", HANDLE);
        self.emit("addi a0, a0, 8");
        self.emit_sp_addi("a1", STAGING + wire::POLICY_DOMAIN.len());
        self.emit(format!("li a2, {}", wire::POLICY_BYTES));
        self.emit("call __cellscript_memcpy_fixed");
        self.emit_sp_addi("a0", STAGING);
        self.emit(format!("li a1, {}", wire::POLICY_DOMAIN.len() + wire::POLICY_BYTES));
        self.emit_sp_addi("a2", HASH);
        self.emit("call __ckb_hash_blake2b_var");
        self.emit_near_cond("bnez a0", &invalid);
        self.emit_sp_addi("a0", HASH);
        self.emit_sp_addi("a1", CONSUMER + 53 + 8);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit_near_cond("bnez a0", &invalid);

        // The old selection is unchanged. Its template Script is checked as
        // implementation evidence, and is never used as a wildcard instance.
        self.emit_stack_load("a0", DEP);
        self.emit_stack_load("a1", HANDLE);
        self.emit("addi a3, a1, 24");
        self.emit(format!("addi a1, a1, {}", wire::IMPLEMENTATION_OFFSET));
        self.emit("li a2, 656");
        self.emit("li a4, 32");
        self.emit("call __ckb_require_cell_dep_open_script_handle");
        self.emit_near_cond("bnez a0", &invalid);

        // Observe the selected participant's whole Script, not just witness
        // bytes. Only input/output (including group-scoped) views are valid.
        let source_ok = self.fresh_label("instance_source_ok");
        let field_ok = self.fresh_label("instance_field_ok");
        self.emit_stack_load("a0", SUBJECT);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        for source in
            [CKB_SOURCE_INPUT, CKB_SOURCE_OUTPUT, CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT, CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT]
        {
            self.emit(format!("li t0, {source}"));
            self.emit_near_cond("beq t0,t2", &source_ok);
        }
        self.emit(format!("j {invalid}"));
        self.emit_label(&source_ok);
        self.emit("mv a3, t1");
        self.emit("mv a4, t2");
        self.emit_stack_load("t3", HANDLE);
        self.emit(format!("lbu t0, {}(t3)", wire::IMPLEMENTATION_OFFSET + 17));
        self.emit(format!("li a5, {}", ckb_abi::cell_field::LOCK));
        self.emit_near_cond("beqz t0", &field_ok);
        self.emit(format!("li a5, {}", ckb_abi::cell_field::TYPE));
        self.emit_label(&field_ok);
        self.emit(format!("li t0, {}", 53 + wire::MAX_ARGS));
        self.emit_stack_store("t0", SIZE);
        self.emit_sp_addi("a0", PARTICIPANT);
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_CELL_BY_FIELD));
        self.emit("ecall");
        self.emit_near_cond("bnez a0", &invalid);
        self.emit_stack_load("t0", ARGS_LEN);
        self.emit("addi t0, t0, 53");
        self.emit_stack_load("t1", SIZE);
        self.emit_near_cond("bne t0,t1", &invalid);
        self.emit_sp_addi("a0", PARTICIPANT + 16);
        self.emit_stack_load("a1", HANDLE);
        self.emit(format!("addi a1, a1, {}", wire::IMPLEMENTATION_OFFSET + 356));
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit_near_cond("bnez a0", &invalid);
        self.emit_stack_load("t3", HANDLE);
        self.emit(format!("lbu t0, {}(t3)", wire::IMPLEMENTATION_OFFSET + 205));
        let hash_type_ok = self.fresh_label("instance_hash_type");
        self.emit("li t1, 3");
        self.emit_near_cond("bne t0,t1", &hash_type_ok);
        self.emit("li t0, 4");
        self.emit_label(&hash_type_ok);
        self.emit_stack_load_byte("t1", PARTICIPANT + 48);
        self.emit_near_cond("bne t0,t1", &invalid);
        self.emit_sp_addi("a0", PARTICIPANT + 53);
        self.emit_stack_load("a1", HANDLE);
        self.emit(format!("addi a1, a1, {}", wire::ARGS_OFFSET));
        self.emit_stack_load("a2", ARGS_LEN);
        self.emit("call __cellscript_memcmp_fixed");
        self.emit_near_cond("bnez a0", &invalid);
        self.emit_stack_load("a0", HANDLE);
        self.emit(format!("addi a0, a0, {}", wire::ARGS_OFFSET));
        self.emit_stack_load("t1", ARGS_LEN);
        self.emit("add a0, a0, t1");
        self.emit(format!("li a1, {}", wire::MAX_ARGS));
        self.emit("sub a1, a1, t1");
        self.emit_instance_zero_check(&invalid);
        self.emit_sp_addi("a0", PARTICIPANT);
        self.emit_stack_load("a1", SIZE);
        self.emit_sp_addi("a2", HASH);
        self.emit("call __ckb_hash_blake2b_var");
        self.emit_near_cond("bnez a0", &invalid);
        self.emit_sp_addi("a0", HASH);
        self.emit_sp_addi("a1", CONSUMER + 53 + 40);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit_near_cond("bnez a0", &invalid);
        let constrained = self.fresh_label("instance_constrained");
        self.emit_stack_load("t3", HANDLE);
        self.emit("lbu t0, 16(t3)");
        self.emit_near_cond("bnez t0", &constrained);
        self.emit_sp_addi("a0", HASH);
        self.emit(format!("addi a1, t3, {}", wire::IMPLEMENTATION_OFFSET + 324));
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit_near_cond("bnez a0", &invalid);
        self.emit_label(&constrained);

        self.emit_instance_constraints(&invalid);
        self.emit("li a0, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::OpenHandleInvalid.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", RA);
        self.emit(format!("addi sp, sp, {FRAME}"));
        self.emit("ret");
    }

    fn emit_instance_constraints(&mut self, invalid: &str) {
        let scan = self.fresh_label("instance_rule_scan");
        let finished = self.fresh_label("instance_rules_done");
        let numeric = self.fresh_label("instance_rule_numeric");
        let config = self.fresh_label("instance_rule_config");
        let any = self.fresh_label("instance_rule_any");
        let next = self.fresh_label("instance_rule_next");
        // Canonical schema preimage starts zeroed, including unused rows.
        self.emit_sp_addi("a0", SCHEMA);
        self.emit(format!("li a1, {SCHEMA_BYTES}"));
        let clear = self.fresh_label("instance_schema_clear");
        self.emit_label(&clear);
        self.emit("sb zero, 0(a0)");
        self.emit("addi a0, a0, 1");
        self.emit("addi a1, a1, -1");
        self.emit_near_cond("bnez a1", &clear);
        self.emit_sp_addi("t3", SCHEMA);
        for (index, byte) in wire::ARGS_SCHEMA_DOMAIN.iter().enumerate() {
            self.emit(format!("li t0, {byte}"));
            self.emit(format!("sb t0, {index}(t3)"));
        }
        self.emit_stack_load("t0", ARGS_LEN);
        self.emit_stack_store_byte("t0", SCHEMA + wire::ARGS_SCHEMA_DOMAIN.len());
        self.emit("srli t0, t0, 8");
        self.emit_stack_store_byte("t0", SCHEMA + wire::ARGS_SCHEMA_DOMAIN.len() + 1);
        self.emit_stack_load("t0", COUNT);
        self.emit_stack_store_byte("t0", SCHEMA + wire::ARGS_SCHEMA_DOMAIN.len() + 2);
        self.emit_stack_store("zero", CURSOR);
        self.emit_stack_store("zero", CONSUMED);
        self.emit_label(&scan);
        self.emit_stack_load("t0", CURSOR);
        self.emit_stack_load("t1", COUNT);
        self.emit_near_cond("beq t0,t1", &finished);
        self.emit("li t1, 48");
        self.emit("mul t1, t0, t1");
        self.emit_stack_load("t3", HANDLE);
        self.emit("addi t3, t3, 88");
        self.emit("add t3, t3, t1");
        self.emit_stack_store("t3", ROW);
        self.emit("lbu t2, 0(t3)");
        self.emit("li t1, 3");
        self.emit_near_cond("bltu t1,t2", invalid);
        // Exact mode accepts only exact-byte fields.
        self.emit_stack_load("t4", HANDLE);
        self.emit("lbu t1, 16(t4)");
        self.emit("seqz t1, t1");
        self.emit("mul t1, t1, t2");
        self.emit_near_cond("bnez t1", invalid);
        self.emit_instance_le("t4", "t3", 2, 2, "t1");
        self.emit_near_cond("beqz t4", invalid);
        self.emit_instance_le("t5", "t3", 4, 2, "t1");
        self.emit_stack_load("t1", CONSUMED);
        self.emit_near_cond("bne t5,t1", invalid);
        self.emit("add t1, t1, t4");
        self.emit_stack_load("t6", ARGS_LEN);
        self.emit_near_cond("bltu t6,t1", invalid);
        self.emit_stack_store("t1", CONSUMED);
        self.emit("slli t0, t0, 3");
        self.emit_sp_addi("t6", SCHEMA + wire::ARGS_SCHEMA_DOMAIN.len() + 8);
        self.emit("add t6, t6, t0");
        for (source, destination) in [(4, 0), (5, 1), (2, 2), (3, 3)] {
            self.emit(format!("lbu t1, {source}(t3)"));
            self.emit(format!("sb t1, {destination}(t6)"));
        }
        self.emit("addi t1, t2, -1");
        self.emit("seqz t1, t1");
        self.emit("sb t1, 4(t6)");
        self.emit("lbu t0, 1(t3)");
        self.emit_near_cond("bnez t0", invalid);
        self.emit("addi a0, t3, 8");
        self.emit("li a1, 8");
        self.emit_instance_zero_check(invalid);
        self.emit("li t1, 2");
        self.emit_near_cond("beq t2,t1", &config);
        self.emit_instance_le("t0", "t3", 6, 2, "t1");
        self.emit_near_cond("bnez t0", invalid);
        self.emit("li t1, 1");
        self.emit_near_cond("beq t2,t1", &numeric);
        self.emit_near_cond("bnez t2", &any);
        // Exact bytes: width <=32, exact value, zero padding.
        self.emit("li t0, 32");
        self.emit_near_cond("bltu t0,t4", invalid);
        self.emit("addi a0, t3, 16");
        self.emit("add a0, a0, t4");
        self.emit("sub a1, t0, t4");
        self.emit_instance_zero_check(invalid);
        self.emit("addi a0, t3, 16");
        self.emit_sp_addi("a1", PARTICIPANT + 53);
        self.emit("add a1, a1, t5");
        self.emit("mv a2, t4");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit_near_cond("bnez a0", invalid);
        self.emit(format!("j {next}"));

        self.emit_label(&config);
        self.emit_instance_le("t2", "t3", 6, 2, "t1");
        self.emit("add t0, t2, t4");
        self.emit_stack_load("t1", CONFIG_LEN);
        self.emit_near_cond("bltu t1,t0", invalid);
        self.emit_sp_addi("a0", CONFIG);
        self.emit("add a0, a0, t2");
        self.emit_sp_addi("a1", PARTICIPANT + 53);
        self.emit("add a1, a1, t5");
        self.emit("mv a2, t4");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit_near_cond("bnez a0", invalid);
        self.emit_stack_load("t3", ROW);
        self.emit_label(&any);
        self.emit("addi a0, t3, 16");
        self.emit("li a1, 32");
        self.emit_instance_zero_check(invalid);
        self.emit(format!("j {next}"));

        self.emit_label(&numeric);
        let width_ok = self.fresh_label("instance_numeric_width");
        for width in [1, 2, 4, 8] {
            self.emit(format!("li t0, {width}"));
            self.emit_near_cond("beq t4,t0", &width_ok);
        }
        self.emit(format!("j {invalid}"));
        self.emit_label(&width_ok);
        self.emit_instance_le("t1", "t3", 16, 8, "t0");
        self.emit_instance_le("t2", "t3", 24, 8, "t0");
        self.emit_near_cond("bltu t2,t1", invalid);
        let fits = self.fresh_label("instance_numeric_fits");
        self.emit("li t0, 8");
        self.emit_near_cond("beq t4,t0", &fits);
        self.emit("slli t0, t4, 3");
        self.emit("srl t0, t2, t0");
        self.emit_near_cond("bnez t0", invalid);
        self.emit_label(&fits);
        self.emit_sp_addi("t6", PARTICIPANT + 53);
        self.emit("add t6, t6, t5");
        self.emit("add t6, t6, t4");
        self.emit("li t5, 0");
        let number_scan = self.fresh_label("instance_number_scan");
        self.emit_label(&number_scan);
        self.emit("addi t6, t6, -1");
        self.emit("lbu t0, 0(t6)");
        self.emit("slli t5, t5, 8");
        self.emit("or t5, t5, t0");
        self.emit("addi t4, t4, -1");
        self.emit_near_cond("bnez t4", &number_scan);
        self.emit_near_cond("bltu t5,t1", invalid);
        self.emit_near_cond("bltu t2,t5", invalid);
        self.emit("addi a0, t3, 32");
        self.emit("li a1, 16");
        self.emit_instance_zero_check(invalid);
        self.emit_label(&next);
        self.emit_stack_load("t0", CURSOR);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", CURSOR);
        self.emit(format!("j {scan}"));
        self.emit_label(&finished);
        self.emit_stack_load("t0", CONSUMED);
        self.emit_stack_load("t1", ARGS_LEN);
        self.emit_near_cond("bne t0,t1", invalid);
        self.emit_stack_load("t1", COUNT);
        self.emit("li t2, 48");
        self.emit("mul t1, t1, t2");
        self.emit_stack_load("a0", HANDLE);
        self.emit("addi a0, a0, 88");
        self.emit("add a0, a0, t1");
        self.emit(format!("li a1, {}", wire::MAX_CONSTRAINTS * wire::CONSTRAINT_BYTES));
        self.emit("sub a1, a1, t1");
        self.emit_instance_zero_check(invalid);
        self.emit_sp_addi("a0", SCHEMA);
        self.emit(format!("li a1, {SCHEMA_BYTES}"));
        self.emit_sp_addi("a2", HASH);
        self.emit("call __ckb_hash_blake2b_var");
        self.emit_near_cond("bnez a0", invalid);
        self.emit_sp_addi("a0", HASH);
        self.emit_stack_load("a1", HANDLE);
        self.emit("addi a1, a1, 56");
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit_near_cond("bnez a0", invalid);
    }
}
