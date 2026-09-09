//! Maximum-shape resource evidence for the frozen 0.30 cryptographic portfolio.

use cellscript::{
    compile_path_with_executable_surface_policy, compile_with_executable_surface_policy, strip_vm_abi_trailer, CellScriptEdition,
    CompileEntryScope, CompileOptions, EntryWitnessArg, ExecutableSurfacePolicy,
};
use ckb_sdk::{types::ScriptGroup, unlock::generate_message};
use ckb_testtool::{
    builtin::ALWAYS_SUCCESS,
    ckb_hash::blake2b_256,
    ckb_types::{bytes::Bytes, core::TransactionView, packed, prelude::*},
};
use sha2::{Digest, Sha256};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;
#[path = "support/cryptographic_resource_budgets.rs"]
mod cryptographic_resource_budgets;

use ckb_script_runner::{
    build_simple_fixture, execute_cellscript_script, execute_cellscript_script_with_transaction_transform, CkbScriptExecutionResult,
    FixtureCell,
};
use cryptographic_resource_budgets::{assert_metrics, manifest, ResourceMetrics};

const HANDLE_BYTES: usize = 202;

fn byte_string(bytes: &[u8]) -> String {
    let escaped = bytes.iter().map(|byte| format!("\\x{byte:02x}")).collect::<String>();
    format!("b\"{escaped}\"")
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
    .unwrap_or_else(|error| panic!("resource fixture must compile: {error}\n{source}"))
}

fn max_stack_frame_bytes(result: &cellscript::CompileResult) -> u32 {
    result
        .verified_lowering_record
        .iter()
        .flat_map(|record| &record.entries)
        .map(|entry| entry.frame_size_bytes)
        .max()
        .expect("verified lowering record contains an entry")
}

fn metrics(result: &cellscript::CompileResult, execution: &CkbScriptExecutionResult) -> ResourceMetrics {
    ResourceMetrics {
        cycles: execution.cycles,
        elf_bytes: strip_vm_abi_trailer(&result.artifact_bytes).len(),
        max_stack_frame_bytes: max_stack_frame_bytes(result),
        witness_bytes: execution.witness_bytes,
        transaction_bytes: execution.transaction_bytes,
        dependency_bytes: execution.dependency_bytes,
    }
}

fn canonical_script_hash_max_args() -> ResourceMetrics {
    let code_hash = [0x5a; 32];
    let args = (0..cellscript::CKB_SCRIPT_HASH_MAX_ARGS_BYTES).map(|index| ((index * 73 + 19) & 0xff) as u8).collect::<Vec<_>>();
    let script = packed::Script::new_builder()
        .code_hash(code_hash.pack())
        .hash_type(ckb_testtool::ckb_types::core::ScriptHashType::Data2)
        .args(Bytes::copy_from_slice(&args).pack())
        .build();
    let expected: [u8; 32] = script.calc_script_hash().unpack();
    let source = format!(
        r#"
module resource_budget::script_hash

action verify(witness expected: ScriptHash) -> u64 {{
    let script = script::new(
        Hash::from_bytes({code_hash}),
        script::hash_type_data2(),
        script::args({args})
    )
    require script::hash(script) == expected
    return 0
}}
"#,
        code_hash = byte_string(&code_hash),
        args = byte_string(&args),
    );
    let result = compile(&source);
    let payload =
        result.metadata.actions[0].entry_witness_args(&[EntryWitnessArg::Hash(expected)]).expect("encode expected Script hash");
    let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes();
    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.witnesses = vec![witness];
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &fixture);
    assert_eq!(execution.exit_code, 0, "maximum Script args fixture failed: {:?}", execution.captured_debug);
    metrics(&result, &execution)
}

fn bounded_witness_blake2b_max_bytes() -> ResourceMetrics {
    let lock = (0..65_536).map(|index| ((index * 29 + index / 251 + 7) & 0xff) as u8).collect::<Vec<_>>();
    let expected = blake2b_256(&lock);
    let source = format!(
        r#"
module resource_budget::bounded_witness

action verify() -> u64 {{
    let bytes = witness::bounded_lock(witness::args(0), 65536)
    require bytes.size == 65536
    require witness::byte(bytes, 0) == {first}
    require witness::byte(bytes, 65535) == {last}
    require witness::blake2b(bytes) == Hash::from_bytes({expected})
    return 0
}}
"#,
        first = lock[0],
        last = lock[65_535],
        expected = byte_string(&expected),
    );
    let result = compile(&source);
    let witness = packed::WitnessArgs::new_builder().lock(Some(Bytes::from(lock)).pack()).build().as_bytes();
    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.witnesses = vec![witness];
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &fixture);
    assert_eq!(execution.exit_code, 0, "maximum bounded witness fixture failed: {:?}", execution.captured_debug);
    metrics(&result, &execution)
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn sha256d(bytes: &[u8]) -> [u8; 32] {
    sha256(&sha256(bytes))
}

