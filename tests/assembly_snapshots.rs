//! Assembly snapshot/diff-style guard for generated RISC-V output.
//!
//! These tests are **behaviour-preserving stability guards**, not exhaustive
//! correctness proofs. They catch accidental changes to codegen shape (label
//! numbering, instruction selection, frame layout, ABI comment format) during
//! backend refactors.
//!
//! If a snapshot legitimately changes:
//!   1. Verify the new assembly is semantically equivalent.
//!   2. Update the expected string and explain the change in the commit message.
//!   3. Do not disable the test or widen assertions beyond recognition.

use cellscript::{compile, CompileOptions};

/// Normalise assembly text for comparison.
///
/// Only unstable/non-semantic text is rewritten:
///   - Header comment lines (contains opt_level/debug flags).
///   - Auto-generated numeric suffixes on local labels so that adding a new
///     codegen path earlier in the pipeline does not shift every label ID.
///   - `.rodata` label names that include derived numeric identifiers.
///
/// Semantic content (mnemonics, registers, immediate values, frame sizes,
/// syscall numbers, ABI comments) is kept exactly.
fn normalise_assembly(assembly: &str) -> String {
    let mut out = Vec::new();
    for line in assembly.lines() {
        // Strip the header comment lines; they mention opt_level/debug.
        if line.starts_with("# CellScript Generated Assembly") || line.starts_with("# opt_level=") || line.starts_with("# debug=") {
            continue;
        }
        // Normalise auto-generated numeric label suffixes.
        // Pattern: .L<word>_<digits>  or  .L<word>_<digits>:
        // We map the first occurrence of each base to _0, second to _1, etc.
        // But simpler: since the assembly is deterministic for a fixed source,
        // we don't actually need full remapping — exact match works today.
        // We keep this hook for future instability.
        out.push(line.to_string());
    }
    out.join("\n")
}

fn compile_to_asm(source: &str) -> String {
    let result = compile(source, CompileOptions { target: Some("riscv64-asm".to_string()), ..CompileOptions::default() })
        .unwrap_or_else(|e| panic!("compilation failed: {}", e.message));
    String::from_utf8(result.artifact_bytes).expect("assembly should be valid UTF-8")
}

// ---------------------------------------------------------------------------
// Snapshot 1: simplest possible action (scalar return, no params, no syscalls)
// ---------------------------------------------------------------------------

const SIMPLE_ACTION_SOURCE: &str = r"module __snap_action

action main() -> u64
where {
    42
}
";

#[test]
fn snapshot_simple_action_assembly() {
    let assembly = compile_to_asm(SIMPLE_ACTION_SOURCE);
    let n = normalise_assembly(&assembly);

    // Structural shape: entry calls main through the direct wrapper, main has one block, shared epilogue.
    assert!(n.contains(".global _cellscript_entry"), "missing entry label:\n{}", n);
    assert!(n.contains("# cellscript entry abi: _cellscript_entry calls no-arg main"), "missing entry ABI comment:\n{}", n);
    assert!(n.contains("mv s10, sp"), "entry should stage direct-wrapper stack base:\n{}", n);
    assert!(n.contains("la s11, .Lentry_direct_done_"), "entry should stage direct-wrapper return label:\n{}", n);
    assert!(n.contains("call main"), "entry should call main through the direct wrapper:\n{}", n);
    assert!(n.contains("ld ra, 8(sp)\n    addi sp, sp, 16\n    ret"), "entry should restore wrapper frame and return:\n{}", n);
    assert!(n.contains(".global main"), "missing main label:\n{}", n);
    assert!(n.contains("main:"), "missing main function label:\n{}", n);
    assert!(n.contains("addi sp, sp, -1184"), "main should have expected frame size:\n{}", n);
    assert!(n.contains(".Lmain_block_0:\n    li a0, 42"), "main block should load return value 42:\n{}", n);
    assert!(n.contains("j .Lmain_epilogue"), "main should jump to shared epilogue:\n{}", n);
    assert!(n.contains(".Lmain_epilogue:"), "main should have shared epilogue:\n{}", n);
    assert!(
        n.contains("ld ra, 1176(sp)\n    ld fp, 1168(sp)\n    addi sp, sp, 1184\n    ret"),
        "main epilogue should restore callee-saved registers and return:\n{}",
        n
    );
}

