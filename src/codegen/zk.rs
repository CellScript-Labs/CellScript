//! Exact ZK parent transport. Statement values come from CKB, never the witness.
use super::*;
use cellscript_artifact_checker::zk as wire;

impl CodeGenerator {
    fn zk_check_zero(&mut self, error: CellScriptRuntimeError) {
        let ok = self.fresh_label("zk_checked");
        self.emit(format!("beqz a0, {ok}"));
        self.emit_fail(error);
        self.emit_label(&ok);
    }

    // No live registers across this helper. All destinations are parent-local
    // frame offsets; t0/t1 are explicit scratch and syscalls clobber a0..a7.
    fn zk_load(&mut self, syscall: u64, output: usize, width: usize, index: u64, source: u64, field: u64) {
        let size = self.runtime_expr_temp_offset(3);
        self.emit(format!("li t0, {width}"));
        self.emit_stack_store("t0", size);
        self.emit_sp_addi("a0", output);
        self.emit_sp_addi("a1", size);
        self.emit("li a2, 0");
        self.emit(format!("li a3, {index}"));
        self.emit(format!("li a4, {source}"));
        self.emit(format!("li a5, {field}"));
        self.emit(format!("li a7, {syscall}"));
        self.emit("ecall");
        self.zk_check_zero(CellScriptRuntimeError::ZkContextInvalid);
        self.emit_stack_load("t0", size);
        self.emit(format!("li t1, {width}"));
        let ok = self.fresh_label("zk_exact_load");
        self.emit(format!("beq t0, t1, {ok}"));
        self.emit_fail(CellScriptRuntimeError::ZkContextInvalid);
        self.emit_label(&ok);
    }

