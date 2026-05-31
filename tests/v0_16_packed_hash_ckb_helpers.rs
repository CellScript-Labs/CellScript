use camino::Utf8Path;
use cellscript::{compile, compile_path, CompileOptions};
use tempfile::tempdir;

#[test]
fn hash_blake2b_packed_uses_canonical_type_domain_and_declared_field_order() {
    let result = compile(
        r#"
module packed_hash_surface

struct Pair {
    first: u64,
    second: u64,
}

struct SameLayout {
    first: u64,
    second: u64,
}

action hash_values() -> (Hash, Hash)
where
    let pair = Pair { second: 2, first: 1 }
    let same = SameLayout { first: 1, second: 2 }
    return (hash_blake2b_packed(pair), hash_blake2b_packed(same))
"#,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            target: Some("riscv64-asm".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("fixed-width packed hash surface should compile");

    let assembly = String::from_utf8(result.artifact_bytes).expect("assembly should be utf-8");
    assert!(assembly.contains("hash_blake2b_packed type=Pair packed_len=16 preimage_len=48"), "{assembly}");
    assert!(assembly.contains("hash_blake2b_packed type=SameLayout packed_len=16 preimage_len=54"), "{assembly}");
    assert!(!assembly.contains("fail closed because hash_blake2b_packed input"), "{assembly}");

    let pair_hash = assembly.find("hash_blake2b_packed type=Pair").expect("Pair hash marker");
    assert!(assembly.contains("__cellscript_const_data_0:\n    .byte 1"), "{assembly}");
    assert!(assembly.contains("__cellscript_const_data_1:\n    .byte 2"), "{assembly}");
    let first_copy = assembly[pair_hash..].find("la t5, __cellscript_const_data_0").expect("declared first field copy");
    let second_copy = assembly[pair_hash..].find("la t5, __cellscript_const_data_1").expect("declared second field copy");
    assert!(
        first_copy < second_copy,
        "Pair was initialized as second, first, but packed hash must copy declared first, second:\n{assembly}"
    );
}

#[test]
fn hash_blake2b_packed_rejects_dynamic_values() {
    let err = compile(
        r#"
module packed_hash_dynamic

action bad(witness name: String) -> Hash
where
    return hash_blake2b_packed(name)
"#,
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap_err();

    assert!(err.message.contains("hash_blake2b_packed expects a fixed-width value"), "unexpected error: {}", err.message);
}

#[test]
fn ckb_outpoint_capacity_and_lock_args_helpers_emit_checked_runtime_accesses() {
    let result = compile(
        r#"
module ckb_helper_surface

lock guard(
    witness expected_tx_hash: Hash,
    witness expected_index: u32,
    witness min_capacity: u64,
    lock_args expected_lock_args: Hash
) -> bool {
    let input = source::group_input(0)
    let output = source::output(1)
    require ckb::input_previous_tx_hash(input) == expected_tx_hash
    require ckb::input_previous_index(input) == expected_index
    require ckb::cell_data_hash(input) == ckb::hash_data_packed(expected_tx_hash)
    require ckb::cell_capacity(output) >= min_capacity
    require ckb::cell_lock_args32(output) == expected_lock_args
    true
}
"#,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            target: Some("riscv64-asm".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("CKB helper surface should compile");

    let lock = result.metadata.locks.iter().find(|lock| lock.name == "guard").expect("guard lock metadata");
    for feature in ["ckb-input-previous-output", "ckb-cell-data-hash", "ckb-cell-capacity", "ckb-cell-lock-args32"] {
        assert!(
            lock.ckb_runtime_features.iter().any(|actual| actual == feature),
            "missing feature {feature}: {:?}",
            lock.ckb_runtime_features
        );
    }
    for (operation, source) in [
        ("input-previous-tx-hash", "InputSource"),
        ("input-previous-index", "InputSource"),
        ("cell-data-hash", "CellSource"),
        ("cell-capacity", "CellSource"),
        ("cell-lock-args32", "CellSource"),
    ] {
        assert!(
            lock.ckb_runtime_accesses.iter().any(|access| access.operation == operation && access.source == source),
            "missing runtime access {operation}/{source}: {:?}",
            lock.ckb_runtime_accesses
        );
    }

    let assembly = String::from_utf8(result.artifact_bytes).expect("assembly should be utf-8");
    for symbol in [
        "__ckb_input_previous_tx_hash",
        "__ckb_input_previous_index",
        "__ckb_cell_data_hash",
        "__ckb_hash_data_packed",
        "__ckb_cell_capacity",
        "__ckb_cell_lock_args32",
    ] {
        assert!(assembly.contains(&format!("call {symbol}")), "missing {symbol} call:\n{assembly}");
    }
    assert!(assembly.contains("ckb::hash_data_packed packed_len=32 preimage_len=32"), "{assembly}");
}

#[test]
fn nested_packed_receipt_hash_guards_resource_transition_in_strict_mode() {
    let result = compile(
        r#"
module nested_receipt_transition

resource Cell has store, create, consume {
    owner: Hash,
    latest_receipt_hash: Hash,
    nonce: u64,
}

struct IntentCore {
    new_nonce: u64,
}

struct Intent {
    core: IntentCore,
    expected_receipt_hash: Hash,
}

struct ReceiptCommitment {
    old_nonce: u64,
    new_nonce: u64,
}

receipt Receipt has store, create, consume {
    receipt_hash: Hash,
    signed_intent_hash: Hash,
}

action advance(old_cell: Cell, witness intent: Intent) -> (new_cell: Cell, receipt: Receipt)
where
    let expected_nonce = old_cell.nonce + 1
    let receipt_commitment = ReceiptCommitment {
        old_nonce: old_cell.nonce,
        new_nonce: intent.core.new_nonce
    }
    let materialized_receipt_hash = hash_blake2b_packed(receipt_commitment)
    let signed_intent_hash = hash_blake2b_packed(intent)
    require intent.core.new_nonce == expected_nonce
    require intent.expected_receipt_hash == materialized_receipt_hash
    let owner = old_cell.owner
    consume old_cell
    create new_cell = Cell {
        owner: owner,
        latest_receipt_hash: materialized_receipt_hash,
        nonce: intent.core.new_nonce
    }
    create receipt = Receipt {
        receipt_hash: materialized_receipt_hash,
        signed_intent_hash: signed_intent_hash
    }
"#,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.16".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("strict mode should accept nested packed receipt/resource guards");

    let action = result.metadata.actions.iter().find(|action| action.name == "advance").expect("advance action metadata");
    let resource_plan =
        action.proof_plan.iter().find(|plan| plan.feature == "resource-conservation:Cell").expect("resource conservation proof plan");

    assert_eq!(resource_plan.status, "checked-runtime");
    assert_eq!(resource_plan.codegen_coverage_status, "covered");
    assert!(resource_plan.on_chain_checked, "{resource_plan:?}");
    for expected in ["resource-field:owner=preserved", "resource-field:latest_receipt_hash=guarded", "resource-field:nonce=guarded"] {
        assert!(
            resource_plan.input_output_relation_checks.iter().any(|check| check == expected),
            "missing generated relation check {expected}: {:?}",
            resource_plan.input_output_relation_checks
        );
    }

    for feature in ["create-output:Cell:new_cell", "create-output:Receipt:receipt"] {
        let plan = action.proof_plan.iter().find(|plan| plan.feature == feature).expect("create-output proof plan");
        assert_eq!(plan.status, "checked-runtime", "{plan:?}");
        assert_eq!(plan.codegen_coverage_status, "covered", "{plan:?}");
        assert!(plan.on_chain_checked, "{plan:?}");
    }
}

#[test]
fn hash_and_byte32_equality_is_allowed_for_authority_binding() {
    let result = compile(
        r#"
module hash_byte32_authority

action bind_pubkey_to_authority(witness pubkey: [u8; 32], witness authority_hash: Hash) -> u64
where
    require pubkey == authority_hash
    return 0
"#,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            target: Some("riscv64-asm".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("Hash <-> [u8; 32] equality should compile for BIP340 authority binding");

    let assembly = String::from_utf8(result.artifact_bytes).expect("assembly should be utf-8");
    assert!(assembly.contains("fixed-byte"), "expected fixed byte comparison lowering:\n{assembly}");
    assert!(!assembly.contains("mixed-width Eq operands"), "{assembly}");
}

#[test]
fn verifier_namespace_is_reserved_for_source_and_dependencies() {
    let err = compile(
        r#"
module verifier::btc::bip340

fn require_signature(message: Hash, pubkey: [u8; 32], signature: [u8; 64]) {
}
"#,
        CompileOptions::default(),
    )
    .unwrap_err();
    assert!(err.message.contains("module namespace 'verifier::*' is reserved"), "unexpected error: {}", err.message);

    let dir = tempdir().unwrap();
    let root = Utf8Path::from_path(dir.path()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("vendor/spoof/src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "verifier_spoof_root"
version = "0.1.0"
entry = "src/main.cell"

[dependencies]
spoof = { path = "vendor/spoof" }
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/main.cell"),
        r#"
module root

use verifier::btc::bip340::{require_signature}

action entry() -> u64
where
    return 0
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("vendor/spoof/Cell.toml"),
        r#"
[package]
name = "spoof"
version = "0.1.0"
entry = "src/lib.cell"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("vendor/spoof/src/lib.cell"),
        r#"
module verifier::btc::bip340

fn require_signature(message: Hash, pubkey: [u8; 32], signature: [u8; 64]) {
}
"#,
    )
    .unwrap();

    let err = compile_path(root, CompileOptions::default()).unwrap_err();
    assert!(err.message.contains("module namespace 'verifier::*' is reserved"), "unexpected error: {}", err.message);
}

#[test]
fn production_runtime_verifier_manifest_requires_full_non_placeholder_pin() {
    let dir = tempdir().unwrap();
    let root = Utf8Path::from_path(dir.path()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/main.cell"),
        r#"
module production_pin

struct SignaturePayload {
    pubkey: [u8; 32],
    signature: [u8; 64],
}

lock guard(witness message: Hash, witness sig: SignaturePayload) -> bool {
    verifier::btc::bip340::require_signature(message, sig.pubkey, sig.signature)
    true
}
"#,
    )
    .unwrap();

    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "production_pin_missing"
version = "0.1.0"
entry = "src/main.cell"

[build]
target_profile = "ckb"

[policy]
production = true

[[deploy.ckb.cell_deps]]
name = "cellscript_btc_bip340_verifier_riscv"
role = "runtime_verifier"
out_point = "0x1111111111111111111111111111111111111111111111111111111111111111:0"
dep_type = "code"
hash_type = "data1"
data_hash = "0x2222222222222222222222222222222222222222222222222222222222222222"
"#,
    )
    .unwrap();
    let err = compile_path(
        root,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.16".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap_err();
    assert!(err.message.contains("must pin artifact_hash, verifier_id, ipc_abi"), "unexpected error: {}", err.message);

    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "production_pin_placeholder"
version = "0.1.0"
entry = "src/main.cell"

[build]
target_profile = "ckb"

[policy]
production = true

[[deploy.ckb.cell_deps]]
name = "cellscript_btc_bip340_verifier_riscv"
role = "runtime_verifier"
verifier_id = "btc-bip340"
ipc_abi = "cellscript-btc-bip340-ipc-v0"
artifact_hash = "0x3333333333333333333333333333333333333333333333333333333333333333"
out_point = "0x4444444444444444444444444444444444444444444444444444444444444444:0"
dep_type = "code"
hash_type = "data1"
data_hash = "0x2222222222222222222222222222222222222222222222222222222222222222"
"#,
    )
    .unwrap();
    let err = compile_path(
        root,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.16".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap_err();
    assert!(err.message.contains("uses a placeholder out_point tx_hash"), "unexpected error: {}", err.message);
}
