//! CKB-VM and metadata evidence for fixed exact Script handles.

use cellscript::assumptions::validate_transaction_against_metadata;
use cellscript::{
    compile_with_executable_surface_policy, strip_vm_abi_trailer, CellScriptEdition, CompileOptions, EntryWitnessArg,
    ExecutableSurfacePolicy,
};
use ckb_testtool::{
    ckb_hash::blake2b_256,
    ckb_types::{bytes::Bytes, packed, prelude::*},
};
use std::process::Command;

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

#[path = "support/cost_measurement.rs"]
#[allow(dead_code)]
mod cost_measurement;
#[path = "support/cost_provenance.rs"]
#[allow(dead_code)]
mod cost_provenance;
#[path = "support/cost_trace.rs"]
#[allow(dead_code)]
mod cost_trace;

use ckb_script_runner::{build_simple_fixture, execute_cellscript_script, CkbVmFixture, FixtureCell};

const HANDLE_BYTES: usize = 202;
const SCRIPT_HASH_OFFSET: usize = 42;
const ARTIFACT_HASH_OFFSET: usize = 106;
const DEP_DATA: &[u8] = b"cellscript-0.30-exact-verifier";

fn byte_string(bytes: &[u8]) -> String {
    let escaped = bytes.iter().map(|byte| format!("\\x{byte:02x}")).collect::<String>();
    format!("b\"{escaped}\"")
}

fn verifier_handle(dep_hash: [u8; 32]) -> Vec<u8> {
    let mut handle = vec![0u8; HANDLE_BYTES];
    handle[..8].copy_from_slice(b"CSHDLv1\0");
    handle[8] = 1;
    handle[9] = 2;
    for (offset, value) in [(10, 0x11), (42, 0x22), (74, 0x33), (138, 0x44), (170, 0x55)] {
        handle[offset..offset + 32].fill(value);
    }
    handle[ARTIFACT_HASH_OFFSET..ARTIFACT_HASH_OFFSET + 32].copy_from_slice(&dep_hash);
    handle
}

fn script_handle(role: u8, script_hash: [u8; 32], marker: u8) -> Vec<u8> {
    let mut handle = vec![0u8; HANDLE_BYTES];
    handle[..8].copy_from_slice(b"CSHDLv1\0");
    handle[8] = 0;
    handle[9] = role;
    for offset in [10, 74, 106, 138, 170] {
        handle[offset..offset + 32].fill(marker);
    }
    handle[SCRIPT_HASH_OFFSET..SCRIPT_HASH_OFFSET + 32].copy_from_slice(&script_hash);
    handle
}

fn verifier_source(expected_handle_hash: [u8; 32]) -> String {
    format!(
        r#"
module exact_handles::verifier

resource Token has store {{ amount: u64 }}

action verify(witness verifier_handle: ExactScriptHandle) -> u64 {{
    let dep = ckb::cell_dep(0)
    ckb::require_cell_dep_exact_verifier_handle(
        dep,
        verifier_handle,
        Hash::from_bytes({})
    )
    return 0
}}
"#,
        byte_string(&expected_handle_hash)
    )
}

