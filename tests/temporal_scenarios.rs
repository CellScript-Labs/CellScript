//! Exact positive and adversarial CKB-VM transactions for the temporal business inventory.

use cellscript::{
    compile_with_executable_surface_policy, strip_vm_abi_trailer, CellScriptEdition, CompileOptions, EntryWitnessArg,
    ExecutableSurfacePolicy,
};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

use ckb_script_runner::{build_simple_fixture, execute_cellscript_script_with_transaction_transform, FixtureHeaderContext};

const SOURCE: &str = include_str!("fixtures/temporal_scenarios.cell");
const BUSINESS_SCENARIO_EVIDENCE: &str = include_str!("fixtures/business_scenario_evidence.json");

#[derive(Clone, Copy)]
struct Case {
    scenario: &'static str,
    outcome: &'static str,
    mode: u64,
    minimum: u64,
    duration: u64,
    header_index: u64,
    input_since: u64,
    header_count: usize,
    expected_exit_code: i64,
}

fn compile() -> cellscript::CompileResult {
    compile_with_executable_surface_policy(
        SOURCE,
        CompileOptions {
            edition: CellScriptEdition::Edition2027,
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..Default::default()
        },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap_or_else(|error| panic!("temporal scenario artifact must compile: {error}"))
}

fn cases() -> [Case; 9] {
    [
        Case {
            scenario: "absolute_timelock",
            outcome: "positive",
            mode: 1,
            minimum: 123,
            duration: 0,
            header_index: 0,
            input_since: 123,
            header_count: 1,
            expected_exit_code: 0,
        },
        Case {
            scenario: "relative_timelock",
            outcome: "positive",
            mode: 2,
            minimum: 7,
            duration: 0,
            header_index: 0,
            input_since: 9_223_372_036_854_775_815,
            header_count: 1,
            expected_exit_code: 0,
        },
        Case {
            scenario: "vesting",
            outcome: "positive",
            mode: 3,
            minimum: 47,
            duration: 5,
            header_index: 0,
            input_since: 0,
            header_count: 1,
            expected_exit_code: 0,
        },
        Case {
            scenario: "epoch_timestamp_block_since",
            outcome: "positive",
            mode: 4,
            minimum: 0,
            duration: 0,
            header_index: 0,
            input_since: 0,
            header_count: 1,
            expected_exit_code: 0,
        },
        Case {
            scenario: "cross_domain",
            outcome: "adversarial",
            mode: 1,
            minimum: 7,
            duration: 0,
            header_index: 0,
            input_since: 9_223_372_036_854_775_815,
            header_count: 1,
            expected_exit_code: 37,
        },
        Case {
            scenario: "missing_header",
            outcome: "adversarial",
            mode: 4,
            minimum: 0,
            duration: 0,
            header_index: 0,
            input_since: 0,
            header_count: 0,
            expected_exit_code: 45,
        },
        Case {
            scenario: "early_release",
            outcome: "adversarial",
            mode: 1,
            minimum: 123,
            duration: 0,
            header_index: 0,
            input_since: 120,
            header_count: 1,
            expected_exit_code: 5,
        },
        Case {
            scenario: "overflow",
            outcome: "adversarial",
            mode: 3,
            minimum: 47,
            duration: 16_777_216,
            header_index: 0,
            input_since: 0,
            header_count: 1,
            expected_exit_code: 20,
        },
        Case {
            scenario: "one_past_boundary",
            outcome: "adversarial",
            mode: 4,
            minimum: 0,
            duration: 0,
            header_index: 1,
            input_since: 0,
            header_count: 1,
            expected_exit_code: 45,
        },
    ]
}

#[test]
fn temporal_business_inventory_scenarios_are_exact() {
    let result = compile();
    result.validate().expect("independently checked temporal scenario artifact");
    let action = result.metadata.actions.iter().find(|action| action.name == "validate").expect("validate action");
    let elf = strip_vm_abi_trailer(&result.artifact_bytes);
    let mut actual_cases = Vec::new();
    for case in cases() {
        let payload = action
            .entry_witness_args(&[
                EntryWitnessArg::U64(case.mode),
                EntryWitnessArg::U64(case.minimum),
                EntryWitnessArg::U64(case.duration),
                EntryWitnessArg::U64(case.header_index),
            ])
            .expect("temporal scenario witness payload");
        let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build();
        let mut fixture = build_simple_fixture(Bytes::copy_from_slice(case.scenario.as_bytes()), 1, 1);
        fixture.current_type_script_input_indices = vec![0];
        fixture.header_dao_fields = vec![[0; 32]; case.header_count];
        fixture.header_contexts =
            vec![
                FixtureHeaderContext { number: 100, timestamp: 1_700_000_000_123, epoch_number: 42, epoch_index: 3, epoch_length: 10 };
                case.header_count
            ];
        fixture.witnesses = vec![witness.as_bytes()];
        let execution = execute_cellscript_script_with_transaction_transform(elf, &fixture, move |transaction, _| {
            let mut inputs = transaction.inputs().into_iter().collect::<Vec<_>>();
            inputs[0] = inputs[0].clone().as_builder().since(case.input_since).build();
            transaction.as_advanced_builder().set_inputs(inputs).build()
        });
        assert_eq!(
            execution.exit_code, case.expected_exit_code,
            "unexpected {} result: {:?}",
            case.scenario, execution.captured_debug
        );
        actual_cases.push(serde_json::json!({
            "scenario": case.scenario,
            "outcome": case.outcome,
            "expected_exit_code": case.expected_exit_code,
            "raw_transaction_hash": execution.raw_transaction_hash,
            "serialized_transaction_hash": execution.serialized_transaction_hash,
        }));
    }
    let actual = serde_json::json!({
        "schema": "cellscript-temporal-scenarios-v1",
        "source_file": "tests/fixtures/temporal_scenarios.cell",
        "artifact_identities": {
            "artifact_hash": format!("0x{}", result.metadata.artifact_hash.as_deref().expect("artifact hash")),
            "lowering_record_hash": format!("0x{}", result.metadata.verified_artifact.lowering_record_hash.as_deref().expect("lowering hash")),
            "source_map_hash": format!("0x{}", result.metadata.verified_artifact.source_map_hash.as_deref().expect("source-map hash")),
            "verified_bundle_id": format!("0x{}", result.metadata.verified_artifact.verified_bundle_id.as_deref().expect("bundle id")),
        },
        "cases": actual_cases,
    });
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/temporal_scenarios.json")).expect("temporal scenario fixture");
    assert_eq!(actual, fixture, "recorded temporal scenario identities are stale: {actual}");

    let manifest: serde_json::Value = serde_json::from_str(BUSINESS_SCENARIO_EVIDENCE).expect("business scenario evidence JSON");
    let family = &manifest["families"]["temporal"];
    assert_eq!(family["coverage_status"], "exact-artifact-fixtures");
    assert_eq!(family["gaps"], serde_json::json!([]));
    let records = family["records"].as_array().expect("temporal scenario records");
    assert_eq!(records.len(), 9, "temporal inventory requires four positive and five adversarial records");
    for case in actual["cases"].as_array().expect("executed temporal cases") {
        let scenario = case["scenario"].as_str().unwrap();
        let record = records
            .iter()
            .find(|record| record["scenario"] == scenario && record["outcome"] == case["outcome"])
            .unwrap_or_else(|| panic!("missing exact business-scenario record for {scenario}"));
        assert_eq!(record["status"], "exact-artifact-fixture");
        assert_eq!(record["fixture"], "tests/fixtures/temporal_scenarios.json");
        assert_eq!(record["raw_transaction_hash"], case["raw_transaction_hash"]);
        assert_eq!(record["serialized_transaction_hash"], case["serialized_transaction_hash"]);
        assert_eq!(record["artifact_hashes"], serde_json::json!([actual["artifact_identities"]["artifact_hash"]]));
    }
}
