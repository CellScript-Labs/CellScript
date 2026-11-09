use cellscript::{frontend, ir, types, CompileOptions, CompilerDiagnostics, CURRENT_EDITION};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

fn source_call(module: &str, name: &str, args: &str, parameters: &str) -> String {
    format!("module {module};\nfn {name}({parameters}) -> u64 {{ return 77; }}\naction run() -> u64 {{ return {module}::{name}({args}); }}")
}

fn assert_user_binding(source: &str, expected: &str) {
    let ast = frontend::parse(source, CURRENT_EDITION).unwrap();
    types::check(&ast).unwrap();
    let lowered = ir::generate(&ast).unwrap();
    let action = lowered
        .items
        .iter()
        .find_map(|item| match item {
            ir::IrItem::Action(action) if action.name == "run" => Some(action),
            _ => None,
        })
        .unwrap();
    let callees = action
        .body
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction {
            ir::IrInstruction::Call { func, .. } => func.function_symbol(),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(callees, [expected], "lowering must preserve the resolved source callable");
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        let compiled = cellscript::compile(
            source,
            CompileOptions { diagnostics, source_contracts: true, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        let fixture = ckb_script_runner::build_simple_fixture(ckb_testtool::bytes::Bytes::new(), 1, 1);
        assert_eq!(
            ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture).exit_code,
            77,
            "ordinary source calls retain their scalar result instead of entering an internal verifier/status ABI"
        );
        let record = compiled.verified_lowering_record.as_ref().unwrap();
        for entry in &record.typed_semantics.entries {
            for call in entry.blocks.iter().flat_map(|block| &block.operations).filter_map(|operation| operation.call.as_ref()) {
                assert!(
                    !matches!(call.target.as_str(), "__ckb_cell_capacity" | "__ckb_source_input" | "__ckb_commitment_open"),
                    "source binding was replaced during optimized artifact generation: {}",
                    call.target
                );
            }
        }
    }
}

#[test]
fn source_bindings_precede_collection_and_commitment_special_paths() {
    for (module, name, args, params) in
        [("Vec", "new", "", ""), ("Vec", "with_capacity", "5", "value: u64"), ("commitment", "open", "5, 6", "left: u64, right: u64")]
    {
        assert_user_binding(&source_call(module, name, args, params), name);
    }
}

#[test]
fn imported_source_binding_is_preserved_through_its_defining_owner() {
    let dependency =
        frontend::parse("module ckb;\npublic fn cell_capacity(value: u64) -> u64 { return 77; }", CURRENT_EDITION).unwrap();
    let ast = frontend::parse(
        "module consumer;\nuse ckb::{cell_capacity};\naction run() -> u64 {  return ckb::cell_capacity(5); }",
        CURRENT_EDITION,
    )
    .unwrap();
    let mut resolver = cellscript::resolve::ModuleResolver::new();
    resolver.register_module(dependency).unwrap();
    resolver.register_module(ast.clone()).unwrap();
    types::check_with_resolver(&ast, &resolver, "consumer").unwrap();
    let lowered = ir::generate_with_resolver(&ast, &resolver, "consumer").unwrap();
    let target = lowered
        .items
        .iter()
        .find_map(|item| match item {
            ir::IrItem::Action(action) if action.name == "run" => {
                action.body.blocks.iter().flat_map(|block| &block.instructions).find_map(|instruction| match instruction {
                    ir::IrInstruction::Call { func, .. } => Some(func.clone()),
                    _ => None,
                })
            }
            _ => None,
        })
        .unwrap();
    assert_ne!(target.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellCapacity));
    assert!(
        lowered
            .items
            .iter()
            .any(|item| matches!(item, ir::IrItem::Function(function) if target.function_symbol() == Some(function.name.as_str()))),
        "call must retain the imported source definition, not only its spelling"
    );
}