// ---------------------------------------------------------------------------
// Snapshot 2: lock with lock_args and witness (fixed-byte comparison,
// entry witness wrapper logic, shared epilogue)
// ---------------------------------------------------------------------------

const LOCK_ARGS_SOURCE: &str = r"module __snap_lock

lock check(hash: lock_args Hash, witness claimed: Hash) -> bool {
    hash == claimed
}
";

#[test]
fn snapshot_lock_args_assembly() {
    let assembly = compile_to_asm(LOCK_ARGS_SOURCE);
    let n = normalise_assembly(&assembly);

    // Entry point contains witness-wrapper logic.
    assert!(n.contains(".global _cellscript_entry"), "missing entry label:\n{}", n);
    assert!(
        n.contains("# cellscript entry abi: _cellscript_entry loads GroupInput witness args for check"),
        "missing witness-load ABI comment:\n{}",
        n
    );
    assert!(!n.contains("entry_args_group_output"), "lock entry wrapper must not fall back to GroupOutput witness args:\n{}", n);
    assert!(n.contains("# cellscript entry abi: witness magic CSARGv1"), "missing witness magic ABI comment:\n{}", n);
    assert!(
        n.contains("# cellscript entry abi: lock_args parameters are decoded from the executing Script.args bytes"),
        "missing lock_args decode ABI comment:\n{}",
        n
    );
    assert!(n.contains("# cellscript abi: LOAD_SCRIPT reason=entry_lock_args"), "missing LOAD_SCRIPT ABI comment:\n{}", n);

    assert!(n.contains("check:"), "missing check function label:\n{}", n);
    assert!(n.contains("call __cellscript_memcmp_fixed"), "lock_args Hash comparison should use fixed-byte memcmp:\n{}", n);
    assert!(n.contains(".Lcheck_epilogue:"), "check should use shared epilogue:\n{}", n);
    // All error paths and the normal return path should converge to the shared epilogue.
    let check_start = n.find("check:\n").expect("check label");
    let next_global = n[check_start..].find(".global ").map(|o| check_start + o).unwrap_or(n.len());
    let check_body = &n[check_start..next_global];
    let epilogue_jump_count = check_body.matches("j .Lcheck_epilogue").count();
    assert!(
        epilogue_jump_count >= 2,
        "check should have at least two jumps to shared epilogue (success + fail paths):\n{}",
        check_body
    );
    // Only one physical `ret` instruction inside the epilogue itself.
    let ret_count = check_body.lines().filter(|l| l.trim() == "ret").count();
    assert_eq!(ret_count, 1, "check should have exactly one ret instruction in its shared epilogue:\n{}", check_body);
}

// ---------------------------------------------------------------------------
// Snapshot 3: Blake2b-256 runtime helper usage
// ---------------------------------------------------------------------------

const BLAKE2B_SOURCE: &str = include_str!("../examples/language/v0_14_hash_blake2b.cell");

#[test]
fn snapshot_blake2b_helper_assembly() {
    let assembly = compile_to_asm(BLAKE2B_SOURCE);
    let n = normalise_assembly(&assembly);

    assert!(n.contains("blake2b_matches:"), "missing blake2b_matches label:\n{}", n);
    // The blake2b builtin is lowered as either a direct tail-call (j) or
    // a normal call (call) to the runtime helper, depending on whether
    // the result is used immediately or passed to another function.
    let has_blake2b_call = n.contains("call __ckb_hash_blake2b") || n.contains("j __ckb_hash_blake2b");
    assert!(has_blake2b_call, "blake2b should invoke the runtime Blake2b-256 helper (call or j):\n{}", n);
    assert!(
        n.contains("# cellscript abi: fixed-byte Eq comparison size=32"),
        "blake2b result should be compared with fixed-byte memcmp:\n{}",
        n
    );
}

