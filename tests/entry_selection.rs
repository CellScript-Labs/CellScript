use camino::Utf8Path;
use cellscript::{
    compile, compile_file_with_entry_action, compile_file_with_entry_lock, compile_metadata, CompileOptions, CompileResult,
    EntryWitnessArg, CURRENT_EDITION,
};
use ckb_testtool::{
    ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*},
    context::Context,
};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

fn options(target: &str) -> CompileOptions {
    CompileOptions { target: Some(target.to_string()), target_profile: Some("ckb".to_string()), ..CompileOptions::default() }
}

fn assert_contract(result: &CompileResult, kind: &str, name: &str) {
    let contract = &result.metadata.typed_semantics.foundation.entry_contract;
    assert_eq!(contract.exact_entry, format!("{kind}:{name}"));
    assert_eq!(contract.script_role, if kind == "lock" { "lock" } else { "type" });
    assert_eq!(contract.trigger, if kind == "lock" { "lock-group" } else { "type-group" });
    assert!(matches!(contract.dispatch, cellscript_artifact_checker::EntryDispatchContract::SingleEntry));
    if let Some(record) = &result.verified_lowering_record {
        assert_eq!(record.typed_semantics.foundation.entry_contract, *contract);
    }
    result.validate().expect("selected entry bundle must pass independent validation");
}

fn assert_execution(result: &CompileResult, kind: &str, name: &str, expected: Option<u64>, expected_exit: i64) {
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
    if let Some(value) = expected {
        let args = [EntryWitnessArg::U64(value)];
        let payload = if kind == "lock" {
            result.metadata.locks.iter().find(|entry| entry.name == name).unwrap().entry_witness_args(&args).unwrap()
        } else {
            result.metadata.actions.iter().find(|entry| entry.name == name).unwrap().entry_witness_args(&args).unwrap()
        };
        fixture.witnesses = vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
    }
    if kind == "lock" {
        assert_eq!(expected_exit, 0, "Lock entry-selection cases use a successful spend");
        let mut context = Context::new_with_deterministic_rng();
        let code = context.deploy_cell(Bytes::copy_from_slice(cellscript::strip_vm_abi_trailer(&result.artifact_bytes)));
        let lock = context.build_script(&code, Bytes::new()).unwrap();
        let cell = packed::CellOutput::new_builder().capacity::<packed::Uint64>(100_000_000_000u64.pack()).lock(lock).build();
        let input = context.create_cell(cell.clone(), Bytes::new());
        let transaction = context.complete_tx(
            TransactionBuilder::default()
                .input(packed::CellInput::new_builder().previous_output(input).build())
                .output(cell)
                .output_data(Bytes::new().pack())
                .witnesses(fixture.witnesses.pack())
                .build(),
        );
        context.verify_tx(&transaction, 10_000_000).expect("selected Lock must execute as the input Lock Script");
        return;
    }
    let execution = ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&result.artifact_bytes), &fixture);
    assert_eq!(execution.exit_code, expected_exit, "selected entry must run: {:?}", execution.captured_debug);
}

fn assert_default_selection(source: &str, kind: &str, name: &str, expected: Option<u64>) {
    let asm = compile(source, options("riscv64-asm")).unwrap();
    assert_contract(&asm, kind, name);
    let assembly = std::str::from_utf8(&asm.artifact_bytes).unwrap();
    let call = if expected.is_some() { format!("call {name}\n") } else { format!("j {name}\n") };
    assert!(assembly.contains(&call), "wrapper must invoke the advertised entry");

    let metadata = compile_metadata(source, CURRENT_EDITION, None).unwrap();
    assert_eq!(metadata.typed_semantics.foundation.entry_contract, asm.metadata.typed_semantics.foundation.entry_contract);

    let elf = compile(source, options("riscv64-elf")).unwrap();
    assert_contract(&elf, kind, name);
    assert_execution(&elf, kind, name, expected, 0);
}

#[test]
fn named_main_precedes_an_earlier_no_argument_action() {
    assert_default_selection(
        r#"
module entry_selection
action decoy() -> u64 {
    verification
        require false
        return 0
}
action main(witness expected: u64) -> u64 {
    verification
        require expected == 42
        return 0
}
"#,
        "action",
        "main",
        Some(42),
    );
}

#[test]
fn first_no_argument_action_precedes_an_earlier_parameterized_action() {
    assert_default_selection(
        r#"
module entry_selection
action decoy(witness expected: u64) -> u64 {
    verification
        require false
        return expected
}
action selected() -> u64 {
    verification
        return 0
}
action later() -> u64 {
    verification
        require false
        return 0
}
"#,
        "action",
        "selected",
        None,
    );
}

