#[cfg(not(feature = "wasm"))]
use cellscript::{compile, CompileOptions};

fn hash(byte: u8) -> String {
    format!("Hash::from_bytes(b\"{}\")", format!("\\x{byte:02x}").repeat(32))
}

fn source() -> String {
    format!(
        r#"module zk_example
action update(witness proof: ZkTransitionProof, witness verifier: ExactScriptHandle) -> u64 {{
    verification
    zk::require_valid("transition", proof, ckb::cell_dep(0), verifier, {}, {}, {}, {})
    return 0
}}
"#,
        hash(1),
        hash(2).replace("Hash::", "VerificationKeyCommitment::"),
        hash(3),
        hash(4)
    )
}

#[test]
#[cfg(not(feature = "wasm"))]
fn exact_zk_parent_compiles_and_checks() {
    let parsed = cellscript::frontend::parse(&source(), "2026".parse().unwrap()).unwrap();
    let formatted = cellscript::fmt::format_default(&parsed).unwrap();
    compile(
        &formatted,
        CompileOptions { target: Some("riscv64-elf".into()), target_profile: Some("ckb".into()), ..Default::default() },
    )
    .unwrap()
    .validate()
    .unwrap();
    let compiled = compile(
        &source(),
        CompileOptions { target: Some("riscv64-elf".into()), target_profile: Some("ckb".into()), ..Default::default() },
    )
    .expect("compile exact ZK parent");
    compiled.validate().expect("check exact ZK parent");
    assert_eq!(compiled.metadata.actions[0].params[0].ty, "ZkTransitionProof");
    let contract = &compiled.metadata.runtime.zk_verifiers[0];
    assert_eq!(contract.policy, "transition");
    assert_eq!(contract.request_bytes, 464);
    assert_eq!(contract.public_input_count, 15);
    assert!(compiled.metadata.actions[0].proof_plan.iter().any(|plan| plan.category == "cryptographic-verifier-call"));
    assert!(compiled.metadata.constraints.runtime_errors.iter().any(|error| error.code == 74));
    assert!(compiled.metadata.constraints.runtime_errors.iter().any(|error| error.code == 84));
    let ordinary = compile(
        "module ordinary\naction update() -> u64 { verification\nreturn 0\n}",
        CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() },
    )
    .unwrap();
    assert!(!ordinary.metadata.constraints.runtime_errors.iter().any(|error| error.name.starts_with("zk-")));
}

#[test]
#[cfg(not(feature = "wasm"))]
fn zk_rejects_erased_proofs_witness_authority_and_ignored_booleans() {
    let original = source();
    let call = original.lines().find(|line| line.contains("zk::require_valid")).unwrap();
    let cases = [
        original.replace("proof: ZkTransitionProof", "proof: [u8; 128]"),
        original.replace("VerificationKeyCommitment::", "Hash::"),
        original.replace(call, &format!("    require {}", call.trim())),
        original.replace(call, &format!("{call}\n{call}")),
        original
            .replace("witness verifier:", "witness flag: bool, witness verifier:")
            .replace(call, &format!("    if flag {{\n{call}\n    }}")),
        original.replace("witness verifier:", "witness chosen: Hash, witness verifier:").replace(&hash(1), "chosen"),
        format!("{original}\naction wrapper(witness proof: ZkTransitionProof, witness verifier: ExactScriptHandle) -> u64 {{\nverification\nreturn update(proof, verifier)\n}}\n"),
    ];
    for source in cases {
        assert!(
            compile(
                &source,
                CompileOptions { target: Some("riscv64-elf".into()), target_profile: Some("ckb".into()), ..Default::default() }
            )
            .is_err(),
            "accepted invalid ZK source: {source}"
        );
    }
}

#[test]
fn zk_metadata_preserves_exact_contract_and_source_origins() {
    use cellscript_artifact_checker::zk::{PROFILE_ID, PROFILE_PREIMAGE};
    use sha2::{Digest, Sha256};
    assert_eq!(Sha256::digest(PROFILE_PREIMAGE.as_bytes()).as_slice(), PROFILE_ID);
    let metadata = cellscript::compile_metadata(&source(), "2026".parse().unwrap(), None).unwrap();
    let contract = &metadata.runtime.zk_verifiers[0];
    assert_eq!(contract.policy, "transition");
    assert_eq!(contract.verification_key_hash, "02".repeat(32));
    assert_eq!(contract.request_bytes, 464);
    assert_eq!(contract.public_input_count, 15);
    assert_eq!(contract.source.fields, cellscript_artifact_checker::zk_profile::statement_origins());
    assert!(source()[contract.source.start..contract.source.end].contains("zk::require_valid"));
}

