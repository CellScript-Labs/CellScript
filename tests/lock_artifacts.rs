#![cfg(not(feature = "wasm"))]

use cellscript::{
    artifact::{
        compile_artifact, compile_artifact_metadata, compile_sources_artifact, ArtifactContext, ArtifactDeclaration, ArtifactDispatch,
        EntryCellBindings, EntryCoverage,
    },
    strip_vm_abi_trailer, CompileOptions, EntryWitnessArg, ExecutableSurfacePolicy, InMemorySource,
};
use cellscript_artifact_checker::{CellBindingMembership, CellBindingRole, CellBindingSource};
use ckb_testtool::{
    ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*},
    context::Context,
};

const SOURCE: &str = include_str!("fixtures/authoring/lock-artifact.cell");

fn options() -> CompileOptions {
    CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }
}

#[test]
fn named_lock_artifacts_select_one_lock_and_its_helper_closure() {
    let declaration = ArtifactDeclaration::single_lock_group("vault-lock", "unlock");
    let result = compile_artifact(SOURCE, options(), declaration.clone(), ExecutableSurfacePolicy::DenyFailClosed).unwrap();
    result.validate().unwrap();
    assert!(result.metadata.actions.is_empty());
    assert_eq!(result.metadata.locks.len(), 1);
    assert_eq!(result.metadata.locks[0].name, "unlock");
    assert!(result.metadata.functions.iter().any(|helper| helper.name == "permitted"));
    let foundation = &result.metadata.typed_semantics.foundation;
    assert_eq!(
        (&*foundation.entry_contract.script_role, &*foundation.entry_contract.trigger, &*foundation.entry_contract.exact_entry),
        ("lock", "lock-group", "lock:unlock")
    );
    let cells = foundation.entry_contract.cells.as_ref().unwrap();
    assert_eq!(cells.coverage, EntryCoverage::AuthorizationOnly);
    assert_eq!(cells.bindings, EntryCellBindings::GroupRelative);
    assert!(cells.layout.is_none());
    let binding = &result.metadata.typed_semantics.entries.iter().find(|entry| entry.id == "lock:unlock").unwrap().cell_bindings[0];
    assert_eq!(
        (binding.role, binding.source, binding.ordinal, binding.membership),
        (CellBindingRole::ReadOnly, CellBindingSource::GroupInput, 0, CellBindingMembership::CurrentLockGroup)
    );
    let inspected = compile_artifact_metadata(SOURCE, options(), declaration).unwrap();
    assert_eq!(inspected.typed_semantics, result.metadata.typed_semantics);

    let denied = compile_artifact(
        SOURCE,
        options(),
        ArtifactDeclaration::single_lock_group("deny-lock", "deny"),
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    denied.validate().unwrap();
    assert_eq!(denied.metadata.locks[0].name, "deny");
    assert!(denied.metadata.functions.is_empty());
}

#[test]
fn lock_artifact_binding_is_independent_of_schema_imports_and_helper_placement() {
    let declaration = ArtifactDeclaration::single_lock_group("vault", "unlock");
    let schema = "resource Vault has store { value: u64, }";
    let imported = SOURCE.replace(schema, "use lock_schema::Vault as State;").replace("vault: Vault", "vault: State");
    let sources = [
        InMemorySource { path: "main.cell".into(), source: imported, role: None },
        InMemorySource { path: "schema.cell".into(), source: format!("module lock_schema; {schema}"), role: None },
    ];
    compile_sources_artifact(&sources, "main.cell", options(), declaration.clone(), ExecutableSurfacePolicy::DenyFailClosed)
        .unwrap()
        .validate()
        .unwrap();
    let helper = "fn permitted(value: u64, expected: u64) -> bool {\n    value == expected\n}";
    let moved = SOURCE.replace(helper, "").replace("lock unlock", &format!("{helper}\nlock unlock"));
    let before = compile_artifact(SOURCE, options(), declaration.clone(), ExecutableSurfacePolicy::DenyFailClosed).unwrap();
    let after = compile_artifact(&moved, options(), declaration, ExecutableSurfacePolicy::DenyFailClosed).unwrap();
    assert_eq!(before.metadata.typed_semantics, after.metadata.typed_semantics);
    assert_eq!(
        before.metadata.locks[0].entry_witness_args(&[EntryWitnessArg::U64(7)]).unwrap(),
        after.metadata.locks[0].entry_witness_args(&[EntryWitnessArg::U64(7)]).unwrap()
    );
}

#[test]
fn lock_artifact_retains_explicit_read_only_cell_dependencies() {
    let source = "module lock_dependency;\nresource Config has store { value: u64, }\nlock configured(read config: Config) -> bool { config.value == 7 }";
    let result = compile_artifact(
        source,
        options(),
        ArtifactDeclaration::single_lock_group("config", "configured"),
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    result.validate().unwrap();
    let binding =
        &result.metadata.typed_semantics.entries.iter().find(|entry| entry.id == "lock:configured").unwrap().cell_bindings[0];
    assert_eq!(
        (binding.role, binding.source, binding.membership),
        (CellBindingRole::ReadOnly, CellBindingSource::CellDep, CellBindingMembership::Unproven)
    );
    assert!(result.metadata.locks[0].entry_witness_args(&[]).unwrap().is_empty());
}

#[test]
fn lock_artifacts_reject_unsupported_context_coverage_and_port_combinations() {
    for mutation in ["context", "layout", "bindings", "coverage", "dispatch", "tags"] {
        let mut declaration = ArtifactDeclaration::single_lock_group("vault", "unlock");
        match mutation {
            "context" => declaration.context = ArtifactContext::TypeGroup {},
            "layout" => declaration.layout = Some("Vault".into()),
            "bindings" => declaration.bindings = EntryCellBindings::TransactionAbsolute,
            "coverage" => declaration.coverage = EntryCoverage::FixedGroup,
            "dispatch" => declaration.dispatch = ArtifactDispatch::PolicyWitnessV1,
            "tags" => declaration.actions.push(cellscript::artifact::ArtifactAction { tag: 1, action: "unlock".into() }),
            _ => unreachable!(),
        }
        assert!(declaration.validate().is_err(), "{mutation}");
    }
    for source in [
        SOURCE.replace("witness claimed", "input claimed"),
        SOURCE.replace("protected vault: Vault", "protected vault: Vault, protected other: Vault"),
    ] {
        assert!(compile_artifact(
            &source,
            options(),
            ArtifactDeclaration::single_lock_group("vault", "unlock"),
            ExecutableSurfacePolicy::DenyFailClosed
        )
        .is_err());
    }
    assert!(compile_artifact(
        SOURCE,
        options(),
        ArtifactDeclaration::single_lock_group("wrong-kind", "unrelated"),
        ExecutableSurfacePolicy::DenyFailClosed
    )
    .is_err());
}

fn execute(value: u64, claimed: u64, limit: u64, payload: bool) -> Result<u64, String> {
    execute_source(SOURCE, value, claimed, limit, payload)
}

fn execute_source(source: &str, value: u64, claimed: u64, limit: u64, payload: bool) -> Result<u64, String> {
    let native = compile_artifact(
        source,
        options(),
        ArtifactDeclaration::single_lock_group("vault", "unlock"),
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    native.validate().unwrap();
    let foreign = cellscript::compile("module foreign; lock allow() -> bool { true }", options()).unwrap();
    let mut context = Context::new_with_deterministic_rng();
    let foreign_code = context.deploy_cell(Bytes::copy_from_slice(strip_vm_abi_trailer(&foreign.artifact_bytes)));
    let foreign_lock = context.build_script(&foreign_code, Bytes::new()).unwrap();
    let native_code = context.deploy_cell(Bytes::copy_from_slice(strip_vm_abi_trailer(&native.artifact_bytes)));
    let native_lock = context.build_script(&native_code, Bytes::copy_from_slice(&limit.to_le_bytes())).unwrap();
    let make_cell = |lock: packed::Script| packed::CellOutput::new_builder().capacity(10_000_000_000u64).lock(lock).build();
    let unrelated = context.create_cell(make_cell(foreign_lock.clone()), Bytes::copy_from_slice(&99u64.to_le_bytes()));
    let protected = context.create_cell(make_cell(native_lock), Bytes::copy_from_slice(&value.to_le_bytes()));
    let mut builder = TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(unrelated).build())
        .input(packed::CellInput::new_builder().previous_output(protected).build())
        .output(make_cell(foreign_lock))
        .output_data(Bytes::new().pack());
    if payload {
        let bytes = native.metadata.locks[0].entry_witness_args(&[EntryWitnessArg::U64(claimed)]).unwrap();
        let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(bytes)).pack()).build().as_bytes();
        builder = builder.witnesses([Bytes::new(), witness].pack());
    }
    let transaction = context.complete_tx(builder.build());
    context.verify_tx(&transaction, 10_000_000).map_err(|error| format!("{error:?}"))
}

#[test]
fn selected_lock_mutable_witness_remains_a_local_value_binding() {
    let source = SOURCE
        .replace("witness claimed: u64", "mut witness claimed: u64")
        .replace("    claimed == limit", "    claimed = claimed + 1;\n    claimed == limit");
    assert!(source.contains("claimed = claimed + 1;"));
    assert!(execute_source(&source, 8, 7, 8, true).unwrap() > 0);
    assert!(execute_source(&source, 8, 8, 8, true).is_err());
    assert!(execute_source(&source, 7, 7, 8, true).is_err());
}

#[test]
fn selected_lock_artifact_executes_protected_group_input_args_and_witness_in_ckb_vm() {
    assert!(execute(7, 7, 7, true).unwrap() > 0);
    assert!(execute(7, 8, 7, true).is_err());
    assert!(execute(7, 99, 99, true).is_err(), "transaction Input[0] must not supply the protected Cell value");
    assert!(execute(7, 7, 7, false).is_err());
}
