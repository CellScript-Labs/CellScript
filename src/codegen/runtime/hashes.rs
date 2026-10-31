//! Runtime hashing emitters. Assembly bodies are copied without semantic changes.
use super::*;

const BLAKE2B_SIGMA: [[usize; 16]; 12] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
];

impl CodeGenerator {
    pub(super) fn emit_runtime_script_hash_helper(&mut self, enabled: bool) {
        self.emit_global("__ckb_script_hash");
        self.emit_label("__ckb_script_hash");
        self.emit(
            "# cellscript abi: canonical Molecule Script hash; a0=code_hash[32], a1=hash_type, a2=args, a3=args_len, a4=out[32]",
        );
        if !enabled {
            self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptConstructionInvalid.code()));
            self.emit("ret");
            return;
        }

        const PREIMAGE_BYTES: usize = 512;
        const CODE_HASH_PTR_OFFSET: usize = 512;
        const HASH_TYPE_OFFSET: usize = 520;
        const ARGS_PTR_OFFSET: usize = 528;
        const ARGS_LEN_OFFSET: usize = 536;
        const OUT_PTR_OFFSET: usize = 544;
        const RA_OFFSET: usize = 552;
        const FRAME_SIZE: usize = 560;

        let args_pointer_ok = self.fresh_label("script_hash_args_pointer_ok");
        let hash_type_valid = self.fresh_label("script_hash_type_valid");
        let code_hash_copy = self.fresh_label("script_hash_code_copy");
        let code_hash_done = self.fresh_label("script_hash_code_done");
        let args_copy = self.fresh_label("script_hash_args_copy");
        let args_done = self.fresh_label("script_hash_args_done");
        let invalid = self.fresh_label("script_hash_invalid");
        let done = self.fresh_label("script_hash_done");

        self.emit_stack_adjustment(-(FRAME_SIZE as i64));
        self.emit(format!("sd ra, {}(sp)", RA_OFFSET));
        self.emit(format!("sd a0, {}(sp)", CODE_HASH_PTR_OFFSET));
        self.emit(format!("sd a1, {}(sp)", HASH_TYPE_OFFSET));
        self.emit(format!("sd a2, {}(sp)", ARGS_PTR_OFFSET));
        self.emit(format!("sd a3, {}(sp)", ARGS_LEN_OFFSET));
        self.emit(format!("sd a4, {}(sp)", OUT_PTR_OFFSET));

        self.emit(format!("beqz a0, {}", invalid));
        self.emit(format!("beqz a4, {}", invalid));
        self.emit(format!("li t0, {}", crate::CKB_SCRIPT_HASH_MAX_ARGS_BYTES + 1));
        self.emit("sltu t1, a3, t0");
        self.emit(format!("beqz t1, {}", invalid));
        self.emit(format!("beqz a3, {}", args_pointer_ok));
        self.emit(format!("beqz a2, {}", invalid));
        self.emit_label(&args_pointer_ok);

        self.emit(format!("beqz a1, {}", hash_type_valid));
        for hash_type in [1u64, 2, 4] {
            self.emit(format!("li t0, {}", hash_type));
            self.emit("sub t1, a1, t0");
            self.emit(format!("beqz t1, {}", hash_type_valid));
        }
        self.emit(format!("j {}", invalid));
        self.emit_label(&hash_type_valid);

        self.emit(format!("ld t0, {}(sp)", ARGS_LEN_OFFSET));
        self.emit("addi t0, t0, 53");
        self.emit("sw t0, 0(sp)");
        for (offset, value) in [(4usize, 16u64), (8, 48), (12, 49)] {
            self.emit(format!("li t0, {}", value));
            self.emit(format!("sw t0, {}(sp)", offset));
        }

        self.emit(format!("ld t2, {}(sp)", CODE_HASH_PTR_OFFSET));
        self.emit("li t0, 0");
        self.emit_label(&code_hash_copy);
        self.emit("li t1, 32");
        self.emit("sltu t1, t0, t1");
        self.emit(format!("beqz t1, {}", code_hash_done));
        self.emit("add t3, t2, t0");
        self.emit("lbu t4, 0(t3)");
        self.emit("addi t3, sp, 16");
        self.emit("add t3, t3, t0");
        self.emit("sb t4, 0(t3)");
        self.emit("addi t0, t0, 1");
        self.emit(format!("j {}", code_hash_copy));
        self.emit_label(&code_hash_done);

        self.emit(format!("ld t0, {}(sp)", HASH_TYPE_OFFSET));
        self.emit("sb t0, 48(sp)");
        self.emit(format!("ld t0, {}(sp)", ARGS_LEN_OFFSET));
        for byte in 0..4usize {
            if byte > 0 {
                self.emit("srli t0, t0, 8");
            }
            self.emit(format!("sb t0, {}(sp)", 49 + byte));
        }

        self.emit(format!("ld t2, {}(sp)", ARGS_PTR_OFFSET));
        self.emit(format!("ld t5, {}(sp)", ARGS_LEN_OFFSET));
        self.emit("li t0, 0");
        self.emit_label(&args_copy);
        self.emit("sltu t1, t0, t5");
        self.emit(format!("beqz t1, {}", args_done));
        self.emit("add t3, t2, t0");
        self.emit("lbu t4, 0(t3)");
        self.emit("addi t3, sp, 53");
        self.emit("add t3, t3, t0");
        self.emit("sb t4, 0(t3)");
        self.emit("addi t0, t0, 1");
        self.emit(format!("j {}", args_copy));
        self.emit_label(&args_done);

        self.emit("addi a0, sp, 0");
        self.emit(format!("ld a1, {}(sp)", ARGS_LEN_OFFSET));
        self.emit("addi a1, a1, 53");
        self.emit(format!("ld a2, {}(sp)", OUT_PTR_OFFSET));
        self.emit("call __ckb_hash_blake2b_var");
        self.emit(format!("j {}", done));

        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::ScriptConstructionInvalid.code()));
        self.emit_label(&done);
        self.emit(format!("ld ra, {}(sp)", RA_OFFSET));
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");

        debug_assert_eq!(PREIMAGE_BYTES, 53 + crate::CKB_SCRIPT_HASH_MAX_ARGS_BYTES);
    }

    pub(super) fn emit_u32_normalize(&mut self, register: &str) {
        self.emit(format!("slli {0}, {0}, 32", register));
        self.emit(format!("srli {0}, {0}, 32", register));
    }

    pub(super) fn emit_sha256_rotr(&mut self, dest: &str, source: &str, shift: u8, _scratch: &str) {
        self.emit(format!("roriw {dest}, {source}, {shift}"));
        // RORIW sign-extends bit 31; the SHA state is maintained as a
        // zero-extended u32-in-u64 so later logical shifts remain exact.
        self.emit_u32_normalize(dest);
    }

    pub(super) fn emit_runtime_sha256_surface(&mut self, enabled: bool) {
        for symbol in [
            "__cellscript_sha256_compress",
            "__cellscript_sha256_fixed",
            "__ckb_hash_sha256",
            "__ckb_hash_sha256d",
            "__ckb_hash_sha256_pair",
            "__ckb_hash_sha256d_pair",
            "__ckb_require_sha256d_merkle_root",
        ] {
            self.emit_global(symbol);
        }
        if !enabled {
            for symbol in [
                "__cellscript_sha256_compress",
                "__cellscript_sha256_fixed",
                "__ckb_hash_sha256",
                "__ckb_hash_sha256d",
                "__ckb_hash_sha256_pair",
                "__ckb_hash_sha256d_pair",
                "__ckb_require_sha256d_merkle_root",
            ] {
                self.emit_label(symbol);
                self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            }
            return;
        }
        self.emit_runtime_sha256_compress();
        self.emit_runtime_sha256_fixed();
        self.emit_runtime_sha256_wrappers();
        self.emit_runtime_sha256d_merkle_requirement();
    }

    pub(super) fn emit_runtime_sha256_compress(&mut self) {
        const STACK_FRAME_BYTES: i64 = align_frame(32) as i64;
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01,
            0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
            0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
            0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
            0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08,
            0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
            0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
        ];
        self.emit_label("__cellscript_sha256_compress");
        self.emit("# cellscript abi: SHA-256 compression; a0=state[8] u32-in-u64, a1=schedule[64] u32-in-u64");
        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit("sd ra, 0(sp)");
        self.emit("sd s1, 8(sp)");
        self.emit("sd s2, 16(sp)");
        self.emit("mv s1, a0");
        self.emit("mv s2, a1");
        for index in 16..64 {
            self.emit(format!("ld t0, {}(a1)", (index - 15) * 8));
            self.emit_sha256_rotr("t1", "t0", 7, "t2");
            self.emit_sha256_rotr("t3", "t0", 18, "t2");
            self.emit("xor t1, t1, t3");
            self.emit("srli t3, t0, 3");
            self.emit("xor t1, t1, t3");
            self.emit(format!("ld t3, {}(a1)", (index - 2) * 8));
            self.emit_sha256_rotr("t4", "t3", 17, "t5");
            self.emit_sha256_rotr("t6", "t3", 19, "t5");
            self.emit("xor t4, t4, t6");
            self.emit("srli t6, t3, 10");
            self.emit("xor t4, t4, t6");
            self.emit(format!("ld t0, {}(a1)", (index - 16) * 8));
            self.emit("add t1, t1, t0");
            self.emit(format!("ld t0, {}(a1)", (index - 7) * 8));
            self.emit("add t1, t1, t0");
            self.emit("add t1, t1, t4");
            self.emit_u32_normalize("t1");
            self.emit(format!("sd t1, {}(a1)", index * 8));
        }
        let mut state = ["a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7"];
        for (index, register) in state.iter().enumerate() {
            self.emit(format!("ld {register}, {}(s1)", index * 8));
        }
        for (round, constant) in K.iter().enumerate() {
            let [a, b, c, d, e, f, g, h] = state;

            self.emit_sha256_rotr("t0", e, 6, "t1");
            self.emit_sha256_rotr("t2", e, 11, "t1");
            self.emit("xor t0, t0, t2");
            self.emit_sha256_rotr("t2", e, 25, "t1");
            self.emit("xor t0, t0, t2");
            self.emit(format!("and t3, {e}, {f}"));
            self.emit(format!("xori t2, {e}, -1"));
            self.emit_u32_normalize("t2");
            self.emit(format!("and t2, t2, {g}"));
            self.emit("xor t3, t3, t2");
            self.emit(format!("add t4, {h}, t0"));
            self.emit("add t4, t4, t3");
            self.emit(format!("li t6, {constant}"));
            self.emit("add t4, t4, t6");
            self.emit(format!("ld t6, {}(s2)", round * 8));
            self.emit("add t4, t4, t6");
            self.emit_u32_normalize("t4");

            self.emit_sha256_rotr("t0", a, 2, "t1");
            self.emit_sha256_rotr("t2", a, 13, "t1");
            self.emit("xor t0, t0, t2");
            self.emit_sha256_rotr("t2", a, 22, "t1");
            self.emit("xor t0, t0, t2");
            self.emit(format!("and t3, {a}, {b}"));
            self.emit(format!("and t5, {a}, {c}"));
            self.emit("xor t3, t3, t5");
            self.emit(format!("and t5, {b}, {c}"));
            self.emit("xor t3, t3, t5");
            self.emit("add t5, t0, t3");
            self.emit_u32_normalize("t5");

            self.emit(format!("add {d}, {d}, t4"));
            self.emit_u32_normalize(d);
            self.emit(format!("add {h}, t4, t5"));
            self.emit_u32_normalize(h);
            state = [h, a, b, c, d, e, f, g];
        }
        for (index, register) in state.iter().enumerate() {
            self.emit(format!("ld t0, {}(s1)", index * 8));
            self.emit(format!("add t0, t0, {register}"));
            self.emit_u32_normalize("t0");
            self.emit(format!("sd t0, {}(s1)", index * 8));
        }
        self.emit("li a0, 0");
        self.emit("ld ra, 0(sp)");
        self.emit("ld s1, 8(sp)");
        self.emit("ld s2, 16(sp)");
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_sha256_fixed(&mut self) {
        const H: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
        const STATE_OFFSET: usize = 0;
        const W_OFFSET: usize = 64;
        const INPUT_OFFSET: usize = 576;
        const LEN_OFFSET: usize = 584;
        const OUTPUT_OFFSET: usize = 592;
        const RA_OFFSET: usize = 600;
        const FRAME_SIZE: usize = align_frame(RA_OFFSET + 8);
        self.emit_label("__cellscript_sha256_fixed");
        self.emit("# cellscript abi: bounded SHA-256 for exactly 32 or 64 input bytes; a0=input, a1=len, a2=output[32]");
        let len32 = self.fresh_label("sha256_len32");
        let len64 = self.fresh_label("sha256_len64");
        let after_first = self.fresh_label("sha256_after_first");
        let output = self.fresh_label("sha256_output");
        let invalid = self.fresh_label("sha256_invalid");
        let done = self.fresh_label("sha256_done");
        self.emit_stack_adjustment(-(FRAME_SIZE as i64));
        self.emit_stack_store("ra", RA_OFFSET);
        self.emit_stack_store("a0", INPUT_OFFSET);
        self.emit_stack_store("a1", LEN_OFFSET);
        self.emit_stack_store("a2", OUTPUT_OFFSET);
        self.emit(format!("beqz a0, {}", invalid));
        self.emit(format!("beqz a2, {}", invalid));
        self.emit("li t0, 32");
        self.emit("sub t1, a1, t0");
        self.emit(format!("beqz t1, {}", len32));
        self.emit("li t0, 64");
        self.emit("sub t1, a1, t0");
        self.emit(format!("beqz t1, {}", len64));
        self.emit(format!("j {}", invalid));
        self.emit_label(&len32);
        for (index, value) in H.iter().enumerate() {
            self.emit(format!("li t0, {}", value));
            self.emit_stack_store("t0", STATE_OFFSET + index * 8);
        }
        self.emit("li t6, 8");
        self.emit(format!("j {}", after_first));
        self.emit_label(&len64);
        for (index, value) in H.iter().enumerate() {
            self.emit(format!("li t0, {}", value));
            self.emit_stack_store("t0", STATE_OFFSET + index * 8);
        }
        self.emit("li t6, 16");
        self.emit_label(&after_first);
        self.emit_stack_load("a0", INPUT_OFFSET);
        for word in 0..16 {
            let skip = self.fresh_label("sha256_input_word_skip");
            self.emit(format!("li t5, {}", word + 1));
            self.emit(format!("bltu t6, t5, {}", skip));
            self.emit("li t4, 0");
            for byte in 0..4 {
                self.emit(format!("lbu t0, {}(a0)", word * 4 + byte));
                let shift = 24 - byte * 8;
                if shift != 0 {
                    self.emit(format!("slli t0, t0, {}", shift));
                }
                self.emit("or t4, t4, t0");
            }
            self.emit_stack_store("t4", W_OFFSET + word * 8);
            self.emit_label(&skip);
        }
        let first_is64 = self.fresh_label("sha256_first_is64");
        self.emit_stack_load("t0", LEN_OFFSET);
        self.emit("li t1, 64");
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", first_is64));
        self.emit("li t0, 2147483648");
        self.emit_stack_store("t0", W_OFFSET + 8 * 8);
        for word in 9..15 {
            self.emit_stack_store("zero", W_OFFSET + word * 8);
        }
        self.emit("li t0, 256");
        self.emit_stack_store("t0", W_OFFSET + 15 * 8);
        self.emit_label(&first_is64);
        self.emit_sp_addi("a0", STATE_OFFSET);
        self.emit_sp_addi("a1", W_OFFSET);
        self.emit("call __cellscript_sha256_compress");
        self.emit_stack_load("t0", LEN_OFFSET);
        self.emit("li t1, 32");
        self.emit("sub t2, t0, t1");
        self.emit(format!("beqz t2, {}", output));
        self.emit("li t0, 2147483648");
        self.emit_stack_store("t0", W_OFFSET);
        for word in 1..15 {
            self.emit_stack_store("zero", W_OFFSET + word * 8);
        }
        self.emit("li t0, 512");
        self.emit_stack_store("t0", W_OFFSET + 15 * 8);
        self.emit_sp_addi("a0", STATE_OFFSET);
        self.emit_sp_addi("a1", W_OFFSET);
        self.emit("call __cellscript_sha256_compress");
        self.emit_label(&output);
        self.emit_stack_load("a0", OUTPUT_OFFSET);
        for word in 0..8 {
            self.emit_stack_load("t0", STATE_OFFSET + word * 8);
            for byte in 0..4 {
                let shift = 24 - byte * 8;
                if shift == 0 {
                    self.emit("addi t1, t0, 0");
                } else {
                    self.emit(format!("srli t1, t0, {}", shift));
                }
                self.emit(format!("sb t1, {}(a0)", word * 4 + byte));
            }
        }
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::BoundsCheckFailed.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", RA_OFFSET);
        self.emit_stack_adjustment(FRAME_SIZE as i64);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_sha256_wrappers(&mut self) {
        self.emit_label("__ckb_hash_sha256");
        self.emit("# cellscript abi: SHA-256 over one 32-byte Hash; a0=input, a1=output");
        self.emit("addi a2, a1, 0");
        self.emit("li a1, 32");
        self.emit("j __cellscript_sha256_fixed");

        self.emit_label("__ckb_hash_sha256d");
        self.emit("# cellscript abi: SHA256d over one 32-byte Hash; a0=input, a1=output");
        self.emit_stack_adjustment(-64);
        self.emit_stack_store("ra", 24);
        self.emit_stack_store("a0", 8);
        self.emit_stack_store("a1", 16);
        self.emit_stack_load("a0", 8);
        self.emit("li a1, 32");
        self.emit_sp_addi("a2", 32);
        self.emit("call __cellscript_sha256_fixed");
        let sha256d_first_ok = self.fresh_label("sha256d_first_ok");
        let sha256d_done = self.fresh_label("sha256d_done");
        self.emit(format!("beqz a0, {}", sha256d_first_ok));
        self.emit(format!("j {}", sha256d_done));
        self.emit_label(&sha256d_first_ok);
        self.emit_sp_addi("a0", 32);
        self.emit("li a1, 32");
        self.emit_stack_load("a2", 16);
        self.emit("call __cellscript_sha256_fixed");
        self.emit_label(&sha256d_done);
        self.emit_stack_load("ra", 24);
        self.emit_stack_adjustment(64);
        self.emit("ret");

        self.emit_label("__ckb_hash_sha256_pair");
        self.emit("# cellscript abi: SHA-256 over left[32] || right[32]; a2=output");
        self.emit_stack_adjustment(-96);
        self.emit_stack_store("ra", 0);
        self.emit_stack_store("a0", 8);
        self.emit_stack_store("a1", 16);
        self.emit_stack_store("a2", 24);
        self.emit_stack_load("a0", 8);
        self.emit_sp_addi("a1", 32);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcpy_fixed");
        self.emit_stack_load("a0", 16);
        self.emit_sp_addi("a1", 64);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcpy_fixed");
        self.emit_sp_addi("a0", 32);
        self.emit("li a1, 64");
        self.emit_stack_load("a2", 24);
        self.emit("call __cellscript_sha256_fixed");
        self.emit_stack_load("ra", 0);
        self.emit_stack_adjustment(96);
        self.emit("ret");

        self.emit_label("__ckb_hash_sha256d_pair");
        self.emit("# cellscript abi: SHA256d over left[32] || right[32]; a2=output");
        self.emit_stack_adjustment(-128);
        self.emit_stack_store("ra", 0);
        self.emit_stack_store("a0", 8);
        self.emit_stack_store("a1", 16);
        self.emit_stack_store("a2", 24);
        self.emit_stack_load("a0", 8);
        self.emit_sp_addi("a1", 32);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcpy_fixed");
        self.emit_stack_load("a0", 16);
        self.emit_sp_addi("a1", 64);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcpy_fixed");
        self.emit_sp_addi("a0", 32);
        self.emit("li a1, 64");
        self.emit_sp_addi("a2", 96);
        self.emit("call __cellscript_sha256_fixed");
        let pair_first_ok = self.fresh_label("sha256d_pair_first_ok");
        let pair_done = self.fresh_label("sha256d_pair_done");
        self.emit(format!("beqz a0, {}", pair_first_ok));
        self.emit(format!("j {}", pair_done));
        self.emit_label(&pair_first_ok);
        self.emit_sp_addi("a0", 96);
        self.emit("li a1, 32");
        self.emit_stack_load("a2", 24);
        self.emit("call __cellscript_sha256_fixed");
        self.emit_label(&pair_done);
        self.emit_stack_load("ra", 0);
        self.emit_stack_adjustment(128);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_sha256d_merkle_requirement(&mut self) {
        const STACK_FRAME_BYTES: i64 = align_frame(112) as i64;
        self.emit_label("__ckb_require_sha256d_merkle_root");
        self.emit("# cellscript abi: bounded SHA256d Merkle path; siblings is exactly 16 Hash values, depth <= 16");
        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit_stack_store("ra", 0);
        self.emit_stack_store("a1", 8);
        self.emit_stack_store("a2", 16);
        self.emit_stack_store("a3", 24);
        self.emit_stack_store("a4", 32);
        let invalid = self.fresh_label("sha256d_merkle_invalid");
        let loop_label = self.fresh_label("sha256d_merkle_loop");
        let right_child = self.fresh_label("sha256d_merkle_right_child");
        let hash_ready = self.fresh_label("sha256d_merkle_hash_ready");
        let loop_done = self.fresh_label("sha256d_merkle_loop_done");
        let mismatch = self.fresh_label("sha256d_merkle_mismatch");
        let done = self.fresh_label("sha256d_merkle_done");
        self.emit(format!("beqz a0, {}", invalid));
        self.emit(format!("beqz a1, {}", invalid));
        self.emit(format!("beqz a4, {}", invalid));
        self.emit("li t0, 16");
        self.emit(format!("bltu t0, a2, {}", invalid));
        self.emit_sp_addi("a1", 48);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcpy_fixed");
        self.emit_stack_store("zero", 40);
        self.emit_label(&loop_label);
        self.emit_stack_load("t0", 40);
        self.emit_stack_load("t1", 16);
        self.emit(format!("bgeu t0, t1, {}", loop_done));
        self.emit("slli t1, t0, 5");
        self.emit_stack_load("t2", 8);
        self.emit("add t2, t2, t1");
        self.emit_stack_load("t3", 24);
        self.emit("li t4, 1");
        self.emit("and t4, t3, t4");
        self.emit(format!("bnez t4, {}", right_child));
        self.emit_sp_addi("a0", 48);
        self.emit("addi a1, t2, 0");
        self.emit_sp_addi("a2", 80);
        self.emit("call __ckb_hash_sha256d_pair");
        self.emit(format!("j {}", hash_ready));
        self.emit_label(&right_child);
        self.emit("addi a0, t2, 0");
        self.emit_sp_addi("a1", 48);
        self.emit_sp_addi("a2", 80);
        self.emit("call __ckb_hash_sha256d_pair");
        self.emit_label(&hash_ready);
        self.emit(format!("bnez a0, {}", invalid));
        self.emit_sp_addi("a0", 80);
        self.emit_sp_addi("a1", 48);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcpy_fixed");
        self.emit_stack_load("t0", 24);
        self.emit("srli t0, t0, 1");
        self.emit_stack_store("t0", 24);
        self.emit_stack_load("t0", 40);
        self.emit("addi t0, t0, 1");
        self.emit_stack_store("t0", 40);
        self.emit(format!("j {}", loop_label));
        self.emit_label(&loop_done);
        self.emit_stack_load("t0", 24);
        self.emit(format!("bnez t0, {}", invalid));
        self.emit_sp_addi("a0", 48);
        self.emit_stack_load("a1", 32);
        self.emit("li a2, 32");
        self.emit("call __cellscript_memcmp_fixed");
        self.emit(format!("bnez a0, {}", mismatch));
        self.emit("li a0, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::BoundsCheckFailed.code()));
        self.emit(format!("j {}", done));
        self.emit_label(&mismatch);
        self.emit("# cellscript runtime error 64 merkle-root-mismatch");
        self.emit(format!("li a0, {}", CellScriptRuntimeError::MerkleRootMismatch.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", 0);
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_blake2b_hash_var(&mut self, enabled: bool, input: RuntimeHashInput) {
        let streaming = !matches!(input, RuntimeHashInput::Memory);
        let prefixed = matches!(input, RuntimeHashInput::PrefixedTransaction);
        let segments = matches!(input, RuntimeHashInput::Segments);
        let symbol = match input {
            RuntimeHashInput::Memory => "__ckb_hash_blake2b_var",
            RuntimeHashInput::Transaction => "__cellscript_blake2b_transaction_span",
            RuntimeHashInput::PrefixedTransaction => "__cellscript_blake2b_prefixed_transaction_span",
            RuntimeHashInput::Segments => "__cellscript_blake2b_segments",
        };
        self.emit_global(symbol);
        self.emit_label(symbol);
        if segments {
            self.emit("# cellscript abi: checked segments; a0=descriptors, a1=count, a2=index, a3=source, a4=load syscall, a5=out[32], a6=total length; returns status");
        } else if streaming {
            self.emit("# cellscript abi: prevalidated transaction span; a0=index, a1=source, a2=offset, a3=len, a4=out[32], a5=load syscall; returns status");
            if prefixed {
                self.emit(
                    "# a6=prefix pointer, a7=prefix length <=128; total length includes prefix; caller proves span and sum bounds",
                );
            }
        } else {
            self.emit("# cellscript abi: CKB Blake2b-256 variable helper; a0=input, a1=len, a2=output[32], returns a0=0");
        }
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }

        const IV: [u64; 8] = [
            0x6a09e667f3bcc908,
            0xbb67ae8584caa73b,
            0x3c6ef372fe94f82b,
            0xa54ff53a5f1d36f1,
            0x510e527fade682d1,
            0x9b05688c2b3e6c1f,
            0x1f83d9abfb41bd6b,
            0x5be0cd19137e2179,
        ];
        const H_BASE: usize = 0;
        const V_BASE: usize = 64;
        const M_BASE: usize = 192;
        const PTR: usize = 320;
        const LEN: usize = 328;
        const OUT: usize = 336;
        const POS: usize = 344;
        const CHUNK: usize = 352;
        const INDEX: usize = 360;
        const SOURCE: usize = 368;
        const LOADED_SIZE: usize = 376;
        const SYSCALL: usize = 384;
        const PREFIX: usize = 392;
        const PREFIX_LEN: usize = 400;
        const READ_LEN: usize = 408;
        let (frame, saved_ra) = if segments {
            // The segmented reader owns slot 424 for its per-segment READ
            // length. Keep the return address in a distinct aligned trailer.
            (448, 440)
        } else if prefixed {
            (432, 424)
        } else if streaming {
            (400, 392)
        } else {
            (384, 376)
        };

        let personal0 = u64::from_le_bytes(*b"ckb-defa");
        let personal1 = u64::from_le_bytes(*b"ult-hash");
        let h = [IV[0] ^ 0x01010020, IV[1], IV[2], IV[3], IV[4], IV[5], IV[6] ^ personal0, IV[7] ^ personal1];

        self.emit_large_addi("sp", "sp", -frame);
        self.emit_stack_store("ra", saved_ra);
        if segments {
            // a0=checked descriptors, a1=count, a2=index, a3=source,
            // a4=syscall, a5=out[32], a6=checked total concatenated length.
            self.emit_stack_store("a0", PTR);
            self.emit_stack_store("a1", 416);
            self.emit_stack_store("a2", INDEX);
            self.emit_stack_store("a3", SOURCE);
            self.emit_stack_store("a4", SYSCALL);
            self.emit_stack_store("a5", OUT);
            self.emit_stack_store("a6", LEN);
            self.emit_stack_store("zero", 392);
            self.emit_stack_store("zero", 400);
        } else if streaming {
            self.emit_stack_store("a0", INDEX);
            self.emit_stack_store("a1", SOURCE);
            self.emit_stack_store("a2", PTR);
            self.emit_stack_store("a3", LEN);
            self.emit_stack_store("a4", OUT);
            self.emit_stack_store("a5", SYSCALL);
            if prefixed {
                self.emit_stack_store("a6", PREFIX);
                self.emit_stack_store("a7", PREFIX_LEN);
                self.emit("add t0, a3, a7");
                self.emit_stack_store("t0", LEN);
            }
        } else {
            self.emit_stack_store("a0", PTR);
            self.emit_stack_store("a1", LEN);
            self.emit_stack_store("a2", OUT);
        }
        self.emit_stack_store("zero", POS);
        for (index, value) in h.iter().enumerate() {
            self.emit_blake2b_store_const(*value, H_BASE + index * 8);
        }

        let block_label = self.fresh_label("blake2b_var_block");
        let done_label = self.fresh_label("blake2b_var_done");
        self.emit_label(&block_label);
        self.emit_stack_load("t0", POS);
        self.emit_stack_load("t1", LEN);
        self.emit("sub t2, t1, t0");
        let empty_first_block_label = self.fresh_label("blake2b_var_empty_first_block");
        self.emit(format!("bnez t2, {}", empty_first_block_label));
        self.emit(format!("beqz t0, {}", empty_first_block_label));
        self.emit(format!("j {}", done_label));
        self.emit_label(&empty_first_block_label);
        self.emit("li t3, 128");
        self.emit("sltu t4, t3, t2");
        let chunk_rem_label = self.fresh_label("blake2b_var_chunk_rem");
        let chunk_set_label = self.fresh_label("blake2b_var_chunk_set");
        self.emit(format!("beqz t4, {}", chunk_rem_label));
        self.emit("li t2, 128");
        self.emit(format!("j {}", chunk_set_label));
        self.emit_label(&chunk_rem_label);
        self.emit("# chunk already in t2");
        self.emit_label(&chunk_set_label);
        self.emit_stack_store("t2", CHUNK);
        let zero_loop = self.fresh_label("blake2b_var_zero_loop");
        let zero_done = self.fresh_label("blake2b_var_zero_done");
        // The message frame is 8-byte aligned. Clear it a machine word at a
        // time: every non-final block is overwritten by the source read, but
        // this bounded 16-store loop is still substantially cheaper than the
        // former 128-iteration byte loop and keeps final-block padding exact.
        self.emit_sp_addi("t0", M_BASE);
        self.emit_sp_addi("t1", M_BASE + 128);
        self.emit_label(&zero_loop);
        self.emit("sd zero, 0(t0)");
        self.emit("addi t0, t0, 8");
        self.emit(format!("bne t0, t1, {}", zero_loop));
        self.emit_label(&zero_done);

        if segments {
            self.emit_runtime_blake2b_segment_block();
        } else if streaming {
            let failure = self.fresh_label("blake2b_span_read_failed");
            let next = self.fresh_label("blake2b_span_read_done");
            if prefixed {
                // Only the first block can contain prefix bytes: the caller
                // proves prefix_len <= 128. Copy its intersection with the
                // current block, then fill the rest from the checked span.
                let copied = self.fresh_label("blake2b_prefix_copied");
                let copy = self.fresh_label("blake2b_prefix_copy");
                self.emit("li t2, 0");
                self.emit_stack_load("t0", POS);
                self.emit_stack_load("t1", PREFIX_LEN);
                self.emit(format!("bgeu t0, t1, {copied}"));
                self.emit_stack_load("t3", PREFIX);
                self.emit_sp_addi("t5", M_BASE);
                self.emit_label(&copy);
                self.emit(format!("bgeu t2, t1, {copied}"));
                self.emit("add t6, t3, t2");
                self.emit("lbu t6, 0(t6)");
                self.emit("add t4, t5, t2");
                self.emit("sb t6, 0(t4)");
                self.emit("addi t2, t2, 1");
                self.emit(format!("j {copy}"));
                self.emit_label(&copied);
                self.emit_stack_load("t0", CHUNK);
                self.emit("sub t0, t0, t2");
                self.emit(format!("beqz t0, {next}"));
                self.emit_stack_store("t0", READ_LEN);
                self.emit_stack_store("t0", LOADED_SIZE);
                self.emit_sp_addi("a0", M_BASE);
                self.emit("add a0, a0, t2");
                self.emit_sp_addi("a1", LOADED_SIZE);
                self.emit_stack_load("a2", PTR);
                self.emit_stack_load("t0", POS);
                self.emit("add t0, t0, t2");
                self.emit_stack_load("t1", PREFIX_LEN);
                self.emit("sub t0, t0, t1");
                self.emit("add a2, a2, t0");
            } else {
                self.emit_stack_load("t0", CHUNK);
                self.emit(format!("beqz t0, {next}"));
                self.emit_stack_store("t0", LOADED_SIZE);
                self.emit_sp_addi("a0", M_BASE);
                self.emit_sp_addi("a1", LOADED_SIZE);
                self.emit_stack_load("a2", PTR);
                self.emit_stack_load("t0", POS);
                // Public wrappers proved offset + length <= total without
                // overflow; POS and CHUNK never exceed that length.
                self.emit("add a2, a2, t0");
            }
            self.emit_stack_load("a3", INDEX);
            self.emit_stack_load("a4", SOURCE);
            self.emit_stack_load("a7", SYSCALL);
            self.emit("ecall");
            self.emit(format!("bnez a0, {failure}"));
            self.emit_stack_load("t0", LOADED_SIZE);
            self.emit_stack_load("t1", if prefixed { READ_LEN } else { CHUNK });
            self.emit(format!("bltu t0, t1, {failure}"));
            self.emit(format!("j {next}"));
            self.emit_label(&failure);
            self.emit_stack_load("ra", saved_ra);
            self.emit_large_addi("sp", "sp", frame);
            self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            self.emit_label(&next);
        } else {
            let copy_loop = self.fresh_label("blake2b_var_copy_loop");
            let copy_done = self.fresh_label("blake2b_var_copy_done");
            self.emit("li t0, 0");
            self.emit_label(&copy_loop);
            self.emit_stack_load("t1", CHUNK);
            self.emit("sltu t2, t0, t1");
            self.emit(format!("beqz t2, {}", copy_done));
            self.emit_stack_load("t3", PTR);
            self.emit_stack_load("t4", POS);
            self.emit("add t3, t3, t4");
            self.emit("add t3, t3, t0");
            self.emit("lbu t5, 0(t3)");
            self.emit(format!("li t6, {}", M_BASE));
            self.emit("add t6, sp, t6");
            self.emit("add t6, t6, t0");
            self.emit("sb t5, 0(t6)");
            self.emit("addi t0, t0, 1");
            self.emit(format!("j {}", copy_loop));
            self.emit_label(&copy_done);
        }

        for index in 0..8 {
            self.emit_stack_load("t0", H_BASE + index * 8);
            self.emit_stack_store("t0", V_BASE + index * 8);
        }
        for (index, value) in IV.iter().enumerate() {
            self.emit_blake2b_store_const(*value, V_BASE + (index + 8) * 8);
        }
        self.emit_stack_load("t0", POS);
        self.emit_stack_load("t1", CHUNK);
        self.emit("add t0, t0, t1");
        self.emit_stack_load("t2", V_BASE + 12 * 8);
        self.emit("xor t2, t2, t0");
        self.emit_stack_store("t2", V_BASE + 12 * 8);
        self.emit_stack_load("t2", V_BASE + 13 * 8);
        self.emit_stack_store("t2", V_BASE + 13 * 8);
        let not_final_label = self.fresh_label("blake2b_var_not_final");
        self.emit_stack_load("t3", LEN);
        self.emit("sub t4, t3, t0");
        self.emit(format!("bnez t4, {}", not_final_label));
        self.emit_stack_load("t5", V_BASE + 14 * 8);
        self.emit("xori t5, t5, -1");
        self.emit_stack_store("t5", V_BASE + 14 * 8);
        self.emit_label(&not_final_label);

        self.emit("mv a0, sp");
        self.emit("call __cellscript_blake2b_compress");
        self.emit_stack_load("t0", POS);
        self.emit_stack_load("t1", CHUNK);
        self.emit("add t0, t0, t1");
        self.emit_stack_store("t0", POS);
        self.emit(format!("beqz t1, {}", done_label));
        self.emit(format!("j {}", block_label));

        self.emit_label(&done_label);
        self.emit_stack_load("t6", OUT);
        for index in 0..4 {
            self.emit_stack_load("t0", H_BASE + index * 8);
            self.emit(format!("sd t0, {}(t6)", index * 8));
        }
        self.emit_stack_load("ra", saved_ra);
        self.emit_large_addi("sp", "sp", frame);
        self.emit("li a0, 0");
        self.emit("ret");
    }

    pub(super) fn emit_runtime_raw_transaction_hash_without_cell_deps(&mut self, enabled: bool) {
        const PREFIX: usize = 0; // canonical raw header, version, empty deps
        const HEADER: usize = 40; // first 48 bytes of the full Transaction
        const TAIL_OFFSET: usize = 88;
        const TAIL_LEN: usize = 96;
        const SIZE: usize = 104;
        const OUT: usize = 112;
        const RA: usize = 120;
        const FRAME: i64 = 128;
        self.emit_global("__ckb_raw_transaction_hash_without_cell_deps");
        self.emit_label("__ckb_raw_transaction_hash_without_cell_deps");
        self.emit("# cellscript abi: a3=out[32], returns a0=status; canonical node Transaction, empty raw CellDepVec");
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }
        let failed = self.fresh_label("raw_transaction_hash_failed");
        let done = self.fresh_label("raw_transaction_hash_done");
        self.emit_large_addi("sp", "sp", -FRAME);
        self.emit_stack_store("ra", RA);
        self.emit_stack_store("a3", OUT);
        self.emit("li t0, 48");
        self.emit_stack_store("t0", SIZE);
        self.emit_sp_addi("a0", HEADER);
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_TRANSACTION));
        self.emit("ecall");
        self.emit(format!("bnez a0, {failed}"));
        self.emit_stack_load("t1", SIZE);
        self.emit("li t0, 68"); // minimal raw 52 + outer 12 + witnesses 4
        self.emit(format!("bltu t1, t0, {failed}"));
        self.emit_stack_u32_le_to("t0", HEADER);
        self.emit(format!("bne t0, t1, {failed}"));
        self.emit_stack_u32_le_to("t0", HEADER + 4);
        self.emit("li t2, 12");
        self.emit(format!("bne t0, t2, {failed}"));
        self.emit_stack_u32_le_to("t2", HEADER + 8);
        self.emit("addi t0, t2, 4");
        self.emit(format!("bltu t1, t0, {failed}"));
        self.emit_stack_u32_le_to("t3", HEADER + 12);
        self.emit("li t0, 52");
        self.emit(format!("bltu t3, t0, {failed}"));
        self.emit("addi t0, t3, 12");
        self.emit(format!("bne t0, t2, {failed}"));
        for (offset, expected) in [(16, 28), (20, 32)] {
            self.emit_stack_u32_le_to("t0", HEADER + offset);
            self.emit(format!("li t1, {expected}"));
            self.emit(format!("bne t0, t1, {failed}"));
        }
        self.emit_stack_u32_le_to("t2", HEADER + 24); // header_deps start in raw
        self.emit_stack_u32_le_to("t0", HEADER + 44); // CellDepVec item count
        self.emit("li t1, 37"); // fixed CellDep: OutPoint[36] + dep_type[1]
        self.emit("mul t0, t0, t1");
        self.emit("addi t0, t0, 36"); // raw header/version + vector count
        self.emit(format!("bne t0, t2, {failed}"));
        self.emit("addi t1, t2, 0");
        for offset in [28, 32, 36, 12] {
            self.emit_stack_u32_le_to("t0", HEADER + offset);
            self.emit("addi t1, t1, 4"); // every retained vector has a u32 header
            self.emit(format!("bltu t0, t1, {failed}"));
            self.emit("addi t1, t0, 0");
        }
        // Bounds now prove tail_offset + tail_len == witness_offset. Hash
        // neither the outer table nor witnesses, and do not copy a whole tx.
        self.emit("addi t0, t2, 12");
        self.emit_stack_store("t0", TAIL_OFFSET);
        self.emit("sub t0, t3, t2");
        self.emit_stack_store("t0", TAIL_LEN);
        self.emit("addi t5, t2, -36"); // bytes removed from the CellDepVec
        for (destination, original, adjusted) in [
            (0, 12, true),
            (4, 16, false),
            (8, 20, false),
            (12, 24, true),
            (16, 28, true),
            (20, 32, true),
            (24, 36, true),
            (28, 40, false),
        ] {
            self.emit_stack_u32_le_to("t0", HEADER + original); // clobbers t4 only
            if adjusted {
                self.emit("sub t0, t0, t5");
            }
            for byte in 0..4 {
                self.emit_stack_store_byte("t0", PREFIX + destination + byte);
                self.emit("srli t0, t0, 8");
            }
        }
        for byte in 32..36 {
            self.emit_stack_store_byte("zero", PREFIX + byte);
        }
        self.emit("li a0, 0");
        self.emit("li a1, 0");
        self.emit_stack_load("a2", TAIL_OFFSET);
        self.emit_stack_load("a3", TAIL_LEN);
        self.emit_stack_load("a4", OUT);
        self.emit(format!("li a5, {}", ckb_abi::syscall::LOAD_TRANSACTION));
        self.emit_sp_addi("a6", PREFIX);
        self.emit("li a7, 36");
        self.emit("call __cellscript_blake2b_prefixed_transaction_span");
        self.emit(format!("j {done}"));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", RA);
        self.emit_large_addi("sp", "sp", FRAME);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_span_hash_helper(&mut self, symbol: &str, source: RuntimeByteSource, enabled: bool) {
        const OFFSET: usize = 0;
        const LEN: usize = 8;
        const OUT: usize = 16;
        const INDEX: usize = 24;
        const SOURCE: usize = 32;
        const SIZE: usize = 40;
        const BUFFER: usize = 48;
        const RA: usize = 56;
        const FRAME: i64 = 64;
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit("# cellscript abi: exact CKB Blake2b span; a0=SourceView, a1=offset, a2=len, a3=out[32]; returns a0=status");
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }
        let invalid = self.fresh_label("span_hash_invalid_source");
        let ready = self.fresh_label("span_hash_source_ready");
        let failed = self.fresh_label("span_hash_failed");
        let done = self.fresh_label("span_hash_done");
        let abi = self.runtime_abi();
        let (syscall, error) = match source {
            RuntimeByteSource::CellData => (abi.load_cell_data, CellScriptRuntimeError::CellLoadFailed),
            RuntimeByteSource::Witness => (abi.load_witness, CellScriptRuntimeError::SyscallFailed),
            RuntimeByteSource::CellLock | RuntimeByteSource::CellType => {
                unreachable!("Script fields do not use the CellData/Witness span-hash ABI")
            }
        };
        self.emit_large_addi("sp", "sp", -FRAME);
        self.emit_stack_store("ra", RA);
        self.emit_stack_store("a1", OFFSET);
        self.emit_stack_store("a2", LEN);
        self.emit_stack_store("a3", OUT);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        for allowed in
            [CKB_SOURCE_INPUT, CKB_SOURCE_OUTPUT, CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT, CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT]
        {
            self.emit(format!("li t0, {allowed}"));
            self.emit(format!("beq t0, t2, {ready}"));
        }
        if matches!(source, RuntimeByteSource::CellData) {
            self.emit(format!("li t0, {CKB_SOURCE_CELL_DEP}"));
            self.emit(format!("beq t0, t2, {ready}"));
        }
        self.emit(format!("j {invalid}"));
        self.emit_label(&ready);
        self.emit_stack_store("t1", INDEX);
        self.emit_stack_store("t2", SOURCE);
        self.emit_stack_store("zero", SIZE);
        self.emit_sp_addi("a0", BUFFER);
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit("mv a3, t1");
        self.emit("mv a4, t2");
        self.emit(format!("li a7, {syscall}"));
        self.emit("ecall");
        self.emit(format!("bnez a0, {failed}"));
        self.emit_stack_load("t0", SIZE);
        self.emit_stack_load("t1", OFFSET);
        self.emit(format!("bltu t0, t1, {failed}"));
        self.emit("sub t0, t0, t1");
        self.emit_stack_load("t1", LEN);
        self.emit(format!("bltu t0, t1, {failed}"));
        self.emit_stack_load("a0", INDEX);
        self.emit_stack_load("a1", SOURCE);
        self.emit_stack_load("a2", OFFSET);
        self.emit_stack_load("a3", LEN);
        self.emit_stack_load("a4", OUT);
        self.emit(format!("li a5, {syscall}"));
        self.emit("call __cellscript_blake2b_transaction_span");
        self.emit(format!("bnez a0, {failed}"));
        self.emit(format!("j {done}"));
        self.emit_label(&invalid);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {done}"));
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", error.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", RA);
        self.emit_large_addi("sp", "sp", FRAME);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_blake2b_hash32(&mut self, enabled: bool) {
        self.emit_global("__ckb_hash_blake2b");
        self.emit_label("__ckb_hash_blake2b");
        self.emit("# cellscript abi: CKB Blake2b-256 helper; a0=input[32], a1=output[32], returns a0=0");
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }

        const IV: [u64; 8] = [
            0x6a09e667f3bcc908,
            0xbb67ae8584caa73b,
            0x3c6ef372fe94f82b,
            0xa54ff53a5f1d36f1,
            0x510e527fade682d1,
            0x9b05688c2b3e6c1f,
            0x1f83d9abfb41bd6b,
            0x5be0cd19137e2179,
        ];
        const H_BASE: usize = 0;
        const V_BASE: usize = 64;
        const M_BASE: usize = 192;
        const RA: usize = 320;
        const OUT: usize = 328;
        const FRAME: usize = 336;

        let personal0 = u64::from_le_bytes(*b"ckb-defa");
        let personal1 = u64::from_le_bytes(*b"ult-hash");
        let h = [IV[0] ^ 0x01010020, IV[1], IV[2], IV[3], IV[4], IV[5], IV[6] ^ personal0, IV[7] ^ personal1];

        self.emit_large_addi("sp", "sp", -(FRAME as i64));
        self.emit_stack_store("ra", RA);
        self.emit_stack_store("a1", OUT);
        for (index, value) in h.iter().enumerate() {
            self.emit_blake2b_store_const(*value, H_BASE + index * 8);
        }
        for index in 0..4 {
            self.emit_blake2b_load_input_word(index, M_BASE + index * 8);
        }
        for index in 4..16 {
            self.emit_stack_store("zero", M_BASE + index * 8);
        }
        for index in 0..8 {
            self.emit_stack_load("t0", H_BASE + index * 8);
            self.emit_stack_store("t0", V_BASE + index * 8);
        }
        for (index, value) in IV.iter().enumerate() {
            self.emit_blake2b_store_const(*value, V_BASE + (index + 8) * 8);
        }
        self.emit_stack_load("t0", V_BASE + 12 * 8);
        self.emit("xori t0, t0, 32");
        self.emit_stack_store("t0", V_BASE + 12 * 8);
        self.emit_stack_load("t0", V_BASE + 14 * 8);
        self.emit("xori t0, t0, -1");
        self.emit_stack_store("t0", V_BASE + 14 * 8);

        self.emit("mv a0, sp");
        self.emit("call __cellscript_blake2b_compress");
        self.emit_stack_load("a1", OUT);
        for index in 0..4 {
            self.emit_stack_load("t0", H_BASE + index * 8);
            self.emit(format!("sd t0, {}(a1)", index * 8));
        }
        self.emit_stack_load("ra", RA);
        self.emit_large_addi("sp", "sp", FRAME as i64);
        self.emit("li a0, 0");
        self.emit("ret");
    }

    pub(super) fn emit_runtime_blake2b_hash_pair(&mut self, enabled: bool) {
        self.emit_global("__ckb_hash_pair");
        self.emit_label("__ckb_hash_pair");
        self.emit("# cellscript abi: hash_pair combines two 32-byte Hash inputs with CKB Blake2b-256; a0=left[32], a1=right[32], a2=output[32]");
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }

        const IV: [u64; 8] = [
            0x6a09e667f3bcc908,
            0xbb67ae8584caa73b,
            0x3c6ef372fe94f82b,
            0xa54ff53a5f1d36f1,
            0x510e527fade682d1,
            0x9b05688c2b3e6c1f,
            0x1f83d9abfb41bd6b,
            0x5be0cd19137e2179,
        ];
        const H_BASE: usize = 0;
        const V_BASE: usize = 64;
        const M_BASE: usize = 192;
        const RA: usize = 320;
        const OUT: usize = 328;
        const FRAME: usize = 336;

        let personal0 = u64::from_le_bytes(*b"ckb-defa");
        let personal1 = u64::from_le_bytes(*b"ult-hash");
        let h = [IV[0] ^ 0x01010020, IV[1], IV[2], IV[3], IV[4], IV[5], IV[6] ^ personal0, IV[7] ^ personal1];

        self.emit_large_addi("sp", "sp", -(FRAME as i64));
        self.emit_stack_store("ra", RA);
        self.emit_stack_store("a2", OUT);
        for (index, value) in h.iter().enumerate() {
            self.emit_blake2b_store_const(*value, H_BASE + index * 8);
        }
        for index in 0..4 {
            self.emit_blake2b_load_input_word(index, M_BASE + index * 8);
        }
        for index in 0..4 {
            self.emit_blake2b_load_input_word_from("a1", index, M_BASE + (index + 4) * 8);
        }
        for index in 8..16 {
            self.emit_stack_store("zero", M_BASE + index * 8);
        }
        for index in 0..8 {
            self.emit_stack_load("t0", H_BASE + index * 8);
            self.emit_stack_store("t0", V_BASE + index * 8);
        }
        for (index, value) in IV.iter().enumerate() {
            self.emit_blake2b_store_const(*value, V_BASE + (index + 8) * 8);
        }
        self.emit_stack_load("t0", V_BASE + 12 * 8);
        self.emit("xori t0, t0, 64");
        self.emit_stack_store("t0", V_BASE + 12 * 8);
        self.emit_stack_load("t0", V_BASE + 14 * 8);
        self.emit("xori t0, t0, -1");
        self.emit_stack_store("t0", V_BASE + 14 * 8);

        self.emit("mv a0, sp");
        self.emit("call __cellscript_blake2b_compress");
        self.emit_stack_load("a2", OUT);
        for index in 0..4 {
            self.emit_stack_load("t0", H_BASE + index * 8);
            self.emit(format!("sd t0, {}(a2)", index * 8));
        }
        self.emit_stack_load("ra", RA);
        self.emit_large_addi("sp", "sp", FRAME as i64);
        self.emit("li a0, 0");
        self.emit("ret");
    }

    pub(super) fn emit_blake2b_store_const(&mut self, value: u64, stack_offset: usize) {
        self.emit(format!("li t0, 0x{:016x}", value));
        self.emit_stack_store("t0", stack_offset);
    }

    pub(super) fn emit_blake2b_load_input_word(&mut self, word_index: usize, stack_offset: usize) {
        self.emit_blake2b_load_input_word_from("a0", word_index, stack_offset);
    }

    pub(super) fn emit_blake2b_load_input_word_from(&mut self, source_reg: &str, word_index: usize, stack_offset: usize) {
        self.emit("li t0, 0");
        for byte_index in 0..8 {
            let absolute = word_index * 8 + byte_index;
            self.emit(format!("lbu t1, {}({})", absolute, source_reg));
            if byte_index > 0 {
                self.emit(format!("slli t1, t1, {}", byte_index * 8));
            }
            self.emit("or t0, t0, t1");
        }
        self.emit_stack_store("t0", stack_offset);
    }

    /// Shared BLAKE2b compression over one caller-owned state frame.
    ///
    /// `a0` points to the common `[h(64), v(128), m(128)]` layout used by
    /// fixed, variable, transaction-span, prefixed-span, and gather hashing.
    /// The caller initializes counters/final flags and retains ownership of
    /// the frame; this leaf helper performs only the twelve rounds and the
    /// final `h ^= v[0..8] ^ v[8..16]` merge.
    pub(super) fn emit_runtime_blake2b_compress(&mut self) {
        const STACK_FRAME_BYTES: i64 = align_frame(32) as i64;
        const H_BASE: usize = 0;
        const V_BASE: usize = 64;
        const M_BASE: usize = 192;
        const V_REGISTERS: [&str; 16] =
            ["a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7", "t0", "t1", "t2", "t3", "t4", "t5", "t6", "ra"];
        self.emit_global("__cellscript_blake2b_compress");
        self.emit_label("__cellscript_blake2b_compress");
        self.emit(
            "# cellscript abi: shared BLAKE2b compression; a0=caller state frame; clobbers caller-saved registers; preserves s1-s3",
        );
        self.emit_stack_adjustment(-STACK_FRAME_BYTES);
        self.emit("sd ra, 0(sp)");
        self.emit("sd s1, 8(sp)");
        self.emit("sd s2, 16(sp)");
        self.emit("sd s3, 24(sp)");
        self.emit("mv s1, a0");
        for (index, register) in V_REGISTERS.iter().enumerate() {
            self.emit(format!("ld {register}, {}(s1)", V_BASE + index * 8));
        }
        for round in BLAKE2B_SIGMA {
            self.emit_blake2b_register_g(&V_REGISTERS, M_BASE, 0, 4, 8, 12, round[0], round[1]);
            self.emit_blake2b_register_g(&V_REGISTERS, M_BASE, 1, 5, 9, 13, round[2], round[3]);
            self.emit_blake2b_register_g(&V_REGISTERS, M_BASE, 2, 6, 10, 14, round[4], round[5]);
            self.emit_blake2b_register_g(&V_REGISTERS, M_BASE, 3, 7, 11, 15, round[6], round[7]);
            self.emit_blake2b_register_g(&V_REGISTERS, M_BASE, 0, 5, 10, 15, round[8], round[9]);
            self.emit_blake2b_register_g(&V_REGISTERS, M_BASE, 1, 6, 11, 12, round[10], round[11]);
            self.emit_blake2b_register_g(&V_REGISTERS, M_BASE, 2, 7, 8, 13, round[12], round[13]);
            self.emit_blake2b_register_g(&V_REGISTERS, M_BASE, 3, 4, 9, 14, round[14], round[15]);
        }
        for index in 0..8 {
            self.emit(format!("ld s2, {}(s1)", H_BASE + index * 8));
            self.emit(format!("xor s2, s2, {}", V_REGISTERS[index]));
            self.emit(format!("xor s2, s2, {}", V_REGISTERS[index + 8]));
            self.emit(format!("sd s2, {}(s1)", H_BASE + index * 8));
        }
        self.emit("ld ra, 0(sp)");
        self.emit("ld s1, 8(sp)");
        self.emit("ld s2, 16(sp)");
        self.emit("ld s3, 24(sp)");
        self.emit_stack_adjustment(STACK_FRAME_BYTES);
        self.emit("ret");
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_blake2b_register_g(
        &mut self,
        v: &[&str; 16],
        m_base: usize,
        a: usize,
        b: usize,
        c: usize,
        d: usize,
        mx: usize,
        my: usize,
    ) {
        self.emit(format!("add {}, {}, {}", v[a], v[a], v[b]));
        self.emit(format!("ld s2, {}(s1)", m_base + mx * 8));
        self.emit(format!("add {}, {}, s2", v[a], v[a]));
        self.emit(format!("xor {}, {}, {}", v[d], v[d], v[a]));
        self.emit_blake2b_register_rotr(v[d], 32);
        self.emit(format!("add {}, {}, {}", v[c], v[c], v[d]));
        self.emit(format!("xor {}, {}, {}", v[b], v[b], v[c]));
        self.emit_blake2b_register_rotr(v[b], 24);
        self.emit(format!("add {}, {}, {}", v[a], v[a], v[b]));
        self.emit(format!("ld s2, {}(s1)", m_base + my * 8));
        self.emit(format!("add {}, {}, s2", v[a], v[a]));
        self.emit(format!("xor {}, {}, {}", v[d], v[d], v[a]));
        self.emit_blake2b_register_rotr(v[d], 16);
        self.emit(format!("add {}, {}, {}", v[c], v[c], v[d]));
        self.emit(format!("xor {}, {}, {}", v[b], v[b], v[c]));
        self.emit_blake2b_register_rotr(v[b], 63);
    }

    pub(super) fn emit_blake2b_register_rotr(&mut self, register: &str, bits: usize) {
        self.emit(format!("rori {register}, {register}, {bits}"));
    }
}
