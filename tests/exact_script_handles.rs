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
