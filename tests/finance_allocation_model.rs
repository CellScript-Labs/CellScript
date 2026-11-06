//! Independent table economics from strict archived bytes. Script authorization,
//! allocator operation/cancellation, escrow custody and node finality are separate.
#![cfg(all(feature = "cli", not(feature = "wasm")))]

#[path = "support/finance_archive.rs"]
pub mod finance_archive;

use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
use finance_archive::load_archive;
use serde_json::{json, Value};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn script(value: packed::Script) -> Value {
    json!({"hash":hex::encode(value.calc_script_hash().as_slice()),"code_hash":hex::encode(value.code_hash().as_slice()),
        "hash_type":u8::from(value.hash_type()),"args":hex::encode(value.args().raw_data())})
}

fn cell(output: packed::CellOutput, data: Bytes) -> Value {
    let configuration = if data.len() >= 280 {
        Some(hex::encode(packed::CellOutput::calc_data_hash(&Bytes::copy_from_slice(&data[..280])).as_slice()))
    } else {
        None
    };
    json!({"data":hex::encode(&data),"data_hash":hex::encode(packed::CellOutput::calc_data_hash(&data).as_slice()),
        "configuration_hash":configuration,"lock":script(output.lock()),"type":output.type_().to_opt().map(script)})
}

#[test]
#[ignore = "requires completed strict v2 C2 archive; no financial source compilation"]
fn actual_allocation_economics_match_an_independent_bigint_model() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = repository.join("target/finance-protocol-evidence/c2");
    let archive = load_archive(&directory).unwrap();
    let hash = |name| {
        hex::encode(
            packed::CellOutput::calc_data_hash(&archive.programs().iter().find(|program| program.name == name).unwrap().elf)
                .as_slice(),
        )
    };
    let expected = json!({"order_code":hash("order"),"allocation_code":hash("allocation"),"asset_code":hash("asset"),"type_id_code":hex::encode(cellscript::ckb_abi::type_id::CODE_HASH)});
    let selected = [
        ("individually-sufficient-payment-cannot-be-credited-twice", false, "economic-negative"),
        ("rebound-table-duplicates-an-order-identity", false, "economic-negative"),
        ("actual-order-set-cannot-be-omitted-from-table", false, "economic-negative"),
        ("listed-order-must-actually-execute-in-transaction", false, "economic-negative"),
        ("rebound-table-repeats-order-output-credit", false, "economic-negative"),
        ("inactive-credit-padding-must-be-canonical-zero", false, "economic-negative"),
        ("shared-payment-recipient-is-derived-from-every-authentic-order", false, "economic-negative"),
        ("genuine-foreign-asset-cannot-satisfy-requested-credit", false, "economic-negative"),
        (
            "allocator-cancel-mode-cannot-bypass-orders-shared-payment-checks",
            true,
            "authorization-layer-negative-with-valid-economics",
        ),
        (
            "orders-independently-reject-shared-underpayment-after-valid-allocator-refund",
            false,
            "economic-negative-with-accepted-allocator-refund-group",
        ),
        ("two-authentic-orders-share-one-aggregate-maker-payment", true, "economic-positive"),
        ("fifth-credit-exceeds-authenticated-four-row-domain", false, "economic-negative"),
        ("c2-thirty-three-inputs-exceed-the-frozen-domain", false, "domain-negative"),
        ("four-credits-two-shared-outputs-at-thirty-two-cell-domain", true, "economic-positive"),
        ("c2-maximum-domain-last-order-revision-mismatch", false, "state-transition-negative"),
        ("c2-canonical-maximum-unrelated-cell-layout-preflight", true, "economic-positive"),
        ("c2-rebuilt-order-witnesses-admit-equivalent-successor-layout", true, "economic-positive"),
        ("c2-payment-output-permutation-cannot-retain-stale-allocation-indices", false, "economic-negative"),
        (
            "c2-successor-block-permutation-cannot-reuse-old-order-output-witnesses",
            true,
            "authorization-layer-negative-with-valid-economics",
        ),
        ("c2-one-order-cannot-reuse-another-participants-witness", true, "authorization-layer-negative-with-valid-economics"),
        (
            "c2-trailing-real-signer-cannot-rebind-committed-authorizer-index",
            true,
            "authorization-layer-negative-with-valid-economics",
        ),
    ];
    let mut cases = Vec::new();
    for (name, _, category) in &selected {
        let matches = archive.cases().iter().enumerate().filter(|(_, case)| case["case"] == *name).collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "{name}: expected one actual archived transaction");
        let (index, record) = matches[0];
        let (context, tx, _) = archive.load_case(index).unwrap();
        let inputs = tx
            .inputs()
            .into_iter()
            .map(|input| {
                let (output, data) = context.get_cell(&input.previous_output()).unwrap();
                cell(output, data)
            })
            .collect::<Vec<_>>();
        let outputs =
            tx.outputs().into_iter().zip(tx.outputs_data()).map(|(output, data)| cell(output, data.raw_data())).collect::<Vec<_>>();
        cases.push(json!({"id":record["case_id"],"name":name,"category":category,"vm_outcome":record["outcome"],"expected":expected,"inputs":inputs,"outputs":outputs}));
    }
    let request = json!({"schema":"cellscript-finance-allocation-economic-input-v1","cases":cases});
    let model_path = repository.join("tests/support/finance_allocation_model.mjs");
    let mut child =
        Command::new("node").arg(&model_path).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(&serde_json::to_vec(&request).unwrap()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let model: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(model["schema"], "cellscript-finance-allocation-economic-model-v1");
    let results = model["results"].as_array().unwrap();
    assert_eq!(results.len(), selected.len());
    for (((name, valid, category), case), result) in selected.iter().zip(request["cases"].as_array().unwrap()).zip(results) {
        assert_eq!(result["id"], case["id"]);
        assert_eq!(result["economic_valid"], *valid, "{name}: {result}");
        if !valid {
            let reason = match *name {
                "individually-sufficient-payment-cannot-be-credited-twice"
                | "orders-independently-reject-shared-underpayment-after-valid-allocator-refund" => "shared_output_underfunded",
                "rebound-table-duplicates-an-order-identity" => "duplicate_order",
                "actual-order-set-cannot-be-omitted-from-table" | "listed-order-must-actually-execute-in-transaction" => {
                    "participant_count"
                }
                "rebound-table-repeats-order-output-credit" => "duplicate_credit",
                "inactive-credit-padding-must-be-canonical-zero" => "inactive_credit_padding",
                "shared-payment-recipient-is-derived-from-every-authentic-order" => "proceeds_recipient",
                "genuine-foreign-asset-cannot-satisfy-requested-credit" => "requested_asset",
                "fifth-credit-exceeds-authenticated-four-row-domain" => "credit_count",
                "c2-thirty-three-inputs-exceed-the-frozen-domain" => "cell_domain",
                "c2-maximum-domain-last-order-revision-mismatch" => "successor_state",
                "c2-payment-output-permutation-cannot-retain-stale-allocation-indices" => "payment_data_shape",
                _ => panic!("selected economic negative lacks an intended reason"),
            };
            assert_eq!(result["reason"], reason, "{name}: reject on the actual economic rule, not a parse/model failure");
        }
        if *category == "economic-positive" {
            assert_eq!(case["vm_outcome"], "accepted");
        } else {
            assert_eq!(case["vm_outcome"], "rejected");
        }
        if *category == "authorization-layer-negative-with-valid-economics" {
            assert_eq!(result["economic_valid"], true);
        }
    }
    let shared = &results[10]["evidence"];
    assert_eq!(
        shared["orders"].as_array().unwrap().iter().map(|order| order["due"].as_str().unwrap()).collect::<Vec<_>>(),
        vec!["5", "7"]
    );
    assert_eq!(shared["payments"][0]["amount"], "12");
    assert_eq!(shared["payments"][0]["allocated"], "12");
    let four = &results[13]["evidence"];
    assert_eq!(
        four["orders"].as_array().unwrap().iter().map(|order| order["due"].as_str().unwrap()).collect::<Vec<_>>(),
        vec!["5", "7"]
    );
    assert_eq!(four["rows"].as_array().unwrap().len(), 4);
    assert_eq!(four["payments"].as_array().unwrap().len(), 2);
    for payment in four["payments"].as_array().unwrap() {
        assert_eq!(payment["amount"], "6");
        assert_eq!(payment["allocated"], "6");
    }
    let evidence = json!({"schema":"cellscript-finance-allocation-economic-model-evidence-v1","archive_id":archive.identity(),
        "scope":"actual partial-fill descriptor/order/output economics; allocator operation/cancel, entry witness bindings, signatures, escrow custody and node acceptance are separate",
        "model_source_hash":hex::encode(cellscript_artifact_checker::ckb_blake2b256(&std::fs::read(model_path).unwrap())),
        "vm_case_count":archive.cases().len(),"local_observation_count":archive.local_observations().len(),"selected_economic_cases":selected.len(),"request":request,"model":model});
    std::fs::write(directory.join("allocation-economic-model.json"), serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
}
