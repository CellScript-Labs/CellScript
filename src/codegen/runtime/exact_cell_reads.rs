use super::super::CKB_LENGTH_NOT_ENOUGH;
use super::super::{
    ckb_abi, CellScriptRuntimeError, CodeGenerator, LoadOp, StoreOp, CKB_CELL_FIELD_LOCK, CKB_CELL_FIELD_TYPE, CKB_INPUT_FIELD_SINCE,
    CKB_SOURCE_CELL_DEP, CKB_SOURCE_GROUP_FLAG, CKB_SOURCE_INPUT, CKB_SOURCE_OUTPUT, RUNTIME_EXACT_READ_CACHE_CAPACITY,
    RUNTIME_EXACT_READ_CACHE_ENTRY_SIZE, RUNTIME_EXACT_READ_CACHE_WAYS,
};
use super::RuntimeByteSource;

impl CodeGenerator {
    pub(super) fn emit_runtime_exact_byte_word_helper(
        &mut self,
        symbol: &str,
        source: RuntimeByteSource,
        width: usize,
        enabled: bool,
    ) {
        debug_assert!(matches!(width, 1 | 4 | 8));
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!(
            "# cellscript abi: a0=SourceView, a1=offset, a2=function-local read cache; exactly {width} little-endian bytes"
        ));
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }
        let kind = match source {
            RuntimeByteSource::CellData => 0,
            RuntimeByteSource::Witness => 1,
            RuntimeByteSource::CellLock => 2,
            RuntimeByteSource::CellType => 3,
        };
        // The entry function keeps the most-recent validated source window in
        // callee-saved registers. Small scalar reads can therefore avoid both
        // the four-way metadata scan and its generic width dispatch.
        if self.module_uses_exact_read_hot_cache && width <= 4 {
            let slow = self.fresh_label("exact_read_scalar_slow");
            self.emit(format!("beqz s10, {slow}"));
            self.emit(format!("bne s9, a0, {slow}"));
            self.emit(format!("li t0, {kind}"));
            self.emit(format!("bne s8, t0, {slow}"));
            self.emit(format!("bltu a1, s7, {slow}"));
            self.emit("sub t4, a1, s7");
            self.emit(format!("bltu s6, t4, {slow}"));
            self.emit("sub t1, s6, t4");
            self.emit(format!("li t2, {width}"));
            self.emit(format!("bltu t1, t2, {slow}"));
            self.emit("add t0, s5, t4");
            self.emit_load(LoadOp::ByteUnsigned, "a0", "t0", 0);
            for byte in 1..width {
                self.emit_load(LoadOp::ByteUnsigned, "t1", "t0", byte);
                self.emit(format!("slli t1, t1, {}", byte * 8));
                self.emit("or a0, a0, t1");
            }
            self.emit("li a1, 0");
            self.emit("ret");
            self.emit_label(&slow);
        }
        self.emit(format!("li a3, {width}"));
        self.emit(format!("li a4, {kind}"));
        self.emit("j __cellscript_exact_read_cached");
    }

    pub(super) fn emit_runtime_exact_read_cached(&mut self) {
        const VIEW: usize = 0;
        const START: usize = 8;
        const LEN: usize = 16;
        const VALID: usize = 24;
        const KIND: usize = 32;
        const WIDTH: usize = 40;
        const CACHE_BASE: usize = 48;
        const BUFFER: usize = 56;
        const CAPACITY: usize = RUNTIME_EXACT_READ_CACHE_CAPACITY;
        const LAST: usize = 8;
        let scan = self.fresh_label("exact_read_cache_scan");
        let miss = self.fresh_label("exact_read_cache_miss");
        let source_ready = self.fresh_label("exact_read_source_ready");
        let cell_data_syscall = self.fresh_label("exact_read_cell_data_syscall");
        let witness_syscall = self.fresh_label("exact_read_witness_syscall");
        let lock_syscall = self.fresh_label("exact_read_lock_syscall");
        let type_syscall = self.fresh_label("exact_read_type_syscall");
        let syscall_ready = self.fresh_label("exact_read_syscall_ready");
        let syscall_status_ok = self.fresh_label("exact_read_syscall_status_ok");
        let len_ready = self.fresh_label("exact_read_cache_len_ready");
        let load = self.fresh_label("exact_read_cache_load");
        let load1 = self.fresh_label("exact_read_u8");
        let load4 = self.fresh_label("exact_read_u32");
        let load8 = self.fresh_label("exact_read_u64");
        let success = self.fresh_label("exact_read_success");
        let invalid = self.fresh_label("exact_read_invalid_source");
        let failed = self.fresh_label("exact_read_failed");
        let failed_witness = self.fresh_label("exact_read_failed_witness");
        let abi = self.runtime_abi();

        self.emit_global("__cellscript_exact_read_cached");
        self.emit_label("__cellscript_exact_read_cached");
        self.emit(format!(
            "# cellscript abi: four-way source-bound {CAPACITY}-byte exact-read windows; a0=view,a1=offset,a2=cache,a3=width,a4=kind"
        ));
        if self.module_uses_exact_read_hot_cache {
            self.emit(format!("beqz s10, {scan}"));
            self.emit(format!("bne s9, a0, {scan}"));
            self.emit(format!("bne s8, a4, {scan}"));
            self.emit(format!("bltu a1, s7, {scan}"));
            self.emit("sub t4, a1, s7");
            self.emit(format!("bltu s6, t4, {scan}"));
            self.emit("sub t1, s6, t4");
            self.emit(format!("bltu t1, a3, {scan}"));
            self.emit("mv a6, s10");
            self.emit(format!("j {load}"));
        } else {
            self.emit_load(LoadOp::Double, "a6", "a2", LAST);
            self.emit(format!("beqz a6, {scan}"));
            self.emit_load(LoadOp::Double, "t0", "a6", VALID);
            self.emit(format!("beqz t0, {scan}"));
            self.emit_load(LoadOp::Double, "t0", "a6", VIEW);
            self.emit(format!("bne t0, a0, {scan}"));
            self.emit_load(LoadOp::Double, "t0", "a6", KIND);
            self.emit(format!("bne t0, a4, {scan}"));
            self.emit_load(LoadOp::Double, "t0", "a6", START);
            self.emit(format!("bltu a1, t0, {scan}"));
            self.emit("sub t4, a1, t0");
            self.emit_load(LoadOp::Double, "t1", "a6", LEN);
            self.emit(format!("bltu t1, t4, {scan}"));
            self.emit("sub t1, t1, t4");
            self.emit(format!("bltu t1, a3, {scan}"));
            self.emit(format!("j {load}"));
        }

        self.emit_label(&scan);
        for way in 0..RUNTIME_EXACT_READ_CACHE_WAYS {
            let next = self.fresh_label("exact_read_cache_next_way");
            self.emit_add_immediate("a6", "a2", self.exact_read_cache_header_size() + way * RUNTIME_EXACT_READ_CACHE_ENTRY_SIZE);
            self.emit_load(LoadOp::Double, "t0", "a6", VALID);
            self.emit(format!("beqz t0, {next}"));
            self.emit_load(LoadOp::Double, "t0", "a6", VIEW);
            self.emit(format!("bne t0, a0, {next}"));
            self.emit_load(LoadOp::Double, "t0", "a6", KIND);
            self.emit(format!("bne t0, a4, {next}"));
            self.emit_load(LoadOp::Double, "t0", "a6", START);
            self.emit(format!("bltu a1, t0, {next}"));
            self.emit("sub t4, a1, t0");
            self.emit_load(LoadOp::Double, "t1", "a6", LEN);
            self.emit(format!("bltu t1, t4, {next}"));
            self.emit("sub t1, t1, t4");
            self.emit(format!("bltu t1, a3, {next}"));
            self.emit_store(StoreOp::Double, "a6", "a2", LAST);
            if self.module_uses_exact_read_hot_cache {
                self.emit("mv s10, a6");
                self.emit("mv s9, a0");
                self.emit("mv s8, a4");
                self.emit_load(LoadOp::Double, "s7", "a6", START);
                self.emit_load(LoadOp::Double, "s6", "a6", LEN);
                self.emit_add_immediate("s5", "a6", BUFFER);
            }
            self.emit(format!("j {load}"));
            self.emit_label(&next);
        }
        self.emit(format!("j {miss}"));

        self.emit_label(&miss);
        self.emit_load(LoadOp::Double, "t0", "a2", 0);
        self.emit_add_immediate("t1", "t0", 1);
        self.emit(format!("li t2, {}", RUNTIME_EXACT_READ_CACHE_WAYS - 1));
        self.emit("and t1, t1, t2");
        self.emit_store(StoreOp::Double, "t1", "a2", 0);
        self.emit(format!("li t2, {RUNTIME_EXACT_READ_CACHE_ENTRY_SIZE}"));
        self.emit("mul t1, t0, t2");
        self.emit("add a6, a2, t1");
        self.emit_add_immediate("a6", "a6", self.exact_read_cache_header_size());
        self.emit_store(StoreOp::Double, "zero", "a6", VALID);
        self.emit_store(StoreOp::Double, "a0", "a6", VIEW);
        self.emit_store(StoreOp::Double, "a1", "a6", START);
        self.emit_store(StoreOp::Double, "a4", "a6", KIND);
        self.emit_store(StoreOp::Double, "a3", "a6", WIDTH);
        self.emit_store(StoreOp::Double, "a2", "a6", CACHE_BASE);
        self.emit_decode_source_view_to_t1_t2(&invalid);
        for allowed in
            [CKB_SOURCE_INPUT, CKB_SOURCE_OUTPUT, CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT, CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT]
        {
            self.emit(format!("li t0, {allowed}"));
            self.emit(format!("beq t0, t2, {source_ready}"));
        }
        self.emit(format!("li t0, {CKB_SOURCE_CELL_DEP}"));
        self.emit(format!("bne t0, t2, {invalid}"));
        self.emit_load(LoadOp::Double, "t0", "a6", KIND);
        self.emit("li t3, 1");
        self.emit(format!("beq t0, t3, {invalid}"));

        self.emit_label(&source_ready);
        self.emit(format!("li t0, {CAPACITY}"));
        self.emit_store(StoreOp::Double, "t0", "a6", LEN);
        self.emit_add_immediate("a0", "a6", BUFFER);
        self.emit_add_immediate("a1", "a6", LEN);
        self.emit_load(LoadOp::Double, "a2", "a6", START);
        self.emit("mv a3, t1");
        self.emit("mv a4, t2");
        self.emit_load(LoadOp::Double, "t0", "a6", KIND);
        self.emit(format!("beqz t0, {cell_data_syscall}"));
        self.emit("li t3, 1");
        self.emit(format!("beq t0, t3, {witness_syscall}"));
        self.emit("li t3, 2");
        self.emit(format!("beq t0, t3, {lock_syscall}"));
        self.emit("li t3, 3");
        self.emit(format!("beq t0, t3, {type_syscall}"));
        self.emit(format!("j {invalid}"));
        self.emit_label(&cell_data_syscall);
        self.emit(format!("li a7, {}", abi.load_cell_data));
        self.emit(format!("j {syscall_ready}"));
        self.emit_label(&witness_syscall);
        self.emit(format!("li a7, {}", abi.load_witness));
        self.emit(format!("j {syscall_ready}"));
        self.emit_label(&lock_syscall);
        self.emit(format!("li a5, {CKB_CELL_FIELD_LOCK}"));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit(format!("j {syscall_ready}"));
        self.emit_label(&type_syscall);
        self.emit(format!("li a5, {CKB_CELL_FIELD_TYPE}"));
        self.emit(format!("li a7, {}", abi.load_cell_by_field));
        self.emit_label(&syscall_ready);
        self.emit("ecall");
        self.emit(format!("beqz a0, {syscall_status_ok}"));
        self.emit(format!("li t0, {CKB_LENGTH_NOT_ENOUGH}"));
        self.emit(format!("bne a0, t0, {failed}"));
        self.emit_label(&syscall_status_ok);
        self.emit_load(LoadOp::Double, "t0", "a6", LEN);
        self.emit(format!("li t1, {CAPACITY}"));
        self.emit(format!("bltu t0, t1, {len_ready}"));
        self.emit("mv t0, t1");
        self.emit_label(&len_ready);
        self.emit_store(StoreOp::Double, "t0", "a6", LEN);
        self.emit_load(LoadOp::Double, "a3", "a6", WIDTH);
        self.emit(format!("bltu t0, a3, {failed}"));
        self.emit("li t1, 1");
        self.emit_store(StoreOp::Double, "t1", "a6", VALID);
        self.emit_load(LoadOp::Double, "t2", "a6", CACHE_BASE);
        self.emit_store(StoreOp::Double, "a6", "t2", LAST);
        if self.module_uses_exact_read_hot_cache {
            self.emit("mv s10, a6");
            self.emit_load(LoadOp::Double, "s9", "a6", VIEW);
            self.emit_load(LoadOp::Double, "s8", "a6", KIND);
            self.emit_load(LoadOp::Double, "s7", "a6", START);
            self.emit_load(LoadOp::Double, "s6", "a6", LEN);
            self.emit_add_immediate("s5", "a6", BUFFER);
        }
        self.emit("li t4, 0");

        self.emit_label(&load);
        self.emit_add_immediate("t0", "a6", BUFFER);
        self.emit("add t0, t0, t4");
        self.emit("li t1, 1");
        self.emit(format!("beq a3, t1, {load1}"));
        self.emit("li t1, 4");
        self.emit(format!("beq a3, t1, {load4}"));
        self.emit(format!("j {load8}"));
        self.emit_label(&load1);
        self.emit_load(LoadOp::ByteUnsigned, "a0", "t0", 0);
        self.emit(format!("j {success}"));
        for (label, width) in [(&load4, 4usize), (&load8, 8usize)] {
            self.emit_label(label);
            self.emit("li a0, 0");
            for byte in 0..width {
                self.emit_load(LoadOp::ByteUnsigned, "t1", "t0", byte);
                if byte != 0 {
                    self.emit(format!("slli t1, t1, {}", byte * 8));
                }
                self.emit("or a0, a0, t1");
            }
            self.emit(format!("j {success}"));
        }
        self.emit_label(&success);
        self.emit("li a1, 0");
        self.emit("ret");

        self.emit_label(&invalid);
        self.emit_process_failure(CellScriptRuntimeError::CkbSourceViewInvalid);
        self.emit_label(&failed);
        self.emit_load(LoadOp::Double, "t0", "a6", KIND);
        self.emit(format!("bnez t0, {failed_witness}"));
        self.emit_process_failure(CellScriptRuntimeError::CellLoadFailed);
        self.emit_label(&failed_witness);
        self.emit_process_failure(CellScriptRuntimeError::SyscallFailed);
    }

    /// a0=absolute CellDep index, a1=argc (0..=4), a2..a5=bytes (0..=255).
    /// Each byte becomes a NUL-terminated C string (zero means the empty string).
    /// EXEC replaces this process on success. Every return is an unconditional
    /// failure through the standard a0=status requirement ABI; it must never
    /// allow the parent to continue after a failed or unexpectedly returning EXEC.
    /// Only caller-saved registers are used. The private 64-byte frame holds four
    /// two-byte strings, five argv pointers including a null trailer, and ra.
    pub(super) fn emit_runtime_exec_cell_dep_u8_args(&mut self, enabled: bool) {
        const ARGV: usize = 8;
        const RA: usize = 56;
        const FRAME: usize = 64;
        self.emit_global("__ckb_exec_cell_dep_u8_args");
        self.emit_label("__ckb_exec_cell_dep_u8_args");
        self.emit("# cellscript abi: process replacement; no successful return; a0=error on failure");
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }
        let failed = self.fresh_label("exec_cell_dep_failed");
        self.emit_stack_adjustment(-(FRAME as i64));
        self.emit_stack_store("ra", RA);
        self.emit("li t0, 4");
        self.emit(format!("bltu t0, a1, {failed}"));
        self.emit("li t0, 255");
        for register in ["a2", "a3", "a4", "a5"] {
            self.emit(format!("bltu t0, {register}, {failed}"));
        }
        self.emit_stack_store("zero", 0);
        for (index, register) in ["a2", "a3", "a4", "a5"].iter().enumerate() {
            self.emit_stack_store_byte(register, index * 2);
            self.emit_add_immediate("t0", "sp", index * 2);
            self.emit_stack_store("t0", ARGV + index * 8);
        }
        self.emit_stack_store("zero", ARGV + 4 * 8);
        // argv[argc] is null even when fewer than four arguments are supplied.
        self.emit("slli t0, a1, 3");
        self.emit_add_immediate("t1", "sp", ARGV);
        self.emit("add t0, t0, t1");
        self.emit_store(StoreOp::Double, "zero", "t0", 0);
        self.emit_add_immediate("a4", "a1", 0);
        self.emit_add_immediate("a5", "sp", ARGV);
        self.emit(format!("li a1, {CKB_SOURCE_CELL_DEP}"));
        self.emit("li a2, 0");
        self.emit("li a3, 0");
        self.emit(format!("li a7, {}", crate::ckb_abi::syscall::EXEC));
        self.emit("ecall");
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit_stack_load("ra", RA);
        self.emit_stack_adjustment(FRAME as i64);
        self.emit("ret");
    }

    /// a0=absolute CellDep, a1=proven local Vec<u8> data, a2=actual count,
    /// a3..a6=four segment lengths. Only caller-saved registers are clobbered.
    /// Maximum 256 input bytes become 512 ASCII bytes and four terminators;
    /// the disjoint argv region contains a fifth, null pointer.
    pub(super) fn emit_runtime_cell_dep_hex4(&mut self, enabled: bool, returning: bool) {
        const ARGV: usize = 520;
        const RA: usize = 560;
        const DEP: usize = 568;
        const SPAWN_ARGS: usize = 576;
        const PID: usize = 608;
        const EXIT: usize = 616;
        let frame: i64 = if returning { 624 } else { 576 };
        let symbol = if returning { "__ckb_spawn_wait_cell_dep_hex4" } else { "__ckb_exec_cell_dep_hex4" };
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(if returning {
            "# cellscript abi: four hex argv; SPAWN+WAIT returns only after successful child exit"
        } else {
            "# cellscript abi: four hex argv; non-returning EXEC; any return is failure"
        });
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }
        let failed = self.fresh_label("exec_hex4_failed");
        self.emit_large_addi("sp", "sp", -frame);
        self.emit_stack_store("ra", RA);
        self.emit_stack_store("a0", DEP);
        self.emit("li t0, 256");
        for register in ["a2", "a3", "a4", "a5", "a6"] {
            self.emit(format!("bltu t0, {register}, {failed}"));
        }
        self.emit("add t0, a3, a4");
        self.emit("add t0, t0, a5");
        self.emit("add t0, t0, a6");
        self.emit(format!("bne t0, a2, {failed}"));
        self.emit_add_immediate("t1", "a1", 0); // input cursor
        self.emit_add_immediate("t2", "sp", 0); // encoded output cursor
        for (index, register) in ["a3", "a4", "a5", "a6"].iter().enumerate() {
            let scan = self.fresh_label("exec_hex4_encode");
            let done = self.fresh_label("exec_hex4_encoded");
            self.emit_stack_store("t2", ARGV + index * 8);
            self.emit_add_immediate("t3", register, 0);
            self.emit_label(&scan);
            self.emit(format!("beqz t3, {done}"));
            self.emit_load(LoadOp::ByteUnsigned, "t4", "t1", 0);
            for (shift, offset) in [(4, 0), (0, 1)] {
                let digit = self.fresh_label("exec_hex4_digit");
                let ready = self.fresh_label("exec_hex4_char");
                self.emit(format!("srli t5, t4, {shift}"));
                self.emit("li t6, 15");
                self.emit("and t5, t5, t6");
                self.emit("li t6, 10");
                self.emit(format!("bltu t5, t6, {digit}"));
                self.emit_add_immediate("t5", "t5", 87);
                self.emit(format!("j {ready}"));
                self.emit_label(&digit);
                self.emit_add_immediate("t5", "t5", 48);
                self.emit_label(&ready);
                self.emit_store(StoreOp::Byte, "t5", "t2", offset);
            }
            self.emit_add_immediate("t1", "t1", 1);
            self.emit_add_immediate("t2", "t2", 2);
            self.emit_add_immediate("t3", "t3", -1);
            self.emit(format!("j {scan}"));
            self.emit_label(&done);
            self.emit_store(StoreOp::Byte, "zero", "t2", 0);
            self.emit_add_immediate("t2", "t2", 1);
        }
        self.emit_stack_store("zero", ARGV + 32);
        self.emit_stack_load("a0", DEP);
        self.emit(format!("li a1, {CKB_SOURCE_CELL_DEP}"));
        self.emit("li a2, 0");
        self.emit("li a3, 0");
        let done = returning.then(|| self.fresh_label("spawn_hex4_done"));
        if returning {
            // SpawnArgs is four u64 fields: argc, argv, process_id pointer,
            // inherited_fds pointer. No FDs are inherited. PID and exit byte
            // have disjoint storage and survive both scheduler transitions.
            self.emit("li t0, 4");
            self.emit_stack_store("t0", SPAWN_ARGS);
            self.emit_sp_addi("t0", ARGV);
            self.emit_stack_store("t0", SPAWN_ARGS + 8);
            self.emit_sp_addi("t0", PID);
            self.emit_stack_store("t0", SPAWN_ARGS + 16);
            self.emit_stack_store("zero", SPAWN_ARGS + 24);
            self.emit_stack_store("zero", PID);
            self.emit_stack_store("zero", EXIT);
            self.emit_sp_addi("a4", SPAWN_ARGS);
            self.emit("li a5, 0");
            self.emit(format!("li a7, {}", ckb_abi::syscall::SPAWN));
            self.emit("ecall");
            self.emit(format!("bnez a0, {failed}"));
            self.emit_stack_load("a0", PID);
            self.emit_sp_addi("a1", EXIT);
            self.emit(format!("li a7, {}", ckb_abi::syscall::WAIT));
            self.emit("ecall");
            self.emit(format!("bnez a0, {failed}"));
            self.emit_stack_load_byte("t0", EXIT);
            self.emit(format!("bnez t0, {failed}"));
            self.emit("li a0, 0");
            self.emit(format!("j {}", done.as_ref().expect("returning label")));
        } else {
            self.emit("li a4, 4");
            self.emit_sp_addi("a5", ARGV);
            self.emit(format!("li a7, {}", ckb_abi::syscall::EXEC));
            self.emit("ecall");
        }
        self.emit_label(&failed);
        self.emit(format!("li a0, {}", CellScriptRuntimeError::SyscallFailed.code()));
        if let Some(done) = done {
            self.emit_label(&done);
        }
        self.emit_stack_load("ra", RA);
        self.emit_large_addi("sp", "sp", frame);
        self.emit("ret");
    }

    /// a0=SourceView; byte reads additionally take a1=offset. a0=value,
    /// a1=status. Only caller-saved registers are touched; the private frame
    /// preserves offset across SourceView decoding (which clobbers t0..t6).
    /// An absent Type Script is a read failure, not an empty Script.
    pub(super) fn emit_runtime_cell_script_read(&mut self, symbol: &str, field: u64, byte: bool, enabled: bool) {
        debug_assert!(matches!(field, CKB_CELL_FIELD_LOCK | CKB_CELL_FIELD_TYPE));
        const OFFSET: usize = 0;
        const SIZE: usize = 8;
        const BUFFER: usize = 16;
        const RA: usize = 24;
        const FRAME: usize = 32;
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit("# cellscript abi: complete serialized Script size or exact byte; a0=value, a1=error");
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }
        let invalid = self.fresh_label("script_read_invalid_source");
        let ready = self.fresh_label("script_read_source_ready");
        let failed = self.fresh_label("script_read_failed");
        let done = self.fresh_label("script_read_done");
        self.emit_stack_adjustment(-(FRAME as i64));
        self.emit_stack_store("ra", RA);
        if byte {
            self.emit_stack_store("a1", OFFSET);
        }
        self.emit_decode_source_view_to_t1_t2(&invalid);
        for allowed in [
            CKB_SOURCE_INPUT,
            CKB_SOURCE_OUTPUT,
            CKB_SOURCE_CELL_DEP,
            CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT,
            CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT,
        ] {
            self.emit(format!("li t0, {allowed}"));
            self.emit(format!("beq t0, t2, {ready}"));
        }
        self.emit(format!("j {invalid}"));
        self.emit_label(&ready);
        self.emit(format!("li t0, {}", u8::from(byte)));
        self.emit_stack_store("t0", SIZE);
        self.emit_sp_addi("a0", BUFFER);
        self.emit_sp_addi("a1", SIZE);
        if byte {
            self.emit_stack_load("a2", OFFSET);
        } else {
            self.emit("li a2, 0");
        }
        self.emit("mv a3, t1");
        self.emit("mv a4, t2");
        self.emit(format!("li a5, {field}"));
        self.emit(format!("li a7, {}", self.runtime_abi().load_cell_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {failed}"));
        self.emit_stack_load("a0", SIZE);
        if byte {
            // The syscall reports full remaining length, not bytes copied.
            // Offset at/beyond EOF clamps to an empty read and must fail.
            self.emit(format!("beqz a0, {failed}"));
            self.emit_stack_load_byte("a0", BUFFER);
        }
        self.emit("li a1, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&invalid);
        self.emit_process_failure(CellScriptRuntimeError::CkbSourceViewInvalid);
        self.emit_label(&failed);
        self.emit_process_failure(CellScriptRuntimeError::CellLoadFailed);
        self.emit_label(&done);
        self.emit_stack_load("ra", RA);
        self.emit_stack_adjustment(FRAME as i64);
        self.emit("ret");
    }

    /// a0=Input/GroupInput SourceView; a0=raw u64 since, a1=status.
    /// This does not interpret or authorize a timelock. The no-argument
    /// __ckb_input_since helper retains its old GroupInput-0 contract.
    pub(super) fn emit_runtime_input_since_at(&mut self, enabled: bool) {
        const SIZE: usize = 0;
        const VALUE: usize = 8;
        const RA: usize = 24;
        const FRAME: usize = 32;
        self.emit_global("__ckb_input_since_at");
        self.emit_label("__ckb_input_since_at");
        if !enabled {
            self.emit_fail(CellScriptRuntimeError::SyscallFailed);
            return;
        }
        let invalid = self.fresh_label("since_at_invalid_source");
        let failed = self.fresh_label("since_at_load_failed");
        let done = self.fresh_label("since_at_done");
        self.emit_stack_adjustment(-(FRAME as i64));
        self.emit_stack_store("ra", RA);
        self.emit_decode_input_source_view_to_t1_t2(&invalid);
        self.emit("li t0, 8");
        self.emit_stack_store("t0", SIZE);
        self.emit_sp_addi("a0", VALUE);
        self.emit_sp_addi("a1", SIZE);
        self.emit("li a2, 0");
        self.emit("mv a3, t1");
        self.emit("mv a4, t2");
        self.emit(format!("li a5, {}", CKB_INPUT_FIELD_SINCE));
        self.emit(format!("li a7, {}", self.runtime_abi().load_input_by_field));
        self.emit("ecall");
        self.emit(format!("bnez a0, {failed}"));
        self.emit_stack_load("t0", SIZE);
        self.emit("li t1, 8");
        self.emit(format!("bne t0, t1, {failed}"));
        self.emit_stack_load("a0", VALUE);
        self.emit("li a1, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&invalid);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSourceViewInvalid.code()));
        self.emit(format!("j {done}"));
        self.emit_label(&failed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
        self.emit_label(&done);
        self.emit_stack_load("ra", RA);
        self.emit_stack_adjustment(FRAME as i64);
        self.emit("ret");
    }
}