fn sha256d_merkle_max_depth() -> ResourceMetrics {
    let leaf = std::array::from_fn::<_, 32, _>(|index| (index * 13 + 5) as u8);
    let siblings =
        std::array::from_fn::<[u8; 32], 16, _>(|depth| std::array::from_fn(|index| ((depth * 41 + index * 17 + 3) & 0xff) as u8));
    let leaf_index = 0xa55a_u64;
    let mut root = leaf;
    for (depth, sibling) in siblings.iter().enumerate() {
        let mut pair = Vec::with_capacity(64);
        if (leaf_index >> depth) & 1 == 0 {
            pair.extend_from_slice(&root);
            pair.extend_from_slice(sibling);
        } else {
            pair.extend_from_slice(sibling);
            pair.extend_from_slice(&root);
        }
        root = sha256d(&pair);
    }
    let mut first_pair = Vec::with_capacity(64);
    first_pair.extend_from_slice(&leaf);
    first_pair.extend_from_slice(&siblings[0]);
    let expected_pair = sha256d(&first_pair);
    let source = format!(
        r#"
module resource_budget::sha_merkle

action verify(
    witness leaf: Hash,
    witness other: Hash,
    witness siblings: [Hash; 16],
    witness expected_sha256: Hash,
    witness expected_sha256d: Hash,
    witness expected_pair: Hash,
    witness expected_root: Hash,
) -> u64 {{
    require ckb::hash_sha256(leaf) == expected_sha256
    require ckb::hash_sha256d(leaf) == expected_sha256d
    require ckb::hash_sha256d_pair(leaf, other) == expected_pair
    ckb::require_sha256d_merkle_root(leaf, siblings, 16, {leaf_index}, expected_root)
    return 0
}}
"#
    );
    let result = compile(&source);
    let mut payload = b"CSARGv1\0".to_vec();
    payload.extend_from_slice(&leaf);
    payload.extend_from_slice(&siblings[0]);
    payload.extend(siblings.iter().flatten());
    payload.extend_from_slice(&sha256(&leaf));
    payload.extend_from_slice(&sha256d(&leaf));
    payload.extend_from_slice(&expected_pair);
    payload.extend_from_slice(&root);
    let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes();
    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.witnesses = vec![witness];
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &fixture);
    assert_eq!(execution.exit_code, 0, "maximum SHA256d Merkle fixture failed: {:?}", execution.captured_debug);
    metrics(&result, &execution)
}

fn install_max_shape_sdk_message(tx: TransactionView, script: packed::Script) -> TransactionView {
    let mut group = ScriptGroup::from_type_script(&script);
    group.input_indices = (0..64).collect();
    let message = generate_message(&tx, &group, Bytes::from(vec![0u8; 32])).expect("ckb-sdk-rust maximum-shape message");
    let first =
        packed::WitnessArgs::from_slice(tx.witnesses().get(0).expect("first witness").raw_data().as_ref()).expect("first WitnessArgs");
    let first = first.as_builder().lock(Some(message).pack()).build();
    let mut witnesses: Vec<packed::Bytes> = tx.witnesses().into_iter().collect();
    witnesses[0] = first.as_bytes().pack();
    tx.as_advanced_builder().set_witnesses(witnesses).build()
}

fn zero_lock_sighash_max_shape() -> ResourceMetrics {
    let source = r#"
module resource_budget::sighash

action verify() -> u64 {
    let digest = env::sighash_all_zero_lock(64, 256, 64, 65536)
    require Hash::from_sighash_all(digest) == witness::args(0).lock
    return 0
}
"#;
    let result = compile(source);
    let first = packed::WitnessArgs::new_builder().lock(Some(Bytes::from(vec![0u8; 32])).pack()).build().as_bytes();
    let mut fixture = build_simple_fixture(Bytes::default(), 256, 1);
    fixture.current_type_script_input_indices = (0..64).collect();
    fixture.witnesses = (0..320)
        .map(|index| {
            if index == 0 {
                first.clone()
            } else if index == 319 {
                Bytes::from(vec![0x5a; 65_536])
            } else {
                Bytes::default()
            }
        })
        .collect();
    let execution = execute_cellscript_script_with_transaction_transform(
        strip_vm_abi_trailer(&result.artifact_bytes),
        &fixture,
        install_max_shape_sdk_message,
    );
    assert_eq!(execution.exit_code, 0, "maximum signing-message shape failed: {:?}", execution.captured_debug);
    metrics(&result, &execution)
}

fn verifier_handle(dep_hash: [u8; 32]) -> Vec<u8> {
    let mut handle = vec![0u8; HANDLE_BYTES];
    handle[..8].copy_from_slice(b"CSHDLv1\0");
    handle[8] = 1;
    handle[9] = 2;
    for (offset, value) in [(10, 0x11), (42, 0x22), (74, 0x33), (138, 0x44), (170, 0x55)] {
        handle[offset..offset + 32].fill(value);
    }
    handle[106..138].copy_from_slice(&dep_hash);
    handle
}