// ---------------------------------------------------------------------------
// Snapshot 4: witness source with schema loading + CKB syscalls
// ---------------------------------------------------------------------------

const WITNESS_SOURCE: &str = include_str!("../examples/language/v0_14_witness_source.cell");
const COLLECTION_SOURCE: &str = include_str!("../examples/language/registry.cell");
const SPAWN_IPC_SOURCE: &str = include_str!("../examples/language/v0_14_multi_step_pipeline.cell");
const TYPE_ID_SOURCE: &str = include_str!("../examples/language/v0_14_ckb_type_id_create.cell");

#[test]
fn snapshot_witness_schema_syscall_assembly() {
    let assembly = compile_to_asm(WITNESS_SOURCE);
    let n = normalise_assembly(&assembly);

    // Entry + lock function.
    assert!(n.contains("owner_witness:"), "missing owner_witness label:\n{}", n);
    assert!(
        n.contains("# cellscript entry abi: _cellscript_entry loads GroupInput witness args for owner_witness"),
        "missing entry witness load ABI comment:\n{}",
        n
    );

    // Schema loading syscalls.
    assert!(
        n.contains("# cellscript abi: LOAD_CELL_DATA reason=read_ref_param_input"),
        "missing LOAD_CELL_DATA ABI comment for schema param:\n{}",
        n
    );
    assert!(n.contains("li a7, 2092"), "LOAD_CELL_DATA should use syscall 2092 (CKB-VM syscall 2048+44):\n{}", n);

    // Runtime helpers.
    assert!(n.contains("call __ckb_source_group_input"), "should call __ckb_source_group_input:\n{}", n);
    assert!(n.contains("call __ckb_witness_lock"), "should call __ckb_witness_lock:\n{}", n);
    assert!(n.contains("call __ckb_sighash_all"), "should call __ckb_sighash_all:\n{}", n);

    // Fixed-byte comparisons for Hash/Address equality.
    assert!(n.contains("call __cellscript_memcmp_fixed"), "should use fixed-byte memcmp for schema field comparisons:\n{}", n);

    // Error paths.
    assert!(n.contains("# cellscript runtime error 1 syscall-failed"), "should have syscall-failed error path:\n{}", n);
    assert!(n.contains("# cellscript runtime error 2 bounds-check-failed"), "should have bounds-check-failed error path:\n{}", n);
    assert!(n.contains("# cellscript runtime error 4 exact-size-mismatch"), "should have exact-size-mismatch error path:\n{}", n);
    assert!(n.contains("# cellscript runtime error 5 assertion-failed"), "should have assertion-failed error path:\n{}", n);
}

#[test]
fn snapshot_collection_lowering_assembly() {
    let assembly = compile_to_asm(COLLECTION_SOURCE);
    let n = normalise_assembly(&assembly);

    for marker in [
        "# cellscript abi: stack collection push element_size=32",
        "# cellscript abi: stack collection insert element_size=32",
        "# cellscript abi: stack collection remove element_size=32",
        "# cellscript abi: stack collection contains element_size=32",
        "# cellscript abi: stack collection swap element_size=32",
        "# cellscript abi: stack collection truncate",
    ] {
        assert!(n.contains(marker), "registry collection assembly should contain marker {marker}:\n{}", n);
    }
    assert!(
        n.contains("sltu t5, t3, t2\n    beqz t5, .Lstack_collection_push_used_bytes_ok_"),
        "collection push should check used_bytes <= capacity before subtracting remaining capacity:\n{}",
        n
    );
}