#[test]
fn first_parameterized_action_is_the_fallback_without_main_or_no_argument_actions() {
    assert_default_selection(
        r#"
module entry_selection
action selected(witness expected: u64) -> u64 {
    verification
        require expected == 42
        return 0
}
action later(witness other: u64) -> u64 {
    verification
        require false
        return other
}
"#,
        "action",
        "selected",
        Some(42),
    );
}

#[test]
fn an_action_precedes_a_lock_even_when_the_lock_is_declared_first() {
    assert_default_selection(
        r#"
module entry_selection
lock decoy() -> bool {
    verification
        false
}
action selected() -> u64 {
    verification
        return 0
}
"#,
        "action",
        "selected",
        None,
    );
}

#[test]
fn first_lock_is_selected_when_no_actions_exist() {
    assert_default_selection(
        r#"
module entry_selection
lock selected(witness expected: u64) -> bool {
    verification
        expected == 42
}
lock later() -> bool {
    verification
        false
}
"#,
        "lock",
        "selected",
        Some(42),
    );
}

#[test]
fn explicit_action_and_lock_scopes_keep_their_resolved_contracts() {
    let source = r#"
module entry_selection
action main() -> u64 {
    verification
        require false
        return 0
}
action selected(witness expected: u64) -> u64 {
    verification
        require expected == 42
        return 0
}
lock first() -> bool {
    verification
        false
}
lock selected_lock(witness expected: u64) -> bool {
    verification
        expected == 42
}
"#;
    let directory = tempfile::tempdir().unwrap();
    let input = Utf8Path::from_path(directory.path()).unwrap().join("policy.cell");
    std::fs::write(&input, source).unwrap();
    let action = compile_file_with_entry_action(&input, options("riscv64-elf"), "selected").unwrap();
    assert_contract(&action, "action", "selected");
    assert_execution(&action, "action", "selected", Some(42), 0);
    let lock = compile_file_with_entry_lock(&input, options("riscv64-elf"), "selected_lock").unwrap();
    assert_contract(&lock, "lock", "selected_lock");
    assert_execution(&lock, "lock", "selected_lock", Some(42), 0);
}

#[test]
fn explicit_scope_cannot_be_overridden_by_retained_main_or_no_argument_actions() {
    for dependency in ["main", "zero_argument"] {
        let source = format!(
            r#"
module entry_selection
fn zero(value: u64) -> u64 {{
    return value - value
}}
action {dependency}() -> u64 {{
    verification
        return zero(1)
}}
action selected(witness expected: u64) -> u64 {{
    verification
        require expected == 42
        return {dependency}()
}}
"#
        );
        let directory = tempfile::tempdir().unwrap();
        let input = Utf8Path::from_path(directory.path()).unwrap().join("policy.cell");
        std::fs::write(&input, &source).unwrap();
        let result = compile_file_with_entry_action(&input, options("riscv64-elf"), "selected").unwrap();
        assert_contract(&result, "action", "selected");
        assert!(result.metadata.actions.iter().any(|action| action.name == dependency), "called action must remain available");
        assert!(result.metadata.functions.iter().any(|function| function.name == "zero"), "transitive helper must remain available");
        assert_execution(&result, "action", "selected", Some(42), 0);
        assert_execution(&result, "action", "selected", Some(13), 5);
    }
}

#[test]
fn explicit_entry_keeps_transitive_enum_payload_layouts_without_retaining_unrelated_types() {
    let source = r#"
module nested_entry_scope
enum Inner { Value(u64), Empty }
struct Payload { inner: Inner }
enum Outer { Wrapped(Payload), Empty }
struct Unrelated { ignored: u64 }
action selected(witness choice: Outer) { verification require true }
"#;
    let directory = tempfile::tempdir().unwrap();
    let input = Utf8Path::from_path(directory.path()).unwrap().join("nested.cell");
    std::fs::write(&input, source).unwrap();
    for opt_level in 0..=3 {
        let result =
            compile_file_with_entry_action(&input, CompileOptions { opt_level, ..options("riscv64-elf") }, "selected").unwrap();
        assert_contract(&result, "action", "selected");
        let types = &result.verified_lowering_record.as_ref().unwrap().typed_semantics.types;
        for name in ["Inner", "Payload", "Outer"] {
            assert!(types.iter().any(|ty| ty.name == name), "missing {name} at optimization level {opt_level}");
        }
        assert!(!types.iter().any(|ty| ty.name == "Unrelated"));
        // Outer::Wrapped(Payload { inner: Inner::Value(42) }): two u8 tags
        // followed by the fixed little-endian u64 payload.
        let mut value = vec![0, 0];
        value.extend_from_slice(&42u64.to_le_bytes());
        let action = result.metadata.actions.iter().find(|entry| entry.name == "selected").unwrap();
        let payload = action.entry_witness_args(&[EntryWitnessArg::Bytes(value)]).unwrap();
        let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        fixture.witnesses = vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
        let execution =
            ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&result.artifact_bytes), &fixture);
        assert_eq!(execution.exit_code, 0, "nested witness at optimization level {opt_level}: {:?}", execution.captured_debug);
    }
}