#[test]
fn imported_spawn_functions_and_aliases_keep_their_source_binding() {
    for (import, call) in [
        ("use foreign::spawn;", "spawn(7)"),
        ("use foreign::spawn as relay;", "relay(7)"),
        ("use foreign::relay as spawn;", "spawn(7)"),
        ("", "foreign::spawn(7)"),
    ] {
        let dependency = frontend::parse(
            "module foreign;\npublic fn spawn(value: u64) -> u64 { return value; }\npublic fn relay(value: u64) -> u64 { return value; }",
            CURRENT_EDITION,
        )
        .unwrap();
        let ast = frontend::parse(&format!("module consumer;\n{import}\naction run() -> u64 {{ return {call}; }}"), CURRENT_EDITION)
            .unwrap();
        let mut resolver = cellscript::resolve::ModuleResolver::new();
        resolver.register_module(dependency).unwrap();
        resolver.register_module(ast.clone()).unwrap();
        types::check_with_resolver(&ast, &resolver, "consumer").unwrap();
        let lowered = ir::generate_with_resolver(&ast, &resolver, "consumer").unwrap();
        let target = lowered
            .items
            .iter()
            .find_map(|item| match item {
                ir::IrItem::Action(action) => {
                    action.body.blocks.iter().flat_map(|block| &block.instructions).find_map(|instruction| match instruction {
                        ir::IrInstruction::Call { func, .. } => Some(func),
                        _ => None,
                    })
                }
                _ => None,
            })
            .unwrap();
        assert!(lowered
            .items
            .iter()
            .any(|item| matches!(item, ir::IrItem::Function(function) if target.function_symbol() == Some(function.name.as_str()))));
    }
}

#[test]
fn distinct_definition_owners_keep_distinct_lowered_call_targets() {
    for (owners, symbols) in
        [(["a::b", "a__b"], ["value", "value"]), (["甲", "乙"], ["取", "取"]), (["producer", "producer"], ["甲", "乙"])]
    {
        let source = format!(
            "module owner_labels;\naction run() -> u64 {{  require {}::{}() == 11; require {}::{}() == 22; return 0; }}",
            owners[0], symbols[0], owners[1], symbols[1]
        );
        let ast = frontend::parse(&source, CURRENT_EDITION).unwrap();
        let mut dependencies = std::collections::BTreeMap::<&str, String>::new();
        for (index, (owner, symbol)) in owners.into_iter().zip(symbols).enumerate() {
            dependencies
                .entry(owner)
                .or_insert_with(|| format!("module {owner};\n"))
                .push_str(&format!("public fn {symbol}() -> u64 {{ return {}; }}\n", 11 * (index + 1)));
        }
        let mut resolver = cellscript::resolve::ModuleResolver::new();
        for source in dependencies.values() {
            resolver.register_module(frontend::parse(source, CURRENT_EDITION).unwrap()).unwrap();
        }
        resolver.register_module(ast.clone()).unwrap();
        types::check_with_resolver(&ast, &resolver, "owner_labels").unwrap();
        let lowered = ir::generate_with_resolver(&ast, &resolver, "owner_labels").unwrap();
        let action = lowered
            .items
            .iter()
            .find_map(|item| match item {
                ir::IrItem::Action(action) => Some(action),
                _ => None,
            })
            .unwrap();
        let targets = action
            .body
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter_map(|instruction| match instruction {
                ir::IrInstruction::Call { func, .. } => Some(func),
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(targets.len(), 2, "different resolved definitions must never collapse into the same assembly callable");
        for target in targets {
            assert!(lowered.items.iter().any(
                |item| matches!(item, ir::IrItem::Function(function) if target.function_symbol() == Some(function.name.as_str()))
            ));
        }
    }
}

#[test]
fn owner_identity_and_transitive_aliases_reach_the_actual_vm() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("a")).unwrap();
    std::fs::write(
        root.path().join("a/b.cell"),
        "module a::b;\npublic struct LeftAnchor { value: u64, }\npublic fn value() -> u64 { return 11; }",
    )
    .unwrap();
    std::fs::write(
        root.path().join("a__b.cell"),
        "module a__b;\npublic struct RightAnchor { value: u64, }\npublic fn value() -> u64 { return 22; }",
    )
    .unwrap();
    std::fs::write(root.path().join("bridge.cell"),
        "module bridge;\nuse a::b::LeftAnchor;\npublic struct BridgeAnchor { value: u64, }\npublic fn relay() -> u64 { return a::b::value(); }").unwrap();
    let path = root.path().join("main.cell");
    std::fs::write(&path,
        "module owner_labels;\nuse a::b::LeftAnchor;\nuse a__b::RightAnchor;\nuse bridge::BridgeAnchor;\nuse a::b::value as direct;\npublic action run() -> u64 {  require direct() == 11; require a__b::value() == 22; require bridge::relay() == 11; return 0; }").unwrap();
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        let compiled = cellscript::compile_path_with_executable_surface_policy(
            path.to_str().unwrap(),
            CompileOptions { source_contracts: true, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            None,
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        cellscript_artifact_checker::interface::inspect_bundle(
            &compiled.artifact_bytes,
            &serde_json::to_vec(&compiled.metadata).unwrap(),
            &serde_json::to_vec(compiled.verified_lowering_record.as_ref().unwrap()).unwrap(),
            &serde_json::to_vec(compiled.source_artifact_map.as_ref().unwrap()).unwrap(),
            &Default::default(),
        )
        .unwrap();
        let tx = ckb_script_runner::build_simple_fixture(ckb_testtool::ckb_types::bytes::Bytes::new(), 1, 1);
        let result = ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes), &tx);
        assert_eq!(result.exit_code, 0, "each owner and the transitive alias must execute its own definition");
    }
}