#[test]
#[cfg(not(feature = "wasm"))]
fn named_package_policy_binds_receipt_key_and_dependency_before_codegen() {
    use cellscript::{package::*, protocol_bundle::*, script_handle::*, zk_package::NamedZkVerifier};
    let profile = hex::encode(cellscript_artifact_checker::zk::PROFILE_ID);
    let deployment = ProtocolDeploymentIdentity {
        network: ProtocolNetworkIdentity { chain_id: "ckb-testnet".into(), genesis_hash: format!("0x{}", "11".repeat(32)) },
        artifact_hash: "22".repeat(32),
        script: ProtocolScriptIdentity { code_hash: format!("0x{}", "22".repeat(32)), hash_type: "data2".into(), args: "0x".into() },
        code_cell_dep: ProtocolCellDep {
            out_point: ProtocolOutPoint { tx_hash: format!("0x{}", "33".repeat(32)), index: 4 },
            dep_type: ProtocolDepType::Code,
        },
    };
    let entry = ProtocolEntryIdentity { kind: ProtocolEntryKind::Action, name: "verify".into() };
    let (receipt, handle) = build_exact_script_handle(ExactScriptHandleReceiptInput {
        package_coordinate: "test/verifier@0.32.0",
        lock_node_id: "verifier",
        entry: &entry,
        script_role: ProtocolScriptRole::SpawnedVerifier,
        interface_hash: &profile,
        typed_semantics_hash: &profile,
        artifact_hash: &deployment.artifact_hash,
        target_profile_hash: &profile,
        runtime_abi_hash: &profile,
        verified_bundle_id: &profile,
        deployment: &deployment,
    })
    .unwrap();
    let literal = format!(
        "Hash::from_bytes(b\"{}\")",
        hex::decode(exact_script_handle_value_hash(&handle).unwrap())
            .unwrap()
            .iter()
            .map(|b| format!("\\x{b:02x}"))
            .collect::<String>()
    );
    let source = source().replace(&hash(1), &literal);
    let deploy = CkbDeployConfig {
        cell_deps: vec![CkbCellDepConfig {
            name: Some("groth16".into()),
            tx_hash: Some(deployment.code_cell_dep.out_point.tx_hash),
            index: Some(4),
            data_hash: Some("22".repeat(32)),
            ..Default::default()
        }],
        zk_verifiers: vec![NamedZkVerifier {
            policy: "transition".into(),
            dependency: "groth16".into(),
            verification_key_hash: "02".repeat(32),
            receipt,
        }],
        ..Default::default()
    };
    let options = CompileOptions { target: Some("riscv64-elf".into()), target_profile: Some("ckb".into()), ..Default::default() };
    cellscript::compile_with_zk_deploy(&source, options.clone(), &deploy).unwrap().validate().unwrap();
    for mutation in 0..8 {
        let mut changed = deploy.clone();
        match mutation {
            0 => changed.zk_verifiers.clear(),
            1 => changed.zk_verifiers[0].verification_key_hash = "ff".repeat(32),
            2 => changed.cell_deps[0].index = Some(5),
            3 => changed.cell_deps[0].data_hash = Some("ff".repeat(32)),
            4 => changed.zk_verifiers.push(changed.zk_verifiers[0].clone()),
            5 => changed.cell_deps.push(changed.cell_deps[0].clone()),
            6 => changed.cell_deps[0].dep_type = Some("dep_group".into()),
            7 => changed.zk_verifiers[0].receipt.runtime_abi_hash = "ff".repeat(32),
            _ => unreachable!(),
        }
        assert!(cellscript::compile_with_zk_deploy(&source, options.clone(), &changed).is_err(), "mutation {mutation}");
    }
    assert!(
        cellscript::compile_with_zk_deploy(&source.replace("ckb::cell_dep(0)", "ckb::cell_dep(1)"), options.clone(), &deploy).is_err()
    );
    // Exercise actual Cell.toml loading and the metadata/LSP path as well.
    let dir = tempfile::tempdir().unwrap();
    let root = camino::Utf8Path::from_path(dir.path()).unwrap();
    std::fs::write(root.join("main.cell"), &source).unwrap();
    let document = format!(
        "[package]\nname = \"zk_package\"\nversion = \"0.1.0\"\nedition = \"2026\"\nentry = \"main.cell\"\n\n{}",
        toml::to_string(&DeployConfig { ckb: Some(deploy) }).unwrap().replace("[ckb", "[deploy.ckb")
    );
    std::fs::write(root.join("Cell.toml"), document).unwrap();
    cellscript::compile_path(root, options.clone()).unwrap().validate().unwrap();
    let report = cellscript::compile_path_metadata_with_diagnostics(root, options);
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
}