#[test]
fn selected_entry_retains_identity_only_generic_type_evidence() {
    let source = r#"
module generic_identity_scope
private resource IdentityToken has consume { amount: u64 }
private struct Number has copy, drop, store, fixed, serializable, non_linear { value: u64 }
private enum Choice { Value(Number), Empty }
private struct Pair<T: fixed_value> { left: T, right: T }
private struct Unrelated { ignored: u64 }
public struct Marker<phantom T> has copy, drop, store, fixed, serializable, non_linear { value: u64 }
public fn tag<T: fixed_value>() -> u64 { ckb::cell_capacity(source::input(0)) }
public action selected(witness marker: Marker<(IdentityToken, IdentityToken)>, witness nested: Marker<(Choice, [Pair<u64>; 2])>) {
    verification
    require tag<Number>() > 0
}
"#;
    let directory = tempfile::tempdir().unwrap();
    let input = Utf8Path::from_path(directory.path()).unwrap().join("generic.cell");
    std::fs::write(&input, source).unwrap();
    for opt_level in 0..=3 {
        let result =
            compile_file_with_entry_action(&input, CompileOptions { opt_level, ..options("riscv64-elf") }, "selected").unwrap();
        let typed = &result.verified_lowering_record.as_ref().unwrap().typed_semantics;
        for name in ["IdentityToken", "Number", "Choice"] {
            assert!(typed.types.iter().any(|ty| ty.name == name), "missing identity-only {name} at O{opt_level}");
        }
        assert!(typed.types.iter().any(|ty| ty.name.starts_with("Pair__mono__")));
        assert!(!typed.types.iter().any(|ty| ty.name == "Unrelated"));
        assert!(typed.foundation.roles.is_empty(), "phantom Cell identity does not create a runtime Cell role");
        cellscript_artifact_checker::interface::inspect_bundle(
            &result.artifact_bytes,
            &serde_json::to_vec(&result.metadata).unwrap(),
            &serde_json::to_vec(result.verified_lowering_record.as_ref().unwrap()).unwrap(),
            &serde_json::to_vec(result.source_artifact_map.as_ref().unwrap()).unwrap(),
            &cellscript_artifact_checker::CheckerBudgets::default(),
        )
        .unwrap();
        let bytes = EntryWitnessArg::Bytes(42u64.to_le_bytes().to_vec());
        let payload = result.metadata.actions[0].entry_witness_args(&[bytes.clone(), bytes]).unwrap();
        let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        fixture.witnesses = vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
        let execution =
            ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&result.artifact_bytes), &fixture);
        assert_eq!(execution.exit_code, 0, "phantom layout at O{opt_level}: {:?}", execution.captured_debug);
    }
}

#[test]
fn specialization_marker_is_reserved_by_the_source_generic_kernel() {
    for declaration in [
        "struct Note__mono__not_hex { value: u64 }",
        "fn echo__mono__ff(value: u64) -> u64 { value }",
        "fn forged__mono__753634(value: u64) -> u64 { value }",
    ] {
        let source = format!("module reserved_marker\n{declaration}\naction main() {{ verification require true }}");
        let error = compile(&source, options("riscv64-elf")).unwrap_err();
        assert_eq!(error.code.as_deref(), Some("E2110"));
        assert!(error.message.contains("compiler-reserved monomorphization marker"), "{error}");
    }
}

