//! Exact executable evidence for the four previously uncovered Order/AMM rows.

#![cfg(not(feature = "wasm"))]

use std::collections::BTreeSet;

use cellscript::{
    compile_with_executable_surface_policy, strip_vm_abi_trailer, CellScriptEdition, CompileOptions, CompileResult, EntryWitnessArg,
    ExecutableSurfacePolicy,
};
use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionView, packed, prelude::*};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

use ckb_script_runner::{
    build_simple_fixture, deterministic_always_success_lock_hash, deterministic_always_success_script, execute_cellscript_script,
    execute_cellscript_script_with_transaction_transform, CkbScriptExecutionResult, CkbVmFixture,
};

const POOL_MERGE_SOURCE: &str = include_str!("fixtures/cost_corpus/pool_merge.cell");
const AMM_SWAP_SOURCE: &str = include_str!("fixtures/order_amm_swap.cell");

fn options() -> CompileOptions {
    CompileOptions {
        edition: CellScriptEdition::Edition2027,
        opt_level: 3,
        target: Some("riscv64-elf".to_string()),
        target_profile: Some("ckb".to_string()),
        ..Default::default()
    }
}

fn compile(source: &str) -> CompileResult {
    let compiled = compile_with_executable_surface_policy(source, options(), ExecutableSurfacePolicy::DenyFailClosed)
        .unwrap_or_else(|error| panic!("Order/AMM scenario must compile: {error}\n{source}"));
    compiled.validate().expect("independent Order/AMM artifact validation");
    compiled
}

fn artifact_identity(compiled: &CompileResult) -> serde_json::Value {
    serde_json::json!({
        "artifact_hash": format!("0x{}", compiled.metadata.artifact_hash.as_deref().expect("artifact hash")),
        "lowering_record_hash": format!("0x{}", compiled.metadata.verified_artifact.lowering_record_hash.as_deref().expect("lowering hash")),
        "source_map_hash": format!("0x{}", compiled.metadata.verified_artifact.source_map_hash.as_deref().expect("source-map hash")),
        "verified_bundle_id": format!("0x{}", compiled.metadata.verified_artifact.verified_bundle_id.as_deref().expect("bundle id")),
    })
}

fn token_data(amount: u64) -> Bytes {
    Bytes::copy_from_slice(&amount.to_le_bytes())
}

fn pool_data(reserve_a: u64, reserve_b: u64) -> Bytes {
    let mut data = reserve_a.to_le_bytes().to_vec();
    data.extend_from_slice(&reserve_b.to_le_bytes());
    Bytes::from(data)
}

fn merge_fixture(compiled: &CompileResult) -> CkbVmFixture {
    let recipient = deterministic_always_success_lock_hash();
    let payload =
        compiled.metadata.actions[0].entry_witness_args(&[EntryWitnessArg::Address(recipient)]).expect("pool-merge recipient witness");
    let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build();
    let mut fixture = build_simple_fixture(Bytes::from_static(b"order-amm-pool-merge"), 2, 1);
    fixture.current_type_script_input_indices = vec![0, 1];
    fixture.inputs[0].data = token_data(3);
    fixture.inputs[1].data = token_data(4);
    fixture.outputs[0].data = token_data(7);
    fixture.witnesses = vec![witness.as_bytes(), Bytes::default()];
    fixture
}

fn swap_fixture(output_amount: u64, reordered: bool) -> CkbVmFixture {
    let foreign_type = deterministic_always_success_script(Bytes::from_static(b"order-amm-token"));
    let mut fixture = build_simple_fixture(Bytes::from_static(b"order-amm-swap"), 2, 2);
    fixture.current_type_script_input_indices = vec![0];
    fixture.inputs[0].data = pool_data(1_000, 5_000);
    fixture.inputs[1].data = token_data(100);
    fixture.inputs[1].type_script = Some(foreign_type.clone());

    let pool_after = pool_data(1_100, 5_000 - output_amount);
    let token_out = token_data(output_amount);
    if reordered {
        fixture.outputs[0].data = token_out;
        fixture.outputs[0].type_script = Some(foreign_type);
        fixture.outputs[1].data = pool_after;
    } else {
        fixture.outputs[0].data = pool_after;
        fixture.outputs[1].data = token_out;
        fixture.outputs[1].type_script = Some(foreign_type);
    }
    fixture
}

fn case_record(
    scenario: &str,
    outcome: &str,
    artifact: &str,
    rejection_stage: &str,
    expected_exit_code: Option<i64>,
    execution: &CkbScriptExecutionResult,
) -> serde_json::Value {
    serde_json::json!({
        "scenario": scenario,
        "outcome": outcome,
        "artifact": artifact,
        "rejection_stage": rejection_stage,
        "expected_exit_code": expected_exit_code,
        "raw_transaction_hash": execution.raw_transaction_hash,
        "serialized_transaction_hash": execution.serialized_transaction_hash,
    })
}