fn compile(source: &str) -> cellscript::CompileResult {
    compile_with_executable_surface_policy(
        source,
        CompileOptions {
            edition: CellScriptEdition::Edition2027,
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..Default::default()
        },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap_or_else(|error| panic!("exact Script handle source must compile: {error}\n{source}"))
}

fn fixture(result: &cellscript::CompileResult, handle: Vec<u8>, dep_data: Bytes) -> CkbVmFixture {
    let payload =
        result.metadata.actions[0].entry_witness_args(&[EntryWitnessArg::Bytes(handle)]).expect("encode fixed exact verifier handle");
    let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes();
    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.cell_deps.push(FixtureCell { capacity: 100_000_000_000, type_script: None, data: dep_data });
    fixture.witnesses = vec![witness];
    fixture
}

#[test]
fn exact_verifier_handle_executes_and_rejects_every_substitution_class() {
    let dep_data = Bytes::from_static(DEP_DATA);
    let handle = verifier_handle(blake2b_256(&dep_data));
    let result = compile(&verifier_source(blake2b_256(&handle)));
    let elf = strip_vm_abi_trailer(&result.artifact_bytes);

    let execution = execute_cellscript_script(elf, &fixture(&result, handle.clone(), dep_data.clone()));
    assert_eq!(execution.exit_code, 0, "the exact verifier artifact must match: {:?}", execution.captured_debug);

    for offset in [0, 8, 9, 10, 42, 74, 106, 138, 170, 201] {
        let mut substituted = handle.clone();
        substituted[offset] ^= 0xff;
        let execution = execute_cellscript_script(elf, &fixture(&result, substituted, dep_data.clone()));
        assert_eq!(execution.exit_code, 70, "substitution at handle byte {offset} must fail closed");
    }

    let execution = execute_cellscript_script(elf, &fixture(&result, handle, Bytes::from_static(b"substituted-verifier-artifact")));
    assert_eq!(execution.exit_code, 70, "a substituted CellDep verifier artifact must fail closed");
}

#[test]
fn exact_handle_surface_binds_abi_metadata_proof_plan_and_static_hashes() {
    let dep_hash = blake2b_256(DEP_DATA);
    let handle = verifier_handle(dep_hash);
    let expected_handle_hash = blake2b_256(&handle);
    let result = compile(&verifier_source(expected_handle_hash));
    let action = &result.metadata.actions[0];
    let param = &action.params[0];
    assert_eq!(param.ty, "ExactScriptHandle");
    assert_eq!(param.fixed_byte_len, Some(HANDLE_BYTES));
    assert!(param.fixed_byte_pointer_abi && param.fixed_byte_length_abi);
    assert!(!param.schema_pointer_abi && !param.cell_bound_abi);
    assert!(result.metadata.runtime.ckb_runtime_features.contains(&"ckb-exact-script-handle-v1".to_string()));
    assert!(result.metadata.runtime.ckb_runtime_accesses.iter().any(|access| {
        access.operation == "exact-handle-verifier-artifact-binding"
            && access.source == "CellDep"
            && access.syscall == "LOAD_CELL_BY_FIELD"
            && access.provenance.range.length.value == Some(32)
    }));
    let plan = action.proof_plan.iter().find(|plan| plan.category == "exact-script-handle").expect("exact-handle ProofPlan");
    assert_eq!(plan.evidence_tier.as_str(), "checked-runtime");
    assert!(plan.on_chain_checked);
    assert!(plan.feature.ends_with(&hex::encode(expected_handle_hash)));
    assert!(plan.coverage.contains(&"receipt-commitment:bound-by-full-handle-hash".to_string()));

    let static_surface = format!(
        r#"
module exact_handles::static_surface
resource Token has store {{ amount: u64 }}
action verify(
    witness lock_handle: ExactScriptHandle,
    witness type_handle: ExactScriptHandle
) -> u64 {{
    let input = ckb::input<Token>(0)
    ckb::require_cell_lock_exact_handle(input, lock_handle, Hash::from_bytes({hash}))
    ckb::require_cell_type_exact_handle(input, type_handle, Hash::from_bytes({hash}))
    return 0
}}
"#,
        hash = byte_string(&expected_handle_hash)
    );
    let static_result = compile(&static_surface);
    assert_eq!(static_result.metadata.actions[0].proof_plan.iter().filter(|plan| plan.category == "exact-script-handle").count(), 2);

    let dynamic_hash = static_surface
        .replace("witness type_handle: ExactScriptHandle", "witness type_handle: ExactScriptHandle, witness dynamic_hash: Hash");
    let dynamic_hash = dynamic_hash.replace(&format!("Hash::from_bytes({})", byte_string(&expected_handle_hash)), "dynamic_hash");
    let error = compile_with_executable_surface_policy(
        &dynamic_hash,
        CompileOptions {
            edition: CellScriptEdition::Edition2027,
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..Default::default()
        },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .expect_err("dynamic exact-handle commitments must be rejected");
    assert!(error.to_string().contains("handle_hash must be a compile-time Hash literal"));

    let untyped_source = static_surface.replace("let input = ckb::input<Token>(0)", "let input = 0");
    let error = compile_with_executable_surface_policy(
        &untyped_source,
        CompileOptions {
            edition: CellScriptEdition::Edition2027,
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..Default::default()
        },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .expect_err("raw source indexes must not impersonate typed views for exact handles");
    assert!(error.to_string().contains("expects (source_view, handle: ExactScriptHandle"));
}

#[test]
fn tx_validate_binds_exact_handle_to_witness_parameter_and_cell_dep_position() {
    let dep_hash = blake2b_256(DEP_DATA);
    let handle = verifier_handle(dep_hash);
    let handle_hash = blake2b_256(&handle);
    let result = compile(&verifier_source(handle_hash));
    let assumption = result
        .metadata
        .runtime
        .builder_assumptions
        .iter()
        .find(|assumption| assumption.kind == "exact_script_handle")
        .expect("exact handle builder assumption");
    assert_eq!(assumption.required_cell_deps, ["cell_dep:*"]);
    assert_eq!(assumption.required_witness_fields, ["ExactScriptHandle"]);

    let entry_payload = result.metadata.actions[0]
        .entry_witness_args(&[EntryWitnessArg::Bytes(handle.clone())])
        .expect("encode exact handle entry payload");
    let evidence = serde_json::json!({
        "assumption_id": assumption.assumption_id,
        "kind": assumption.kind,
        "origin": assumption.origin,
        "feature": assumption.feature,
        "proof_plan_status": assumption.proof_plan_status,
        "evidence": {
            "handle": format!("0x{}", hex::encode(&handle)),
            "source": { "location": "cell_dep", "index": 0 },
            "witness": { "index": 0, "field": "input_type" },
        },
    });
    let mut tx = serde_json::json!({
        "inputs": [{}],
        "outputs": [{}],
        "cell_deps": [{
            "out_point": { "tx_hash": format!("0x{}", "11".repeat(32)), "index": 0 },
            "dep_type": "code",
            "data_hash": format!("0x{}", hex::encode(dep_hash)),
        }],
        "witnesses": [{ "input_type": format!("0x{}", hex::encode(&entry_payload)) }],
        "builder_assumption_evidence": { assumption.assumption_id.clone(): evidence },
    });
    let report = validate_transaction_against_metadata(&result.metadata, &tx);
    assert_eq!(report.status, "ok", "valid exact handle evidence must pass: {:?}", report.violations);

    let root = tempfile::tempdir().unwrap();
    let metadata_path = root.path().join("exact.meta.json");
    let tx_path = root.path().join("exact.tx.json");
    std::fs::write(&metadata_path, serde_json::to_vec_pretty(&result.metadata).unwrap()).unwrap();
    std::fs::write(&tx_path, serde_json::to_vec_pretty(&tx).unwrap()).unwrap();
    let cli = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .args(["tx", "validate", "--against"])
        .arg(&metadata_path)
        .arg("--tx")
        .arg(&tx_path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(cli.status.success(), "{}", String::from_utf8_lossy(&cli.stderr));
    let summary: serde_json::Value = serde_json::from_slice(&cli.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["validation"]["checked_assumptions"], serde_json::json!([assumption.assumption_id]));

    let raw_witness =
        packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(entry_payload.clone())).pack()).build().as_bytes();
    tx["witnesses"][0] = serde_json::json!(format!("0x{}", hex::encode(raw_witness)));
    let report = validate_transaction_against_metadata(&result.metadata, &tx);
    assert_eq!(report.status, "ok", "raw canonical WitnessArgs must be decoded: {:?}", report.violations);

    tx["witnesses"][0] = serde_json::json!({ "input_type": format!("0x{}", hex::encode(&entry_payload)) });
    let valid = tx.clone();
    for offset in [0, 8, 9, 10, 42, 74, 106, 138, 170, 201] {
        tx = valid.clone();
        let mut changed = handle.clone();
        changed[offset] ^= 0xff;
        tx["builder_assumption_evidence"][&assumption.assumption_id]["evidence"]["handle"] =
            serde_json::json!(format!("0x{}", hex::encode(changed)));
        let report = validate_transaction_against_metadata(&result.metadata, &tx);
        assert_eq!(report.status, "failed", "handle substitution at byte {offset} must fail tx validate");
    }

    tx = valid.clone();
    tx["cell_deps"][0]["data_hash"] = serde_json::json!(format!("0x{}", "22".repeat(32)));
    assert_eq!(validate_transaction_against_metadata(&result.metadata, &tx).status, "failed");

    tx = valid.clone();
    tx["builder_assumption_evidence"][&assumption.assumption_id]["evidence"]["source"]["index"] = serde_json::json!(1);
    assert_eq!(validate_transaction_against_metadata(&result.metadata, &tx).status, "failed");

    tx = valid.clone();
    tx["builder_assumption_evidence"][&assumption.assumption_id]["evidence"]["witness"]["index"] = serde_json::json!(1);
    assert_eq!(validate_transaction_against_metadata(&result.metadata, &tx).status, "failed");

    tx = valid.clone();
    let mut wrong_parameter = entry_payload;
    wrong_parameter[8] ^= 0xff;
    tx["witnesses"][0]["input_type"] = serde_json::json!(format!("0x{}", hex::encode(wrong_parameter)));
    assert_eq!(validate_transaction_against_metadata(&result.metadata, &tx).status, "failed");
}

#[test]
fn tx_validate_binds_lock_and_type_handles_to_full_script_identity_and_parameter_order() {
    let lock_script = packed::Script::new_builder()
        .code_hash(packed::Byte32::from_slice(&[0x31; 32]).unwrap())
        .hash_type(ckb_testtool::ckb_types::core::ScriptHashType::Data2)
        .args(Bytes::from_static(b"lock-args").pack())
        .build();
    let type_script = packed::Script::new_builder()
        .code_hash(packed::Byte32::from_slice(&[0x41; 32]).unwrap())
        .hash_type(ckb_testtool::ckb_types::core::ScriptHashType::Type)
        .args(Bytes::from_static(b"type-args").pack())
        .build();
    let lock_hash: [u8; 32] = lock_script.calc_script_hash().as_slice().try_into().unwrap();
    let type_hash: [u8; 32] = type_script.calc_script_hash().as_slice().try_into().unwrap();
    let lock_handle = script_handle(0, lock_hash, 0x51);
    let type_handle = script_handle(1, type_hash, 0x61);
    let source = format!(
        r#"
module exact_handles::script_tx_validation
resource Token has store {{ amount: u64 }}
action verify(
    witness lock_handle: ExactScriptHandle,
    witness type_handle: ExactScriptHandle
) -> u64 {{
    let input = ckb::input<Token>(0)
    ckb::require_cell_lock_exact_handle(input, lock_handle, Hash::from_bytes({lock_hash}))
    ckb::require_cell_type_exact_handle(input, type_handle, Hash::from_bytes({type_hash}))
    return 0
}}
"#,
        lock_hash = byte_string(&blake2b_256(&lock_handle)),
        type_hash = byte_string(&blake2b_256(&type_handle)),
    );
    let result = compile(&source);
    let entry_payload = result.metadata.actions[0]
        .entry_witness_args(&[EntryWitnessArg::Bytes(lock_handle.clone()), EntryWitnessArg::Bytes(type_handle.clone())])
        .unwrap();
    let script_json = |code_hash: u8, hash_type: &str, args: &[u8]| {
        serde_json::json!({
            "code_hash": format!("0x{}", hex::encode([code_hash; 32])),
            "hash_type": hash_type,
            "args": format!("0x{}", hex::encode(args)),
        })
    };
    let mut evidence = serde_json::Map::new();
    for assumption in result.metadata.runtime.builder_assumptions.iter().filter(|assumption| assumption.kind == "exact_script_handle")
    {
        let handle = if assumption.feature.starts_with("lock:") { &lock_handle } else { &type_handle };
        evidence.insert(
            assumption.assumption_id.clone(),
            serde_json::json!({
                "assumption_id": assumption.assumption_id,
                "kind": assumption.kind,
                "origin": assumption.origin,
                "feature": assumption.feature,
                "proof_plan_status": assumption.proof_plan_status,
                "evidence": {
                    "handle": format!("0x{}", hex::encode(handle)),
                    "source": { "location": "input", "index": 0 },
                    "witness": { "index": 0, "field": "input_type" },
                },
            }),
        );
    }
    assert_eq!(evidence.len(), 2);
    let mut tx = serde_json::json!({
        "inputs": [{
            "out_point": { "tx_hash": format!("0x{}", "71".repeat(32)), "index": 0 },
            "lock": script_json(0x31, "data2", b"lock-args"),
            "type": script_json(0x41, "type", b"type-args"),
        }],
        "outputs": [],
        "cell_deps": [],
        "witnesses": [{ "input_type": format!("0x{}", hex::encode(&entry_payload)) }],
        "builder_assumption_evidence": evidence,
    });
    let report = validate_transaction_against_metadata(&result.metadata, &tx);
    assert_eq!(report.status, "ok", "full lock/type Script identities must validate: {:?}", report.violations);

    let valid = tx.clone();
    let swapped_payload = result.metadata.actions[0]
        .entry_witness_args(&[EntryWitnessArg::Bytes(type_handle), EntryWitnessArg::Bytes(lock_handle)])
        .unwrap();
    tx["witnesses"][0]["input_type"] = serde_json::json!(format!("0x{}", hex::encode(swapped_payload)));
    assert_eq!(
        validate_transaction_against_metadata(&result.metadata, &tx).status,
        "failed",
        "handle bytes in the wrong compiled parameter positions must fail"
    );

    tx = valid;
    tx["inputs"][0]["lock"]["args"] = serde_json::json!("0x00");
    assert_eq!(
        validate_transaction_against_metadata(&result.metadata, &tx).status,
        "failed",
        "a Script args substitution must fail the complete Script hash binding"
    );
}

#[test]
fn compatible_open_handle_spellings_stay_reserved() {
    use cellscript::{compile_with_executable_surface_policy, CompileOptions, ExecutableSurfacePolicy};
    let cases = [
        ("reference-applied", "module reserved\npublic struct Marker { tag: u64 }\npublic struct Envelope { handle: ScriptHandle<Marker> }\npublic action verify() { verification require true }"),
        ("reference-bare", "module reserved\npublic action verify(witness handle: VerifierHandle) { verification require true }"),
        (
            "declaration",
            "module reserved\npublic struct ScriptHandle { value: u64 }\npublic action verify() { verification require true }",
        ),
        (
            "template-declaration",
            "module reserved\npublic struct VerifierHandle<T: fixed_value> { value: T }\npublic action verify() { verification require true }",
        ),
        ("instance-bare", "module reserved\npublic action verify(witness handle: InstanceScriptHandle) { verification require true }"),
        ("instance-declaration", "module reserved\npublic struct InstanceScriptHandle { value: u64 }\npublic action verify() { verification require true }"),
    ];
    for (label, source) in cases {
        for opt in 0..=3 {
            let error = compile_with_executable_surface_policy(
                source,
                CompileOptions { opt_level: opt, ..CompileOptions::default() },
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap_err();
            // The applied form now reports the marker contract (single-file
            // compilation has no project resolver); bare spellings and user
            // declarations keep the reserved-surface message.
            let message = error.to_string();
            assert!(
                message.contains("reserved for the compatible-open handle surface")
                    || message.contains("interface parameter")
                    || message.contains("project resolver")
                    || message.contains("designate")
                    || message.contains("imported module interface designation"),
                "{label} opt={opt}: {message}"
            );
        }
    }
}

const OPEN_IFACE: &str = r#"
module openiface::types
public struct Marker { tag: u64 }
"#;

fn open_handle_source(root: [u8; 32], bust: u32) -> String {
    format!(
        r#"
// cache-bust {bust}
module openmain::verify
use openiface::types::Marker

public action verify(witness handle: ScriptHandle<openiface::types>) -> u64 {{
    verification
    let dep = ckb::cell_dep(0)
    ckb::require_cell_dep_open_script_handle(
        dep,
        handle,
        Hash::from_bytes({})
    )
    return 0
}}
"#,
        byte_string(&root)
    )
}

fn compile_open_handle(source: &str, opt: u8) -> cellscript::CompileResult {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(directory.path().join("src")).unwrap();
    std::fs::write(directory.path().join("src/iface.cell"), OPEN_IFACE).unwrap();
    std::fs::write(directory.path().join("src/main.cell"), source).unwrap();
    cellscript::compile_path_with_executable_surface_policy(
        camino::Utf8PathBuf::from_path_buf(directory.path().join("src/main.cell")).expect("utf8 tempdir path"),
        CompileOptions {
            opt_level: opt,
            target: Some("riscv64-elf".into()),
            target_profile: Some("ckb".into()),
            source_contracts: true,
            ..CompileOptions::default()
        },
        None,
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap_or_else(|error| panic!("open handle source must compile: {error}"))
}

fn open_authorization_set(script_hash: [u8; 32]) -> (cellscript_artifact_checker::open_handle_policy::AuthorizationSet, [u8; 656]) {
    open_authorization_set_for_identity(script_hash, None)
}

fn open_authorization_set_for_identity(
    script_hash: [u8; 32],
    verifier_data_hash: Option<[u8; 32]>,
) -> (cellscript_artifact_checker::open_handle_policy::AuthorizationSet, [u8; 656]) {
    use cellscript_artifact_checker::open_handle_policy::*;
    let fill = |byte: u8| {
        let mut hash = [0u8; 32];
        hash.fill(byte);
        hash
    };
    let header = PolicyHeader {
        class: if verifier_data_hash.is_some() { HandleClass::Verifier } else { HandleClass::Script },
        role: if verifier_data_hash.is_some() { ScriptRole::SpawnedVerifier } else { ScriptRole::Type },
        mode: SelectionMode::Compatible,
        member_count: 1,
        sequence: 1,
        minimum_admission_sequence: 0,
        required_interface: fill(0x41),
        exact_receipt: [0; 32],
        network_genesis: fill(0x42),
        target_profile: fill(0x43),
        runtime_abi: fill(0x44),
    };
    let member = PolicyMember {
        status: MemberStatus::Active,
        hash_type: CodeHashType::Data2,
        admission_sequence: 0,
        deployment_sequence: 0,
        receipt: fill(0x51),
        interface: fill(0x52),
        artifact: verifier_data_hash.unwrap_or_else(|| blake2b_256(b"open-script-code")),
        script: script_hash,
        // Data-mode members bind their code hash to the artifact identity.
        code_hash: verifier_data_hash.unwrap_or_else(|| blake2b_256(b"open-script-code")),
        code_tx_hash: blake2b_256([b"cellscript-test-cell-dep-v1".as_slice(), &0u64.to_le_bytes()].concat()),
        deployment_line: [0; 32],
        history_tip: [0; 32],
        code_output_index: 0,
    };
    let set = AuthorizationSet::new(header, &[member]).expect("structurally valid authorization set");
    let selection = set.selection(&fill(0x51)).expect("single-member selection");
    (set, selection)
}

fn open_handle_fixture(result: &cellscript::CompileResult, selection: Vec<u8>, dep_script: &packed::Script) -> CkbVmFixture {
    let payload = result.metadata.actions[0]
        .entry_witness_args(&[EntryWitnessArg::Bytes(selection)])
        .expect("encode the 656-byte open handle selection");
    let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes();
    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.cell_deps.push(FixtureCell {
        capacity: 100_000_000_000,
        type_script: Some(dep_script.clone()),
        data: Bytes::from_static(b"open-script-code"),
    });
    fixture.witnesses = vec![witness];
    fixture
}

/// The constant-root helper binds the selection and selected dependency at
/// every optimization level, with independent substitution negatives.
#[test]
fn open_script_handle_requirement_executes_and_rejects_substitutions() {
    let dep_script = packed::Script::new_builder()
        .code_hash([9u8; 32].pack())
        .hash_type(ckb_testtool::ckb_types::core::ScriptHashType::Data2)
        .args(Bytes::default().pack())
        .build();
    let dep_type_hash: [u8; 32] = dep_script.calc_script_hash().unpack();
    let (set, selection) = open_authorization_set(dep_type_hash);
    let root: [u8; 32] = set.root();
    // The wire itself must be self-consistent under the authority recompute.
    cellscript_artifact_checker::open_handle_policy::verify_selection(&root, &selection)
        .expect("the constructed selection verifies against its own root");

    for opt in 0..=3u8 {
        let result = compile_open_handle(&open_handle_source(root, std::process::id() as u32), opt);
        let elf = strip_vm_abi_trailer(&result.artifact_bytes);
        let fixture = open_handle_fixture(&result, selection.to_vec(), &dep_script);
        let execution = execute_cellscript_script(elf, &fixture);
        assert_eq!(
            execution.exit_code, 0,
            "opt={opt} the admitted selection must verify: cycles={} debug={:?}",
            execution.cycles, execution.captured_debug
        );
        // Frozen budget for the helper's first measured profile: the whole
        // verification action stays inside the policy cycle ceiling, and the
        // witness payload keeps its fixed 664-byte size (8-byte entry ABI
        // magic plus the 656-byte selection).
        assert!(
            execution.cycles <= 3_000_000,
            "opt={opt} open-handle verification exceeds its measured cycle budget: {}",
            execution.cycles
        );
        assert_eq!(
            result.metadata.actions[0].entry_witness_args(&[EntryWitnessArg::Bytes(selection.to_vec())]).unwrap().len(),
            664,
            "the generated witness payload keeps its frozen size"
        );

        // Wrong expected root: a fresh compilation with a different constant.
        let mut wrong = root;
        wrong[0] ^= 0xff;
        let wrong_result = compile_open_handle(&open_handle_source(wrong, std::process::id() as u32 + 1000), opt);
        let wrong_elf = strip_vm_abi_trailer(&wrong_result.artifact_bytes);
        let execution = execute_cellscript_script(wrong_elf, &fixture);
        assert_eq!(execution.exit_code, 85, "opt={opt} a substituted expected root must fail closed");

        for offset in [0usize, 8, 9, 11, 196, 324, 488, 496, 655] {
            let mut substituted = selection.to_vec();
            substituted[offset] ^= 0xff;
            let fixture = open_handle_fixture(&result, substituted, &dep_script);
            let execution = execute_cellscript_script(elf, &fixture);
            assert_eq!(execution.exit_code, 85, "opt={opt} substitution at selection byte {offset} must fail closed");
        }

        // Code bytes, not the Script guarding the code Cell, are selected.
        let mut fixture = open_handle_fixture(&result, selection.to_vec(), &dep_script);
        fixture.cell_deps[0].data = Bytes::from_static(b"substituted-code");
        assert_eq!(execute_cellscript_script(elf, &fixture).exit_code, 85);
        let mut fixture = open_handle_fixture(&result, selection.to_vec(), &dep_script);
        fixture.cell_deps.push(fixture.cell_deps[0].clone());
        assert_eq!(execute_cellscript_script(elf, &fixture).exit_code, 85, "ambiguous code identity");
    }
}

fn open_handle_from_args_source(bust: u32) -> String {
    format!(
        r#"
// cache-bust {bust}
module openmain::verify
use openiface::types::Marker

public action verify(witness handle: ScriptHandle<openiface::types>) -> u64 {{
    verification
    let dep = ckb::cell_dep(0)
    ckb::require_cell_dep_open_script_handle_from_args(
        dep,
        handle
    )
    return 0
}}
"#
    )
}

fn open_handle_from_args_fixture(
    result: &cellscript::CompileResult,
    script_args: Vec<u8>,
    selection: Vec<u8>,
    dep_script: &packed::Script,
) -> CkbVmFixture {
    let payload = result.metadata.actions[0]
        .entry_witness_args(&[EntryWitnessArg::Bytes(selection)])
        .expect("encode the 656-byte open handle selection");
    let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes();
    let mut fixture = build_simple_fixture(Bytes::from(script_args), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.cell_deps.push(FixtureCell {
        capacity: 100_000_000_000,
        type_script: Some(dep_script.clone()),
        data: Bytes::from_static(b"open-script-code"),
    });
    fixture.witnesses = vec![witness];
    fixture
}

/// The from-args variant derives the expected root from the current Script's
/// committed 32-byte args (frozen decision #4): the witness never authorizes
/// its own root. A root committed in args verifies; every substituted args
/// root, a wrong-length args field and the substitution classes all fail
/// closed with the open-handle error.
/// Regression: the saved selection length and LOAD_SCRIPT's eight-byte length
/// output must not overlap. The former layout corrupted the saved length and
/// rejected even the valid committed root before membership verification.
#[test]
fn open_handle_from_args_binds_the_root_to_committed_script_args() {
    let dep_script = packed::Script::new_builder()
        .code_hash([9u8; 32].pack())
        .hash_type(ckb_testtool::ckb_types::core::ScriptHashType::Data2)
        .args(Bytes::default().pack())
        .build();
    let dep_type_hash: [u8; 32] = dep_script.calc_script_hash().unpack();
    let (set, selection) = open_authorization_set(dep_type_hash);
    let root: [u8; 32] = set.root();

    for opt in 0..=3u8 {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(directory.path().join("src")).unwrap();
        std::fs::write(directory.path().join("src/iface.cell"), OPEN_IFACE).unwrap();
        std::fs::write(directory.path().join("src/main.cell"), open_handle_from_args_source(std::process::id() as u32)).unwrap();
        let result = cellscript::compile_path_with_executable_surface_policy(
            camino::Utf8PathBuf::from_path_buf(directory.path().join("src/main.cell")).expect("utf8 path"),
            CompileOptions {
                opt_level: opt,
                target: Some("riscv64-elf".into()),
                target_profile: Some("ckb".into()),
                source_contracts: true,
                ..CompileOptions::default()
            },
            None,
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap_or_else(|error| panic!("from-args source must compile: {error}"));
        let elf = strip_vm_abi_trailer(&result.artifact_bytes);

        // The committed 32-byte args root admits the selection.
        let fixture = open_handle_from_args_fixture(&result, root.to_vec(), selection.to_vec(), &dep_script);
        let execution = execute_cellscript_script(elf, &fixture);
        assert_eq!(
            execution.exit_code, 0,
            "opt={opt} the args-committed root must verify: cycles={} debug={:?}",
            execution.cycles, execution.captured_debug
        );
        assert!(execution.cycles <= 3_000_000, "opt={opt} from-args verification exceeds its budget: {}", execution.cycles);

        // A substituted committed root rejects.
        let mut wrong_root = root;
        wrong_root[0] ^= 0xff;
        let fixture = open_handle_from_args_fixture(&result, wrong_root.to_vec(), selection.to_vec(), &dep_script);
        let execution = execute_cellscript_script(elf, &fixture);
        assert_eq!(execution.exit_code, 85, "opt={opt} a substituted committed root must fail closed");

        // A wrong-length args field rejects: neither 31 nor 33 bytes is a
        // canonical committed root.
        for length in [31usize, 33] {
            let mut args = root.to_vec();
            args.truncate(length.min(32));
            args.resize(length, 0);
            let fixture = open_handle_from_args_fixture(&result, args, selection.to_vec(), &dep_script);
            let execution = execute_cellscript_script(elf, &fixture);
            assert_eq!(execution.exit_code, 85, "opt={opt} args length {length} must fail closed");
        }

        // Selection substitutions still reject under the committed root.
        for offset in [0usize, 16, 19, 204, 324, 488, 496] {
            let mut substituted = selection.to_vec();
            substituted[offset] ^= 0xff;
            let fixture = open_handle_from_args_fixture(&result, root.to_vec(), substituted, &dep_script);
            let execution = execute_cellscript_script(elf, &fixture);
            assert_eq!(execution.exit_code, 85, "opt={opt} substitution at byte {offset} must fail closed");
        }
    }
}

#[test]
fn open_verifier_handle_binds_artifact_data_for_constant_and_args_roots() {
    let dep_script = packed::Script::new_builder()
        .code_hash([9u8; 32].pack())
        .hash_type(ckb_testtool::ckb_types::core::ScriptHashType::Data2)
        .args(Bytes::default().pack())
        .build();
    let script_hash: [u8; 32] = dep_script.calc_script_hash().unpack();
    let dep_data = Bytes::from_static(b"open-verifier-artifact");
    let data_hash = blake2b_256(&dep_data);
    assert_ne!(data_hash, script_hash);
    let (set, selection) = open_authorization_set_for_identity(script_hash, Some(data_hash));
    let root = set.root();

    for from_args in [false, true] {
        for opt in 0..=3 {
            let source = if from_args { open_handle_from_args_source(0) } else { open_handle_source(root, 0) }
                .replace("ScriptHandle<", "VerifierHandle<")
                .replace("open_script_handle", "open_verifier_handle");
            let result = compile_open_handle(&source, opt);
            let elf = strip_vm_abi_trailer(&result.artifact_bytes);
            let mut fixture = open_handle_from_args_fixture(&result, root.to_vec(), selection.to_vec(), &dep_script);
            fixture.cell_deps[0].data = dep_data.clone();
            let execution = execute_cellscript_script(elf, &fixture);
            assert_eq!(execution.exit_code, 0, "opt={opt} from_args={from_args}: {:?}", execution.captured_debug);
            assert!(execution.cycles <= 3_000_000, "opt={opt} from_args={from_args} exceeds the cycle ceiling");

            // Membership alone cannot admit different executable bytes, even
            // when the guarding Script and the proof are unchanged.
            fixture.cell_deps[0].data = Bytes::from_static(b"substituted-open-verifier");
            assert_eq!(execute_cellscript_script(elf, &fixture).exit_code, 85, "opt={opt} from_args={from_args}");

            // A correctly committed policy whose artifact does not match the
            // dependency still rejects; a matching Script hash is insufficient.
            let (wrong_set, wrong_selection) = open_authorization_set_for_identity(script_hash, Some([0x7a; 32]));
            let wrong_source = if from_args { open_handle_from_args_source(0) } else { open_handle_source(wrong_set.root(), 0) }
                .replace("ScriptHandle<", "VerifierHandle<")
                .replace("open_script_handle", "open_verifier_handle");
            let wrong_result = compile_open_handle(&wrong_source, opt);
            let mut wrong_fixture =
                open_handle_from_args_fixture(&wrong_result, wrong_set.root().to_vec(), wrong_selection.to_vec(), &dep_script);
            wrong_fixture.cell_deps[0].data = dep_data.clone();
            assert_eq!(
                execute_cellscript_script(strip_vm_abi_trailer(&wrong_result.artifact_bytes), &wrong_fixture).exit_code,
                85,
                "opt={opt} from_args={from_args} wrong artifact under an authorized root"
            );
        }
    }
}

#[test]
fn open_code_selection_enforces_type_bytes_outpoints_modes_and_floors() {
    use cellscript_artifact_checker::open_handle_policy::{AuthorizationSet, CodeHashType, MemberStatus, SelectionMode};
    let code_type = packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(1u8).build();
    let script_hash = code_type.calc_script_hash().unpack();
    let (baseline, _) = open_authorization_set(script_hash);
    for opt in 0..=3 {
        for type_hash in [false, true] {
            for case in ["valid", "exact", "wrong-exact", "yanked", "floor", "outpoint", "index"] {
                let mut header = baseline.header().clone();
                let mut member = baseline.members()[0].clone();
                if type_hash {
                    member.hash_type = CodeHashType::Type;
                    member.code_hash = script_hash;
                    member.deployment_line = script_hash;
                    member.history_tip = [0x31; 32];
                }
                match case {
                    "exact" => {
                        header.mode = SelectionMode::Exact;
                        header.exact_receipt = member.receipt;
                    }
                    "wrong-exact" => {
                        header.mode = SelectionMode::Exact;
                        header.exact_receipt = [0x91; 32];
                    }
                    "yanked" => member.status = MemberStatus::Yanked,
                    "floor" => header.minimum_admission_sequence = 1,
                    "outpoint" => member.code_tx_hash[0] ^= 1,
                    "index" => member.code_output_index = 1,
                    _ => {}
                }
                // Include a second member for a valid exact-mode policy which
                // authorizes one receipt while the witness selects the other.
                let mut members = vec![member.clone()];
                if case == "wrong-exact" {
                    let mut other = member.clone();
                    other.receipt = header.exact_receipt;
                    other.code_tx_hash[0] ^= 2;
                    other.artifact[0] ^= 2;
                    if !type_hash {
                        other.code_hash = other.artifact;
                    }
                    if type_hash {
                        other.deployment_line[0] ^= 2;
                    }
                    members.push(other);
                    header.member_count = 2;
                }
                let set = AuthorizationSet::new(header, &members).unwrap();
                let selection = set.selection(&member.receipt).unwrap();
                let result = compile_open_handle(&open_handle_from_args_source(0), opt);
                let elf = strip_vm_abi_trailer(&result.artifact_bytes);
                let fixture = open_handle_from_args_fixture(&result, set.root().to_vec(), selection.to_vec(), &code_type);
                let execution = execute_cellscript_script(elf, &fixture);
                let expected = if matches!(case, "valid" | "exact") { 0 } else { 85 };
                assert_eq!(
                    execution.exit_code, expected,
                    "opt={opt} type_hash={type_hash} case={case}: {:?}",
                    execution.captured_debug
                );
                if expected == 0 {
                    assert!(execution.cycles <= 3_000_000);
                    let mut replacement = fixture.clone();
                    replacement.cell_deps[0].data = Bytes::from_static(b"replacement-under-unchanged-type");
                    assert_eq!(execute_cellscript_script(elf, &replacement).exit_code, 85);
                    let mut ambiguous = fixture.clone();
                    ambiguous.cell_deps.push(ambiguous.cell_deps[0].clone());
                    assert_eq!(execute_cellscript_script(elf, &ambiguous).exit_code, 85);
                    let mut reserved = selection.to_vec();
                    reserved[489] = 1; // outside the hashed header and member
                    let changed = open_handle_from_args_fixture(&result, set.root().to_vec(), reserved, &code_type);
                    assert_eq!(execute_cellscript_script(elf, &changed).exit_code, 85);
                }
            }
        }
    }
}

#[test]
fn open_code_dependency_budget_covers_every_direct_cell() {
    use ckb_script_runner::execute_cellscript_script_with_observer;
    let code_type = packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(1u8).build();
    let (set, selection) = open_authorization_set(code_type.calc_script_hash().unpack());
    for opt in 0..=3 {
        let result = compile_open_handle(&open_handle_from_args_source(0), opt);
        let elf = strip_vm_abi_trailer(&result.artifact_bytes);
        let mut fixture = open_handle_from_args_fixture(&result, set.root().to_vec(), selection.to_vec(), &code_type);
        // The harness adds the executing Script and always-success code deps.
        // Sixty-two supplied cells therefore exercise the full 64-entry bound.
        for index in 1..62u8 {
            fixture.cell_deps.push(FixtureCell { capacity: 100_000_000_000, type_script: None, data: Bytes::from(vec![index]) });
        }
        let (execution, tx_hash) = execute_cellscript_script_with_observer(
            elf,
            &fixture,
            |tx, _| tx,
            |_, tx, _| {
                assert_eq!(tx.cell_deps().len(), 64);
                tx.hash()
            },
        );
        assert_eq!(execution.exit_code, 0, "opt={opt}: {:?}", execution.captured_debug);
        assert!(execution.cycles <= 3_000_000, "opt={opt}: {} cycles", execution.cycles);
        let (replay, replay_hash) = execute_cellscript_script_with_observer(elf, &fixture, |tx, _| tx, |_, tx, _| tx.hash());
        assert_eq!(replay.exit_code, 0);
        assert_eq!(tx_hash, replay_hash);
        assert_eq!(execution.cycles, replay.cycles);
        eprintln!(
            "open-direct-64 opt={opt} artifact={} raw_tx={} cycles={} witness_payload=664 helper_frame=992",
            hex::encode(blake2b_256(elf)),
            hex::encode(tx_hash.as_slice()),
            execution.cycles
        );
        // A duplicate in the last permitted slot cannot hide after a valid first
        // dependency. Adding a 65th direct dependency separately exceeds bounds.
        fixture.cell_deps[61].data = fixture.cell_deps[0].data.clone();
        assert_eq!(execute_cellscript_script(elf, &fixture).exit_code, 85);
        fixture.cell_deps[61].data = Bytes::from(vec![61]);
        fixture.cell_deps.push(FixtureCell { capacity: 100_000_000_000, type_script: None, data: Bytes::from(vec![62]) });
        assert_eq!(execute_cellscript_script(elf, &fixture).exit_code, 85);
    }
}

#[test]
fn open_code_dependency_rejects_actual_dep_group_expansion() {
    use ckb_script_runner::execute_cellscript_script_with_transaction_transform;
    use ckb_testtool::ckb_types::core::DepType;
    let code_type = packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(1u8).build();
    let (set, selection) = open_authorization_set(code_type.calc_script_hash().unpack());
    let mut point_seed = b"cellscript-test-cell-dep-v1".to_vec();
    point_seed.extend_from_slice(&0u64.to_le_bytes());
    let point = packed::OutPoint::new_builder().tx_hash(blake2b_256(&point_seed).pack()).index(0u32).build();
    let group: packed::OutPointVec = vec![point].pack();
    for opt in 0..=3 {
        let result = compile_open_handle(&open_handle_from_args_source(0), opt);
        let elf = strip_vm_abi_trailer(&result.artifact_bytes);
        let mut fixture = open_handle_from_args_fixture(&result, set.root().to_vec(), selection.to_vec(), &code_type);
        fixture.cell_deps.push(FixtureCell { capacity: 100_000_000_000, type_script: None, data: group.as_bytes() });
        assert_eq!(execute_cellscript_script(elf, &fixture).exit_code, 0);
        // The group contains an existing valid code OutPoint. It is otherwise
        // resolvable and does not fail because its member is missing or malformed.
        let execution = execute_cellscript_script_with_transaction_transform(elf, &fixture, |tx, _| {
            let deps = tx
                .cell_deps()
                .into_iter()
                .enumerate()
                .map(|(index, dep)| if index == 1 { dep.as_builder().dep_type(DepType::DepGroup).build() } else { dep })
                .collect();
            tx.as_advanced_builder().set_cell_deps(deps).build()
        });
        assert_eq!(execution.exit_code, 85, "opt={opt}: {:?}", execution.captured_debug);
    }
}

#[test]
fn open_participant_observation_binds_role_source_and_complete_script_without_peer_authority() {
    let participant = packed::Script::new_builder()
        .code_hash(blake2b_256(&*ckb_testtool::builtin::ALWAYS_SUCCESS).pack())
        .hash_type(4u8)
        .args(Bytes::from_static(b"participant-args").pack())
        .build();
    let (original, _) = open_authorization_set(participant.calc_script_hash().unpack());
    let mut member = original.members()[0].clone();
    member.artifact = blake2b_256(&*ckb_testtool::builtin::ALWAYS_SUCCESS);
    member.code_hash = member.artifact;
    let set =
        cellscript_artifact_checker::open_handle_policy::AuthorizationSet::new(original.header().clone(), &[member.clone()]).unwrap();
    let selection = set.selection(&member.receipt).unwrap();
    for opt in 0..=3 {
        for subject in ["ckb::input<Token>(1)", "ckb::output<Token>(0)"] {
            let source = open_handle_from_args_source(0)
                .replace("public action", "resource Token has store { amount: u64 }\npublic action")
                .replace("require_cell_dep_open_script_handle_from_args", "require_open_script_participant_from_args")
                .replace("        handle\n", &format!("        handle,\n        {subject}\n"));
            let result = compile_open_handle(&source, opt);
            let elf = strip_vm_abi_trailer(&result.artifact_bytes);
            let mut fixture = open_handle_from_args_fixture(&result, set.root().to_vec(), selection.to_vec(), &participant);
            fixture.cell_deps[0].data = ckb_testtool::builtin::ALWAYS_SUCCESS.clone();
            fixture.inputs.push(FixtureCell {
                capacity: 100_000_000_000,
                type_script: Some(participant.clone()),
                data: Bytes::default(),
            });
            fixture.outputs[0].type_script = Some(participant.clone());
            fixture.witnesses.push(Bytes::default());
            let execution = execute_cellscript_script(elf, &fixture);
            assert_eq!(execution.exit_code, 0, "O{opt} {subject}: {:?}", execution.captured_debug);
            assert!(execution.cycles < 3_000_000);
            let mut changed = fixture.clone();
            let replacement = participant.clone().as_builder().args(Bytes::from_static(b"substituted").pack()).build();
            if subject == "ckb::input<Token>(1)" {
                changed.inputs[1].type_script = Some(replacement);
            } else {
                changed.outputs[0].type_script = Some(replacement);
            }
            assert_eq!(execute_cellscript_script(elf, &changed).exit_code, 85, "complete Script args O{opt} {subject}");
            let wrong_source = source.replace(subject, "ckb::group_input<Token>(0)");
            let wrong_result = compile_open_handle(&wrong_source, opt);
            assert_eq!(
                execute_cellscript_script(strip_vm_abi_trailer(&wrong_result.artifact_bytes), &fixture).exit_code,
                85,
                "group-relative source must observe the actual own group"
            );
            let missing_source = source.replace(subject, "ckb::input<Token>(2)");
            let missing_result = compile_open_handle(&missing_source, opt);
            assert_eq!(execute_cellscript_script(strip_vm_abi_trailer(&missing_result.artifact_bytes), &fixture).exit_code, 85);
        }
    }
}

fn instance_handle_source() -> String {
    open_handle_from_args_source(0)
        .replace("public action", "public resource Document has store { digest: Hash }\npublic action")
        .replace("ScriptHandle<", "InstanceScriptHandle<")
        .replace("require_cell_dep_open_script_handle_from_args", "require_instance_script_participant_from_args")
        .replace("        handle\n", "        handle,\n        ckb::input<Document>(1)\n")
}

#[test]
fn instance_participant_reads_actual_role_and_global_or_group_relative_views() {
    use cellscript_artifact_checker::instance_policy::{self, ArgsConstraint, ConsumerArgs, InstanceMode, InstancePolicy};
    use cellscript_artifact_checker::open_handle_policy::{AuthorizationSet, ScriptRole};
    use ckb_script_runner::execute_cellscript_script_with_context_setup_and_observer;

    let participant = packed::Script::new_builder()
        .code_hash(blake2b_256(&*ckb_testtool::builtin::ALWAYS_SUCCESS).pack())
        .hash_type(4u8)
        .args(Bytes::from_static(b"instance-A").pack())
        .build();
    let (original, _) = open_authorization_set(participant.calc_script_hash().unpack());
    let mut member = original.members()[0].clone();
    member.artifact = blake2b_256(&*ckb_testtool::builtin::ALWAYS_SUCCESS);
    member.code_hash = member.artifact;
    for role in [ScriptRole::Lock, ScriptRole::Type] {
        let mut header = original.header().clone();
        header.role = role;
        let set = AuthorizationSet::new(header, &[member.clone()]).unwrap();
        let policy = InstancePolicy {
            implementation_root: set.root(),
            mode: InstanceMode::Constrained,
            args_length: 10,
            config_length: 0,
            constraints: vec![ArgsConstraint::Any { offset: 0, length: 10 }],
        };
        let selection = instance_policy::encode_selection(&policy, &set.selection(&member.receipt).unwrap(), b"instance-A").unwrap();
        let committed =
            ConsumerArgs { policy_root: policy.root().unwrap(), subject: participant.calc_script_hash().unpack(), config: Vec::new() }
                .encode()
                .unwrap();
        for opt in 0..=3 {
            for (view, is_input, is_group) in
                [("input", true, false), ("output", false, false), ("group_input", true, true), ("group_output", false, true)]
            {
                let index = usize::from(!is_group);
                let source = instance_handle_source().replace("ckb::input<Document>(1)", &format!("ckb::{view}<Document>({index})"));
                let result = compile_open_handle(&source, opt);
                let elf = strip_vm_abi_trailer(&result.artifact_bytes);
                let mut fixture = open_handle_from_args_fixture(&result, committed.to_vec(), selection.to_vec(), &participant);
                fixture.cell_deps[0].data = ckb_testtool::builtin::ALWAYS_SUCCESS.clone();
                fixture.inputs.push(FixtureCell {
                    capacity: 100_000_000_000,
                    type_script: Some(participant.clone()),
                    data: Bytes::new(),
                });
                fixture.outputs.push(fixture.inputs[1].clone());
                fixture.witnesses.push(Bytes::new());
                if role == ScriptRole::Lock {
                    // The current Type group starts at global index 1. Its
                    // group index 0 must not alias the unrelated global Cell 0.
                    fixture.current_type_script_input_indices = vec![1];
                    fixture.outputs[0].type_script = Some(participant.clone());
                    fixture.outputs[1].type_script = None;
                    fixture.witnesses.swap(0, 1);
                }
                let run = |observed: &packed::Script, fixture: &CkbVmFixture, elf: &[u8]| {
                    execute_cellscript_script_with_context_setup_and_observer(
                        elf,
                        fixture,
                        |tx, _| {
                            let outputs = tx
                                .outputs()
                                .into_iter()
                                .enumerate()
                                .map(|(index, output)| {
                                    if index != 1 || is_input {
                                        output
                                    } else if role == ScriptRole::Lock {
                                        output.as_builder().lock(observed.clone()).build()
                                    } else {
                                        output.as_builder().type_(Some(observed.clone()).pack()).build()
                                    }
                                })
                                .collect();
                            tx.as_advanced_builder().set_outputs(outputs).build()
                        },
                        |context, tx, _| {
                            if is_input {
                                let point = tx.inputs().get(1).unwrap().previous_output();
                                let (output, data) = context.get_cell(&point).unwrap();
                                let output = if role == ScriptRole::Lock {
                                    output.as_builder().lock(observed.clone()).build()
                                } else {
                                    output.as_builder().type_(Some(observed.clone()).pack()).build()
                                };
                                context.create_cell_with_out_point(point, output, data);
                            }
                        },
                        |_, _, _| (),
                    )
                    .0
                };
                // For Type-role group views the actual observed Type is the
                // consumer itself, not the foreign participant at global 1.
                let expected = if role == ScriptRole::Type && is_group { 85 } else { 0 };
                let execution = run(&participant, &fixture, elf);
                assert_eq!(execution.exit_code, expected, "O{opt} {role:?} {view}: {:?}", execution.captured_debug);
                let other = participant.clone().as_builder().args(Bytes::from_static(b"instance-B").pack()).build();
                assert_eq!(run(&other, &fixture, elf).exit_code, 85, "O{opt} {role:?} {view}: substituted instance");
                let missing = compile_open_handle(&source.replace(&format!("<Document>({index})"), "<Document>(99)"), opt);
                assert_eq!(
                    run(&participant, &fixture, strip_vm_abi_trailer(&missing.artifact_bytes)).exit_code,
                    85,
                    "O{opt} {role:?} {view}: missing view"
                );
            }
        }
    }
}

#[test]
fn instance_handle_authoring_preserves_format_and_rejects_cross_profile_types() {
    let source = instance_handle_source();
    let tokens = cellscript::lexer::lex(&source).unwrap();
    let parsed = cellscript::parser::parse(&tokens).unwrap();
    let formatted = cellscript::fmt::format_default(&parsed).unwrap();
    let result = compile_open_handle(&formatted, 0);
    assert_eq!(result.metadata.actions[0].params[0].ty, "InstanceScriptHandle<openiface::types>");
    let plan = result.metadata.actions[0].proof_plan.iter().find(|plan| plan.category == "script-instance-admission").unwrap();
    assert!(plan.on_chain_checked && plan.identity_lifecycle_policy.contains("application predicates"));
    // This initial decoder profile is intentionally one handle only; accepting
    // its dedicated width must not widen the old aggregate payload limit.
    let multiple = compile_open_handle(
        &source.replace(
            "witness handle: InstanceScriptHandle<openiface::types>",
            "witness handle: InstanceScriptHandle<openiface::types>, witness extra: u64",
        ),
        0,
    );
    assert!(
        cellscript_artifact_checker::entry_codec::check_fixed_policy_parameter_decoders(
            &multiple.artifact_bytes,
            &serde_json::to_vec(&multiple.metadata).unwrap(),
            &serde_json::to_vec(multiple.verified_lowering_record.as_ref().unwrap()).unwrap(),
            &serde_json::to_vec(multiple.source_artifact_map.as_ref().unwrap()).unwrap(),
            &Default::default(),
        )
        .is_err(),
        "instance-only width cannot authorize additional entry parameters"
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.cell");
    std::fs::write(directory.path().join("iface.cell"), OPEN_IFACE).unwrap();
    for changed in [
        formatted.replace("InstanceScriptHandle<", "ScriptHandle<"),
        formatted.replace("InstanceScriptHandle<", "VerifierHandle<"),
        formatted.replace("ckb::input<Document>(1)", "ckb::cell_dep(1)"),
        formatted.replace("require_instance_script_participant_from_args", "require_open_script_participant_from_args"),
    ] {
        assert_ne!(changed, formatted, "mutation must change formatted source");
        std::fs::write(&path, &changed).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&path).arg("--json").output().unwrap();
        assert!(!output.status.success(), "cross-profile source must reject");
        let diagnostic: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(diagnostic.to_string().contains("expects (dep: CellDepView"), "{diagnostic}");
    }
    #[cfg(feature = "lsp")]
    {
        let mut server = cellscript::lsp::LspServer::new();
        let uri = format!("file://{}", path.display());
        std::fs::write(&path, &formatted).unwrap();
        server.open_document(uri.clone(), formatted.clone());
        assert!(server.get_diagnostics(&uri).is_empty(), "{:?}", server.get_diagnostics(&uri));
        server.update_document(uri.clone(), formatted.replace("InstanceScriptHandle<", "ScriptHandle<"));
        assert!(server.get_diagnostics(&uri).iter().any(|diagnostic| diagnostic.message.contains("InstanceScriptHandle<module>")));
        server.update_document(uri.clone(), "ckb::".into());
        let completions = server.completion(&uri, cellscript::lsp::Position { line: 0, character: 5 });
        assert!(completions.iter().any(|item| item.label == "require_instance_script_participant_from_args"));
    }
}

#[test]
fn constrained_instance_reads_committed_configuration_and_actual_participant_args() {
    use cellscript_artifact_checker::instance_policy::{self, ArgsConstraint, ConsumerArgs, InstanceMode, InstancePolicy};
    let mut args = vec![1];
    args.extend_from_slice(&[0x41; 32]);
    args.extend_from_slice(&150u64.to_le_bytes());
    args.extend_from_slice(&1u64.to_le_bytes());
    let participant = packed::Script::new_builder()
        .code_hash(blake2b_256(&*ckb_testtool::builtin::ALWAYS_SUCCESS).pack())
        .hash_type(4u8)
        .args(Bytes::from(args.clone()).pack())
        .build();
    let (original, _) = open_authorization_set(participant.calc_script_hash().unpack());
    let mut member = original.members()[0].clone();
    member.artifact = blake2b_256(&*ckb_testtool::builtin::ALWAYS_SUCCESS);
    member.code_hash = member.artifact;
    let set =
        cellscript_artifact_checker::open_handle_policy::AuthorizationSet::new(original.header().clone(), &[member.clone()]).unwrap();
    let inner = set.selection(&member.receipt).unwrap();
    let policy = InstancePolicy {
        implementation_root: set.root(),
        mode: InstanceMode::Constrained,
        args_length: 49,
        config_length: 32,
        constraints: vec![
            ArgsConstraint::Bytes { offset: 0, value: vec![1] },
            ArgsConstraint::Config { offset: 1, length: 32, config_offset: 0 },
            ArgsConstraint::Unsigned { offset: 33, width: 8, minimum: 100, maximum: 200 },
            ArgsConstraint::Any { offset: 41, length: 8 },
        ],
    };
    let selection = instance_policy::encode_selection(&policy, &inner, &args).unwrap();
    let committed =
        ConsumerArgs { policy_root: policy.root().unwrap(), subject: participant.calc_script_hash().unpack(), config: vec![0x41; 32] }
            .encode()
            .unwrap();
    for opt in 0..=3 {
        let source = instance_handle_source();
        let result = compile_open_handle(&source, opt);
        assert_eq!(result.metadata.actions[0].params[0].fixed_byte_len, Some(instance_policy::SELECTION_BYTES));
        cellscript_artifact_checker::check_bundle_values(
            &result.artifact_bytes,
            &serde_json::to_value(&result.metadata).unwrap(),
            result.verified_lowering_record.as_ref().unwrap(),
            result.source_artifact_map.as_ref().unwrap(),
            &Default::default(),
        )
        .unwrap();
        cellscript_artifact_checker::entry_codec::check_fixed_policy_parameter_decoders(
            &result.artifact_bytes,
            &serde_json::to_vec(&result.metadata).unwrap(),
            &serde_json::to_vec(result.verified_lowering_record.as_ref().unwrap()).unwrap(),
            &serde_json::to_vec(result.source_artifact_map.as_ref().unwrap()).unwrap(),
            &Default::default(),
        )
        .unwrap();
        let elf = strip_vm_abi_trailer(&result.artifact_bytes);
        let fixture_for = |consumer: Vec<u8>, wire: Vec<u8>, script: packed::Script| {
            let mut fixture = open_handle_from_args_fixture(&result, consumer, wire, &script);
            fixture.cell_deps[0].data = ckb_testtool::builtin::ALWAYS_SUCCESS.clone();
            fixture.inputs.push(FixtureCell { capacity: 100_000_000_000, type_script: Some(script), data: Bytes::new() });
            fixture.witnesses.push(Bytes::new());
            fixture
        };
        let fixture = fixture_for(committed.to_vec(), selection.to_vec(), participant.clone());
        let execution = execute_cellscript_script(elf, &fixture);
        assert_eq!(execution.exit_code, 0, "O{opt}: {:?}", execution.captured_debug);
        // B is also policy-valid, but changing only the observed Cell cannot
        // substitute it for the subject authorized by the consumer's args.
        let mut other_args = args.clone();
        other_args[41] = 2;
        let mut other = fixture.clone();
        other.inputs[1].type_script = Some(participant.clone().as_builder().args(Bytes::from(other_args).pack()).build());
        assert_eq!(execute_cellscript_script(elf, &other).exit_code, 85);
        // Grant the changed subject explicitly, so permission predicates must
        // reject invalid values after all hashes have been recomputed.
        for (offset, value, expected) in
            [(0, 2, 85), (1, 0x42, 85), (33, 99, 85), (33, 100, 0), (33, 200, 0), (33, 201, 85), (41, 2, 0)]
        {
            let mut changed = args.clone();
            changed[offset] = value;
            let script = participant.clone().as_builder().args(Bytes::from(changed.clone()).pack()).build();
            let wire = instance_policy::encode_selection(&policy, &inner, &changed).unwrap();
            let consumer = ConsumerArgs {
                policy_root: policy.root().unwrap(),
                subject: script.calc_script_hash().unpack(),
                config: vec![0x41; 32],
            }
            .encode()
            .unwrap();
            assert_eq!(instance_policy::verify_selection(&consumer, &wire).is_ok(), expected == 0);
            assert_eq!(
                execute_cellscript_script(elf, &fixture_for(consumer.to_vec(), wire.to_vec(), script)).exit_code,
                expected,
                "O{opt} field offset={offset} value={value}"
            );
        }
        // Reconstruct the advertised schema and policy roots even for malformed
        // wire rows. Rejecting merely because a digest is stale is insufficient.
        for (offset, value) in [
            (16, 2),
            (17, 17),
            (19, 2),
            (20, 129),
            (22, 1),
            (88, 4),
            (89, 1),
            (90, 0),
            (90, 33),
            (92, 1),
            (94, 1),
            (96, 1),
            (105, 1),
            (136 + 6, 1),
            (136 + 16, 1),
            (184 + 2, 3),
            (184 + 16, 201),
            (184 + 32, 1),
            (232 + 16, 1),
            (280, 1),
            (instance_policy::ARGS_OFFSET + 49, 1),
        ] {
            let mut wire = selection;
            wire[offset] = value;
            let mut schema = instance_policy::ARGS_SCHEMA_DOMAIN.to_vec();
            let mut layout = [0u8; 136];
            layout[..2].copy_from_slice(&wire[18..20]);
            layout[2] = wire[17];
            for index in 0..usize::from(wire[17]).min(16) {
                let row = 88 + index * 48;
                let slot = 8 + index * 8;
                layout[slot..slot + 2].copy_from_slice(&wire[row + 4..row + 6]);
                layout[slot + 2..slot + 4].copy_from_slice(&wire[row + 2..row + 4]);
                layout[slot + 4] = u8::from(wire[row] == 1);
            }
            schema.extend_from_slice(&layout);
            wire[56..88].copy_from_slice(&blake2b_256(&schema));
            let mut root = instance_policy::POLICY_DOMAIN.to_vec();
            root.extend_from_slice(&wire[8..instance_policy::IMPLEMENTATION_OFFSET]);
            let mut consumer = committed;
            consumer[8..40].copy_from_slice(&blake2b_256(&root));
            assert!(instance_policy::verify_selection(&consumer, &wire).is_err(), "host offset={offset}");
            assert_eq!(
                execute_cellscript_script(elf, &fixture_for(consumer.to_vec(), wire.to_vec(), participant.clone())).exit_code,
                85,
                "rebound O{opt} offset={offset}"
            );
        }
        for offset in [0, 8, 40, 72, 73, 74, 112, 207] {
            let mut consumer = committed;
            consumer[offset] ^= 1;
            assert_eq!(
                execute_cellscript_script(elf, &fixture_for(consumer.to_vec(), selection.to_vec(), participant.clone())).exit_code,
                85,
                "consumer O{opt} offset={offset}"
            );
        }
        for length in [0, 32, 207, 209] {
            let mut consumer = committed.to_vec();
            consumer.resize(length, 0);
            assert_eq!(execute_cellscript_script(elf, &fixture_for(consumer, selection.to_vec(), participant.clone())).exit_code, 85);
        }
        eprintln!("instance-smoke O{opt} ELF={} cycles={}", elf.len(), execution.cycles);
    }
}

#[test]
fn instance_runtime_empty_maximum_scalar_widths_and_late_rejection_are_bounded() {
    use cellscript_artifact_checker::instance_policy::{self, ArgsConstraint, ConsumerArgs, InstanceMode, InstancePolicy};
    use cellscript_artifact_checker::open_handle_policy::{AuthorizationSet, SelectionMode};
    use ckb_testtool::ckb_script::ScriptGroupType;
    let mut resource_records = Vec::new();
    for opt in 0..=3 {
        let compiled = compile_open_handle(&instance_handle_source(), opt);
        let elf = strip_vm_abi_trailer(&compiled.artifact_bytes);
        let mut profiles = Vec::new();
        for mode in [InstanceMode::Exact, InstanceMode::Constrained] {
            profiles.push((format!("empty-{mode:?}"), mode, Vec::new(), Vec::new(), Vec::new(), false));
            let args = vec![0x41; 256];
            let config = if mode == InstanceMode::Constrained { vec![0x41; 128] } else { Vec::new() };
            let rules = (0..16)
                .map(|index| {
                    if mode == InstanceMode::Constrained && index < 8 {
                        ArgsConstraint::Config { offset: index * 16, length: 16, config_offset: index * 16 }
                    } else {
                        ArgsConstraint::Bytes { offset: index * 16, value: vec![0x41; 16] }
                    }
                })
                .collect::<Vec<_>>();
            profiles.push((format!("maximum-{mode:?}"), mode, args.clone(), config.clone(), rules.clone(), false));
            if mode == InstanceMode::Constrained {
                profiles.push(("late-invalid".into(), mode, args, config, rules, true));
            }
        }
        for width in [1, 2, 4, 8] {
            let maximum = if width == 8 { u64::MAX } else { (1u64 << (8 * width)) - 1 };
            for value in [0, maximum] {
                profiles.push((
                    format!("unsigned-{width}-{value}"),
                    InstanceMode::Constrained,
                    value.to_le_bytes()[..width as usize].to_vec(),
                    Vec::new(),
                    vec![ArgsConstraint::Unsigned { offset: 0, width, minimum: 0, maximum }],
                    false,
                ));
            }
        }
        // Exercise each constraint-family cost at the largest representable
        // partition, together with all 32 members and all 64 direct deps.
        // Sixteen eight-byte scalars necessarily occupy 128, not 256, bytes.
        for family in ["bytes", "config", "scalar", "any", "scalar-any"] {
            let length = if family == "scalar" { 128 } else { 256 };
            let rules = (0..16)
                .map(|index| match family {
                    "bytes" => ArgsConstraint::Bytes { offset: index * 16, value: vec![0x41; 16] },
                    "config" => ArgsConstraint::Config { offset: index * 16, length: 16, config_offset: (index % 8) * 16 },
                    "scalar" | "scalar-any" if index < 15 || family == "scalar" => ArgsConstraint::Unsigned {
                        offset: index * 8,
                        width: 8,
                        minimum: 0x4141414141414141,
                        maximum: 0x4141414141414141,
                    },
                    "scalar-any" => ArgsConstraint::Any { offset: 120, length: 136 },
                    _ => ArgsConstraint::Any { offset: index * 16, length: 16 },
                })
                .collect::<Vec<_>>();
            profiles.push((
                format!("full-{family}"),
                InstanceMode::Constrained,
                vec![0x41; length],
                vec![0x41; 128],
                rules.clone(),
                false,
            ));
            if family == "bytes" || family == "config" {
                profiles.push((
                    format!("full-{family}-late"),
                    InstanceMode::Constrained,
                    vec![0x41; length],
                    vec![0x41; 128],
                    rules,
                    true,
                ));
            }
        }
        for width in [1u16, 2, 4] {
            let value = u64::from_le_bytes([0x41; 8]) >> (64 - width as u32 * 8);
            let mut rules = (0..15)
                .map(|index| ArgsConstraint::Unsigned { offset: index * width, width, minimum: value, maximum: value })
                .collect::<Vec<_>>();
            rules.push(ArgsConstraint::Any { offset: 15 * width, length: 256 - 15 * width });
            profiles.push((
                format!("full-scalar-{width}-any"),
                InstanceMode::Constrained,
                vec![0x41; 256],
                vec![0x41; 128],
                rules,
                false,
            ));
        }
        for (name, mode, mut args, config, constraints, late_invalid) in profiles {
            // Both implementation modes remain orthogonal to instance mode.
            for exact_implementation in [false, true] {
                let code_hash = blake2b_256(&*ckb_testtool::builtin::ALWAYS_SUCCESS);
                let original_script = packed::Script::new_builder()
                    .code_hash(code_hash.pack())
                    .hash_type(4u8)
                    .args(Bytes::from(args.clone()).pack())
                    .build();
                let (original, _) = open_authorization_set(original_script.calc_script_hash().unpack());
                let mut member = original.members()[0].clone();
                member.artifact = code_hash;
                member.code_hash = code_hash;
                let mut header = original.header().clone();
                let full = name.starts_with("full-");
                if full {
                    member.receipt = [255; 32];
                }
                if exact_implementation {
                    header.mode = SelectionMode::Exact;
                    header.exact_receipt = member.receipt;
                }
                let mut members = vec![member.clone()];
                if full {
                    header.member_count = 32;
                    for index in 1..32 {
                        let mut sibling = member.clone();
                        sibling.receipt = [index; 32];
                        members.push(sibling);
                    }
                }
                let set = AuthorizationSet::new(header, &members).unwrap();
                let policy = InstancePolicy {
                    implementation_root: set.root(),
                    mode,
                    args_length: args.len() as u16,
                    config_length: config.len() as u16,
                    constraints: constraints.clone(),
                };
                if late_invalid {
                    args[255] = 0x42;
                }
                let script = original_script.as_builder().args(Bytes::from(args.clone()).pack()).build();
                let committed = ConsumerArgs {
                    policy_root: policy.root().unwrap(),
                    subject: script.calc_script_hash().unpack(),
                    config: config.clone(),
                }
                .encode()
                .unwrap();
                let selection = instance_policy::encode_selection(&policy, &set.selection(&member.receipt).unwrap(), &args).unwrap();
                assert_eq!(instance_policy::verify_selection(&committed, &selection).is_ok(), !late_invalid);
                let mut fixture = open_handle_from_args_fixture(&compiled, committed.to_vec(), selection.to_vec(), &script);
                fixture.cell_deps[0].data = ckb_testtool::builtin::ALWAYS_SUCCESS.clone();
                fixture.inputs.push(FixtureCell { capacity: 100_000_000_000, type_script: Some(script.clone()), data: Bytes::new() });
                fixture.witnesses.push(Bytes::new());
                if full {
                    // Two harness dependencies supplement these 62 Cells.
                    for index in 1..62u8 {
                        fixture.cell_deps.push(FixtureCell {
                            capacity: 100_000_000_000,
                            type_script: None,
                            data: Bytes::from(vec![index]),
                        });
                    }
                }
                let execution = execute_cellscript_script(elf, &fixture);
                assert_eq!(
                    execution.exit_code,
                    if late_invalid { 85 } else { 0 },
                    "{name} O{opt} exact={exact_implementation}: {:?}",
                    execution.captured_debug
                );
                assert!(execution.cycles <= 1_000_000, "{name}: {}", execution.cycles);
                if full || ((name == "maximum-Constrained" || late_invalid) && !exact_implementation) {
                    let (observed, trace) = ckb_script_runner::execute_cellscript_script_with_observer(
                        elf,
                        &fixture,
                        |tx, _| tx,
                        |context, tx, script| {
                            if full {
                                assert_eq!(tx.cell_deps().len(), 64);
                            }
                            (cost_trace::measure(context, tx, script, ScriptGroupType::Type, 3_000_000), tx.data().as_slice().len())
                        },
                    );
                    let (trace, transaction_bytes) = trace;
                    assert_eq!(observed.exit_code, execution.exit_code);
                    assert_eq!(trace.status, "measured", "{trace:?}");
                    assert_eq!(trace.authoritative_cycles, trace.stepped_cycles);
                    assert_eq!(trace.exit_code.map(i64::from), Some(execution.exit_code));
                    let group_cycles = trace.authoritative_cycles.unwrap();
                    assert!(group_cycles <= if full { 275_000 } else { 205_000 }, "{name}: group cycles={group_cycles}");
                    let stack = trace.vms.iter().map(|vm| vm.observed_stack_bytes).max().unwrap();
                    assert!(stack <= 10_240, "{name}: observed stack={stack}");
                    assert!(trace.vms.iter().all(|vm| vm.touched_guest_page_bytes <= 32_768), "{name}: guest-page budget");
                    assert!(transaction_bytes <= 4_800, "{name}: transaction-byte budget");
                    assert!(elf.len() <= 16_384, "instance helper ELF exceeded the frozen fixture budget");
                    eprintln!(
                        "instance-bound O{opt} {name}: ELF={} witness={} group_cycles={group_cycles} observed_stack={stack}",
                        elf.len(),
                        selection.len() + 8
                    );
                    let phoff = u64::from_le_bytes(elf[32..40].try_into().unwrap()) as usize;
                    let phsize = u16::from_le_bytes(elf[54..56].try_into().unwrap()) as usize;
                    let count = u16::from_le_bytes(elf[56..58].try_into().unwrap()) as usize;
                    let load_segment_bytes: u64 = (0..count)
                        .filter_map(|index| {
                            let header = &elf[phoff + index * phsize..phoff + (index + 1) * phsize];
                            (u32::from_le_bytes(header[..4].try_into().unwrap()) == 1)
                                .then(|| u64::from_le_bytes(header[40..48].try_into().unwrap()))
                        })
                        .sum();
                    resource_records.push(serde_json::json!({
                        "case": name, "optimization": opt, "exact_implementation": exact_implementation,
                        "members": set.members().len(), "direct_dependencies": if full { 64 } else { 3 },
                        "args_bytes": args.len(), "config_bytes": config.len(), "constraints": constraints.len(),
                        "group_cycles": group_cycles, "observed_stack_bytes": stack, "elf_bytes": elf.len(),
                        "elf_load_segment_bytes": load_segment_bytes, "vm_memory_limit_bytes": ckb_vm::RISCV_MAX_MEMORY,
                        "touched_guest_page_bytes": trace.vms.iter().map(|vm| vm.touched_guest_page_bytes).collect::<Vec<_>>(),
                        "memory_measurement": "successful guest memory initialization/read/write page union per VM; not host heap/RSS",
                        "witness_payload_bytes": selection.len() + 8, "transaction_bytes": transaction_bytes,
                        "artifact_hash": hex::encode(blake2b_256(elf)), "trace": trace,
                    }));
                }
                if args.len() == 256 && !late_invalid {
                    let mut oversized = fixture.clone();
                    let mut args = args.clone();
                    args.push(0);
                    oversized.inputs[1].type_script = Some(script.as_builder().args(Bytes::from(args).pack()).build());
                    assert_eq!(execute_cellscript_script(elf, &oversized).exit_code, 85, "257-byte actual args must reject");
                }
            }
        }
    }
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/instance-policy-evidence/resources");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema": "cellscript-instance-resource-evidence-v1", "scope": "isolated-admitting-group; always-success peer",
            "records": resource_records,
        }))
        .unwrap(),
    )
    .unwrap();
}