#[test]
fn snapshot_spawn_ipc_executable_status_checked_assembly() {
    let assembly = compile_to_asm(SPAWN_IPC_SOURCE);
    let n = normalise_assembly(&assembly);

    for (helper, status_reg) in [
        ("__ckb_pipe", "a2"),
        ("__ckb_pipe_write", "a1"),
        ("__ckb_spawn", "a1"),
        ("__ckb_wait", "a1"),
        ("__ckb_pipe_read", "a1"),
        ("__ckb_close", "a1"),
    ] {
        assert!(n.contains(&format!("call {helper}")), "pipeline action should call {helper}:\n{}", n);
        let after_call = n.split_once(&format!("\n    call {helper}")).map(|(_, after)| after).expect("helper call should be present");
        let status_check = after_call
            .find(&format!("beqz {status_reg}"))
            .unwrap_or_else(|| panic!("{helper} status should be checked in {status_reg}"));
        let epilogue_jump = after_call.find("j .Lpipe_to_delegate_epilogue").unwrap_or_else(|| panic!("{helper} failure should exit"));
        assert!(status_check < epilogue_jump, "{helper} status must be checked before continuing or returning:\n{}", after_call);
    }

    let spawn_helper = n.split_once("__ckb_spawn:").map(|(_, after)| after).expect("spawn helper body should be present");
    let spawn_helper = spawn_helper.split_once(".global ").map(|(body, _)| body).unwrap_or(spawn_helper);
    assert!(
        spawn_helper.contains("li a7, 2601")
            && spawn_helper.contains("ecall")
            && spawn_helper.contains("spawn resolves the static target to CellDep#0 with no argv and no inherited fds")
            && !spawn_helper.contains("withheld raw syscall 2601"),
        "spawn helper should issue the VM2 spawn syscall through the conservative static CellDep#0 wrapper:\n{}",
        spawn_helper
    );

    let pipe_helper = n.split_once("__ckb_pipe:").map(|(_, after)| after).expect("pipe helper body should be present");
    let pipe_helper = pipe_helper.split_once(".global ").map(|(body, _)| body).unwrap_or(pipe_helper);
    assert!(
        pipe_helper.contains("li a7, 2604") && pipe_helper.contains("ecall") && pipe_helper.contains("mv a2, a0"),
        "pipe helper should preserve read/write fds in a0/a1 and move raw status to a2:\n{}",
        pipe_helper
    );

    for (helper, syscall) in [("__ckb_pipe_write", 2605), ("__ckb_pipe_read", 2606), ("__ckb_wait", 2602), ("__ckb_close", 2608)] {
        let helper_body = n.split_once(&format!("{helper}:")).map(|(_, after)| after).expect("helper body should be present");
        let helper_body = helper_body.split_once(".global ").map(|(body, _)| body).unwrap_or(helper_body);
        assert!(
            helper_body.contains(&format!("li a7, {syscall}")) && helper_body.contains("ecall"),
            "{helper} should issue VM2 syscall {syscall}:\n{}",
            helper_body
        );
    }
}

#[test]
fn snapshot_type_id_create_output_assembly() {
    let assembly = compile_to_asm(TYPE_ID_SOURCE);
    let n = normalise_assembly(&assembly);

    assert!(n.contains("__type_desc_IdentityToken:"), "TYPE_ID assembly should materialize the type descriptor:\n{}", n);
    assert!(
        n.contains("# cellscript entry abi: action entries fall back to GroupOutput witness args for output-only type scripts"),
        "TYPE_ID action entry should keep the output-only witness fallback visible:\n{}",
        n
    );
    assert!(
        n.contains("# cellscript abi: LOAD_WITNESS reason=entry_args_group_output source=GroupOutput index=0"),
        "TYPE_ID action entry should try GroupOutput witness args after GroupInput absence:\n{}",
        n
    );
    assert!(
        n.contains("# cellscript abi: LOAD_CELL_DATA reason=output_param source=Output index=0"),
        "TYPE_ID create output verifier should load output cell data:\n{}",
        n
    );
    assert!(
        n.contains("# cellscript abi: output field verification deferred to ordered create constraint"),
        "TYPE_ID create output assembly should retain ordered create verification marker:\n{}",
        n
    );
    assert!(n.contains("li a7, 2092"), "TYPE_ID output data load should use LOAD_CELL_DATA syscall 2092:\n{}", n);
}