#[test]
fn imported_generic_contracts_keep_owner_identity_and_lowered_aliases() {
    let directory = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(directory.path()).unwrap();
    std::fs::write(root.join("other.cell"), "module imported_generic_other\npublic struct Pair<T: fixed_value> { value: T }").unwrap();
    std::fs::write(
        root.join("dep.cell"),
        r#"
module imported_generic_owner
public struct Pair<T: fixed_value> { left: T, right: T }
public fn first<T: fixed_value>(value: T) -> T { value }
public fn swap<T: fixed_value>(pair: Pair<T>) -> Pair<T> { Pair<T> { left: pair.right, right: pair.left } }
"#,
    )
    .unwrap();
    let input = root.join("main.cell");
    std::fs::write(
        &input,
        r#"
module imported_generic_consumer
use imported_generic_owner::Pair as Duo
use imported_generic_owner::first as pick
use imported_generic_owner::swap as flip
use imported_generic_other::Pair as Other
public action selected(witness pair: Duo<u64>, witness other: Other<u64>) {
    verification
    require pick<u64>(pair.left) == pair.left
    let swapped: Duo<u64> = flip<u64>(pair)
    require swapped.left == pair.right
}
"#,
    )
    .unwrap();
    for opt_level in 0..=3 {
        let result =
            compile_file_with_entry_action(&input, CompileOptions { opt_level, ..options("riscv64-elf") }, "selected").unwrap();
        let typed = &result.verified_lowering_record.as_ref().unwrap().typed_semantics;
        let pair = typed
            .instantiations
            .iter()
            .find(|instance| instance.template == "Pair" && instance.module == "imported_generic_owner")
            .unwrap();
        assert_eq!(pair.module, "imported_generic_owner");
        assert!(pair.lowered_names.iter().any(|name| name.starts_with("Duo__mono__")));
        assert_eq!(pair.parameters.len(), 1);
        let other = typed
            .instantiations
            .iter()
            .find(|instance| instance.template == "Pair" && instance.module == "imported_generic_other")
            .unwrap();
        assert!(other.lowered_names.iter().any(|name| name.starts_with("Other__mono__")));
        assert_eq!(typed.types.iter().find(|ty| ty.name.starts_with("Duo__mono__")).unwrap().fields.len(), 2);
        assert_eq!(typed.types.iter().find(|ty| ty.name.starts_with("Other__mono__")).unwrap().fields.len(), 1);
        if opt_level == 0 {
            let first = typed.instantiations.iter().find(|instance| instance.template == "first").unwrap();
            assert_eq!(first.module, "imported_generic_owner");
            assert!(first.lowered_names.iter().any(|name| name.starts_with("pick__mono__")));
            let swap = typed.instantiations.iter().find(|instance| instance.template == "swap").unwrap();
            assert_eq!(swap.module, "imported_generic_owner");
            assert!(swap.lowered_names.iter().any(|name| name.starts_with("flip__mono__")));
        }
        cellscript_artifact_checker::interface::inspect_bundle(
            &result.artifact_bytes,
            &serde_json::to_vec(&result.metadata).unwrap(),
            &serde_json::to_vec(result.verified_lowering_record.as_ref().unwrap()).unwrap(),
            &serde_json::to_vec(result.source_artifact_map.as_ref().unwrap()).unwrap(),
            &cellscript_artifact_checker::CheckerBudgets::default(),
        )
        .unwrap();
        let pair_bytes = [11u64.to_le_bytes(), 23u64.to_le_bytes()].concat();
        let payload = result.metadata.actions[0]
            .entry_witness_args(&[EntryWitnessArg::Bytes(pair_bytes), EntryWitnessArg::Bytes(42u64.to_le_bytes().to_vec())])
            .unwrap();
        let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        fixture.witnesses = vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
        let execution =
            ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&result.artifact_bytes), &fixture);
        assert_eq!(execution.exit_code, 0, "imported generic aggregate at O{opt_level}: {:?}", execution.captured_debug);
    }
}

#[test]
fn missing_or_ambiguous_explicit_ir_entries_fail_before_codegen() {
    let source = "module entry_selection\naction main() -> u64 { verification return 0 }";
    let ast = cellscript::frontend::parse(source, CURRENT_EDITION).unwrap();
    let mut ir = cellscript::ir::generate(&ast).unwrap();
    ir.entry_selection = cellscript::ir::IrEntrySelection::Action("missing".to_string());
    let error =
        cellscript::codegen::generate(&ir, &cellscript::codegen::CodegenOptions::default(), cellscript::ArtifactFormat::RiscvElf)
            .expect_err("missing explicit entry must never fall back to main");
    assert_eq!(error.code.as_deref(), Some("E2101"));
    assert!(error.message.contains("found 0"));

    ir.entry_selection = cellscript::ir::IrEntrySelection::Action("main".to_string());
    ir.items.push(ir.items[0].clone());
    let error =
        cellscript::codegen::generate(&ir, &cellscript::codegen::CodegenOptions::default(), cellscript::ArtifactFormat::RiscvElf)
            .expect_err("ambiguous explicit entry must never choose the first callable");
    assert_eq!(error.code.as_deref(), Some("E2101"));
    assert!(error.message.contains("found 2"));
}

#[test]
fn helper_only_modules_have_no_advertised_entry() {
    let metadata =
        compile_metadata("module entry_selection\nfn identity(value: u64) -> u64 { return value }", CURRENT_EDITION, None).unwrap();
    let contract = &metadata.typed_semantics.foundation.entry_contract;
    assert_eq!(contract.exact_entry, "none");
    assert_eq!(contract.script_role, "none");
    assert_eq!(contract.trigger, "none");
}
