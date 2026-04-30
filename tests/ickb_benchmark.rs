use camino::Utf8PathBuf;
use cellscript::{compile_file, CompileOptions};
use serde_json::Value;
use std::collections::BTreeSet;

#[path = "support/ickb_model.rs"]
mod ickb_model;

use ickb_model::{evaluate_fixture, verify_fixture_root, NEGATIVE_FIXTURES, POSITIVE_FIXTURES};

#[test]
fn ickb_benchmark_specs_compile_and_expose_expected_entries() {
    let cases = [
        (
            "ickb_logic.cell",
            ["deposit_phase_1", "mint_from_receipt", "transfer_ickb", "request_withdrawal", "redeem_mature"].as_slice(),
        ),
        ("limit_order.cell", ["mint_order", "fulfill_ckb_to_udt", "fulfill_udt_to_ckb", "cancel_order"].as_slice()),
        ("owned_owner.cell", ["mint_owned_owner", "melt_owned_owner"].as_slice()),
    ];

    for (file, actions) in cases {
        let result = compile_file(
            spec_path(file),
            CompileOptions {
                target: Some("riscv64-asm".to_string()),
                target_profile: Some("ckb".to_string()),
                ..CompileOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("{file} should compile: {}", err.message));
        let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly utf-8");
        assert!(assembly.contains(".section .text"), "{file} emitted no text section");
        assert!(
            assembly.contains(".global __ckb_require_current_script_args_empty"),
            "{file} should lower current-script empty-args entry guard:\n{assembly}"
        );

        let emitted_actions = result.metadata.actions.iter().map(|action| action.name.as_str()).collect::<BTreeSet<_>>();
        for action in actions {
            assert!(emitted_actions.contains(action), "{file} missing action {action}: {emitted_actions:?}");
            assert!(assembly.contains(&format!(".global {action}")), "{file} missing global symbol for {action}");
        }
        if file == "limit_order.cell" {
            assert!(
                assembly.contains(".global __ckb_require_lock_type_metapoint_pairs"),
                "limit order mint path should lower lock-only order/type-only master cardinality:\n{assembly}"
            );
            assert!(
                assembly.contains(".global __ckb_require_lock_type_metapoint_pairs_from_i32_data"),
                "limit order mint path should lower data-driven master-distance cardinality:\n{assembly}"
            );
            assert!(
                assembly.contains(".global __ckb_require_lock_match_master_out_point_pairs_from_data"),
                "limit order match path should lower absolute master OutPoint cardinality:\n{assembly}"
            );
        }
        if file == "ickb_logic.cell" {
            assert!(
                assembly.contains(".global __xudt_require_group_amount_minted")
                    && assembly.contains(".global __xudt_require_group_amount_burned")
                    && assembly.contains(".global __ckb_require_cell_type_script_hash_type"),
                "iCKB logic benchmark should only use protocol-neutral runtime helpers in core codegen:\n{assembly}"
            );
            assert!(
                !assembly.contains("__dao_require_output_deposit_receipt_pairs")
                    && !assembly.contains("__dao_require_input_receipt_data"),
                "iCKB-specific receipt/deposit helpers must stay out of the generic compiler surface:\n{assembly}"
            );
        }
        if file == "owned_owner.cell" {
            assert!(
                assembly.contains(".global __ckb_require_type_lock_metapoint_pairs_from_i32_data")
                    && assembly.contains(".global __ckb_require_type_lock_metapoint_pairs_from_i32_data_filtered")
                    && assembly.contains(".global __dao_has_dao_type")
                    && assembly.contains(".global __dao_is_withdrawal_request_data")
                    && !assembly.contains("__dao_require_type_lock_withdrawal_metapoint_pairs_from_i32_data"),
                "owned-owner benchmark must compose protocol-neutral helpers, not expose an iCKB-specific combined helper:\n{assembly}"
            );
        }
    }
}

#[test]
fn ickb_positive_fixtures_pass_model_verifier() {
    for fixture_name in POSITIVE_FIXTURES {
        let fixture = read_fixture("ickb_positive", fixture_name);
        assert_eq!(fixture["expected"], "pass", "{fixture_name}");
        evaluate_fixture(&fixture).unwrap_or_else(|reason| panic!("{fixture_name} should pass, failed with {reason}"));
        assert_eq!(fixture["model_level_only"], true, "{fixture_name} must be labelled honestly");
    }
}

#[test]
fn ickb_fixture_root_model_report_is_test_only() {
    let root = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("benchmarks");
    let report = verify_fixture_root(root.as_std_path());
    assert_eq!(report["status"], "ok");
    assert_eq!(report["execution_level"], "MODEL");
    assert_eq!(report["ckb_vm_execution"], false);
    assert_eq!(report["positive_fixture_count"], 6);
    assert_eq!(report["negative_fixture_count"], 18);
}

#[test]
fn ickb_negative_fixtures_fail_for_expected_invariant() {
    for fixture_name in NEGATIVE_FIXTURES {
        let fixture = read_fixture("ickb_negative", fixture_name);
        assert_eq!(fixture["expected"], "fail", "{fixture_name}");
        let expected_reason = fixture["expected_reason"].as_str().expect("expected_reason");
        let actual_reason = evaluate_fixture(&fixture).expect_err("negative fixture should fail");
        assert_eq!(actual_reason, expected_reason, "{fixture_name}");
        assert_eq!(fixture["model_level_only"], true, "{fixture_name} must be labelled honestly");
    }
}

#[test]
fn ickb_diff_matrix_is_partial_and_consistent_with_model_fixtures() {
    let matrix = read_fixture("ickb_diff", "matrix.json");
    assert_eq!(matrix["schema"], "cellscript-ickb-diff-matrix-v1");
    // Mode can be MODEL_LEVEL_ONLY, CELL_SCRIPT_CKB_VM_EXECUTED, or the current
    // partial differential VM evidence state.
    let mode = matrix["mode"].as_str().expect("mode");
    assert!(
        matches!(mode, "MODEL_LEVEL_ONLY" | "CELL_SCRIPT_CKB_VM_EXECUTED" | "PARTIAL_CKB_VM_EXECUTION"),
        "unexpected mode: {mode}"
    );
    assert_eq!(matrix["equivalence_status"], "NOT_PROVEN");
    assert_eq!(matrix["production_equivalence_claim"], false);
    let rows = matrix["rows"].as_array().expect("rows");
    let model_rows: Vec<_> = rows.iter().filter(|row| row["evidence_level"].as_str() == Some("MODEL")).collect();
    assert!(model_rows.is_empty(), "active matrix must not retain legacy model rows: {model_rows:#?}");
    assert!(
        rows.iter().all(|row| !row["result"].as_str().is_some_and(|result| result.starts_with("model-"))),
        "active matrix must not retain model-* result rows"
    );

    let remaining_blockers = matrix["remaining_model_blockers"].as_array().expect("remaining_model_blockers");
    assert!(remaining_blockers.is_empty(), "remaining_model_blockers should mirror zero active MODEL rows");

    let assumptions = matrix["non_executable_model_assumptions"].as_array().expect("non_executable_model_assumptions");
    let assumption_scenarios =
        assumptions.iter().map(|row| row["scenario"].as_str().expect("assumption scenario")).collect::<BTreeSet<_>>();
    assert_eq!(
        assumption_scenarios,
        BTreeSet::from(["duplicate receipt", "wrong owner", "immature redeem"]),
        "legacy non-executable model assumptions should be explicit"
    );
    for assumption in assumptions {
        assert_eq!(assumption["evidence_level"], "NON_EXECUTABLE_MODEL_ASSUMPTION", "{assumption:#?}");
        assert_eq!(assumption["ckb_vm_execution"], false, "{assumption:#?}");
        assert!(assumption["reason"].as_str().is_some_and(|reason| !reason.is_empty()), "{assumption:#?}");
        let replacement = assumption["replacement_evidence"].as_str().expect("replacement evidence");
        let replacement_row = rows
            .iter()
            .find(|row| row["scenario"].as_str() == Some(replacement))
            .unwrap_or_else(|| panic!("missing replacement evidence row {replacement}"));
        assert_eq!(replacement_row["evidence_level"], "DIFFERENTIAL_CKB_VM_EXECUTED", "{replacement}");
        assert_eq!(replacement_row["full_differential"], true, "{replacement}");
    }
}

fn spec_path(file: &str) -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("benchmarks").join("ickb_specs").join(file)
}

fn read_fixture(dir: &str, file: &str) -> Value {
    let path = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("benchmarks").join(dir).join(file);
    let content = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("failed to read {path}: {err}"));
    serde_json::from_str(&content).unwrap_or_else(|err| panic!("failed to parse {path}: {err}"))
}