#[test]
fn a_source_callable_with_an_intrinsic_namespace_keeps_its_binding() {
    assert_user_binding(&source_call("ckb", "cell_capacity", "5", "value: u64"), "cell_capacity");
}

#[test]
fn an_explicit_source_binding_precedes_a_same_spelled_view_constructor() {
    assert_user_binding(&source_call("ckb", "input", "5", "value: u64"), "input");
}

#[test]
fn an_unshadowed_runtime_observation_still_lowers_to_the_registered_helper() {
    let source = "module builtin_observation;\naction run() -> u64 {  return ckb::cell_capacity(source::input(0)); }";
    let ast = frontend::parse(source, CURRENT_EDITION).unwrap();
    types::check(&ast).unwrap();
    let lowered = ir::generate(&ast).unwrap();
    assert!(lowered.items.iter().any(|item| match item {
        ir::IrItem::Action(action) => action
            .body
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(instruction, ir::IrInstruction::Call { func, .. } if func.runtime_helper() == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellCapacity))),
        _ => false,
    }));
}

#[test]
fn source_function_labels_do_not_select_bip340_or_deferred_runtime_contracts() {
    for name in ["__novaseal_bip340_require_signature", "__novaseal_bip340_require_signature_from_cell_dep", "__ckb_sighash_all"] {
        assert_user_binding(&source_call("ordinary_labels", name, "5", "value: u64"), name);
    }
}