#[test]
fn runtime_u64_helpers_fail_closed_before_value_use() {
    let assembly = compile_to_asm(
        r"module __snap_runtime_u64

lock check() -> bool {
    ckb::header_epoch_number() > 0
}
",
    );
    let n = normalise_assembly(&assembly);
    let after_call =
        n.split_once("\n    call __ckb_header_epoch_number").map(|(_, after)| after).expect("header helper call should be present");
    let status_check = after_call.find("beqz a1").expect("header helper status should be checked");
    let value_store = after_call.find("sd a0").expect("header helper value should be stored");
    assert!(status_check < value_store, "header helper status must be checked before a0 is used as a value:\n{}", after_call);
    assert!(
        after_call.contains("mv a0, a1\n    j .Lcheck_epilogue"),
        "runtime helper failure should return the helper status through the function epilogue:\n{}",
        after_call
    );

    let helper = n.split_once("__ckb_header_epoch_number:").map(|(_, after)| after).expect("header helper body should be present");
    assert!(helper.contains("li a1, 0"), "header helper success must clear status in a1:\n{}", helper);
    assert!(
        helper.contains("# cellscript runtime error 1 syscall-failed\n    li a0, 0\n    li a1, 1"),
        "header helper failure must not forge a0=1 as a value:\n{}",
        helper
    );
}

#[test]
fn runtime_void_helpers_fail_closed_before_continuing() {
    let assembly = compile_to_asm(
        r"module __snap_runtime_void

lock check() -> bool {
    require_maturity(100)
    true
}
",
    );
    let n = normalise_assembly(&assembly);
    let after_call = n
        .split_once("\n    call __ckb_require_maturity")
        .map(|(_, after)| after)
        .expect("require_maturity helper call should be present");
    let status_check = after_call.find("beqz a1").expect("void helper status should be checked");
    let success_value = after_call.find("li a0, 1").expect("lock success value should be present");
    assert!(status_check < success_value, "void runtime helper status must be checked before execution continues:\n{}", after_call);
    assert!(
        after_call.contains("mv a0, a1\n    j .Lcheck_epilogue"),
        "runtime helper failure should return the helper status through the function epilogue:\n{}",
        after_call
    );
    assert!(
        after_call.contains("__ckb_require_maturity:\n    # cellscript abi: v0.14 CKB semantic helper")
            && after_call.contains("# cellscript runtime error 1 syscall-failed\n    li a0, 0\n    li a1, 1"),
        "fail-closed helper should return status in a1 without forging success data:\n{}",
        after_call
    );
}

#[test]
fn runtime_witness_helpers_fail_closed_before_pointer_use() {
    let assembly = compile_to_asm(WITNESS_SOURCE);
    let n = normalise_assembly(&assembly);
    for helper in ["__ckb_witness_lock", "__ckb_sighash_all"] {
        let after_call = n
            .split_once(&format!("\n    call {}", helper))
            .map(|(_, after)| after)
            .unwrap_or_else(|| panic!("{helper} call should be present"));
        let status_check = after_call.find("beqz a1").unwrap_or_else(|| panic!("{helper} status should be checked"));
        let pointer_store = after_call.find("sd a0").unwrap_or_else(|| panic!("{helper} pointer/result should be stored"));
        assert!(
            status_check < pointer_store,
            "{helper} status must be checked before a0 is stored or compared as data:\n{}",
            after_call
        );
    }
}

// ---------------------------------------------------------------------------
// Cross-cutting assertion: no leaked assembler overflow diagnostics
// ---------------------------------------------------------------------------

#[test]
fn snapshot_assemblies_contain_no_leaked_overflow_diagnostics() {
    let sources = [
        ("simple_action", SIMPLE_ACTION_SOURCE),
        ("lock_args", LOCK_ARGS_SOURCE),
        ("blake2b", BLAKE2B_SOURCE),
        ("witness", WITNESS_SOURCE),
        ("collections", COLLECTION_SOURCE),
        ("spawn_ipc", SPAWN_IPC_SOURCE),
        ("type_id", TYPE_ID_SOURCE),
    ];
    for (name, source) in &sources {
        let assembly = compile_to_asm(source);
        assert!(!assembly.contains("immediate '"), "{} assembly leaked an assembler overflow diagnostic", name);
    }
}
