//! Consume actual compiler/VM fixtures without linking compiler or CKB SDK code.
use cellscript_artifact_checker::code_origin::{check_counter_set, decode_counter_set, encode_counter_set, SuppliedInputCell};
use serde_json::{json, Value};
use std::{fs, path::Path};
fn bytes(v: &Value) -> Vec<u8> {
    v.as_str()
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
#[ignore = "run the native client counter_set producer first"]
fn complete_set_selection_and_public_inputs_reject_rebound_mutations() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/counter-set");
    let fixtures: Vec<Value> = serde_json::from_slice(&fs::read(directory.join("fixtures.json")).unwrap()).unwrap();
    assert_eq!(fixtures.len(), 3);
    let mut rows = Vec::new();
    for fixture in fixtures {
        let count = fixture["count"].as_u64().unwrap() as usize;
        let input_count = if count == 4 { 64 } else { count + 1 };
        let full = bytes(&fixture["transaction"]);
        let script = bytes(&fixture["selected_script"]);
        let records = fixture["cells"]
            .as_array()
            .unwrap()
            .iter()
            .take(input_count)
            .map(|cell| (bytes(&cell["out_point"]), bytes(&cell["output"]), bytes(&cell["data"])))
            .collect::<Vec<_>>();
        let inputs = records
            .iter()
            .map(|(p, c, d)| SuppliedInputCell { out_point: p.as_slice().try_into().unwrap(), output: c, data: d })
            .collect::<Vec<_>>();
        let checked = check_counter_set(&full, &inputs, &script).unwrap();
        checked.check_statement(&bytes(&fixture["set_statement"])).unwrap();
        checked.check_unchanged(&full, &inputs).unwrap();
        let (family, leaves) = decode_counter_set(checked.encoded()).unwrap();
        assert_eq!(leaves.len(), count);
        for (leaf, pi) in leaves.iter().zip(fixture["public_inputs"].as_array().unwrap()) {
            assert_eq!(leaf.statement.public_inputs().as_slice(), bytes(pi));
        }
        for field in ["domain", "action", "script", "old", "new", "input-tx", "input-index", "raw-tx"] {
            let mut mutated = leaves.clone();
            match field {
                "domain" => mutated[0].statement.domain[0] ^= 1,
                "action" => mutated[0].statement.action[0] ^= 1,
                "script" => mutated[0].statement.script_hash[0] ^= 1,
                "old" => mutated[0].statement.old_data_hash[0] ^= 1,
                "new" => mutated[0].statement.new_data_hash[0] ^= 1,
                "input-tx" => mutated[0].statement.input_transaction_hash[0] ^= 1,
                "input-index" => mutated[0].statement.input_output_index ^= 1,
                _ => {
                    for leaf in &mut mutated {
                        leaf.statement.transaction_hash[0] ^= 1
                    }
                }
            }
            let encoded = encode_counter_set(family, &mutated).unwrap();
            assert!(checked.check_statement(&encoded).is_err(), "unbound {field}");
            rows.push(
                json!({"count":count,"case":field,"statement":cellscript_artifact_checker::hex_encode(&encoded),"accepted":false}),
            );
        }
        let mut changed = leaves.clone();
        changed.last_mut().unwrap().input_index = 63;
        let encoded = encode_counter_set(family, &changed).unwrap();
        assert!(checked.check_statement(&encoded).is_err());
        rows.push(json!({"count":count,"case":"unrelated-fee-index","statement":cellscript_artifact_checker::hex_encode(&encoded),"accepted":false}));
        if count > 1 {
            let omitted = encode_counter_set(family, &leaves[1..]).unwrap();
            assert!(checked.check_statement(&omitted).is_err());
            rows.push(json!({"count":count,"case":"omitted-real-protected-input","statement":cellscript_artifact_checker::hex_encode(&omitted),"accepted":false}));
            let mut duplicated = leaves.clone();
            duplicated[1] = duplicated[0].clone();
            assert!(encode_counter_set(family, &duplicated).is_err());
            duplicated = leaves.clone();
            duplicated.reverse();
            assert!(encode_counter_set(family, &duplicated).is_err());
        }
        for offset in [0, 4, 28, 36, 72, 108] {
            let mut bad = checked.encoded().to_vec();
            bad[offset] ^= 0x80;
            assert!(decode_counter_set(&bad).is_err());
        }
        let mut trailing = checked.encoded().to_vec();
        trailing.push(0);
        assert!(decode_counter_set(&trailing).is_err());
        assert!(check_counter_set(&full, &inputs[..inputs.len() - 1], &script).is_err());
        let mut wrong_point =
            inputs.iter().map(|i| SuppliedInputCell { out_point: i.out_point, output: i.output, data: i.data }).collect::<Vec<_>>();
        wrong_point[0].out_point[0] ^= 1;
        assert!(check_counter_set(&full, &wrong_point, &script).is_err());
        let mut changed_data = records.last().unwrap().2.clone();
        changed_data.push(9);
        let mut altered =
            inputs.iter().map(|i| SuppliedInputCell { out_point: i.out_point, output: i.output, data: i.data }).collect::<Vec<_>>();
        altered.last_mut().unwrap().data = &changed_data;
        assert!(checked.check_unchanged(&full, &altered).is_err());
        if count == 4 {
            let mut changed_witness = full.clone();
            *changed_witness.last_mut().unwrap() ^= 1;
            assert!(checked.check_unchanged(&changed_witness, &inputs).is_err());
        }
    }
    fs::write(directory.join("checker-mutations.json"), serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
}
