//! Real CKB scheduler evidence with a deterministic, non-authorizing circuit.
use super::{
    tests::{fixture, fixture_for},
    wire,
};
use cellscript::protocol_bundle::*;
use cellscript::script_handle::{build_exact_script_handle, ExactScriptHandleReceiptInput};
use cellscript::{compile, CompileOptions, EntryWitnessArg};
use ckb_testtool::{
    builtin::ALWAYS_SUCCESS,
    ckb_hash::blake2b_256,
    ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*},
    context::Context,
};

fn literal(bytes: &[u8]) -> String {
    format!("Hash::from_bytes(b\"{}\")", bytes.iter().map(|byte| format!("\\x{byte:02x}")).collect::<String>())
}

#[test]
fn exact_parent_child_scheduler_binds_transaction_and_state() {
    let mut context = Context::new_with_deterministic_rng();
    let child = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/riscv64imac-unknown-none-elf/release/cellscript-zk-transition-verifier"),
    )
    .unwrap();
    let child_hash = blake2b_256(&child);
    let child_bytes = child.len();
    let child_out = context.deploy_cell(Bytes::from(child));
    let (key, _) = fixture();
    let key_hash = blake2b_256(&key);
    let key_bytes = key.len();
    let key_out = context.deploy_cell(Bytes::from(key));
    let code_hash = hex::encode(child_hash);
    let profile_hash = hex::encode(wire::PROFILE_ID);
    let entry = ProtocolEntryIdentity { kind: ProtocolEntryKind::Action, name: "verify".into() };
    let deployment = ProtocolDeploymentIdentity {
        network: ProtocolNetworkIdentity { chain_id: "ckb-testnet".into(), genesis_hash: format!("0x{}", "11".repeat(32)) },
        artifact_hash: code_hash.clone(),
        script: ProtocolScriptIdentity { code_hash: format!("0x{code_hash}"), hash_type: "data".into(), args: "0x".into() },
        code_cell_dep: ProtocolCellDep {
            out_point: ProtocolOutPoint {
                tx_hash: format!("0x{}", hex::encode(child_out.tx_hash().as_slice())),
                index: child_out.index().unpack(),
            },
            dep_type: ProtocolDepType::Code,
        },
    };
    let (_, handle) = build_exact_script_handle(ExactScriptHandleReceiptInput {
        package_coordinate: "test/zk-verifier@0.32.0",
        lock_node_id: "zk-verifier-test",
        entry: &entry,
        script_role: ProtocolScriptRole::SpawnedVerifier,
        interface_hash: &profile_hash,
        typed_semantics_hash: &profile_hash,
        artifact_hash: &code_hash,
        target_profile_hash: &profile_hash,
        runtime_abi_hash: &profile_hash,
        verified_bundle_id: &profile_hash,
        deployment: &deployment,
    })
    .unwrap();
    let handle = hex::decode(handle.encoded.trim_start_matches("0x")).unwrap();
    let source = format!(
        r#"module zk_scheduler
action update(witness proof: ZkTransitionProof, witness verifier: ExactScriptHandle) -> u64 {{
    verification
    zk::require_valid("transition", proof, ckb::cell_dep(0), verifier, {}, {}, {}, {})
    return 0
}}
"#,
        literal(&blake2b_256(&handle)),
        literal(&key_hash).replace("Hash::", "VerificationKeyCommitment::"),
        literal(&[1; 32]),
        literal(&[2; 32])
    );
    let compiled = compile(
        &source,
        CompileOptions { target: Some("riscv64-elf".into()), target_profile: Some("ckb".into()), ..Default::default() },
    )
    .unwrap();
    compiled.validate().unwrap();
    let parent = context.deploy_cell(Bytes::copy_from_slice(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes)));
    let lock_out = context.deploy_cell(ALWAYS_SUCCESS.clone());
    let lock = context.build_script(&lock_out, Bytes::new()).unwrap();
    let script = context.build_script(&parent, Bytes::new()).unwrap();
    let cell = packed::CellOutput::new_builder().capacity(100_000_000_000u64).lock(lock).type_(Some(script.clone()).pack()).build();
    let old = Bytes::from_static(b"old-state");
    let new = Bytes::from_static(b"new-state");
    let input = packed::OutPoint::new_builder().tx_hash([0x32u8; 32]).index(0u32).build();
    context.create_cell_with_out_point(input.clone(), cell.clone(), old.clone());
    let dep = |out| packed::CellDep::new_builder().out_point(out).build();
    let transaction = context.complete_tx(
        TransactionBuilder::default()
            .input(packed::CellInput::new_builder().previous_output(input.clone()).build())
            .output(cell.clone())
            .output_data(new.pack())
            .cell_dep(dep(child_out))
            .cell_dep(dep(key_out))
            .build(),
    );
    let statement = wire::Statement {
        domain: [1; 32],
        action: [2; 32],
        script_hash: script.calc_script_hash().as_slice().try_into().unwrap(),
        old_data_hash: blake2b_256(&old),
        new_data_hash: blake2b_256(&new),
        input_transaction_hash: input.tx_hash().as_slice().try_into().unwrap(),
        input_output_index: input.index().unpack(),
        transaction_hash: transaction.hash().as_slice().try_into().unwrap(),
    };
    let (_, request) = fixture_for(statement);
    assert_eq!(request.verification_key, key_hash);
    let witness = |proof: Vec<u8>, handle: Vec<u8>| {
        let payload =
            compiled.metadata.actions[0].entry_witness_args(&[EntryWitnessArg::Bytes(proof), EntryWitnessArg::Bytes(handle)]).unwrap();
        packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()
    };
    let transaction = transaction.as_advanced_builder().witness(witness(request.proof.to_vec(), handle.clone()).pack()).build();
    let cycles = context.verify_tx(&transaction, 300_000_000).expect("real parent/child scheduler accepts proof");
    assert!(cycles > 90_000_000 && cycles < 250_000_000);
    let mut rows =
        vec![serde_json::json!({"case":"valid","cycles":cycles,"transaction_hash":hex::encode(transaction.hash().as_slice())})];
    // Consume the exact preceding output in a second transaction. Reusing the
    // first proof must fail even though the Type Script and verifier stay fixed.
    let successor = packed::OutPoint::new_builder().tx_hash(transaction.hash()).index(0u32).build();
    context.create_cell_with_out_point(successor.clone(), cell.clone(), new.clone());
    let next_data = Bytes::from_static(b"third-state");
    let next = transaction
        .as_advanced_builder()
        .set_inputs(vec![packed::CellInput::new_builder().previous_output(successor.clone()).build()])
        .set_outputs_data(vec![next_data.pack()])
        .build();
    let replay = context.verify_tx(&next, 300_000_000).unwrap_err().to_string();
    assert!(replay.contains("error code 79"), "{replay}");
    rows.push(serde_json::json!({"case":"successor-replay","error":replay}));
    let (_, next_proof) = fixture_for(wire::Statement {
        old_data_hash: blake2b_256(&new),
        new_data_hash: blake2b_256(&next_data),
        input_transaction_hash: successor.tx_hash().as_slice().try_into().unwrap(),
        input_output_index: 0,
        transaction_hash: next.hash().as_slice().try_into().unwrap(),
        ..request.statement.clone()
    });
    let next = next.as_advanced_builder().set_witnesses(vec![witness(next_proof.proof.to_vec(), handle.clone()).pack()]).build();
    let next_cycles = context.verify_tx(&next, 300_000_000).expect("successor state transition");
    rows.push(serde_json::json!({"case":"successor-valid","cycles":next_cycles,
        "transaction_hash":hex::encode(next.hash().as_slice())}));
    let changed_output = cell.clone().as_builder().capacity(99_000_000_000u64).build();
    for (name, tx) in [
        ("output-data", transaction.as_advanced_builder().set_outputs_data(vec![Bytes::from_static(b"redirect").pack()]).build()),
        ("output-capacity", transaction.as_advanced_builder().set_outputs(vec![changed_output]).build()),
        ("extra-group-output", transaction.as_advanced_builder().output(cell).output_data(new.pack()).build()),
    ] {
        let error = context.verify_tx(&tx, 300_000_000).expect_err(name).to_string();
        let code = if name == "extra-group-output" { 74 } else { 79 };
        assert!(error.contains(&format!("error code {code}")), "{name}: {error}");
        rows.push(serde_json::json!({"case":name,"error":error}));
    }
    let mut changed_handle = handle;
    changed_handle[106] ^= 1;
    let tx = transaction.as_advanced_builder().set_witnesses(vec![witness(request.proof.to_vec(), changed_handle).pack()]).build();
    let error = context.verify_tx(&tx, 300_000_000).unwrap_err().to_string();
    assert!(error.contains("error code 70"), "{error}");
    rows.push(serde_json::json!({"case":"handle-substitution","error":error}));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/scheduler-evidence.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema":"cellscript-zk-scheduler-evidence-v1", "profile":wire::PROFILE,
            "profile_hash":hex::encode(wire::PROFILE_ID),
            "child_data_hash":hex::encode(child_hash), "child_bytes":child_bytes,
            "verification_key_data_hash":hex::encode(key_hash), "verification_key_bytes":key_bytes,
            "parent_data_hash":hex::encode(blake2b_256(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes))),
            "parent_bytes":cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes).len(),
            "request_bytes":wire::REQUEST_BYTES, "max_call_cycles":250_000_000,
            "transaction_bytes":transaction.data().as_slice().len(),
            "scope":"ckb-testtool real scheduler; deterministic non-authorizing circuit; no chain admission", "rows":rows,
        }))
        .unwrap(),
    )
    .unwrap();
}