    pub(super) fn emit_zk_transition(&mut self, args: &[IrOperand]) -> Result<()> {
        let [_, proof, dependency, _, key, domain, action] = args else {
            return Err(CompileError::without_span("invalid exact ZK lowering arguments"));
        };
        let hash = |value: &IrOperand| match value {
            IrOperand::Const(IrConst::Hash(bytes)) => Ok(*bytes),
            _ => Err(CompileError::without_span("ZK identities must be compile-time hashes")),
        };
        let request = wire::Request {
            verification_key: hash(key)?,
            proof: [0; wire::PROOF_BYTES],
            statement: wire::Statement {
                domain: hash(domain)?,
                action: hash(action)?,
                script_hash: [0; 32],
                old_data_hash: [0; 32],
                new_data_hash: [0; 32],
                input_transaction_hash: [0; 32],
                input_output_index: 0,
                transaction_hash: [0; 32],
            },
        }
        .encode();
        let proof = self
            .expected_fixed_byte_source(proof, wire::PROOF_BYTES)
            .ok_or_else(|| CompileError::without_span("ZK proof has no exact fixed-byte storage"))?;
        self.emit_prepare_fixed_byte_source(&proof, wire::PROOF_BYTES, "ZK proof");
        let buffer = self.runtime_scratch_buffer_offset();
        let read_fd = self.runtime_expr_temp_offset(0);
        let write_fd = self.runtime_expr_temp_offset(1);
        let pid = self.runtime_expr_temp_offset(2);
        let size = self.runtime_expr_temp_offset(3);
        let cycles = self.runtime_expr_temp_offset(4);
        self.emit("# cellscript zk: exact transition profile v2 begin");
        self.emit(format!("li a7, {}", ckb_abi::syscall::CURRENT_CYCLES));
        self.emit("ecall");
        self.emit_stack_store("a0", cycles);
        let begin = self.fresh_label("zk_begin");
        self.emit_label(&begin);
        for (index, word) in request.chunks_exact(8).enumerate() {
            self.emit(format!("li t0, {}", u64::from_le_bytes(word.try_into().expect("eight bytes"))));
            self.emit_stack_store("t0", buffer + index * 8);
        }
        let initialized = self.fresh_label("zk_request_initialized");
        self.emit_label(&initialized);
        if !self.emit_fixed_byte_source_pointer_or_const_to("a0", &proof) {
            return Err(CompileError::without_span("ZK proof cannot be materialized"));
        }
        self.emit_sp_addi("a1", buffer + 108);
        self.emit("li a2, 128");
        self.emit("call __cellscript_memcpy_fixed");
        self.zk_load(ckb_abi::syscall::LOAD_SCRIPT_HASH, buffer + 300, 32, 0, 0, 0);
        for (source, offset) in [(ckb_abi::source::GROUP_INPUT, 332), (ckb_abi::source::GROUP_OUTPUT, 364)] {
            self.zk_load(ckb_abi::syscall::LOAD_CELL_BY_FIELD, buffer + offset, 32, 0, source, ckb_abi::cell_field::DATA_HASH);
            self.zk_load(ckb_abi::syscall::LOAD_CELL_BY_FIELD, buffer + 464, 32, 0, source, ckb_abi::cell_field::TYPE_HASH);
            self.emit_sp_addi("a0", buffer + 300);
            self.emit_sp_addi("a1", buffer + 464);
            self.emit("li a2, 32");
            self.emit("call __cellscript_memcmp_fixed");
            self.zk_check_zero(CellScriptRuntimeError::ZkContextInvalid);
            // Reject extra group members: only INDEX_OUT_OF_BOUND is accepted.
            self.emit("li t0, 32");
            self.emit_stack_store("t0", size);
            self.emit_sp_addi("a0", buffer + 464);
            self.emit_sp_addi("a1", size);
            self.emit("li a2, 0");
            self.emit("li a3, 1");
            self.emit(format!("li a4, {source}"));
            self.emit(format!("li a5, {}", ckb_abi::cell_field::DATA_HASH));
            self.emit(format!("li a7, {}", ckb_abi::syscall::LOAD_CELL_BY_FIELD));
            self.emit("ecall");
            self.emit("addi a0, a0, -1");
            self.zk_check_zero(CellScriptRuntimeError::ZkContextInvalid);
        }
        self.zk_load(
            ckb_abi::syscall::LOAD_INPUT_BY_FIELD,
            buffer + 396,
            36,
            0,
            ckb_abi::source::GROUP_INPUT,
            ckb_abi::input_field::OUT_POINT,
        );
        // Raw transaction hash includes all outputs, output data, inputs and
        // dependency outpoints; this prevents recipient/capacity substitution.
        self.zk_load(ckb_abi::syscall::LOAD_TX_HASH, buffer + 432, 32, 0, 0, 0);
        self.emit("call __ckb_pipe");
        self.zk_check_zero(CellScriptRuntimeError::ZkPipeFailed);
        self.emit_stack_store("a1", read_fd);
        self.emit_stack_store("a2", write_fd);
        self.emit_operand_to_register("a0", dependency);
        self.emit("slli a0, a0, 32");
        self.emit("srli a0, a0, 32");
        self.emit_stack_load("a1", read_fd);
        self.emit("call __ckb_spawn_with_fd1");
        self.zk_check_zero(CellScriptRuntimeError::ZkSpawnFailed);
        self.emit_stack_store("a1", pid);
        // Spawn transfers read-fd ownership to the child. Parent owns only
        // the writer. A successful short write is rejected explicitly.
        for word in 0..wire::REQUEST_WORDS {
            self.emit("li t0, 8");
            self.emit_stack_store("t0", size);
            self.emit_stack_load("a0", write_fd);
            self.emit_sp_addi("a1", buffer + word * 8);
            self.emit_sp_addi("a2", size);
            self.emit(format!("li a7, {}", ckb_abi::syscall::WRITE));
            self.emit("ecall");
            self.zk_check_zero(CellScriptRuntimeError::ZkWriteFailed);
            self.emit_stack_load("t0", size);
            self.emit("li t1, 8");
            let ok = self.fresh_label("zk_complete_word");
            self.emit(format!("beq t0, t1, {ok}"));
            self.emit_fail(CellScriptRuntimeError::ZkWriteFailed);
            self.emit_label(&ok);
        }
        self.emit_stack_load("a0", write_fd);
        self.emit("call __ckb_close");
        self.zk_check_zero(CellScriptRuntimeError::ZkCloseFailed);
        self.emit_stack_load("a0", pid);
        self.emit("call __ckb_wait");
        self.zk_check_zero(CellScriptRuntimeError::ZkChildRejected);
        self.emit(format!("li a7, {}", ckb_abi::syscall::CURRENT_CYCLES));
        self.emit("ecall");
        self.emit_stack_load("t0", cycles);
        self.emit("sub t0, a0, t0");
        self.emit("li t1, 250000000");
        let ok = self.fresh_label("zk_verified");
        self.emit(format!("bgeu t1, t0, {ok}"));
        self.emit_fail(CellScriptRuntimeError::ZkCycleBoundExceeded);
        self.emit_label(&ok);
        self.emit("# cellscript zk: exact transition profile v2 checked");
        Ok(())
    }
}