fn exact_script_handle_fixed_receipt() -> ResourceMetrics {
    let dep_data = Bytes::from_static(b"cellscript-0.30-resource-budget-verifier");
    let handle = verifier_handle(blake2b_256(&dep_data));
    let source = format!(
        r#"
module resource_budget::exact_handle

action verify(witness handle: ExactScriptHandle) -> u64 {{
    ckb::require_cell_dep_exact_verifier_handle(
        ckb::cell_dep(0),
        handle,
        Hash::from_bytes({commitment})
    )
    return 0
}}
"#,
        commitment = byte_string(&blake2b_256(&handle)),
    );
    let result = compile(&source);
    let payload =
        result.metadata.actions[0].entry_witness_args(&[EntryWitnessArg::Bytes(handle)]).expect("encode exact Script handle");
    let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes();
    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.witnesses = vec![witness];
    fixture.cell_deps = vec![FixtureCell { capacity: 100_000_000_000, type_script: None, data: dep_data }];
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &fixture);
    assert_eq!(execution.exit_code, 0, "exact-handle resource fixture failed: {:?}", execution.captured_debug);
    metrics(&result, &execution)
}

fn trusted_external_fixed_exec() -> ResourceMetrics {
    let hash = cellscript_artifact_checker::ckb_blake2b256(ALWAYS_SUCCESS.as_ref());
    let source = format!(
        r#"
module resource_budget::trusted_exec

action verify() -> u64 {{
    ckb::trusted_exec_cell_dep_u8_args(
        0,
        Hash::from_bytes({hash}),
        0,
        0,
        0,
        0,
        0
    )
    require false
    return 0
}}
"#,
        hash = byte_string(&hash),
    );
    let directory = tempfile::tempdir().expect("temporary resource package");
    let root = camino::Utf8Path::from_path(directory.path()).expect("UTF-8 temporary path");
    let manager = cellscript::package::PackageManager::new(directory.path());
    manager.init("resource_budget_trusted_exec").expect("initialize resource package");
    let mut package = manager.read_manifest().expect("read resource package manifest");
    package.package.edition = CellScriptEdition::Edition2027;
    package.deploy.ckb = Some(cellscript::package::CkbDeployConfig {
        trusted_external_verifiers: vec![cellscript::package::CkbTrustedExternalVerifierConfig {
            schema: "cellscript-trusted-external-verifier-v1".to_string(),
            name: "resource-budget-exec".to_string(),
            scope: "action:verify".to_string(),
            operation: "exec".to_string(),
            adapter: "u8-args-v1".to_string(),
            code_hash: cellscript_artifact_checker::hex_encode(&hash),
            hash_type: "data".to_string(),
            source_identity: "ckb-testtool::builtin::ALWAYS_SUCCESS".to_string(),
            applicability: "fixed maximum-u8 adapter resource fixture".to_string(),
            trust_basis: "pinned fixture bytes and CKB-VM execution".to_string(),
            guarantees: vec!["returns zero for the fixed bounded invocation".to_string()],
        }],
        ..Default::default()
    });
    manager.write_manifest(&package).expect("write resource package manifest");
    std::fs::write(root.join("src/main.cell"), source).expect("write resource package source");
    let result = compile_path_with_executable_surface_policy(
        root,
        CompileOptions {
            edition: CellScriptEdition::Edition2027,
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..Default::default()
        },
        Some(CompileEntryScope::Action("verify".to_string())),
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .expect("compile exact trusted-external resource package");
    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.cell_deps = vec![FixtureCell { capacity: 100_000_000_000, type_script: None, data: ALWAYS_SUCCESS.clone() }];
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &fixture);
    assert_eq!(execution.exit_code, 0, "trusted-external resource fixture failed: {:?}", execution.captured_debug);
    metrics(&result, &execution)
}

#[test]
fn maximum_cryptographic_shapes_match_recorded_resource_budgets() {
    let budget_manifest = manifest();
    assert_eq!(budget_manifest.schema, "cellscript-cryptographic-resource-budgets-v1");
    assert_eq!(
        budget_manifest.measurement_backend,
        "ckb-testtool-full-transaction-context plus pinned NovaSeal child-verifier CKB-VM and ckb-system-scripts multisig-v2"
    );
    assert!(budget_manifest
        .profiles
        .iter()
        .all(|profile| !profile.capabilities.is_empty() && !profile.maximum_shape.is_empty() && !profile.evidence.is_empty()));
    let observed = [
        ("canonical-script-hash-max-args", canonical_script_hash_max_args()),
        ("bounded-witness-blake2b-max-bytes", bounded_witness_blake2b_max_bytes()),
        ("sha256d-merkle-max-depth", sha256d_merkle_max_depth()),
        ("zero-lock-sighash-max-shape", zero_lock_sighash_max_shape()),
        ("exact-script-handle-fixed-receipt", exact_script_handle_fixed_receipt()),
        ("trusted-external-fixed-exec", trusted_external_fixed_exec()),
    ];
    for (id, actual) in &observed {
        eprintln!("{id}={}", serde_json::to_string(actual).expect("serialize observed resource metrics"));
    }
    for (id, actual) in &observed {
        assert_metrics(id, actual);
    }
}
