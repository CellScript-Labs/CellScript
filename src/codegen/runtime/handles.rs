//! Exact, compatible-open and deployment-line handle admission emitters.
use super::*;

impl CodeGenerator {
    pub(super) fn emit_runtime_exact_script_handle_requirement_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        field_id: u64,
        expected_class: u8,
        expected_role: u8,
        identity_offset: usize,
        enabled: bool,
    ) {
        use crate::script_handle_contract::{
            EXACT_SCRIPT_HANDLE_BYTES, EXACT_SCRIPT_HANDLE_CLASS_OFFSET, EXACT_SCRIPT_HANDLE_HASH_BYTES, EXACT_SCRIPT_HANDLE_MAGIC,
            EXACT_SCRIPT_HANDLE_ROLE_OFFSET,
        };

        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: exact Script handle requirement ({detail})"));
        self.emit("# cellscript abi: args a0=SourceView, a1=handle_ptr, a2=handle_len, a3=handle_hash_ptr, a4=handle_hash_len");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const HANDLE_PTR: usize = 0;
        const EXPECTED_HASH_PTR: usize = 8;
        const VIEW: usize = 16;
        const INDEX: usize = 24;
        const SOURCE: usize = 32;
        const SIZE: usize = 40;
        const IDENTITY_HASH: usize = 48;
        const HANDLE_HASH: usize = 80;
        const RA: usize = 120;
        const FRAME: usize = 128;

        let invalid_source = self.fresh_label("exact_handle_source_invalid");
        let load_failed = self.fresh_label("exact_handle_identity_load_failed");
        let invalid_handle = self.fresh_label("exact_handle_invalid");
        let done = self.fresh_label("exact_handle_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-(FRAME as i64));
        self.emit(format!("sd ra, {RA}(sp)"));
        self.emit(format!("sd a1, {HANDLE_PTR}(sp)"));
        self.emit(format!("sd a3, {EXPECTED_HASH_PTR}(sp)"));
        self.emit(format!("sd a0, {VIEW}(sp)"));
        self.emit(format!("beqz a1, {invalid_handle}"));
        self.emit(format!("beqz a3, {invalid_handle}"));
        self.emit(format!("li t0, {EXACT_SCRIPT_HANDLE_BYTES}"));
        self.emit("sub t1, a2, t0");
        self.emit(format!("bnez t1, {invalid_handle}"));
        self.emit(format!("li t0, {EXACT_SCRIPT_HANDLE_HASH_BYTES}"));
        self.emit("sub t1, a4, t0");
        self.emit(format!("bnez t1, {invalid_handle}"));

        self.emit(format!("ld t3, {HANDLE_PTR}(sp)"));
        for (offset, byte) in EXACT_SCRIPT_HANDLE_MAGIC.iter().enumerate() {
            self.emit(format!("lbu t0, {offset}(t3)"));
            self.emit(format!("li t1, {byte}"));
            self.emit(format!("bne t0, t1, {invalid_handle}"));
        }
        self.emit(format!("lbu t0, {EXACT_SCRIPT_HANDLE_CLASS_OFFSET}(t3)"));
        self.emit(format!("li t1, {expected_class}"));
        self.emit(format!("bne t0, t1, {invalid_handle}"));
        self.emit(format!("lbu t0, {EXACT_SCRIPT_HANDLE_ROLE_OFFSET}(t3)"));
        self.emit(format!("li t1, {expected_role}"));
        self.emit(format!("bne t0, t1, {invalid_handle}"));

        self.emit(format!("ld a0, {HANDLE_PTR}(sp)"));
        self.emit(format!("li a1, {EXACT_SCRIPT_HANDLE_BYTES}"));
        self.emit(format!("addi a2, sp, {HANDLE_HASH}"));
        self.emit("call __ckb_hash_blake2b_var");
        self.emit(format!("bnez a0, {invalid_handle}"));
        self.emit(format!("addi a0, sp, {HANDLE_HASH}"));
        self.emit(format!("ld a1, {EXPECTED_HASH_PTR}(sp)"));
        self.emit(format!("li a2, {EXACT_SCRIPT_HANDLE_HASH_BYTES}"));
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {invalid_handle}"));

        self.emit(format!("ld a0, {VIEW}(sp)"));
        self.emit_decode_source_view_to_t1_t2(&invalid_source);
        self.emit(format!("sd t1, {INDEX}(sp)"));
        self.emit(format!("sd t2, {SOURCE}(sp)"));
        self.emit(format!("li t0, {EXACT_SCRIPT_HANDLE_HASH_BYTES}"));
        self.emit(format!("sd t0, {SIZE}(sp)"));
        self.emit(format!("addi a0, sp, {IDENTITY_HASH}"));
        self.emit(format!("addi a1, sp, {SIZE}"));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {INDEX}(sp)"));
        self.emit(format!("ld a4, {SOURCE}(sp)"));
        self.emit(format!("li a5, {field_id}"));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {load_failed}"));
        self.emit(format!("ld t0, {SIZE}(sp)"));
        self.emit(format!("li t1, {EXACT_SCRIPT_HANDLE_HASH_BYTES}"));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {invalid_handle}"));
        self.emit(format!("addi a0, sp, {IDENTITY_HASH}"));
        self.emit(format!("ld a1, {HANDLE_PTR}(sp)"));
        self.emit(format!("addi a1, a1, {identity_offset}"));
        self.emit(format!("li a2, {EXACT_SCRIPT_HANDLE_HASH_BYTES}"));
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {invalid_handle}"));
        self.emit("li a0, 0");
        self.emit(format!("j {done}"));

        self.emit_label(&invalid_source);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {done}"));
        self.emit_label(&load_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {done}"));
        self.emit_label(&invalid_handle);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ExactScriptHandleInvalid.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {RA}(sp)"));
        self.emit_stack_adjustment(FRAME as i64);
        self.emit("ret");
    }

    /// Conditional branches reach at most +-2048 bytes and the assembler
    /// performs no relaxation; route every far conditional through a local
    /// trampoline so long helpers never wrap a branch immediate.
    pub(crate) fn emit_near_cond(&mut self, branch: &str, far: &str) {
        let fail = self.fresh_label("near_cond_fail");
        let skip = self.fresh_label("near_cond_skip");
        self.emit(format!("{branch},{fail}"));
        self.emit(format!("j {skip}"));
        self.emit_label(&fail);
        self.emit(format!("j {far}"));
        self.emit_label(&skip);
    }

    /// Compatible-open handle requirement helper (#28 H2): validates the
    /// 656-byte selection witness structurally, recomputes the five-level
    /// authorization-tree membership with the frozen domain strings and
    /// compares the reconstructed policy root against the caller-supplied
    /// expected root, then binds the member's complete Script hash to the
    /// selected CellDep's Lock/Type (or data) hash. Every mismatch fails
    /// closed with OpenHandleInvalid.
    #[allow(clippy::too_many_lines)]
    /// The from-args variant derives the expected authorization root from
    /// the current Script's committed args (exactly 32 bytes) instead of a
    /// caller-supplied constant: the witness never authorizes its own root.
    pub(super) fn emit_runtime_open_handle_requirement_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        script_variant: bool,
        enabled: bool,
    ) {
        self.emit_open_handle_requirement_body(symbol, detail, script_variant, false, enabled)
    }

    pub(super) fn emit_runtime_open_handle_from_args_requirement_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        script_variant: bool,
        enabled: bool,
    ) {
        self.emit_open_handle_requirement_body(symbol, detail, script_variant, true, enabled)
    }

    pub(super) fn emit_open_handle_requirement_body(
        &mut self,
        symbol: &str,
        detail: &str,
        script_variant: bool,
        root_from_args: bool,
        enabled: bool,
    ) {
        use crate::script_handle_contract::{
            OPEN_HANDLE_CLASS_SCRIPT, OPEN_HANDLE_CLASS_VERIFIER, OPEN_HANDLE_HEADER_CLASS_OFFSET,
            OPEN_HANDLE_HEADER_MEMBER_COUNT_OFFSET, OPEN_HANDLE_HEADER_ROLE_OFFSET, OPEN_HANDLE_INDEX_OFFSET,
            OPEN_HANDLE_MEMBER_DOMAIN, OPEN_HANDLE_MEMBER_OFFSET, OPEN_HANDLE_MEMBER_STATUS_OFFSET, OPEN_HANDLE_NODE_DOMAIN,
            OPEN_HANDLE_POLICY_DOMAIN, OPEN_HANDLE_SELECTION_BYTES, OPEN_HANDLE_SELECTION_HEADER_OFFSET, OPEN_HANDLE_SELECTION_MAGIC,
            OPEN_HANDLE_SIBLING_OFFSET, OPEN_HANDLE_TREE_DEPTH,
        };

        const HANDLE_PTR: usize = 0;
        const ROOT_PTR: usize = 8;
        const VIEW: usize = 16;
        const INDEX: usize = 24;
        const SUBJECT: usize = 32;
        const IDENTITY_HASH: usize = 48;
        const CURRENT_HASH: usize = 80;
        const RA: usize = 112;
        const LEAF_STAGING: usize = 128;
        const LEAF_INPUT: usize = OPEN_HANDLE_MEMBER_DOMAIN.len() + 1 + 292;
        const NODE_STAGING: usize = LEAF_STAGING + 336;
        const NODE_INPUT: usize = OPEN_HANDLE_NODE_DOMAIN.len() + 64;
        const POLICY_STAGING: usize = NODE_STAGING + 97;
        const POLICY_INPUT: usize = OPEN_HANDLE_POLICY_DOMAIN.len() + 188 + 32;
        // Both lengths occupy eight bytes. Place them after the hash staging
        // buffers and keep LOAD_SCRIPT's in/out length separate from the saved
        // selection length, which is reloaded after the syscall.
        const ARGS_LEN: usize = (POLICY_STAGING + POLICY_INPUT).div_ceil(8) * 8;
        const ARGS_SIZE: usize = ARGS_LEN + 8;
        const ARGS_BUFFER: usize = 848;
        const FRAME: usize = align_frame(ARGS_BUFFER + 128);

        let invalid = self.fresh_label("open_handle_invalid");
        let hash_failed = self.fresh_label("open_handle_hash_failed");
        let done = self.fresh_label("open_handle_done");
        let abi = self.runtime_abi();

        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: compatible-open handle requirement ({detail})"));
        self.emit("# cellscript abi: args a0=CellDepView, a1=selection_ptr, a2=selection_len, a3=root_ptr, a4=root_len");
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::OpenHandleInvalid.code()));
            self.emit("ret");
            return;
        }
        self.emit_stack_adjustment(-(FRAME as i64));
        self.emit(format!("sd ra, {RA}(sp)"));
        self.emit(format!("sd a1, {HANDLE_PTR}(sp)"));
        // The from-args LOAD_SCRIPT clobbers caller-saved registers, so the
        // CellDepView and selection length must be staged before the syscall.
        self.emit(format!("sd a0, {VIEW}(sp)"));
        let participant = symbol == "__ckb_require_open_script_participant_from_args";
        if participant {
            self.emit_stack_store("a3", SUBJECT);
        }
        self.emit("mv t4, a2");
        self.emit(format!("sd t4, {ARGS_LEN}(sp)"));
        if root_from_args {
            // Derive the expected root from the current Script's committed
            // args: LOAD_SCRIPT, require the serialized args field to be
            // exactly 32 bytes (53-byte Script prefix + 32), and stage the
            // pointer to those committed bytes as the root. The witness
            // never authorizes its own root.
            let args_loaded = self.fresh_label("open_handle_args_loaded");
            self.emit("li t0, 128");
            self.emit(format!("sd t0, {ARGS_SIZE}(sp)"));
            self.emit(format!("addi a0, sp, {ARGS_BUFFER}"));
            self.emit(format!("addi a1, sp, {ARGS_SIZE}"));
            self.emit("li a2, 0");
            self.emit(format!("li a7, {}", abi.load_script));
            self.emit("ecall");
            self.emit_near_cond("beqz a0", &args_loaded);
            self.emit(format!("j {invalid}"));
            self.emit_label(&args_loaded);
            self.emit(format!("ld t0, {ARGS_SIZE}(sp)"));
            self.emit("li t1, 85");
            self.emit("sub t2, t0, t1");
            self.emit_near_cond("bnez t2", &invalid);
            self.emit(format!("addi t0, sp, {ARGS_BUFFER}"));
            self.emit("addi t0, t0, 53");
            self.emit(format!("sd t0, {ROOT_PTR}(sp)"));
        } else {
            self.emit(format!("sd a3, {ROOT_PTR}(sp)"));
            self.emit(format!("sd a0, {VIEW}(sp)"));
        }

        // Structural checks: null pointers, exact lengths, magic, class, role,
        // active status and the canonical index below the declared count.
        // The from-args variant receives no root argument (a3/a4 are zero);
        // its expected root pointer is already staged from committed args.
        if root_from_args {
            // Reload the staged selection pointer and length: the prologue's
            // LOAD_SCRIPT clobbered the argument registers.
            self.emit(format!("ld t5, {HANDLE_PTR}(sp)"));
            self.emit(format!("ld a2, {ARGS_LEN}(sp)"));
            self.emit("mv a1, t5");
        }
        self.emit_near_cond("beqz a1", &invalid);
        if !root_from_args {
            self.emit_near_cond("beqz a3", &invalid);
        }
        self.emit(format!("li t0, {OPEN_HANDLE_SELECTION_BYTES}"));
        self.emit("sub t1, a2, t0");
        self.emit_near_cond("bnez t1", &invalid);
        if !root_from_args {
            self.emit("li t0, 32");
            self.emit("sub t1, a4, t0");
            self.emit_near_cond("bnez t1", &invalid);
        }
        self.emit(format!("ld t3, {HANDLE_PTR}(sp)"));
        for (offset, byte) in OPEN_HANDLE_SELECTION_MAGIC.iter().enumerate() {
            self.emit(format!("lbu t0, {offset}(t3)"));
            self.emit(format!("li t1, {byte}"));
            self.emit_near_cond("bne t0,t1", &invalid);
        }
        self.emit(format!("lbu t0, {OPEN_HANDLE_HEADER_CLASS_OFFSET}(t3)"));
        let expected_class = if script_variant { OPEN_HANDLE_CLASS_SCRIPT } else { OPEN_HANDLE_CLASS_VERIFIER };
        self.emit(format!("li t1, {expected_class}"));
        self.emit_near_cond("bne t0,t1", &invalid);
        self.emit(format!("lbu t0, {OPEN_HANDLE_HEADER_ROLE_OFFSET}(t3)"));
        if script_variant {
            // Script-class selections accept the Lock or Type role.
            self.emit("li t2, 2");
            self.emit_near_cond("bgeu t0,t2", &invalid);
        } else {
            self.emit(format!("li t1, {}", crate::script_handle_contract::OPEN_HANDLE_ROLE_SPAWNED_VERIFIER));
            self.emit_near_cond("bne t0,t1", &invalid);
        }
        self.emit(format!("lbu t0, {OPEN_HANDLE_MEMBER_STATUS_OFFSET}(t3)"));
        self.emit_near_cond("bnez t0", &invalid);
        self.emit(format!("lbu t0, {OPEN_HANDLE_INDEX_OFFSET}(t3)"));
        self.emit("li t2, 0");
        self.emit("add t0, t2, t0");
        self.emit(format!("sd t0, {INDEX}(sp)"));
        self.emit(format!("lbu t1, {OPEN_HANDLE_HEADER_MEMBER_COUNT_OFFSET}(t3)"));
        self.emit_near_cond("bgeu t0,t1", &invalid);
        self.emit("li t2, 32");
        self.emit_near_cond("bltu t2,t1", &invalid);
        for (base, magic) in [(8, b"CSOHPv1\0"), (196, b"CSOHMv1\0")] {
            for (offset, byte) in magic.iter().enumerate() {
                self.emit(format!("lbu t0, {}(t3)", base + offset));
                self.emit(format!("li t1, {byte}"));
                self.emit_near_cond("bne t0,t1", &invalid);
            }
        }
        for offset in (206..212).chain(489..496) {
            self.emit(format!("lbu t0, {offset}(t3)"));
            self.emit_near_cond("bnez t0", &invalid);
        }
        // The committed set may retain yanked/below-floor history. Membership
        // alone must never make those members selectable. Decode unaligned
        // little-endian u64 fields without native scalar loads.
        for (register, offset) in [("t0", 212), ("t1", 28), ("t2", 20)] {
            self.emit(format!("li {register}, 0"));
            for byte in (0..8).rev() {
                self.emit(format!("slli {register}, {register}, 8"));
                self.emit(format!("lbu t4, {}(t3)", offset + byte));
                self.emit(format!("or {register}, {register}, t4"));
            }
        }
        self.emit_near_cond("bltu t0,t1", &invalid);
        self.emit_near_cond("bltu t2,t0", &invalid);
        let compatible = self.fresh_label("open_handle_compatible");
        self.emit("lbu t0, 18(t3)");
        self.emit("li t1, 1");
        self.emit_near_cond("bltu t1,t0", &invalid);
        self.emit(format!("bnez t0,{compatible}"));
        self.emit("addi a0, t3, 68");
        self.emit("addi a1, t3, 228");
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit_near_cond("bnez a0", &invalid);
        self.emit_label(&compatible);

        self.emit(format!("addi t3, sp, {LEAF_STAGING}"));
        for (offset, byte) in OPEN_HANDLE_MEMBER_DOMAIN.iter().enumerate() {
            self.emit(format!("li t0, {byte}"));
            self.emit(format!("sb t0, {offset}(t3)"));
        }
        self.emit(format!("ld t0, {INDEX}(sp)"));
        self.emit(format!("sb t0, {}(t3)", OPEN_HANDLE_MEMBER_DOMAIN.len()));
        self.emit(format!("ld a0, {HANDLE_PTR}(sp)"));
        self.emit(format!("addi a0, a0, {OPEN_HANDLE_MEMBER_OFFSET}"));
        self.emit(format!("addi a1, sp, {LEAF_STAGING}"));
        self.emit(format!("addi a1, a1, {}", OPEN_HANDLE_MEMBER_DOMAIN.len() + 1));
        self.emit("li a2, 292");
        self.emit("call __cellscript_memcpy_fixed");
        self.emit(format!("addi a0, sp, {LEAF_STAGING}"));
        self.emit(format!("li a1, {LEAF_INPUT}"));
        self.emit(format!("addi a2, sp, {CURRENT_HASH}"));
        self.emit("call __ckb_hash_blake2b_var");
        self.emit_near_cond("bnez a0", &hash_failed);

        self.emit(format!("addi t3, sp, {NODE_STAGING}"));
        for (offset, byte) in OPEN_HANDLE_NODE_DOMAIN.iter().enumerate() {
            self.emit(format!("li t0, {byte}"));
            self.emit(format!("sb t0, {offset}(t3)"));
        }
        for depth in 0..OPEN_HANDLE_TREE_DEPTH {
            let sibling_left = self.fresh_label("open_handle_sibling_left");
            let joined = self.fresh_label("open_handle_level_joined");
            self.emit(format!("ld t0, {INDEX}(sp)"));
            self.emit(format!("li t1, {}", 1u64 << depth));
            self.emit("and t0, t0, t1");
            self.emit(format!("beqz t0, {sibling_left}"));
            // Index bit set: the sibling is the left child, the current
            // hash the right one.
            self.emit(format!("ld a0, {HANDLE_PTR}(sp)"));
            self.emit(format!("addi a0, a0, {}", OPEN_HANDLE_SIBLING_OFFSET + depth * 32));
            self.emit(format!("addi a1, sp, {NODE_STAGING}"));
            self.emit(format!("addi a1, a1, {}", OPEN_HANDLE_NODE_DOMAIN.len()));
            self.emit("li a2, 32");
            self.emit("call __cellscript_memcpy_fixed");
            self.emit(format!("addi a0, sp, {CURRENT_HASH}"));
            self.emit(format!("addi a1, sp, {NODE_STAGING}"));
            self.emit(format!("addi a1, a1, {}", OPEN_HANDLE_NODE_DOMAIN.len() + 32));
            self.emit("li a2, 32");
            self.emit("call __cellscript_memcpy_fixed");
            self.emit(format!("j {joined}"));
            // Index bit clear: the current hash is the left child.
            self.emit_label(&sibling_left);
            self.emit(format!("addi a0, sp, {CURRENT_HASH}"));
            self.emit(format!("addi a1, sp, {NODE_STAGING}"));
            self.emit(format!("addi a1, a1, {}", OPEN_HANDLE_NODE_DOMAIN.len()));
            self.emit("li a2, 32");
            self.emit("call __cellscript_memcpy_fixed");
            self.emit(format!("ld a0, {HANDLE_PTR}(sp)"));
            self.emit(format!("addi a0, a0, {}", OPEN_HANDLE_SIBLING_OFFSET + depth * 32));
            self.emit(format!("addi a1, sp, {NODE_STAGING}"));
            self.emit(format!("addi a1, a1, {}", OPEN_HANDLE_NODE_DOMAIN.len() + 32));
            self.emit("li a2, 32");
            self.emit("call __cellscript_memcpy_fixed");
            self.emit_label(&joined);
            self.emit(format!("addi a0, sp, {NODE_STAGING}"));
            self.emit(format!("li a1, {NODE_INPUT}"));
            self.emit(format!("addi a2, sp, {CURRENT_HASH}"));
            self.emit("call __ckb_hash_blake2b_var");
            self.emit_near_cond("bnez a0", &hash_failed);
        }

        self.emit(format!("addi t3, sp, {POLICY_STAGING}"));
        for (offset, byte) in OPEN_HANDLE_POLICY_DOMAIN.iter().enumerate() {
            self.emit(format!("li t0, {byte}"));
            self.emit(format!("sb t0, {offset}(t3)"));
        }
        self.emit(format!("ld a0, {HANDLE_PTR}(sp)"));
        self.emit(format!("addi a0, a0, {OPEN_HANDLE_SELECTION_HEADER_OFFSET}"));
        self.emit(format!("addi a1, sp, {POLICY_STAGING}"));
        self.emit(format!("addi a1, a1, {}", OPEN_HANDLE_POLICY_DOMAIN.len()));
        self.emit("li a2, 188");
        self.emit("call __cellscript_memcpy_fixed");
        self.emit(format!("addi a0, sp, {CURRENT_HASH}"));
        self.emit(format!("addi a1, sp, {POLICY_STAGING}"));
        self.emit(format!("addi a1, a1, {}", OPEN_HANDLE_POLICY_DOMAIN.len() + 188));
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcpy_fixed");
        self.emit(format!("addi a0, sp, {POLICY_STAGING}"));
        self.emit(format!("li a1, {POLICY_INPUT}"));
        self.emit(format!("addi a2, sp, {IDENTITY_HASH}"));
        self.emit("call __ckb_hash_blake2b_var");
        self.emit_near_cond("bnez a0", &hash_failed);
        self.emit(format!("addi a0, sp, {IDENTITY_HASH}"));
        self.emit(format!("ld a1, {ROOT_PTR}(sp)"));
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit_near_cond("bnez a0", &invalid);

        self.emit_open_handle_code_dependency(&invalid);
        if participant {
            self.emit_open_handle_participant(&invalid);
        }
        self.emit("li a0, 0");
        self.emit(format!("j {done}"));

        self.emit_label(&hash_failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::OpenHandleInvalid.code()));
        self.emit(format!("j {done}"));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::OpenHandleInvalid.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {RA}(sp)"));
        self.emit_stack_adjustment(FRAME as i64);
        self.emit("ret");
    }

    /// Observe the complete selected Lock/Type Script on one actual Cell. This
    /// grants no lifecycle authority and makes no claim that a peer executed.
    /// The caller's handle at 0, subject at 32 and return address stay live;
    /// syscall scratch reuses the dead size/hash slots at 40 and 48.
    pub(super) fn emit_open_handle_participant(&mut self, invalid: &str) {
        let source_ready = self.fresh_label("open_participant_source");
        let field_ready = self.fresh_label("open_participant_field");
        self.emit_stack_load("a0", 32);
        self.emit_decode_source_view_to_t1_t2(invalid);
        for source in
            [CKB_SOURCE_INPUT, CKB_SOURCE_OUTPUT, CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT, CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT]
        {
            self.emit(format!("li t0, {source}"));
            self.emit_near_cond("beq t0,t2", &source_ready);
        }
        self.emit(format!("j {invalid}"));
        self.emit_label(&source_ready);
        self.emit("mv a3, t1");
        self.emit("mv a4, t2");
        self.emit_stack_load("t3", 0);
        self.emit(format!("lbu t0, {}(t3)", crate::script_handle_contract::OPEN_HANDLE_HEADER_ROLE_OFFSET));
        self.emit(format!("li a5, {CKB_CELL_FIELD_LOCK_HASH}"));
        self.emit_near_cond("beqz t0", &field_ready);
        self.emit(format!("li a5, {CKB_CELL_FIELD_TYPE_HASH}"));
        self.emit_label(&field_ready);
        self.emit("li t0, 32");
        self.emit_stack_store("t0", 40);
        self.emit_sp_addi("a0", 48);
        self.emit_sp_addi("a1", 40);
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_CELL_BY_FIELD));
        self.emit("ecall");
        self.emit_near_cond("bnez a0", invalid);
        self.emit_stack_load("t0", 40);
        self.emit("li t1, 32");
        self.emit_near_cond("bne t0,t1", invalid);
        self.emit_sp_addi("a0", 48);
        self.emit_stack_load("a1", 0);
        self.emit("addi a1, a1, 324");
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit_near_cond("bnez a0", invalid);
    }

    /// Inline into the membership helper after its root comparison. The old
    /// hash staging area at 128..336 is dead here; root/handle/view slots and ra
    /// remain live. All syscall/child-call state lives in distinct stack slots.
    pub(super) fn emit_open_handle_code_dependency(&mut self, invalid: &str) {
        const HANDLE: usize = 0;
        const VIEW: usize = 16;
        const SELECTED: usize = 24;
        const SIZE: usize = 40;
        const HASH: usize = 48;
        const HEADER: usize = 128;
        const COUNT: usize = 176;
        const CURSOR: usize = 184;
        const DEP: usize = 192;
        let scan = self.fresh_label("open_code_scan");
        let next = self.fresh_label("open_code_next");
        let not_selected = self.fresh_label("open_code_not_selected");
        let data_field = self.fresh_label("open_code_data_field");
        let field_ready = self.fresh_label("open_code_field_ready");
        let loaded = self.fresh_label("open_code_loaded");
        let finish = self.fresh_label("open_code_finish");
        self.emit_stack_load("a0", VIEW);
        self.emit_decode_source_view_to_t1_t2(invalid);
        self.emit(format!("li t0, {}", ckb_abi::source::CELL_DEP));
        self.emit_near_cond("bne t0,t2", invalid);
        self.emit_stack_store("t1", SELECTED);
        self.emit("li t0, 48");
        self.emit_stack_store("t0", SIZE);
        self.emit_sp_addi("a0", HEADER);
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_TRANSACTION));
        self.emit("ecall");
        self.emit_near_cond("bnez a0", invalid);
        self.emit_stack_load("t1", SIZE);
        self.emit("li t0, 68");
        self.emit_near_cond("bltu t1,t0", invalid);
        self.emit_stack_u32_le_to("t0", HEADER);
        self.emit_near_cond("bne t0,t1", invalid);
        // LOAD_TRANSACTION supplies a canonical node transaction. Still bind
        // the offsets we use, and bound the complete raw direct dependency set.
        for (offset, value) in [(4, 12), (16, 28), (20, 32)] {
            self.emit_stack_u32_le_to("t0", HEADER + offset);
            self.emit(format!("li t1, {value}"));
            self.emit_near_cond("bne t0,t1", invalid);
        }
        self.emit_stack_u32_le_to("t0", HEADER + 44);
        self.emit_near_cond("beqz t0", invalid);
        self.emit("li t1, 64");
        self.emit_near_cond("bltu t1,t0", invalid);
        self.emit_stack_store("t0", COUNT);
        self.emit_stack_load("t1", SELECTED);
        self.emit_near_cond("bgeu t1,t0", invalid);
        self.emit("li t1, 37");
        self.emit("mul t0, t0, t1");
        self.emit("addi t0, t0, 36");
        self.emit_stack_u32_le_to("t1", HEADER + 24);
        self.emit_near_cond("bne t0,t1", invalid);
        self.emit_stack_store("zero", CURSOR);
        self.emit_label(&scan);
        self.emit("li t0, 37");
        self.emit_stack_store("t0", SIZE);
        self.emit_stack_load("t1", CURSOR);
        self.emit("mul a2, t1, t0");
        self.emit("addi a2, a2, 48");
        self.emit_sp_addi("a0", DEP);
        self.emit_sp_addi("a1", SIZE);
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_TRANSACTION));
        self.emit("ecall");
        self.emit_near_cond("bnez a0", invalid);
        self.emit_stack_load("t0", SIZE);
        self.emit("li t1, 37");
        self.emit_near_cond("bltu t0,t1", invalid);
        self.emit_stack_load_byte("t0", DEP + 36);
        self.emit_near_cond("bnez t0", invalid); // no dep-group expansion
        self.emit_stack_load("t0", CURSOR);
        self.emit_stack_load("t1", SELECTED);
        self.emit(format!("bne t0,t1,{not_selected}"));
        self.emit_sp_addi("a0", DEP);
        self.emit_stack_load("a1", HANDLE);
        self.emit("addi a1, a1, 388");
        // tx_hash and output index are not contiguous in the selection wire.
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit_near_cond("bnez a0", invalid);
        self.emit_sp_addi("a0", DEP + 32);
        self.emit_stack_load("a1", HANDLE);
        self.emit("addi a1, a1, 484");
        self.emit("li a2, 4");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit_near_cond("bnez a0", invalid);
        self.emit_label(&not_selected);
        self.emit_stack_load("t3", HANDLE);
        self.emit("lbu t0, 205(t3)");
        self.emit("li t1, 4");
        self.emit(format!("beq t0,t1,{data_field}"));
        self.emit("li t1, 1");
        self.emit_near_cond("bne t0,t1", invalid);
        self.emit(format!("li a5, {}", ckb_abi::cell_field::TYPE_HASH));
        self.emit(format!("j {field_ready}"));
        self.emit_label(&data_field);
        self.emit(format!("li a5, {}", ckb_abi::cell_field::DATA_HASH));
        self.emit_label(&field_ready);
        self.emit("li t0, 32");
        self.emit_stack_store("t0", SIZE);
        self.emit_sp_addi("a0", HASH);
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit_stack_load("a3", CURSOR);
        self.emit(format!("li a4, {}", ckb_abi::source::CELL_DEP));
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_CELL_BY_FIELD));
        self.emit("ecall");
        self.emit(format!("beqz a0,{loaded}"));
        // ITEM_MISSING is permitted only for an unselected Cell's Type Script.
        self.emit("li t0, 2");
        self.emit_near_cond("bne a0,t0", invalid);
        self.emit_stack_load("t0", CURSOR);
        self.emit_stack_load("t1", SELECTED);
        self.emit_near_cond("beq t0,t1", invalid);
        self.emit(format!("j {next}"));
        self.emit_label(&loaded);
        self.emit_stack_load("t0", SIZE);
        self.emit("li t1, 32");
        self.emit_near_cond("bne t0,t1", invalid);
        self.emit_sp_addi("a0", HASH);
        self.emit_stack_load("a1", HANDLE);
        self.emit("addi a1, a1, 356");
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit_stack_load("t0", CURSOR);
        self.emit_stack_load("t1", SELECTED);
        let matched = self.fresh_label("open_code_matched");
        self.emit(format!("beqz a0,{matched}"));
        self.emit_near_cond("beq t0,t1", invalid);
        self.emit(format!("j {next}"));
        self.emit_label(&matched);
        self.emit_near_cond("bne t0,t1", invalid); // ambiguous code identity
                                                   // Always check actual bytes as well, including Type-hash replacements.
        self.emit("li t0, 32");
        self.emit_stack_store("t0", SIZE);
        self.emit_sp_addi("a0", HASH);
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit_stack_load("a3", SELECTED);
        self.emit(format!("li a4, {}", ckb_abi::source::CELL_DEP));
        self.emit(format!("li a5, {}", ckb_abi::cell_field::DATA_HASH));
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_CELL_BY_FIELD));
        self.emit("ecall");
        self.emit_near_cond("bnez a0", invalid);
        self.emit_sp_addi("a0", HASH);
        self.emit_stack_load("a1", HANDLE);
        self.emit(format!("addi a1, a1, {}", crate::script_handle_contract::OPEN_HANDLE_MEMBER_ARTIFACT_OFFSET));
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit_near_cond("bnez a0", invalid);
        self.emit_label(&next);
        self.emit_stack_load("t0", CURSOR);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", CURSOR);
        self.emit_stack_load("t1", COUNT);
        self.emit(format!("beq t0,t1,{finish}"));
        self.emit(format!("j {scan}"));
        self.emit_label(&finish);
    }

    pub(super) fn emit_runtime_deployment_line_handle_requirement_helper(
        &mut self,
        symbol: &str,
        detail: &str,
        subject_field_id: Option<u64>,
        expected_class: u8,
        expected_role: u8,
        enabled: bool,
    ) {
        use crate::script_handle_contract::{
            DEPLOYMENT_LINE_COMMITMENT_MAGIC, DEPLOYMENT_LINE_HANDLE_ADMISSION_TYPE_HASH_OFFSET, DEPLOYMENT_LINE_HANDLE_BYTES,
            DEPLOYMENT_LINE_HANDLE_CLASS_OFFSET, DEPLOYMENT_LINE_HANDLE_EXACT_HANDLE_OFFSET, DEPLOYMENT_LINE_HANDLE_MAGIC,
            DEPLOYMENT_LINE_HANDLE_RESERVED_BYTES, DEPLOYMENT_LINE_HANDLE_RESERVED_OFFSET, DEPLOYMENT_LINE_HANDLE_ROLE_OFFSET,
            DEPLOYMENT_LINE_HANDLE_STATUS_ACTIVE, DEPLOYMENT_LINE_HANDLE_STATUS_OFFSET, EXACT_SCRIPT_HANDLE_ARTIFACT_HASH_OFFSET,
            EXACT_SCRIPT_HANDLE_BYTES, EXACT_SCRIPT_HANDLE_CLASS_OFFSET, EXACT_SCRIPT_HANDLE_HASH_BYTES, EXACT_SCRIPT_HANDLE_MAGIC,
            EXACT_SCRIPT_HANDLE_ROLE_OFFSET, EXACT_SCRIPT_HANDLE_SCRIPT_HASH_OFFSET,
        };

        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: deployment line handle requirement ({detail})"));
        if subject_field_id.is_some() {
            self.emit("# cellscript abi: args a0=subject SourceView, a1=admission CellDepView, a2=code CellDepView, a3=handle_ptr, a4=handle_len, a5=handle_hash_ptr, a6=handle_hash_len");
        } else {
            self.emit("# cellscript abi: args a0=admission CellDepView, a1=code CellDepView, a2=handle_ptr, a3=handle_len, a4=handle_hash_ptr, a5=handle_hash_len");
        }
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        const HANDLE_PTR: usize = 0;
        const EXPECTED_HASH_PTR: usize = 8;
        const SUBJECT_VIEW: usize = 16;
        const ADMISSION_VIEW: usize = 24;
        const CODE_VIEW: usize = 32;
        const ADMISSION_INDEX: usize = 40;
        const ADMISSION_SOURCE: usize = 48;
        const CODE_INDEX: usize = 56;
        const CODE_SOURCE: usize = 64;
        const SUBJECT_INDEX: usize = 72;
        const SUBJECT_SOURCE: usize = 80;
        const SIZE: usize = 88;
        const LOADED_HASH: usize = 96;
        const HANDLE_HASH: usize = 128;
        const ADMISSION_DATA: usize = 160;
        const RA: usize = 216;
        const FRAME: usize = 224;

        let invalid = self.fresh_label("deployment_line_handle_invalid");
        let done = self.fresh_label("deployment_line_handle_done");
        let abi = self.runtime_abi();

        self.emit_stack_adjustment(-(FRAME as i64));
        self.emit(format!("sd ra, {RA}(sp)"));
        if subject_field_id.is_some() {
            self.emit(format!("sd a0, {SUBJECT_VIEW}(sp)"));
            self.emit(format!("sd a1, {ADMISSION_VIEW}(sp)"));
            self.emit(format!("sd a2, {CODE_VIEW}(sp)"));
            self.emit(format!("sd a3, {HANDLE_PTR}(sp)"));
            self.emit(format!("sd a5, {EXPECTED_HASH_PTR}(sp)"));
            self.emit(format!("li t0, {DEPLOYMENT_LINE_HANDLE_BYTES}"));
            self.emit("sub t1, a4, t0");
            self.emit(format!("bnez t1, {invalid}"));
            self.emit(format!("li t0, {EXACT_SCRIPT_HANDLE_HASH_BYTES}"));
            self.emit("sub t1, a6, t0");
            self.emit(format!("bnez t1, {invalid}"));
        } else {
            self.emit(format!("sd a0, {ADMISSION_VIEW}(sp)"));
            self.emit(format!("sd a1, {CODE_VIEW}(sp)"));
            self.emit(format!("sd a2, {HANDLE_PTR}(sp)"));
            self.emit(format!("sd a4, {EXPECTED_HASH_PTR}(sp)"));
            self.emit(format!("li t0, {DEPLOYMENT_LINE_HANDLE_BYTES}"));
            self.emit("sub t1, a3, t0");
            self.emit(format!("bnez t1, {invalid}"));
            self.emit(format!("li t0, {EXACT_SCRIPT_HANDLE_HASH_BYTES}"));
            self.emit("sub t1, a5, t0");
            self.emit(format!("bnez t1, {invalid}"));
        }
        self.emit(format!("ld t3, {HANDLE_PTR}(sp)"));
        self.emit(format!("beqz t3, {invalid}"));
        self.emit(format!("ld t4, {EXPECTED_HASH_PTR}(sp)"));
        self.emit(format!("beqz t4, {invalid}"));

        for (offset, byte) in DEPLOYMENT_LINE_HANDLE_MAGIC.iter().enumerate() {
            self.emit(format!("lbu t0, {offset}(t3)"));
            self.emit(format!("li t1, {byte}"));
            self.emit_near_cond("bne t0,t1", &invalid);
        }
        self.emit(format!("lbu t0, {DEPLOYMENT_LINE_HANDLE_CLASS_OFFSET}(t3)"));
        self.emit(format!("li t1, {expected_class}"));
        self.emit(format!("bne t0, t1, {invalid}"));
        self.emit(format!("lbu t0, {DEPLOYMENT_LINE_HANDLE_ROLE_OFFSET}(t3)"));
        self.emit(format!("li t1, {expected_role}"));
        self.emit(format!("bne t0, t1, {invalid}"));
        self.emit(format!("lbu t0, {DEPLOYMENT_LINE_HANDLE_STATUS_OFFSET}(t3)"));
        self.emit(format!("li t1, {DEPLOYMENT_LINE_HANDLE_STATUS_ACTIVE}"));
        self.emit(format!("bne t0, t1, {invalid}"));
        for offset in
            DEPLOYMENT_LINE_HANDLE_RESERVED_OFFSET..DEPLOYMENT_LINE_HANDLE_RESERVED_OFFSET + DEPLOYMENT_LINE_HANDLE_RESERVED_BYTES
        {
            self.emit(format!("lbu t0, {offset}(t3)"));
            self.emit(format!("bnez t0, {invalid}"));
        }
        for (offset, byte) in EXACT_SCRIPT_HANDLE_MAGIC.iter().enumerate() {
            self.emit(format!("lbu t0, {}(t3)", DEPLOYMENT_LINE_HANDLE_EXACT_HANDLE_OFFSET + offset));
            self.emit(format!("li t1, {byte}"));
            self.emit(format!("bne t0, t1, {invalid}"));
        }
        self.emit(format!("lbu t0, {}(t3)", DEPLOYMENT_LINE_HANDLE_EXACT_HANDLE_OFFSET + EXACT_SCRIPT_HANDLE_CLASS_OFFSET));
        self.emit(format!("li t1, {expected_class}"));
        self.emit(format!("bne t0, t1, {invalid}"));
        self.emit(format!("lbu t0, {}(t3)", DEPLOYMENT_LINE_HANDLE_EXACT_HANDLE_OFFSET + EXACT_SCRIPT_HANDLE_ROLE_OFFSET));
        self.emit(format!("li t1, {expected_role}"));
        self.emit(format!("bne t0, t1, {invalid}"));
        debug_assert_eq!(DEPLOYMENT_LINE_HANDLE_EXACT_HANDLE_OFFSET + EXACT_SCRIPT_HANDLE_BYTES, DEPLOYMENT_LINE_HANDLE_BYTES);

        self.emit(format!("ld a0, {HANDLE_PTR}(sp)"));
        self.emit(format!("li a1, {DEPLOYMENT_LINE_HANDLE_BYTES}"));
        self.emit(format!("addi a2, sp, {HANDLE_HASH}"));
        self.emit("call __ckb_hash_blake2b_var");
        self.emit(format!("bnez a0, {invalid}"));
        self.emit(format!("addi a0, sp, {HANDLE_HASH}"));
        self.emit(format!("ld a1, {EXPECTED_HASH_PTR}(sp)"));
        self.emit(format!("li a2, {EXACT_SCRIPT_HANDLE_HASH_BYTES}"));
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {invalid}"));

        self.emit(format!("ld a0, {ADMISSION_VIEW}(sp)"));
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {CKB_SOURCE_CELL_DEP}"));
        self.emit(format!("bne t2, t0, {invalid}"));
        self.emit(format!("sd t1, {ADMISSION_INDEX}(sp)"));
        self.emit(format!("sd t2, {ADMISSION_SOURCE}(sp)"));
        self.emit(format!("li t0, {EXACT_SCRIPT_HANDLE_HASH_BYTES}"));
        self.emit(format!("sd t0, {SIZE}(sp)"));
        self.emit(format!("addi a0, sp, {LOADED_HASH}"));
        self.emit(format!("addi a1, sp, {SIZE}"));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {ADMISSION_INDEX}(sp)"));
        self.emit(format!("ld a4, {ADMISSION_SOURCE}(sp)"));
        self.emit(format!("li a5, {CKB_CELL_FIELD_TYPE_HASH}"));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {invalid}"));
        self.emit(format!("ld t0, {SIZE}(sp)"));
        self.emit(format!("li t1, {EXACT_SCRIPT_HANDLE_HASH_BYTES}"));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {invalid}"));
        self.emit(format!("addi a0, sp, {LOADED_HASH}"));
        self.emit(format!("ld a1, {HANDLE_PTR}(sp)"));
        self.emit(format!("addi a1, a1, {DEPLOYMENT_LINE_HANDLE_ADMISSION_TYPE_HASH_OFFSET}"));
        self.emit(format!("li a2, {EXACT_SCRIPT_HANDLE_HASH_BYTES}"));
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {invalid}"));

        let admission_data_bytes = DEPLOYMENT_LINE_COMMITMENT_MAGIC.len() + EXACT_SCRIPT_HANDLE_HASH_BYTES;
        self.emit(format!("li t0, {admission_data_bytes}"));
        self.emit(format!("sd t0, {SIZE}(sp)"));
        self.emit(format!("addi a0, sp, {ADMISSION_DATA}"));
        self.emit(format!("addi a1, sp, {SIZE}"));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {ADMISSION_INDEX}(sp)"));
        self.emit(format!("ld a4, {ADMISSION_SOURCE}(sp)"));
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit("ecall");
        self.emit(format!("bnez a0, {invalid}"));
        self.emit(format!("ld t0, {SIZE}(sp)"));
        self.emit(format!("li t1, {admission_data_bytes}"));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {invalid}"));
        for (offset, byte) in DEPLOYMENT_LINE_COMMITMENT_MAGIC.iter().enumerate() {
            self.emit(format!("lbu t0, {}(sp)", ADMISSION_DATA + offset));
            self.emit(format!("li t1, {byte}"));
            self.emit(format!("bne t0, t1, {invalid}"));
        }
        self.emit(format!("addi a0, sp, {}", ADMISSION_DATA + DEPLOYMENT_LINE_COMMITMENT_MAGIC.len()));
        self.emit(format!("addi a1, sp, {HANDLE_HASH}"));
        self.emit(format!("li a2, {EXACT_SCRIPT_HANDLE_HASH_BYTES}"));
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {invalid}"));

        self.emit(format!("ld a0, {CODE_VIEW}(sp)"));
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {CKB_SOURCE_CELL_DEP}"));
        self.emit(format!("bne t2, t0, {invalid}"));
        self.emit(format!("sd t1, {CODE_INDEX}(sp)"));
        self.emit(format!("sd t2, {CODE_SOURCE}(sp)"));
        self.emit(format!("li t0, {EXACT_SCRIPT_HANDLE_HASH_BYTES}"));
        self.emit(format!("sd t0, {SIZE}(sp)"));
        self.emit(format!("addi a0, sp, {LOADED_HASH}"));
        self.emit(format!("addi a1, sp, {SIZE}"));
        self.emit("li a2, 0");
        self.emit(format!("ld a3, {CODE_INDEX}(sp)"));
        self.emit(format!("ld a4, {CODE_SOURCE}(sp)"));
        self.emit(format!("li a5, {CKB_CELL_FIELD_DATA_HASH}"));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {invalid}"));
        self.emit(format!("ld t0, {SIZE}(sp)"));
        self.emit(format!("li t1, {EXACT_SCRIPT_HANDLE_HASH_BYTES}"));
        self.emit("sub t2, t0, t1");
        self.emit(format!("bnez t2, {invalid}"));
        self.emit(format!("addi a0, sp, {LOADED_HASH}"));
        self.emit(format!("ld a1, {HANDLE_PTR}(sp)"));
        self.emit(format!("addi a1, a1, {}", DEPLOYMENT_LINE_HANDLE_EXACT_HANDLE_OFFSET + EXACT_SCRIPT_HANDLE_ARTIFACT_HASH_OFFSET));
        self.emit(format!("li a2, {EXACT_SCRIPT_HANDLE_HASH_BYTES}"));
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {invalid}"));

        if let Some(field_id) = subject_field_id {
            self.emit(format!("ld a0, {SUBJECT_VIEW}(sp)"));
            self.emit_decode_source_view_to_t1_t2(&invalid);
            self.emit(format!("sd t1, {SUBJECT_INDEX}(sp)"));
            self.emit(format!("sd t2, {SUBJECT_SOURCE}(sp)"));
            self.emit(format!("li t0, {EXACT_SCRIPT_HANDLE_HASH_BYTES}"));
            self.emit(format!("sd t0, {SIZE}(sp)"));
            self.emit(format!("addi a0, sp, {LOADED_HASH}"));
            self.emit(format!("addi a1, sp, {SIZE}"));
            self.emit("li a2, 0");
            self.emit(format!("ld a3, {SUBJECT_INDEX}(sp)"));
            self.emit(format!("ld a4, {SUBJECT_SOURCE}(sp)"));
            self.emit(format!("li a5, {field_id}"));
            self.emit(format!("li a7, {}", abi.load_cell_by_field));
            self.emit("ecall");
            self.emit(format!("bnez a0, {invalid}"));
            self.emit(format!("ld t0, {SIZE}(sp)"));
            self.emit(format!("li t1, {EXACT_SCRIPT_HANDLE_HASH_BYTES}"));
            self.emit("sub t2, t0, t1");
            self.emit(format!("bnez t2, {invalid}"));
            self.emit(format!("addi a0, sp, {LOADED_HASH}"));
            self.emit(format!("ld a1, {HANDLE_PTR}(sp)"));
            self.emit(format!("addi a1, a1, {}", DEPLOYMENT_LINE_HANDLE_EXACT_HANDLE_OFFSET + EXACT_SCRIPT_HANDLE_SCRIPT_HASH_OFFSET));
            self.emit(format!("li a2, {EXACT_SCRIPT_HANDLE_HASH_BYTES}"));
            self.emit("call __cellscript_memcmp_fixed");
            self.emit(format!("bnez a0, {invalid}"));
        }

        self.emit("li a0, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::DeploymentLineHandleInvalid.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {RA}(sp)"));
        self.emit_stack_adjustment(FRAME as i64);
        self.emit("ret");
    }
}
