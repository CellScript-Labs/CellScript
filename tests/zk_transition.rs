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
