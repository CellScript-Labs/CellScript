use super::super::{ckb_abi, CellScriptRuntimeError, CodeGenerator};

impl CodeGenerator {
    pub(super) fn emit_runtime_header_field_u64(
        &mut self,
        symbol: &str,
        field_name: &str,
        field_id: u64,
        enabled: bool,
        disabled_reason: &str,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        if !enabled {
            self.emit(format!("# cellscript abi: {}", disabled_reason));
            self.emit_process_failure(CellScriptRuntimeError::ConsumeInvalidOperand);
            return;
        }

        let abi = self.runtime_abi();
        self.emit_large_addi("sp", "sp", -32);
        self.emit_stack_store("ra", 24);
        self.emit(format!("# cellscript abi: LOAD_HEADER_BY_FIELD field={} source=HeaderDep index=0", field_name));
        self.emit("li t0, 8");
        self.emit_stack_store("t0", 8);
        self.emit_sp_addi("a0", 16);
        self.emit_sp_addi("a1", 8);
        self.emit("li a2, 0");
        self.emit("li a3, 0");
        self.emit(format!("li a4, {}", abi.source_header_dep));
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_header_by_field));
        self.emit("ecall");
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_loaded_schema_exact_size_check(8, 8, "header scalar return");
        self.emit_stack_load("a0", 16);
        self.emit_stack_load("ra", 24);
        self.emit_large_addi("sp", "sp", 32);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_header_dep_field_u64(
        &mut self,
        symbol: &str,
        field_name: &str,
        field_id: u64,
        enabled: bool,
        disabled_reason: &str,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        if !enabled {
            self.emit(format!("# cellscript abi: {}", disabled_reason));
            self.emit_process_failure(CellScriptRuntimeError::ConsumeInvalidOperand);
            return;
        }

        let invalid = self.fresh_label("header_dep_view_invalid");
        let abi = self.runtime_abi();
        self.emit_large_addi("sp", "sp", -32);
        self.emit_stack_store("ra", 24);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", abi.source_header_dep));
        self.emit(format!("bne t2, t0, {}", invalid));
        self.emit(format!("# cellscript abi: LOAD_HEADER_BY_FIELD field={} source=HeaderDep index=SourceView", field_name));
        self.emit("li t0, 8");
        self.emit_stack_store("t0", 8);
        self.emit_sp_addi("a0", 16);
        self.emit_sp_addi("a1", 8);
        self.emit("li a2, 0");
        self.emit("mv a3, t1");
        self.emit("mv a4, t2");
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_header_by_field));
        self.emit("ecall");
        self.emit_return_on_syscall_error(CellScriptRuntimeError::HeaderDepMissing);
        self.emit_loaded_schema_exact_size_check(8, 8, "HeaderDep scalar return");
        self.emit_stack_load("a0", 16);
        self.emit_stack_load("ra", 24);
        self.emit_large_addi("sp", "sp", 32);
        self.emit("ret");

        self.emit_label(&invalid);
        self.emit_process_failure(CellScriptRuntimeError::CkbSourceViewInvalid);
    }

    pub(super) fn emit_runtime_header_dep_full_u64(
        &mut self,
        symbol: &str,
        field_name: &str,
        field_offset: usize,
        enabled: bool,
        disabled_reason: &str,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        if !enabled {
            self.emit(format!("# cellscript abi: {disabled_reason}"));
            self.emit_process_failure(CellScriptRuntimeError::ConsumeInvalidOperand);
            return;
        }

        const SIZE_OFFSET: usize = 0;
        const HEADER_OFFSET: usize = 16;
        const FRAME_SIZE: usize = HEADER_OFFSET + ckb_abi::header::SERIALIZED_SIZE;
        const _: () = assert!(FRAME_SIZE.is_multiple_of(16));

        let invalid = self.fresh_label("header_dep_full_view_invalid");
        let malformed = self.fresh_label("header_dep_full_malformed");
        let abi = self.runtime_abi();
        self.emit_large_addi("sp", "sp", -(FRAME_SIZE as i64));
        self.emit_decode_source_view_to_t1_t2(&invalid);
        self.emit(format!("li t0, {}", abi.source_header_dep));
        self.emit(format!("bne t2, t0, {invalid}"));
        self.emit(format!(
            "# cellscript abi: LOAD_HEADER exact Molecule Header size={} source=HeaderDep index=SourceView; {} offset={} width={}",
            ckb_abi::header::SERIALIZED_SIZE,
            field_name,
            field_offset,
            ckb_abi::header::SCALAR_WIDTH
        ));
        self.emit(format!("li t0, {}", ckb_abi::header::SERIALIZED_SIZE));
        self.emit_stack_store("t0", SIZE_OFFSET);
        self.emit_sp_addi("a0", HEADER_OFFSET);
        self.emit_sp_addi("a1", SIZE_OFFSET);
        self.emit("li a2, 0");
        self.emit("mv a3, t1");
        self.emit("mv a4, t2");
        self.emit(format!("li a7, {}", abi.load_header));
        self.emit("ecall");
        self.emit_return_on_syscall_error(CellScriptRuntimeError::HeaderDepMissing);
        self.emit_stack_load("t0", SIZE_OFFSET);
        self.emit(format!("li t1, {}", ckb_abi::header::SERIALIZED_SIZE));
        self.emit(format!("bne t0, t1, {malformed}"));
        self.emit_stack_load("a0", HEADER_OFFSET + field_offset);
        self.emit_large_addi("sp", "sp", FRAME_SIZE as i64);
        self.emit("ret");

        self.emit_label(&malformed);
        self.emit_process_failure(CellScriptRuntimeError::ExactSizeMismatch);
        self.emit_label(&invalid);
        self.emit_process_failure(CellScriptRuntimeError::CkbSourceViewInvalid);
    }

    pub(super) fn emit_runtime_input_field_u64(
        &mut self,
        symbol: &str,
        field_name: &str,
        field_id: u64,
        enabled: bool,
        disabled_reason: &str,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        if !enabled {
            self.emit(format!("# cellscript abi: {}", disabled_reason));
            self.emit_process_failure(CellScriptRuntimeError::ConsumeInvalidOperand);
            return;
        }

        let abi = self.runtime_abi();
        self.emit_large_addi("sp", "sp", -32);
        self.emit_stack_store("ra", 24);
        self.emit(format!("# cellscript abi: LOAD_INPUT_BY_FIELD field={} source=GroupInput index=0", field_name));
        self.emit("li t0, 8");
        self.emit_stack_store("t0", 8);
        self.emit_sp_addi("a0", 16);
        self.emit_sp_addi("a1", 8);
        self.emit("li a2, 0");
        self.emit("li a3, 0");
        self.emit(format!("li a4, {}", abi.source_group_input));
        self.emit(format!("li a5, {}", field_id));
        self.emit(format!("li a7, {}", abi.load_input_by_field));
        self.emit("ecall");
        self.emit_return_on_syscall_error(CellScriptRuntimeError::SyscallFailed);
        self.emit_loaded_schema_exact_size_check(8, 8, "input scalar return");
        self.emit_stack_load("a0", 16);
        self.emit_stack_load("ra", 24);
        self.emit_large_addi("sp", "sp", 32);
        self.emit("ret");
    }
}
