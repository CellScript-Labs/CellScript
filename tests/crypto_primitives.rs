#![allow(dead_code)]

use ckb_testtool::ckb_hash::blake2b_256;
use ckb_testtool::ckb_types::bytes::Bytes;
use ckb_testtool::ckb_types::packed;
use sha2::{Digest, Sha256};

#[path = "support/ckb_script_runner.rs"]
mod ckb_script_runner;

use ckb_script_runner::{build_simple_fixture, compile_cellscript_source_to_elf, execute_cellscript_script, FixtureCell};

const SHA256_MERKLE_PROGRAM: &str = r#"
module sha256_merkle_vm

action verify(
    witness leaf: Hash,
    witness other: Hash,
    witness siblings: [Hash; 16],
    witness expected_sha256: Hash,
    witness expected_sha256d: Hash,
    witness expected_pair: Hash,
    witness expected_merkle_root: Hash,
) -> u64 {
    verification
        require ckb::hash_sha256(leaf) == expected_sha256
        require ckb::hash_sha256d(leaf) == expected_sha256d
        require ckb::hash_sha256d_pair(leaf, other) == expected_pair
        ckb::require_sha256d_merkle_root(leaf, siblings, 1, 0, expected_merkle_root)
        return 0
}
"#;

const BOUNDED_CELL_DEP_PROGRAM: &str = r#"
module bounded_cell_dep_vm

action verify(witness expected_data_hash: Hash) -> u64 {
    verification
        ckb::require_bounded_cell_dep_data_hash(8, expected_data_hash)
        ckb::require_cell_data_hash(source::cell_dep(0), expected_data_hash)
        return 0
}
"#;

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn sha256d(bytes: &[u8]) -> [u8; 32] {
    sha256(&sha256(bytes))
}

fn sha256_merkle_witness(expected_root: [u8; 32]) -> Vec<u8> {
    let leaf = std::array::from_fn::<_, 32, _>(|index| index as u8);
    let other = std::array::from_fn::<_, 32, _>(|index| (0xff - index) as u8);
    let expected_sha256 = sha256(&leaf);
    let expected_sha256d = sha256d(&leaf);
    let mut pair_preimage = Vec::with_capacity(64);
    pair_preimage.extend_from_slice(&leaf);
    pair_preimage.extend_from_slice(&other);
    let expected_pair = sha256d(&pair_preimage);

    let mut witness = b"CSARGv1\0".to_vec();
    witness.extend_from_slice(&leaf);
    witness.extend_from_slice(&other);
    witness.extend_from_slice(&other);
    witness.resize(8 + 32 + 32 + 16 * 32, 0);
    witness.extend_from_slice(&expected_sha256);
    witness.extend_from_slice(&expected_sha256d);
    witness.extend_from_slice(&expected_pair);
    witness.extend_from_slice(&expected_root);
    witness
}

#[test]
fn bounded_sha256_sha256d_and_merkle_execute_in_ckb_vm() {
    let leaf = std::array::from_fn::<_, 32, _>(|index| index as u8);
    let other = std::array::from_fn::<_, 32, _>(|index| (0xff - index) as u8);
    let mut pair_preimage = Vec::with_capacity(64);
    pair_preimage.extend_from_slice(&leaf);
    pair_preimage.extend_from_slice(&other);
    let expected_pair = sha256d(&pair_preimage);

    let witness = sha256_merkle_witness(expected_pair);

    let elf = compile_cellscript_source_to_elf(SHA256_MERKLE_PROGRAM, "verify", None);
    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1, true, None);
    fixture.witnesses = vec![Bytes::from(witness)];
    let result = execute_cellscript_script(&elf, &fixture);

    assert_eq!(result.exit_code, 0, "bounded SHA-256/SHA256d/Merkle helpers failed in CKB VM: {:?}", result.captured_debug);
    assert!(result.cycles > 0);
}

#[test]
fn bounded_merkle_rejects_wrong_root_in_ckb_vm() {
    let elf = compile_cellscript_source_to_elf(SHA256_MERKLE_PROGRAM, "verify", None);
    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1, true, None);
    fixture.witnesses = vec![Bytes::from(sha256_merkle_witness([0x5a; 32]))];
    let result = execute_cellscript_script(&elf, &fixture);

    assert_eq!(result.exit_code, 64, "wrong Merkle root must fail closed with the stable runtime code: {:?}", result.captured_debug);
}

#[test]
fn bounded_cell_dep_scan_and_exact_identity_execute_in_ckb_vm() {
    let dep_data = Bytes::from_static(b"cellscript-verifier-package-v0");
    let expected_data_hash = blake2b_256(&dep_data);
    let mut witness = b"CSARGv1\0".to_vec();
    witness.extend_from_slice(&expected_data_hash);

    let elf = compile_cellscript_source_to_elf(BOUNDED_CELL_DEP_PROGRAM, "verify", None);
    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1, true, None);
    fixture.witnesses = vec![Bytes::from(witness)];
    fixture.cell_deps.push(FixtureCell { capacity: 0, lock: packed::Script::default(), type_script: None, data: dep_data });
    let result = execute_cellscript_script(&elf, &fixture);

    assert_eq!(result.exit_code, 0, "bounded CellDep scan/exact identity helpers failed in CKB VM: {:?}", result.captured_debug);
}

#[test]
fn bounded_cell_dep_scan_rejects_missing_dep_in_ckb_vm() {
    let expected_data_hash = blake2b_256(b"missing-cell-dep");
    let mut witness = b"CSARGv1\0".to_vec();
    witness.extend_from_slice(&expected_data_hash);

    let elf = compile_cellscript_source_to_elf(BOUNDED_CELL_DEP_PROGRAM, "verify", None);
    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1, true, None);
    fixture.witnesses = vec![Bytes::from(witness)];
    let result = execute_cellscript_script(&elf, &fixture);

    assert_eq!(result.exit_code, 63, "missing CellDep must fail closed with the stable runtime code: {:?}", result.captured_debug);
}
