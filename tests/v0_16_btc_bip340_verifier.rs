use camino::Utf8Path;
use cellscript::{compile, CompileOptions};
use tempfile::tempdir;

const BTC_BIP340_VERIFIER_SOURCE: &str = r#"
module cellscript::btc_bip340_verifier_surface

struct SignaturePayload {
    pubkey: [u8; 32],
    signature: [u8; 64],
}

lock guard(witness message: Hash, witness sig: SignaturePayload) -> bool {
    verifier::btc::bip340::require_signature(message, sig.pubkey, sig.signature)
    true
}
"#;

#[test]
fn btc_bip340_verifier_surface_lowers_to_generic_spawn_ipc() {
    let result = compile(
        BTC_BIP340_VERIFIER_SOURCE,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("btc bip340 verifier helper should compile");
    let lock = result.metadata.locks.iter().find(|lock| lock.name == "guard").expect("guard lock metadata");

    assert!(lock.ckb_runtime_features.iter().any(|feature| feature == "ckb-spawn-ipc"));
    assert!(lock.ckb_runtime_features.iter().any(|feature| feature == "runtime-verifier:btc-bip340:signature"));
    assert!(lock.ckb_runtime_accesses.iter().any(|access| {
        access.operation == "spawn-with-fd"
            && access.syscall == "SPAWN"
            && access.source == "CellDep"
            && access.binding.starts_with("spawn-target-tag:0x")
    }));
    assert!(lock.ckb_runtime_accesses.iter().any(|access| {
        access.operation == "runtime-verifier-btc-bip340"
            && access.binding.contains("btc.bip340.v0")
            && access.binding.contains("cellscript_btc_bip340_verifier_riscv")
    }));
    assert!(lock
        .verifier_obligations
        .iter()
        .any(|obligation| { obligation.category == "runtime-verifier" && obligation.feature == "verifier:btc-bip340:signature" }));
    assert!(lock.proof_plan.iter().any(|plan| {
        plan.category == "runtime-verifier" && plan.feature == "verifier:btc-bip340:ipc-envelope" && plan.on_chain_checked
    }));
    assert!(
        !lock.proof_plan.iter().any(|plan| plan.detail.contains("NovaSeal") || plan.detail.contains("novaseal")),
        "generic verifier proof plan should not mention NovaSeal"
    );

    let assembly = String::from_utf8(result.artifact_bytes).expect("assembly should be utf-8");
    assert!(assembly.contains("call __ckb_spawn_with_fd1"), "verifier should spawn the generic RISC-V helper:\n{assembly}");
    assert_eq!(
        assembly.matches("\n    call __ckb_pipe_write").count(),
        18,
        "BIP340 verifier envelope should emit exactly 18 u64 writes:\n{assembly}"
    );
    assert!(
        assembly.contains("# cellscript abi: fixed_u64_le word=7 offset=56 width=64"),
        "signature word extraction should stay in generic fixed-byte lowering:\n{assembly}"
    );
}

#[test]
fn strict_0_16_accepts_manifest_bound_btc_bip340_verifier() {
    let dir = tempdir().unwrap();
    let root = Utf8Path::from_path(dir.path()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "btc_bip340_verifier_bound"
version = "0.1.0"
entry = "src/main.cell"

[build]
target_profile = "ckb"

[[deploy.ckb.cell_deps]]
name = "cellscript_btc_bip340_verifier_riscv"
role = "runtime_verifier"
verifier_id = "btc-bip340"
ipc_abi = "cellscript-btc-bip340-ipc-v0"
artifact_hash = "0x5555555555555555555555555555555555555555555555555555555555555555"
out_point = "0x4444444444444444444444444444444444444444444444444444444444444444:0"
dep_type = "code"
hash_type = "data1"
data_hash = "0x6666666666666666666666666666666666666666666666666666666666666666"
"#,
    )
    .unwrap();
    std::fs::write(root.join("src/main.cell"), BTC_BIP340_VERIFIER_SOURCE).unwrap();

    let result = cellscript::compile_path(
        root,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.16".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("manifest-bound btc bip340 verifier should satisfy strict 0.16");

    let spawn_plan =
        result.metadata.runtime.proof_plan.iter().find(|plan| plan.category == "spawn-target").expect("spawn target ProofPlan");
    assert_eq!(spawn_plan.status, "builder-required");
    assert!(spawn_plan.detail.contains("cellscript_btc_bip340_verifier_riscv"), "{spawn_plan:#?}");
    let declared_dep = result
        .metadata
        .constraints
        .ckb
        .as_ref()
        .expect("CKB constraints")
        .dep_group_manifest
        .declared_cell_deps
        .iter()
        .find(|dep| dep.name == "cellscript_btc_bip340_verifier_riscv")
        .expect("declared verifier dep");
    assert_eq!(declared_dep.role.as_deref(), Some("runtime_verifier"));
    assert_eq!(declared_dep.verifier_id.as_deref(), Some("btc-bip340"));
    assert_eq!(declared_dep.ipc_abi.as_deref(), Some("cellscript-btc-bip340-ipc-v0"));
    assert_eq!(declared_dep.artifact_hash.as_deref(), Some("0x5555555555555555555555555555555555555555555555555555555555555555"));
    assert!(result.metadata.runtime.proof_plan.iter().any(|plan| {
        plan.category == "runtime-verifier" && plan.feature == "verifier:btc-bip340:signature" && plan.status == "builder-required"
    }));
}

#[test]
fn btc_bip340_verifier_surface_rejects_wrong_argument_widths() {
    let err = compile(
        r#"
module cellscript::bad_btc_bip340_verifier_surface

action bad(message: Hash, witness pubkey: [u8; 33], witness signature: [u8; 64]) -> bool
where
    verifier::btc::bip340::require_signature(message, pubkey, signature)
    true
"#,
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap_err();
    assert!(
        err.message
            .contains("verifier::btc::bip340::require_signature expects (message: Hash, pubkey: [u8; 32], signature: [u8; 64])"),
        "unexpected error: {}",
        err.message
    );
}
