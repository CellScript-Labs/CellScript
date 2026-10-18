//! Source admission remains separate from the bounded codec primitive. The
//! legacy opaque Vec ABI must not invent collection semantics from byte size.
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

use ckb_script_runner::{build_simple_fixture, execute_cellscript_script};

#[test]
fn legacy_nested_vec_length_cannot_treat_encoded_byte_size_as_cardinality() {
    // Canonical one-element DynVec<Bytes>: outer size 12, first offset 8,
    // empty Bytes count 0. Its cardinality is one, never twelve.
    let exit = source_exit("Vec<Vec<u8>>", "items.len() == 12", "0c0000000800000000000000");
    assert_eq!(exit, 24, "unadmitted nested collection operation must fail closed");
}

#[test]
fn legacy_nested_empty_and_dynamic_record_lengths_also_fail_closed() {
    assert_eq!(source_exit("Vec<Vec<u8>>", "!items.is_empty()", "04000000"), 24);
    // One record with id=0x55 and an empty payload. Total 25 bytes,
    // record table 17 bytes, one element.
    assert_eq!(source_exit("Vec<Record>", "items.len() == 25", "1900000008000000110000000c0000000d0000005500000000"), 24);
}

#[test]
fn fixed_element_witness_vectors_keep_cardinality_and_empty_semantics() {
    assert_eq!(source_exit("Vec<u8>", "items.len() == 2", "02000000aabb"), 0);
    assert_eq!(source_exit("Vec<u8>", "items.is_empty()", "00000000"), 0);
    assert_eq!(source_exit("Vec<u64>", "items.len() == 1", "010000000100000000000000"), 0);
    assert_eq!(source_exit("Vec<u64>", "items.len() == 1", "020000000100000000000000"), 2);
}

fn source_exit(ty: &str, predicate: &str, wire: &str) -> i64 {
    let source = format!(
        r#"
module nested_length

struct Record {{
    id: u8,
    payload: Vec<u8>,
}}

action inspect(witness items: {ty}) -> u64 {{
    verification
        require {predicate}
        return 0
}}
"#
    );
    let encoded = hex::decode(wire).unwrap();
    let mut payload = b"CSARGv1\0".to_vec();
    payload.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
    payload.extend_from_slice(&encoded);
    let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build();
    let mut transaction = build_simple_fixture(Bytes::default(), 1, 1);
    transaction.witnesses = vec![witness.as_bytes()];
    let mut observed_exit = None;
    for opt_level in 0..=3 {
        let compiled = cellscript::compile(
            &source,
            cellscript::CompileOptions { opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        let action = compiled.metadata.actions.iter().find(|action| action.name == "inspect").unwrap();
        assert_eq!(
            action.fail_closed_runtime_features.iter().any(|feature| feature == "dynamic-length"),
            matches!(ty, "Vec<Vec<u8>>" | "Vec<Record>"),
            "metadata must agree with the admitted codec at O{opt_level}"
        );
        let elf = cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes);
        let result = execute_cellscript_script(elf, &transaction);
        if let Some(expected) = observed_exit {
            assert_eq!(result.exit_code, expected, "vector cardinality behavior changed at O{opt_level}");
        }
        observed_exit = Some(result.exit_code);
    }
    observed_exit.unwrap()
}
