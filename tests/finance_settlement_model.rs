//! Independent combined C1 economics, projected only from strict archive bytes.
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

fn hash(bytes: &[u8]) -> String {
    hex::encode(cellscript_artifact_checker::ckb_blake2b256(bytes))
}
fn script(value: packed::Script) -> Value {
    json!({"hash":hex::encode(value.calc_script_hash().as_slice()),"code_hash":hex::encode(value.code_hash().as_slice()),
        "hash_type":u8::from(value.hash_type()),"args":hex::encode(value.args().raw_data())})
}
fn cell(output: packed::CellOutput, data: Bytes, point: Option<packed::OutPoint>) -> Value {
    let configuration = match data.len() {
        264 => Some(hash(&data[..208])),
        320 => Some(hash(&data[..280])),
        _ => None,
    };
    let tail = (data.len() == 112).then(|| hash(&data[16..]));
    json!({"capacity":u64::from(output.capacity()).to_string(),"data":hex::encode(&data),"data_hash":hash(&data),
        "configuration_hash":configuration,"tail_hash":tail,"lock":script(output.lock()),"type":output.type_().to_opt().map(script),
        "out_point":point.map(|point|json!({"tx_hash":hex::encode(point.tx_hash().as_slice()),"index":u32::from(point.index())}))})
}