#[test]
fn runtime_looking_source_labels_keep_local_proof_and_checker_contracts() {
    for name in [
        "__ckb_commitment_open",
        "__ckb_hash_blake2b_packed",
        "__ckb_sighash_all_zero_lock",
        "__ckb_require_cell_lock_exact_handle",
        "__ckb_require_cell_type_exact_handle",
        "__ckb_require_cell_dep_exact_verifier_handle",
        "__ckb_require_cell_lock_deployment_line_handle",
        "__ckb_require_cell_type_deployment_line_handle",
        "__ckb_require_cell_dep_deployment_line_verifier_handle",
        "__ckb_require_cell_dep_open_script_handle",
        "__ckb_require_cell_dep_open_verifier_handle",
        "__ckb_require_cell_dep_open_script_handle_from_args",
        "__ckb_require_cell_dep_open_verifier_handle_from_args",
        "__ckb_require_open_script_participant_from_args",
        "__ckb_require_instance_script_participant_from_args",
        "__ckb_script_hash",
        "__ckb_header_dep_epoch_number",
        "__ckb_header_dep_epoch_start_block_number",
        "__ckb_header_dep_epoch_length",
        "__ckb_header_dep_block_number",
        "__ckb_header_dep_timestamp_millis",
        "__ckb_hash_data_packed",
        "__ckb_hash_blake2b_var",
        "__ckb_cell_data_bytes32",
        "__ckb_require_cell_lock_args32",
    ] {
        // A mutable local keeps the ordinary call present through optimization,
        // so the checker sees the source label and its typed-local contract.
        let source = format!(
            "module ordinary_contracts;\nfn {name}(value: u64) -> u64 {{ let mut result = value; return result; }}\naction run() -> u64 {{  return {name}(7); }}"
        );
        let metadata = cellscript::compile_metadata(&source, CURRENT_EDITION, None).unwrap();
        assert!(
            metadata.actions[0].proof_plan.iter().all(|plan| !matches!(
                plan.category.as_str(),
                "committed-state-opening" | "committed-state-commit" | "exact-script-handle" | "deployment-line-handle"
            )),
            "ordinary source label {name} must not acquire runtime ProofPlan claims"
        );
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            let compiled =
                cellscript::compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                    .unwrap_or_else(|error| panic!("ordinary source label {name} must compile: {error}"));
            compiled.validate().expect("independent checker must validate the local callee contract");
            let record = compiled.verified_lowering_record.as_ref().unwrap();
            let call = record
                .typed_semantics
                .entries
                .iter()
                .flat_map(|entry| &entry.blocks)
                .flat_map(|block| &block.operations)
                .filter_map(|operation| operation.call.as_ref())
                .find(|call| call.target == name)
                .unwrap_or_else(|| panic!("ordinary call {name} must survive optimization"));
            assert_eq!(call.contract, "typed-local");
            assert_eq!(call.params, ["u64"]);
            assert_eq!(call.return_type, "u64");
            let fixture = ckb_script_runner::build_simple_fixture(ckb_testtool::bytes::Bytes::new(), 1, 1);
            assert_eq!(
                ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture).exit_code,
                7,
                "ordinary {name} must execute its source definition"
            );
        }
    }
}

#[test]
fn ordinary_view_property_labels_keep_their_local_machine_contracts() {
    for name in [
        "__ckb_cell_capacity",
        "__ckb_cell_occupied_capacity",
        "__ckb_cell_unoccupied_capacity",
        "__ckb_cell_data_size",
        "__ckb_cell_output_index",
        "__ckb_cell_data_hash_field",
        "__ckb_cell_lock_hash",
        "__ckb_cell_type_hash",
    ] {
        let source = format!(
            "module ordinary_view_contracts;\nresource State has store {{ value: u64, }}\nfn {name}(view: InputView<State>) -> u64 {{ let mut result = 7; return result; }}\naction run() -> u64 {{  let view = ckb::input::<State>(0); return {name}(view); }}"
        );
        let compiled = cellscript::compile(
            &source,
            CompileOptions {
                diagnostics: CompilerDiagnostics::without_optimizations(),
                target: Some("riscv64-elf".into()),
                ..Default::default()
            },
        )
        .unwrap_or_else(|error| panic!("ordinary view function {name} must compile: {error}"));
        compiled.validate().expect("local view call must not select a property runtime profile");
        let fixture = ckb_script_runner::build_simple_fixture(ckb_testtool::bytes::Bytes::new(), 1, 1);
        assert_eq!(ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture).exit_code, 7, "{name}");
    }
}

