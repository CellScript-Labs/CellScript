//! Canonical transaction sighash construction is not an executable primitive.
//! AllowFailClosed is useful for audits, but must never supply a synthetic hash.

use cellscript::{
    compile_with_executable_surface_policy, strip_vm_abi_trailer, CellScriptEdition, CompileOptions, CompileResult, EntryWitnessArg,
    ExecutableSurfacePolicy,
};
use ckb_testtool::{
    ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*},
    context::Context,
};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

use ckb_script_runner::{build_simple_fixture, execute_cellscript_script};

const DIRECT_VERIFIER: &str = r#"
module deferred_digest
action verify(witness pubkey: [u8; 32], witness signature: [u8; 64]) -> u64 {
    verification
    verifier::btc::bip340::require_signature_from_cell_dep(
        0, env::sighash_all(source::group_input(0)), pubkey, signature)
    return 0
}
"#;

const DISCARDED: &str = r#"
module deferred_digest
action verify() -> u64 {
    verification
    env::sighash_all(source::group_input(0))
    return 0
}
"#;

const UNUSED_BINDING: &str = r#"
module deferred_digest
action verify() -> u64 {
    verification
    let digest = env::sighash_all(source::group_input(0))
    return 0
}
"#;

const HELPER_RETURN: &str = r#"
module deferred_digest
fn digest() -> Hash { env::sighash_all(source::group_input(0)) }
action verify() -> u64 {
    verification
    let ignored = digest()
    return 0
}
"#;

const HELPER_DISCARDED: &str = r#"
module deferred_digest
fn check() -> u64 {
    env::sighash_all(source::group_input(0))
    return 0
}
action verify() -> u64 {
    verification
    return check()
}
"#;

const IGNORED_ARGUMENT: &str = r#"
module deferred_digest
fn ignore(value: Hash) -> u64 { 0 }
action verify() -> u64 {
    verification
    return ignore(env::sighash_all(source::group_input(0)))
}
"#;

const LOCK: &str = r#"
module deferred_digest
lock verify() -> bool {
    verification
    let _ = env::sighash_all(source::group_input(0))
    true
}
"#;

fn cases() -> Vec<String> {
    [DIRECT_VERIFIER, DISCARDED, UNUSED_BINDING, HELPER_RETURN, HELPER_DISCARDED, IGNORED_ARGUMENT, LOCK]
        .into_iter()
        .map(str::to_string)
        .chain([UNUSED_BINDING.replace("let digest", "let _"), HELPER_RETURN.replace("let ignored", "let _")])
        .collect()
}

fn options(edition: CellScriptEdition, diagnostics: cellscript::CompilerDiagnostics) -> CompileOptions {
    CompileOptions { edition, diagnostics, target: Some("riscv64-elf".to_string()), ..Default::default() }
}

fn source_for_edition(source: &str, edition: CellScriptEdition) -> String {
    if edition == CellScriptEdition::Edition2027 {
        source.replace("verification", "")
    } else {
        source.to_string()
    }
}

fn compile_audit(source: &str, edition: CellScriptEdition, diagnostics: cellscript::CompilerDiagnostics) -> CompileResult {
    compile_with_executable_surface_policy(
        &source_for_edition(source, edition),
        options(edition, diagnostics),
        ExecutableSurfacePolicy::AllowFailClosed,
    )
    .expect("deferred syntax remains inspectable")
}

#[test]
fn sighash_calls_are_rejected_by_production_policy_independently_of_result_use() {
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            for source in cases() {
                let error = compile_with_executable_surface_policy(
                    &source_for_edition(&source, edition),
                    options(edition, diagnostics),
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .err()
                .unwrap_or_else(|| panic!("deferred call disappeared: edition={edition:?}, opt={diagnostics}, source={source}"));
                assert!(error.message.contains("ckb-sighash-all-deferred"), "{}", error.message);
            }
        }
    }
}

#[test]
fn audit_artifacts_terminate_before_returning_or_passing_a_digest_in_ckb_vm() {
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            for source in cases().into_iter().filter(|source| source != LOCK) {
                let compiled = compile_audit(&source, edition, diagnostics);
                compiled.validate().expect("deferred runtime artifact retains valid typed lowering evidence");
                let mut fixture = build_simple_fixture(Bytes::new(), 1, 1);
                fixture.current_type_script_input_indices = vec![0];
                if source == DIRECT_VERIFIER {
                    let payload = compiled.metadata.actions[0]
                        .entry_witness_args(&[EntryWitnessArg::Bytes(vec![0; 32]), EntryWitnessArg::Bytes(vec![0; 64])])
                        .unwrap();
                    fixture.witnesses =
                        vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
                }
                let result = execute_cellscript_script(strip_vm_abi_trailer(&compiled.artifact_bytes), &fixture);
                assert_eq!(
                    result.exit_code, 66,
                    "a deferred call must exit the process, not return an error as a Hash: {result:?}, edition={edition:?}, opt={diagnostics}, source={source}"
                );
            }
        }
    }
}

