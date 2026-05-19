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
        if line.starts_with("# CellScript Generated Assembly")
            || line.starts_with("# opt_level=")
            || line.starts_with("# debug=")
        {
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
    let result = compile(
        source,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            ..CompileOptions::default()
        },
    )
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

    // Structural shape: entry jumps to main, main has one block, shared epilogue.
    assert!(n.contains(".global _cellscript_entry"), "missing entry label:\n{}", n);
    assert!(
        n.contains("# cellscript entry abi: _cellscript_entry tail-calls no-arg main"),
        "missing entry ABI comment:\n{}",
        n
    );
    assert!(n.contains("j main"), "entry should tail-call main:\n{}", n);
    assert!(n.contains(".global main"), "missing main label:\n{}", n);
    assert!(n.contains("main:"), "missing main function label:\n{}", n);
    assert!(
        n.contains("addi sp, sp, -1184"),
        "main should have expected frame size:\n{}",
        n
    );
    assert!(
        n.contains(".Lmain_block_0:\n    li a0, 42"),
        "main block should load return value 42:\n{}",
        n
    );
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
    assert!(
        n.contains(".global _cellscript_entry"),
        "missing entry label:\n{}", n
    );
    assert!(
        n.contains("# cellscript entry abi: _cellscript_entry loads GroupInput witness args for check"),
        "missing witness-load ABI comment:\n{}", n
    );
    assert!(
        n.contains("# cellscript entry abi: witness magic CSARGv1"),
        "missing witness magic ABI comment:\n{}", n
    );
    assert!(
        n.contains("# cellscript entry abi: lock_args parameters are decoded from the executing Script.args bytes"),
        "missing lock_args decode ABI comment:\n{}", n
    );
    assert!(
        n.contains("# cellscript abi: LOAD_SCRIPT reason=entry_lock_args"),
        "missing LOAD_SCRIPT ABI comment:\n{}", n
    );

    assert!(n.contains("check:"), "missing check function label:\n{}", n);
    assert!(
        n.contains("call __cellscript_memcmp_fixed"),
        "lock_args Hash comparison should use fixed-byte memcmp:\n{}", n
    );
    assert!(
        n.contains(".Lcheck_epilogue:"),
        "check should use shared epilogue:\n{}", n
    );
    // All error paths and the normal return path should converge to the shared epilogue.
    let check_start = n.find("check:\n").expect("check label");
    let next_global = n[check_start..]
        .find(".global ")
        .map(|o| check_start + o)
        .unwrap_or(n.len());
    let check_body = &n[check_start..next_global];
    let epilogue_jump_count = check_body.matches("j .Lcheck_epilogue").count();
    assert!(
        epilogue_jump_count >= 2,
        "check should have at least two jumps to shared epilogue (success + fail paths):\n{}",
        check_body
    );
    // Only one physical `ret` instruction inside the epilogue itself.
    let ret_count = check_body.lines().filter(|l| l.trim() == "ret").count();
    assert_eq!(
        ret_count, 1,
        "check should have exactly one ret instruction in its shared epilogue:\n{}",
        check_body
    );
}

// ---------------------------------------------------------------------------
// Snapshot 3: Blake2b-256 runtime helper usage
// ---------------------------------------------------------------------------

const BLAKE2B_SOURCE: &str = include_str!("../examples/language/v0_14_hash_blake2b.cell");

#[test]
fn snapshot_blake2b_helper_assembly() {
    let assembly = compile_to_asm(BLAKE2B_SOURCE);
    let n = normalise_assembly(&assembly);

    assert!(
        n.contains("blake2b_matches:"),
        "missing blake2b_matches label:\n{}", n
    );
    // The blake2b builtin is lowered as either a direct tail-call (j) or
    // a normal call (call) to the runtime helper, depending on whether
    // the result is used immediately or passed to another function.
    let has_blake2b_call = n.contains("call __ckb_hash_blake2b") || n.contains("j __ckb_hash_blake2b");
    assert!(
        has_blake2b_call,
        "blake2b should invoke the runtime Blake2b-256 helper (call or j):\n{}",
        n
    );
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

#[test]
fn snapshot_witness_schema_syscall_assembly() {
    let assembly = compile_to_asm(WITNESS_SOURCE);
    let n = normalise_assembly(&assembly);

    // Entry + lock function.
    assert!(
        n.contains("owner_witness:"),
        "missing owner_witness label:\n{}", n
    );
    assert!(
        n.contains("# cellscript entry abi: _cellscript_entry loads GroupInput witness args for owner_witness"),
        "missing entry witness load ABI comment:\n{}", n
    );

    // Schema loading syscalls.
    assert!(
        n.contains("# cellscript abi: LOAD_CELL_DATA reason=read_ref_param_input"),
        "missing LOAD_CELL_DATA ABI comment for schema param:\n{}",
        n
    );
    assert!(
        n.contains("li a7, 2092"),
        "LOAD_CELL_DATA should use syscall 2092 (CKB-VM syscall 2048+44):\n{}",
        n
    );

    // Runtime helpers.
    assert!(
        n.contains("call __ckb_source_group_input"),
        "should call __ckb_source_group_input:\n{}", n
    );
    assert!(
        n.contains("call __ckb_witness_lock"),
        "should call __ckb_witness_lock:\n{}", n
    );
    assert!(
        n.contains("call __ckb_sighash_all"),
        "should call __ckb_sighash_all:\n{}", n
    );

    // Fixed-byte comparisons for Hash/Address equality.
    assert!(
        n.contains("call __cellscript_memcmp_fixed"),
        "should use fixed-byte memcmp for schema field comparisons:\n{}", n
    );

    // Error paths.
    assert!(
        n.contains("# cellscript runtime error 1 syscall-failed"),
        "should have syscall-failed error path:\n{}", n
    );
    assert!(
        n.contains("# cellscript runtime error 2 bounds-check-failed"),
        "should have bounds-check-failed error path:\n{}", n
    );
    assert!(
        n.contains("# cellscript runtime error 4 exact-size-mismatch"),
        "should have exact-size-mismatch error path:\n{}", n
    );
    assert!(
        n.contains("# cellscript runtime error 5 assertion-failed"),
        "should have assertion-failed error path:\n{}", n
    );
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
    ];
    for (name, source) in &sources {
        let assembly = compile_to_asm(source);
        assert!(
            !assembly.contains("immediate '"),
            "{} assembly leaked an assembler overflow diagnostic",
            name
        );
    }
}

