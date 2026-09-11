//! Exact executable evidence for the frozen external-verifier business rows.

#![cfg(not(feature = "wasm"))]

use cellscript::{
    compile_path_with_executable_surface_policy, compile_with_executable_surface_policy,
    protocol_bundle::{
        ProtocolCellDep, ProtocolDepType, ProtocolDeploymentIdentity, ProtocolEntryIdentity, ProtocolEntryKind,
        ProtocolNetworkIdentity, ProtocolOutPoint, ProtocolScriptIdentity, ProtocolScriptRole,
    },
    script_handle::{build_exact_script_handle, ExactScriptHandleReceiptInput},
    strip_vm_abi_trailer, CellScriptEdition, CompileEntryScope, CompileOptions, CompileResult, EntryWitnessArg,
    ExecutableSurfacePolicy,
};
use ckb_testtool::{
    builtin::ALWAYS_SUCCESS,
    ckb_hash::blake2b_256,
    ckb_types::{bytes::Bytes, packed, prelude::*},
};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

use ckb_script_runner::{build_simple_fixture, execute_cellscript_script, CkbScriptExecutionResult, CkbVmFixture, FixtureCell};

const EXEC_CALLEE_HEX: &str = include_str!("fixtures/external_exec_callee.hex");

fn exec_callee() -> Bytes {
    let bytes = hex::decode(EXEC_CALLEE_HEX.split_whitespace().collect::<String>()).expect("pinned exec callee hex");
    assert_eq!(bytes.len(), 1_232);
    Bytes::from(bytes)
}

fn byte_string(bytes: &[u8]) -> String {
    let escaped = bytes.iter().map(|byte| format!("\\x{byte:02x}")).collect::<String>();
    format!("b\"{escaped}\"")
}

fn hash_literal(hash: &[u8; 32]) -> String {
    byte_string(hash)
}

fn options() -> CompileOptions {
    CompileOptions {
        edition: CellScriptEdition::Edition2027,
        target: Some("riscv64-elf".to_string()),
        target_profile: Some("ckb".to_string()),
        ..Default::default()
    }
}

fn exec_source(hash: &[u8; 32], argument: u64) -> String {
    format!(
        r#"module acceptance::external_exec

action verify() -> u64 {{
    verification
    ckb::trusted_exec_cell_dep_u8_args(
        0,
        Hash::from_bytes({hash}),
        3,
        97,
        98,
        {argument},
        0
    )
    require false
    return 0
}}
"#,
        hash = hash_literal(hash),
    )
}

fn hex_exec_source(hash: &[u8; 32]) -> String {
    format!(
        r#"module acceptance::external_hex_exec

action verify() -> u64 {{
    verification
    let mut bytes = Vec::new()
    bytes.push(97 as u8)
    bytes.push(98 as u8)
    bytes.push(99 as u8)
    ckb::trusted_exec_cell_dep_hex4(
        0,
        Hash::from_bytes({hash}),
        bytes,
        1,
        1,
        1,
        0
    )
    require false
    return 0
}}
"#,
        hash = hash_literal(hash),
    )
}

fn spawn_source(hash: &[u8; 32]) -> String {
    format!(
        r#"module acceptance::external_spawn

action verify() -> u64 {{
    verification
    let mut bytes = Vec::new()
    bytes.push(97 as u8)
    bytes.push(98 as u8)
    bytes.push(99 as u8)
    ckb::trusted_spawn_wait_cell_dep_hex4(
        0,
        Hash::from_bytes({hash}),
        bytes,
        1,
        1,
        1,
        0
    )
    return 0
}}
"#,
        hash = hash_literal(hash),
    )
}