#[test]
#[ignore = "requires completed strict v2 C1 and C1-layout archives; no financial source compilation"]
fn actual_combined_settlement_matches_an_independent_bigint_model() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let selected = [
        ("c1", "c1-actual-pool-input-cannot-exceed-committed-user-limit", Some("gross_input_limit")),
        ("c1", "c1-actual-net-profit-cannot-fall-below-user-limit", Some("net_profit_limit")),
        ("c1", "c1-pool-payment-cannot-substitute-maker-recipient", Some("maker_recipient")),
        ("c1", "c1-real-underpayment-is-not-a-donation-credit", Some("order_underpaid")),
        ("c1", "c1-escrow-remainder-must-match-authentic-order-transition", Some("escrow_remainder")),
        ("c1", "c1-user-capacity-debit-cannot-exceed-committed-maximum", Some("capacity_debit_limit")),
        ("c1", "c1-witness-cannot-redirect-the-committed-intent-source", None),
        ("c1", "c1-authorizer-capacity-cannot-be-credited-through-a-spend-rejecting-type", Some("authorizer_refund_shape")),
        ("c1", "direct-order-pool-settlement-with-no-user-b-input", None),
        ("c1-layout", "c1-trailing-maker-payment-cannot-replace-the-obligation-output", Some("order_underpaid")),
        ("c1-layout", "c1-conserved-assets-do-not-authorize-reserve-state-mismatch", Some("reserve_state")),
        ("c1-layout", "c1-genuine-foreign-asset-cannot-replace-maker-payment", Some("payment_asset")),
        ("c1-layout", "c1-missing-pool-group-cannot-be-replaced-by-honest-external-funding", Some("pool_participant")),
        ("c1-layout", "c1-missing-order-group-cannot-be-replaced-by-honest-external-funding", Some("order_participant")),
        ("c1-layout", "c1-fully-authentic-other-pool-cannot-satisfy-committed-identity", Some("pool_participant")),
        ("c1-layout", "c1-same-controller-prefix-substitution-is-outside-layout-contract", Some("prefix_layout")),
        ("c1-layout", "c1-unrelated-trailing-cells-canonical-preflight", None),
        ("c1-layout", "c1-trailing-witness-reference-cannot-substitute-the-authenticated-intent", None),
        ("c1-layout", "c1-equivalent-trailing-permutation-commits-once", None),
        ("c1-layout", "c1-thirty-three-inputs-exceed-the-layout-domain", Some("cell_domain")),
        ("c1-layout", "c1-thirty-three-outputs-exceed-the-layout-domain", Some("cell_domain")),
    ];
    let mut cases = Vec::new();
    let mut archives = Vec::new();
    for namespace in ["c1", "c1-layout"] {
        let archive = load_archive(&repository.join("target/finance-protocol-evidence").join(namespace)).unwrap();
        let code = |name| hash(&archive.programs().iter().find(|program| program.name == name).unwrap().elf);
        let expected = json!({"pool_code":code("pool"),"order_code":code("order"),"settlement_code":code("settlement"),
            "reserve_code":code("reserve"),"escrow_code":code("escrow"),"asset_code":code("asset"),"type_id_code":hex::encode(cellscript::ckb_abi::type_id::CODE_HASH)});
        archives.push(json!({"namespace":namespace,"archive_id":archive.identity(),"vm_case_count":archive.cases().len()}));
        for (_, name, reason) in selected.iter().filter(|(group, _, _)| *group == namespace) {
            let matches = archive.cases().iter().enumerate().filter(|(_, record)| record["case"] == *name).collect::<Vec<_>>();
            assert_eq!(matches.len(), 1, "{namespace}/{name}: expected one actual transaction");
            let (index, record) = matches[0];
            let (context, tx, _) = archive.load_case(index).unwrap();
            let inputs = tx
                .inputs()
                .into_iter()
                .map(|input| {
                    let point = input.previous_output();
                    let (output, data) = context.get_cell(&point).unwrap();
                    cell(output, data, Some(point))
                })
                .collect::<Vec<_>>();
            let outputs = tx
                .outputs()
                .into_iter()
                .zip(tx.outputs_data())
                .map(|(output, data)| cell(output, data.raw_data(), None))
                .collect::<Vec<_>>();
            cases.push(json!({"id":record["case_id"],"namespace":namespace,"name":name,"vm_outcome":record["outcome"],
                "local_bookkeeping":record["local_bookkeeping"],"expected_reason":reason,"expected":expected,"inputs":inputs,"outputs":outputs}));
        }
    }
    let request = json!({"schema":"cellscript-finance-settlement-economic-input-v1","cases":cases});
    let model_path = repository.join("tests/support/finance_settlement_model.mjs");
    let mut child =
        Command::new("node").arg(&model_path).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(&serde_json::to_vec(&request).unwrap()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let model: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(model["schema"], "cellscript-finance-settlement-economic-model-v1");
    let results = model["results"].as_array().unwrap();
    assert_eq!(results.len(), selected.len());
    for (case, result) in request["cases"].as_array().unwrap().iter().zip(results) {
        assert_eq!(case["id"], result["id"]);
        let valid = case["expected_reason"].is_null();
        assert_eq!(result["economic_valid"], valid, "{}: {result}", case["name"]);
        if !valid {
            assert_eq!(result["reason"], case["expected_reason"], "{}: intended economic boundary", case["name"]);
            assert_eq!(case["vm_outcome"], "rejected");
        }
        if valid {
            let value = &result["evidence"];
            assert_eq!(value["grossInput"], "50");
            assert_eq!(value["poolPayment"], "249");
            assert_eq!(value["orderDue"], "234");
            assert_eq!(value["makerDonation"], "15");
            assert_eq!(value["profitA"], "50");
            assert_eq!(value["beforeB"], "0");
            assert_eq!(value["feePaid"], "100000000");
        }
    }
    let original = &request["cases"][16];
    let permuted = &request["cases"][18];
    assert_eq!(original["local_bookkeeping"], "not-consumed-vm-preflight");
    let points = |case: &Value| {
        let mut values = case["inputs"].as_array().unwrap().iter().map(|cell| cell["out_point"].to_string()).collect::<Vec<_>>();
        values.sort();
        values
    };
    assert_eq!(points(original), points(permuted), "equivalent candidates must use the same exact predecessor set");
    assert_ne!(original["inputs"], permuted["inputs"]);
    assert_ne!(original["outputs"], permuted["outputs"]);
    assert_eq!(original["inputs"].as_array().unwrap().len(), 32);
    assert_eq!(original["outputs"].as_array().unwrap().len(), 32);
    let unrelated_capacity = |case: &Value, side: &str| {
        case[side]
            .as_array()
            .unwrap()
            .iter()
            .filter(|cell| cell["lock"]["hash"] == case["inputs"][9]["lock"]["hash"])
            .map(|cell| cell["capacity"].as_str().unwrap().parse::<u64>().unwrap())
            .sum::<u64>()
    };
    assert_eq!(
        unrelated_capacity(original, "inputs"),
        unrelated_capacity(original, "outputs"),
        "unrelated owner keeps its complete capacity"
    );
    let evidence = json!({"schema":"cellscript-finance-settlement-economic-evidence-v1","archives":archives,
        "scope":"independent combined pool/order/escrow/intent economics from raw resolved Cell data; Rust projects canonical Script/data hashes; operation witnesses, signatures and node consensus are separately verified",
        "authorization_negative_with_valid_economics":["c1-witness-cannot-redirect-the-committed-intent-source","c1-trailing-witness-reference-cannot-substitute-the-authenticated-intent"],
        "model_source_hash":hash(&std::fs::read(model_path).unwrap()),"request":request,"model":model});
    std::fs::write(
        repository.join("target/finance-protocol-evidence/c1-layout/settlement-economic-model.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}