#[test]
fn a_local_commitment_result_does_not_claim_packed_hash_construction() {
    let source = r#"
module ordinary_commitment;
struct State { counter: u64, }
fn __ckb_hash_blake2b_packed(value: Commitment<State>) -> Commitment<State> { return value; }
action run(witness value: Commitment<State>) {

        let forwarded = __ckb_hash_blake2b_packed(value);
        require forwarded == value;
}
"#;
    let metadata = cellscript::compile_metadata(source, CURRENT_EDITION, None).unwrap();
    assert!(metadata.actions[0].proof_plan.iter().all(|plan| plan.category != "committed-state-commit"));
}

fn check_rebound_call_record(
    compiled: &cellscript::CompileResult,
    mut record: cellscript_artifact_checker::VerifiedLoweringRecord,
) -> Result<cellscript_artifact_checker::CheckerReport, cellscript_artifact_checker::CheckerError> {
    use cellscript_artifact_checker::{
        canonical_hash, ValueProvenance, LOWERING_RECORD_SCHEMA, SOURCE_MAP_SCHEMA, TYPED_SEMANTICS_SCHEMA,
    };
    let mut metadata = serde_json::to_value(&compiled.metadata).unwrap();
    let mut source_map = compiled.source_artifact_map.clone().unwrap();
    let typed = &mut record.typed_semantics;
    let programs = typed
        .entries
        .iter()
        .map(|entry| {
            (
                entry.id.as_str(),
                entry.kind.as_str(),
                entry.name.as_str(),
                &entry.params,
                &entry.cell_bindings,
                entry.return_type.as_str(),
                &entry.effects,
                entry.entry_block,
                &entry.locals,
                &entry.blocks,
                &entry.borrows,
                &entry.ownership,
                &entry.obligations,
                entry.zk_origins.iter().map(|origin| (origin.policy.as_str(), &origin.fields)).collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    let foundation = &mut typed.foundation;
    foundation.identities.core_semantic_id = canonical_hash(
        "cellscript-core-semantic-id-v3",
        &(
            typed.failure_semantics,
            &typed.types,
            programs,
            &foundation.roles,
            &foundation.dispositions,
            &foundation.claims,
            &foundation.operation_nodes,
        ),
    )
    .unwrap();
    let roots = foundation
        .provenance
        .nodes
        .iter()
        .filter(|node| !matches!(node.provenance, ValueProvenance::Derived { .. }))
        .map(|node| node.id.as_str())
        .collect::<Vec<_>>();
    foundation.identities.entry_contract_id = canonical_hash(
        "cellscript-entry-contract-id-v1",
        &(
            &foundation.identities.core_semantic_id,
            &foundation.entry_contract,
            roots,
            &foundation.entry_contract.entry_payload_abi,
            &foundation.entry_contract.witness_placement_abi,
        ),
    )
    .unwrap();
    foundation.identities.artifact_contract_id = canonical_hash(
        "cellscript-artifact-contract-id-v1",
        &(&foundation.identities.entry_contract_id, &foundation.artifact_contract),
    )
    .unwrap();
    for (field, identity) in [
        ("core_semantic_id", &foundation.identities.core_semantic_id),
        ("entry_contract_id", &foundation.identities.entry_contract_id),
        ("artifact_contract_id", &foundation.identities.artifact_contract_id),
    ] {
        metadata["verified_artifact"][field] = identity.clone().into();
    }
    // Rebind typed block and outer identities so rejection must come from the
    // independently checked call contract, not an intentionally stale digest.
    for entry in &typed.entries {
        for block in &entry.blocks {
            let hash = canonical_hash("cellscript-typed-block-v1", block).unwrap();
            for lowered in record.entries.iter_mut().filter(|lowered| lowered.id == entry.id) {
                lowered.typed_blocks.iter_mut().find(|binding| binding.id == block.id).unwrap().hash = hash.clone();
            }
            for lowered in record
                .blocks
                .iter_mut()
                .filter(|lowered| lowered.owner_entry == entry.id && lowered.lowering_block_id == Some(block.id))
            {
                lowered.typed_block_hash = Some(hash.clone());
            }
        }
    }
    record.typed_semantics_hash = canonical_hash(TYPED_SEMANTICS_SCHEMA, typed).unwrap();
    metadata["typed_semantics"] = serde_json::to_value(typed).unwrap();
    metadata["typed_semantics_hash"] = record.typed_semantics_hash.clone().into();
    let record_hash = canonical_hash(LOWERING_RECORD_SCHEMA, &record).unwrap();
    source_map.lowering_record_hash = record_hash.clone();
    let map_hash = canonical_hash(SOURCE_MAP_SCHEMA, &source_map).unwrap();
    metadata["verified_artifact"]["verified_bundle_id"] = canonical_hash(
        "cellscript-verified-bundle-id-v1",
        &(
            &record.artifact_hash,
            &record.typed_semantics_hash,
            &record.compatibility_profile_hash,
            &record_hash,
            &map_hash,
            &source_map.source_digest,
        ),
    )
    .unwrap()
    .into();
    metadata["verified_artifact"]["lowering_record_hash"] = record_hash.into();
    metadata["verified_artifact"]["source_map_hash"] = map_hash.into();
    cellscript_artifact_checker::check_bundle_values(&compiled.artifact_bytes, &metadata, &record, &source_map, &Default::default())
}

#[test]
fn hostile_call_contracts_reject_after_complete_identity_rebinding() {
    let options = || CompileOptions {
        diagnostics: CompilerDiagnostics::without_optimizations(),
        target: Some("riscv64-elf".into()),
        ..Default::default()
    };
    let ordinary = cellscript::compile(
        "module call_contract_mutations;\nfn ordinary(value: u64) -> u64 { let mut result = value; return result; }\nfn incompatible(value: u64, extra: u64) -> u64 { return value + extra; }\naction run() -> u64 {  let ignored = incompatible(3, 4); return ordinary(7); }",
        options(),
    ).unwrap();
    let record = ordinary.verified_lowering_record.as_ref().unwrap();
    check_rebound_call_record(&ordinary, record.clone()).expect("rebinding an unchanged local-call bundle preserves validity");
    for (contract, target) in
        [("unknown-call-contract", "ordinary"), ("typed-local", "missing_source_definition"), ("typed-local", "incompatible")]
    {
        let mut changed = record.clone();
        let call = changed
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
            .filter_map(|operation| operation.call.as_mut())
            .find(|call| call.target == "ordinary")
            .unwrap();
        call.contract = contract.into();
        call.target = target.into();
        let error = check_rebound_call_record(&ordinary, changed).expect_err("forged local call authority must reject");
        assert_eq!(
            error.code,
            cellscript_artifact_checker::CheckerRejectionCode::V2419TypedSemanticsInvalid,
            "{contract}/{target}: {error}"
        );
        assert!(!error.message.contains("hash") && !error.message.contains("identity"), "must reject the call rule: {error}");
    }

    let runtime = cellscript::compile(
        "module runtime_contract_mutation;\naction run() -> u64 {  return ckb::cell_capacity(source::input(0)); }",
        options(),
    )
    .unwrap();
    let mut changed = runtime.verified_lowering_record.clone().unwrap();
    assert!(!changed.typed_semantics.entries.iter().any(|entry| entry.name == "__ckb_cell_capacity"));
    check_rebound_call_record(&runtime, changed.clone()).expect("rebinding an unchanged runtime-call bundle preserves validity");
    let call = changed
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .filter_map(|operation| operation.call.as_mut())
        .find(|call| call.target == "__ckb_cell_capacity")
        .unwrap();
    call.contract = "typed-local".into();
    let error =
        check_rebound_call_record(&runtime, changed).expect_err("runtime contracts cannot be forged into missing source definitions");
    assert_eq!(error.code, cellscript_artifact_checker::CheckerRejectionCode::V2419TypedSemanticsInvalid);
    assert!(!error.message.contains("hash") && !error.message.contains("identity"), "must reject the call rule: {error}");
}