#[test]
fn lock_cannot_convert_deferred_failure_into_boolean_success_in_ckb_vm() {
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        let compiled = compile_audit(LOCK, edition, cellscript::CompilerDiagnostics::default());
        let mut context = Context::new_with_deterministic_rng();
        let code = context.deploy_cell(Bytes::copy_from_slice(strip_vm_abi_trailer(&compiled.artifact_bytes)));
        let lock = context.build_script(&code, Bytes::new()).unwrap();
        let output = packed::CellOutput::new_builder().capacity::<packed::Uint64>(100_000_000_000u64.pack()).lock(lock).build();
        let input = context.create_cell(output.clone(), Bytes::new());
        let tx = context.complete_tx(
            TransactionBuilder::default()
                .input(packed::CellInput::new_builder().previous_output(input).build())
                .output(output)
                .output_data(Bytes::new().pack())
                .build(),
        );
        let error = context.verify_tx(&tx, 10_000_000).expect_err("deferred call is a process failure, not true");
        assert!(format!("{error:?}").contains("error code 66"), "{error:?}");
    }
}

#[test]
fn deferred_classification_is_visible_without_claiming_a_digest_syscall() {
    let compiled =
        compile_audit(DIRECT_VERIFIER, CellScriptEdition::Edition2026, cellscript::CompilerDiagnostics::without_optimizations());
    let feature = "ckb-sighash-all-deferred";
    assert!(compiled.metadata.runtime.fail_closed_runtime_features.iter().any(|item| item == feature));
    assert!(compiled.metadata.actions[0].fail_closed_runtime_features.iter().any(|item| item == feature));
    assert!(compiled.metadata.runtime.verifier_obligations.iter().any(|item| item.feature == feature && item.status == "fail-closed"));
    assert!(compiled
        .metadata
        .runtime
        .proof_plan
        .iter()
        .any(|item| { item.feature == feature && !item.on_chain_checked && item.codegen_coverage_status == "fail-closed" }));
    assert!(compiled
        .metadata
        .runtime
        .ckb_runtime_accesses
        .iter()
        .any(|item| { item.binding == "env::sighash_all" && item.syscall == "EXIT" && item.source == "Process" }));
    assert!(compiled.metadata.runtime.ckb_runtime_accesses.iter().all(|item| item.syscall != "CKB_SIGHASH_ALL"));
    let calls = compiled.metadata.typed_semantics.entries.iter().flat_map(|entry| &entry.blocks).flat_map(|block| &block.operations);
    assert!(calls.filter_map(|operation| operation.call.as_ref()).any(|call| {
        call.target == "__ckb_sighash_all" && call.effect == "deferred-runtime-fail-closed:66:ckb-sighash-all-deferred"
    }));
    let helper =
        compile_audit(HELPER_RETURN, CellScriptEdition::Edition2026, cellscript::CompilerDiagnostics::without_optimizations());
    assert!(helper.metadata.functions.iter().any(|item| item.fail_closed_runtime_features.iter().any(|item| item == feature)));
    let lock = compile_audit(LOCK, CellScriptEdition::Edition2026, cellscript::CompilerDiagnostics::without_optimizations());
    assert!(lock.metadata.locks[0].fail_closed_runtime_features.iter().any(|item| item == feature));
}

#[test]
fn imported_digest_aliases_and_local_wrappers_retain_deferred_execution() {
    let helper = "module deferred::helper\nfn digest() -> Hash { env::sighash_all(source::group_input(0)) }\n";
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for invocation in ["let _ = read_digest()", "let unused = wrapper()"] {
            let source = format!(
                "module deferred::main\nuse deferred::helper::digest as read_digest\nfn wrapper() -> Hash {{ read_digest() }}\naction verify() -> u64 {{ verification {invocation}\nreturn 0 }}"
            );
            let temp = tempfile::tempdir().unwrap();
            std::fs::write(
                temp.path().join("Cell.toml"),
                format!(
                    "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nname = \"deferred_test\"\nversion = \"0.1.0\"\nedition = \"{}\"\n",
                    if edition == CellScriptEdition::Edition2026 { "2026" } else { "2027" }
                ),
            )
            .unwrap();
            let source_dir = temp.path().join("src");
            std::fs::create_dir(&source_dir).unwrap();
            let entry = source_dir.join("main.cell");
            std::fs::write(&entry, source_for_edition(&source, edition)).unwrap();
            std::fs::write(source_dir.join("helper.cell"), helper).unwrap();
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let policy_compile = |policy| {
                    cellscript::compile_path_with_executable_surface_policy(
                        entry.to_str().unwrap(),
                        options(edition, diagnostics),
                        Some(cellscript::CompileEntryScope::Action("verify".to_string())),
                        policy,
                    )
                };
                let error = policy_compile(ExecutableSurfacePolicy::DenyFailClosed)
                    .err()
                    .unwrap_or_else(|| panic!("imported deferred call vanished: {invocation}, opt={diagnostics}"));
                assert!(error.message.contains("ckb-sighash-all-deferred"), "{}", error.message);
                let compiled = policy_compile(ExecutableSurfacePolicy::AllowFailClosed).expect("inspect imported deferred call");
                compiled.validate().expect("imported lowering evidence validates");
                let mut fixture = build_simple_fixture(Bytes::new(), 1, 1);
                fixture.current_type_script_input_indices = vec![0];
                let result = execute_cellscript_script(strip_vm_abi_trailer(&compiled.artifact_bytes), &fixture);
                assert_eq!(result.exit_code, 66, "imported helper failure must not be swallowed: {invocation}, opt={diagnostics}");
            }
        }
    }
}