fn compile_trusted(source: &str, hash: &[u8; 32], operation: &str, adapter: &str) -> CompileResult {
    let directory = tempfile::tempdir().expect("external scenario package");
    let root = camino::Utf8Path::from_path(directory.path()).expect("UTF-8 temporary package path");
    let manager = cellscript::package::PackageManager::new(directory.path());
    manager.init("external_verifier_scenario").expect("initialize external scenario package");
    let mut manifest = manager.read_manifest().expect("read external scenario package");
    manifest.package.edition = CellScriptEdition::Edition2027;
    manifest.deploy.ckb = Some(cellscript::package::CkbDeployConfig {
        trusted_external_verifiers: vec![cellscript::package::CkbTrustedExternalVerifierConfig {
            schema: "cellscript-trusted-external-verifier-v1".to_string(),
            name: format!("scenario-{operation}-{adapter}"),
            scope: "action:verify".to_string(),
            operation: operation.to_string(),
            adapter: adapter.to_string(),
            code_hash: cellscript_artifact_checker::hex_encode(hash),
            hash_type: "data".to_string(),
            source_identity: "pinned external scenario child bytes".to_string(),
            applicability: "frozen external-verifier business scenarios".to_string(),
            trust_basis: "exact CellDep hash plus CKB-VM execution".to_string(),
            guarantees: vec!["returns zero only for the declared bounded invocation".to_string()],
        }],
        ..Default::default()
    });
    manager.write_manifest(&manifest).expect("write external scenario package");
    std::fs::write(root.join("src/main.cell"), source).expect("write external scenario source");
    let compiled = compile_path_with_executable_surface_policy(
        root,
        options(),
        Some(CompileEntryScope::Action("verify".to_string())),
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap_or_else(|error| panic!("trusted external scenario must compile: {error}\n{source}"));
    compiled.validate().expect("independent trusted-external artifact validation");
    compiled
}

fn exact_handle_source(expected_handle_hash: &[u8; 32]) -> String {
    format!(
        r#"module acceptance::external_exact_handle

action verify(witness verifier: ExactScriptHandle) -> u64 {{
    let dependency = ckb::cell_dep(0)
    ckb::require_cell_dep_exact_verifier_handle(
        dependency,
        verifier,
        Hash::from_bytes({hash})
    )
    return 0
}}
"#,
        hash = hash_literal(expected_handle_hash),
    )
}

fn compile_exact_handle(expected_handle_hash: &[u8; 32]) -> CompileResult {
    let source = exact_handle_source(expected_handle_hash);
    let compiled = compile_with_executable_surface_policy(&source, options(), ExecutableSurfacePolicy::DenyFailClosed)
        .unwrap_or_else(|error| panic!("external exact-handle scenario must compile: {error}\n{source}"));
    compiled.validate().expect("independent exact-handle artifact validation");
    compiled
}

fn hash(byte: u8) -> String {
    format!("{byte:02x}").repeat(32)
}

fn exact_handle(child_hash: [u8; 32], entry: &str, typed_semantics_hash: &str, runtime_abi_hash: &str) -> Vec<u8> {
    let artifact_hash = hex::encode(child_hash);
    let deployment = ProtocolDeploymentIdentity {
        network: ProtocolNetworkIdentity { chain_id: "ckb-testnet".to_string(), genesis_hash: format!("0x{}", hash(0x10)) },
        artifact_hash: artifact_hash.clone(),
        script: ProtocolScriptIdentity {
            code_hash: format!("0x{artifact_hash}"),
            hash_type: "data".to_string(),
            args: "0x".to_string(),
        },
        code_cell_dep: ProtocolCellDep {
            out_point: ProtocolOutPoint { tx_hash: format!("0x{}", hash(0x20)), index: 0 },
            dep_type: ProtocolDepType::Code,
        },
    };
    let entry = ProtocolEntryIdentity { kind: ProtocolEntryKind::Action, name: entry.to_string() };
    let (_, value) = build_exact_script_handle(ExactScriptHandleReceiptInput {
        package_coordinate: "acceptance/external-child@1.0.0",
        lock_node_id: "external-child@1.0.0|path:external-child|env=acceptance|features=default",
        entry: &entry,
        script_role: ProtocolScriptRole::SpawnedVerifier,
        interface_hash: &hash(0x31),
        typed_semantics_hash,
        artifact_hash: &artifact_hash,
        target_profile_hash: &hash(0x33),
        runtime_abi_hash,
        verified_bundle_id: &hash(0x35),
        deployment: &deployment,
    })
    .expect("canonical exact verifier handle");
    hex::decode(value.encoded.strip_prefix("0x").unwrap()).expect("exact verifier handle bytes")
}

fn fixture(dependencies: Vec<Bytes>, witness: Option<Bytes>) -> CkbVmFixture {
    let mut fixture = build_simple_fixture(Bytes::from_static(b"external-verifier-scenario"), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.cell_deps =
        dependencies.into_iter().map(|data| FixtureCell { capacity: 100_000_000_000, type_script: None, data }).collect();
    if let Some(witness) = witness {
        fixture.witnesses = vec![witness];
    }
    fixture
}

fn exact_handle_witness(compiled: &CompileResult, handle: Vec<u8>) -> Bytes {
    let payload =
        compiled.metadata.actions[0].entry_witness_args(&[EntryWitnessArg::Bytes(handle)]).expect("exact verifier handle witness");
    packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()
}

fn execute(compiled: &CompileResult, fixture: &CkbVmFixture) -> CkbScriptExecutionResult {
    execute_cellscript_script(strip_vm_abi_trailer(&compiled.artifact_bytes), fixture)
}

fn artifact_identity(compiled: &CompileResult) -> serde_json::Value {
    serde_json::json!({
        "artifact_hash": format!("0x{}", compiled.metadata.artifact_hash.as_deref().expect("artifact hash")),
        "lowering_record_hash": format!("0x{}", compiled.metadata.verified_artifact.lowering_record_hash.as_deref().expect("lowering hash")),
        "source_map_hash": format!("0x{}", compiled.metadata.verified_artifact.source_map_hash.as_deref().expect("source-map hash")),
        "verified_bundle_id": format!("0x{}", compiled.metadata.verified_artifact.verified_bundle_id.as_deref().expect("bundle id")),
    })
}

fn case_record(scenario: &str, outcome: &str, artifact: &str, execution: &CkbScriptExecutionResult) -> serde_json::Value {
    serde_json::json!({
        "scenario": scenario,
        "outcome": outcome,
        "artifact": artifact,
        "rejection_stage": "ckb-vm",
        "expected_exit_code": execution.exit_code,
        "raw_transaction_hash": execution.raw_transaction_hash,
        "serialized_transaction_hash": execution.serialized_transaction_hash,
    })
}

#[test]
fn external_verifier_business_inventory_scenarios_are_exact() {
    let exec_child = exec_callee();
    let exec_child_hash = blake2b_256(&exec_child);
    let always_success_hash = blake2b_256(ALWAYS_SUCCESS.as_ref());

    let exec = compile_trusted(&exec_source(&exec_child_hash, 99), &exec_child_hash, "exec", "u8-args-v1");
    let wrong_argument = compile_trusted(&exec_source(&exec_child_hash, 100), &exec_child_hash, "exec", "u8-args-v1");
    let wrong_hash_value = [0x42; 32];
    let wrong_hash = compile_trusted(&exec_source(&wrong_hash_value, 99), &wrong_hash_value, "exec", "u8-args-v1");
    let wrong_adapter = compile_trusted(&hex_exec_source(&exec_child_hash), &exec_child_hash, "exec", "hex4-v1");
    let spawn = compile_trusted(&spawn_source(&always_success_hash), &always_success_hash, "spawn-wait", "hex4-v1");
    let spawn_failure = compile_trusted(&spawn_source(&exec_child_hash), &exec_child_hash, "spawn-wait", "hex4-v1");

    let handle = exact_handle(exec_child_hash, "verify", &hash(0x32), &hash(0x34));
    let handle_compiled = compile_exact_handle(&blake2b_256(&handle));
    let wrong_scope_handle = exact_handle(exec_child_hash, "other_scope", &hash(0x32), &hash(0x34));
    let wrong_statement_handle = exact_handle(exec_child_hash, "verify", &hash(0x36), &hash(0x34));

    let exec_ok = execute(&exec, &fixture(vec![exec_child.clone()], None));
    assert_eq!(exec_ok.exit_code, 0, "exact EXEC failed: {:?}", exec_ok.captured_debug);
    let spawn_ok = execute(&spawn, &fixture(vec![ALWAYS_SUCCESS.clone()], None));
    assert_eq!(spawn_ok.exit_code, 0, "exact SPAWN/WAIT failed: {:?}", spawn_ok.captured_debug);

    let wrong_dependency_result =
        execute(&exec, &fixture(vec![Bytes::from_static(b"wrong-dependency-position"), exec_child.clone()], None));
    assert_ne!(wrong_dependency_result.exit_code, 0);
    let wrong_hash_result = execute(&wrong_hash, &fixture(vec![exec_child.clone()], None));
    assert_ne!(wrong_hash_result.exit_code, 0);
    let wrong_adapter_result = execute(&wrong_adapter, &fixture(vec![exec_child.clone()], None));
    assert_ne!(wrong_adapter_result.exit_code, 0);
    let wrong_argument_result = execute(&wrong_argument, &fixture(vec![exec_child.clone()], None));
    assert_ne!(wrong_argument_result.exit_code, 0);
    let wrong_scope_result = execute(
        &handle_compiled,
        &fixture(vec![exec_child.clone()], Some(exact_handle_witness(&handle_compiled, wrong_scope_handle))),
    );
    assert_eq!(wrong_scope_result.exit_code, 70);
    let wrong_statement_result = execute(
        &handle_compiled,
        &fixture(vec![exec_child.clone()], Some(exact_handle_witness(&handle_compiled, wrong_statement_handle))),
    );
    assert_eq!(wrong_statement_result.exit_code, 70);
    let child_failure_result = execute(&spawn_failure, &fixture(vec![exec_child.clone()], None));
    assert_ne!(child_failure_result.exit_code, 0);
    let post_build_result = execute(
        &handle_compiled,
        &fixture(vec![Bytes::from_static(b"post-build-substituted-child")], Some(exact_handle_witness(&handle_compiled, handle))),
    );
    assert_eq!(post_build_result.exit_code, 70);

    let artifacts = serde_json::json!({
        "exec": artifact_identity(&exec),
        "wrong_argument": artifact_identity(&wrong_argument),
        "wrong_hash": artifact_identity(&wrong_hash),
        "wrong_adapter": artifact_identity(&wrong_adapter),
        "spawn": artifact_identity(&spawn),
        "spawn_failure": artifact_identity(&spawn_failure),
        "exact_handle": artifact_identity(&handle_compiled),
    });
    let mut cases = vec![
        case_record("exact_identity_exec", "positive", "exec", &exec_ok),
        case_record("exact_identity_spawn_wait", "positive", "spawn", &spawn_ok),
        case_record("wrong_dependency", "adversarial", "exec", &wrong_dependency_result),
        case_record("wrong_hash", "adversarial", "wrong_hash", &wrong_hash_result),
        case_record("wrong_adapter", "adversarial", "wrong_adapter", &wrong_adapter_result),
        case_record("wrong_argument", "adversarial", "wrong_argument", &wrong_argument_result),
        case_record("wrong_scope", "adversarial", "exact_handle", &wrong_scope_result),
        case_record("wrong_statement", "adversarial", "exact_handle", &wrong_statement_result),
        case_record("child_failure", "adversarial", "spawn_failure", &child_failure_result),
        case_record("post_build_substitution", "adversarial", "exact_handle", &post_build_result),
    ];
    cases.sort_by(|left, right| left["scenario"].as_str().cmp(&right["scenario"].as_str()));
    let actual = serde_json::json!({
        "schema": "cellscript-external-verifier-scenarios-v1",
        "source_files": [
            "tests/external_verifier_scenarios.rs",
            "tests/fixtures/external_exec_callee.hex"
        ],
        "dependency_identities": {
            "exec_callee_ckb_data_hash": format!("0x{}", hex::encode(exec_child_hash)),
            "always_success_ckb_data_hash": format!("0x{}", hex::encode(always_success_hash)),
        },
        "artifact_identities": artifacts,
        "cases": cases,
    });
    let recorded: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/external_verifier_scenarios.json")).expect("external-verifier fixture JSON");
    assert_eq!(actual, recorded, "recorded external-verifier identities are stale: {actual}");

    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/business_scenario_evidence.json")).expect("business scenario evidence JSON");
    let family = &manifest["families"]["external_verifier"];
    assert_eq!(family["coverage_status"], "exact-artifact-fixtures");
    assert_eq!(family["gaps"], serde_json::json!([]));
    let records = family["records"].as_array().expect("external-verifier scenario records");
    assert_eq!(records.len(), 10, "external-verifier inventory requires two positive and eight adversarial records");
    for case in actual["cases"].as_array().expect("executed external-verifier cases") {
        let scenario = case["scenario"].as_str().unwrap();
        let artifact = case["artifact"].as_str().unwrap();
        let record = records
            .iter()
            .find(|record| record["scenario"] == scenario && record["outcome"] == case["outcome"])
            .unwrap_or_else(|| panic!("missing exact business-scenario record for {scenario}"));
        assert_eq!(record["status"], "exact-artifact-fixture");
        assert_eq!(record["fixture"], "tests/fixtures/external_verifier_scenarios.json");
        assert_eq!(record["raw_transaction_hash"], case["raw_transaction_hash"]);
        assert_eq!(record["serialized_transaction_hash"], case["serialized_transaction_hash"]);
        assert_eq!(record["artifact_hashes"], serde_json::json!([actual["artifact_identities"][artifact]["artifact_hash"]]));
    }
}
