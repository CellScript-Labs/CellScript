#![allow(dead_code)]

use ckb_sdk::{constants::MultisigScript, unlock::MultisigConfig};
use ckb_testtool::ckb_types::{
    bytes::Bytes,
    packed,
    prelude::{Builder, Entity, Pack},
    H160,
};

#[path = "support/ckb_script_runner.rs"]
mod ckb_script_runner;

use ckb_script_runner::{build_simple_fixture, compile_cellscript_source_to_elf, execute_cellscript_script};

const PARAMETERIZED_ENTRY: &str = r#"
module entry_witness_abi

action verify(witness expected: u64) -> u64 {
    verification
        require expected == 42
        return 0
}
"#;

fn canonical_multisig_v2_witness(entry_payload: Bytes) -> packed::WitnessArgs {
    let signer_a = H160::from_slice(&[0x11; 20]).expect("20-byte signer hash");
    let signer_b = H160::from_slice(&[0x22; 20]).expect("20-byte signer hash");
    let config =
        MultisigConfig::new_with(MultisigScript::V2, vec![signer_a, signer_b], 0, 2).expect("canonical 2-of-2 multisig-v2 config");

    config.placeholder_witness().as_builder().input_type(Some(entry_payload).pack()).build()
}

fn raw_entry_payload(value: u64) -> Bytes {
    let mut payload = b"CSARGv1\0".to_vec();
    payload.extend_from_slice(&value.to_le_bytes());
    Bytes::from(payload)
}

fn execute_on_second_group_input(witness: Bytes) -> ckb_script_runner::CkbScriptExecutionResult {
    let elf = compile_cellscript_source_to_elf(PARAMETERIZED_ENTRY, "verify", None);
    let mut fixture = build_simple_fixture(Bytes::default(), 2, 1, true, None);
    fixture.current_type_script_input_indices = vec![1];
    fixture.witnesses = vec![Bytes::from_static(b"unrelated-global-input-zero"), witness];
    execute_cellscript_script(&elf, &fixture)
}

fn execute_on_output_only_group(witness: Bytes) -> ckb_script_runner::CkbScriptExecutionResult {
    let elf = compile_cellscript_source_to_elf(PARAMETERIZED_ENTRY, "verify", None);
    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1, true, None);
    fixture.current_type_script_input_indices.clear();
    fixture.witnesses = vec![witness];
    execute_cellscript_script(&elf, &fixture)
}

#[test]
fn canonical_multisig_v2_lock_and_input_type_entry_payload_execute_in_ckb_vm() {
    let witness = canonical_multisig_v2_witness(raw_entry_payload(42));
    let lock = witness.lock().to_opt().expect("multisig lock field").raw_data();
    assert_eq!(&lock[..4], &[0, 0, 2, 2], "canonical 2-of-2 multisig header");
    assert_eq!(lock.len(), 4 + 2 * 20 + 2 * 65, "multisig config plus two signature slots");

    // Input 0 is outside the type group. A global-input lookup would read the
    // unrelated witness instead of the group input at transaction index 1.
    let result = execute_on_second_group_input(witness.as_bytes());
    assert_eq!(
        result.exit_code, 0,
        "CellScript must read GroupInput#0 and decode CSARGv1 from WitnessArgs.input_type while preserving multisig-v2 lock: {:?}",
        result.captured_debug
    );
}

#[test]
fn raw_v1_group_input_payload_remains_compatible() {
    let result = execute_on_second_group_input(raw_entry_payload(42));
    assert_eq!(result.exit_code, 0, "raw-v1 compatibility failed: {:?}", result.captured_debug);
}

#[test]
fn witnessargs_input_type_falls_back_to_group_output_zero() {
    let witness = canonical_multisig_v2_witness(raw_entry_payload(42));
    let result = execute_on_output_only_group(witness.as_bytes());
    assert_eq!(result.exit_code, 0, "an output-only type group must resolve GroupOutput#0: {:?}", result.captured_debug);
}

#[test]
fn witnessargs_output_type_is_not_an_entry_payload_alias() {
    let witness = canonical_multisig_v2_witness(Bytes::from_static(b"not-the-entry-payload"))
        .as_builder()
        .input_type(None::<Bytes>.pack())
        .output_type(Some(raw_entry_payload(42)).pack())
        .build();
    let result = execute_on_second_group_input(witness.as_bytes());
    assert_eq!(result.exit_code, 25, "wrong WitnessArgs field must fail closed: {:?}", result.captured_debug);
}

#[test]
fn malformed_witnessargs_input_type_length_fails_closed() {
    let witness = canonical_multisig_v2_witness(raw_entry_payload(42));
    let mut encoded = witness.as_slice().to_vec();
    let input_type_offset = u32::from_le_bytes(encoded[8..12].try_into().expect("input_type table offset")) as usize;
    let declared_len =
        u32::from_le_bytes(encoded[input_type_offset..input_type_offset + 4].try_into().expect("input_type Bytes length"));
    encoded[input_type_offset..input_type_offset + 4].copy_from_slice(&(declared_len + 1).to_le_bytes());

    let result = execute_on_second_group_input(Bytes::from(encoded));
    assert_eq!(result.exit_code, 25, "malformed Molecule must fail closed: {:?}", result.captured_debug);
}

#[test]
fn malformed_unselected_witnessargs_field_still_fails_closed() {
    let witness = canonical_multisig_v2_witness(raw_entry_payload(42))
        .as_builder()
        .output_type(Some(Bytes::from_static(b"protocol-output-data")).pack())
        .build();
    let mut encoded = witness.as_slice().to_vec();
    let output_type_offset = u32::from_le_bytes(encoded[12..16].try_into().expect("output_type table offset")) as usize;
    let declared_len =
        u32::from_le_bytes(encoded[output_type_offset..output_type_offset + 4].try_into().expect("output_type Bytes length"));
    encoded[output_type_offset..output_type_offset + 4].copy_from_slice(&(declared_len + 1).to_le_bytes());

    let result = execute_on_second_group_input(Bytes::from(encoded));
    assert_eq!(result.exit_code, 25, "the placement ABI must validate the whole WitnessArgs table: {:?}", result.captured_debug);
}