fn assert_live(transaction: &TransactionView, live: &BTreeSet<packed::OutPoint>) -> Result<(), String> {
    for input in transaction.inputs() {
        if !live.contains(&input.previous_output()) {
            return Err("non-live local input".to_string());
        }
    }
    Ok(())
}

#[test]
fn order_amm_business_inventory_scenarios_are_exact() {
    let merge = compile(POOL_MERGE_SOURCE);
    let swap = compile(AMM_SWAP_SOURCE);

    let mut merge_transaction = None;
    let merge_execution = execute_cellscript_script_with_transaction_transform(
        strip_vm_abi_trailer(&merge.artifact_bytes),
        &merge_fixture(&merge),
        |transaction, _| {
            merge_transaction = Some(transaction.clone());
            transaction
        },
    );
    assert_eq!(merge_execution.exit_code, 0, "pool merge failed: {:?}", merge_execution.captured_debug);
    let merge_transaction = merge_transaction.expect("captured exact merge transaction");
    let mut live = merge_transaction.inputs().into_iter().map(|input| input.previous_output()).collect::<BTreeSet<_>>();
    assert_live(&merge_transaction, &live).expect("merge inputs initially live");
    for input in merge_transaction.inputs() {
        live.remove(&input.previous_output());
    }
    assert!(assert_live(&merge_transaction, &live).unwrap_err().contains("non-live"));

    let correct_swap = execute_cellscript_script(strip_vm_abi_trailer(&swap.artifact_bytes), &swap_fixture(454, false));
    assert_eq!(correct_swap.exit_code, 0, "correct AMM price failed: {:?}", correct_swap.captured_debug);
    let wrong_price = execute_cellscript_script(strip_vm_abi_trailer(&swap.artifact_bytes), &swap_fixture(455, false));
    assert_ne!(wrong_price.exit_code, 0, "wrong AMM price must reject");
    let output_reordering = execute_cellscript_script(strip_vm_abi_trailer(&swap.artifact_bytes), &swap_fixture(454, true));
    assert_ne!(output_reordering.exit_code, 0, "reordered AMM outputs must reject");

    let artifacts = serde_json::json!({
        "pool_merge": artifact_identity(&merge),
        "amm_swap": artifact_identity(&swap),
    });
    let mut cases = vec![
        case_record("pool_merge", "positive", "pool_merge", "ckb-vm", Some(0), &merge_execution),
        case_record("replay", "adversarial", "pool_merge", "local-live-set", None, &merge_execution),
        case_record("output_reordering", "adversarial", "amm_swap", "ckb-vm", Some(output_reordering.exit_code), &output_reordering),
        case_record("wrong_price", "adversarial", "amm_swap", "ckb-vm", Some(wrong_price.exit_code), &wrong_price),
    ];
    cases.sort_by(|left, right| left["scenario"].as_str().cmp(&right["scenario"].as_str()));
    let actual = serde_json::json!({
        "schema": "cellscript-order-amm-scenarios-v1",
        "source_files": [
            "tests/fixtures/cost_corpus/pool_merge.cell",
            "tests/fixtures/order_amm_swap.cell"
        ],
        "artifact_identities": artifacts,
        "cases": cases,
    });
    let recorded: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/order_amm_scenarios.json")).expect("Order/AMM fixture JSON");
    assert_eq!(actual, recorded, "recorded Order/AMM identities are stale: {actual}");

    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/business_scenario_evidence.json")).expect("business scenario evidence JSON");
    let family = &manifest["families"]["order_amm"];
    assert_eq!(family["coverage_status"], "exact-artifact-fixtures");
    assert_eq!(family["gaps"], serde_json::json!([]));
    let records = family["records"].as_array().expect("Order/AMM scenario records");
    assert_eq!(records.len(), 9, "Order/AMM inventory requires four positive and five adversarial records");
    for case in actual["cases"].as_array().expect("executed Order/AMM cases") {
        let scenario = case["scenario"].as_str().unwrap();
        let artifact = case["artifact"].as_str().unwrap();
        let record = records
            .iter()
            .find(|record| record["scenario"] == scenario && record["outcome"] == case["outcome"])
            .unwrap_or_else(|| panic!("missing exact business-scenario record for {scenario}"));
        assert_eq!(record["status"], "exact-artifact-fixture");
        assert_eq!(record["fixture"], "tests/fixtures/order_amm_scenarios.json");
        assert_eq!(record["raw_transaction_hash"], case["raw_transaction_hash"]);
        assert_eq!(record["serialized_transaction_hash"], case["serialized_transaction_hash"]);
        assert_eq!(record["artifact_hashes"], serde_json::json!([actual["artifact_identities"][artifact]["artifact_hash"]]));
    }
}
