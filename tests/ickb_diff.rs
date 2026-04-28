use camino::Utf8PathBuf;
use serde_json::Value;

const REQUIRED_EQUIVALENCE_EVIDENCE: [&str; 14] = [
    "original_ickb_repo_commit",
    "original_ickb_script_binary_sha256",
    "cellscript_source_commit",
    "generated_cellscript_artifact_sha256",
    "ckb_vm_or_testtool_version",
    "transaction_fixture_manifest_sha256",
    "identical_inputs_outputs_cell_deps_header_deps_witnesses",
    "original_and_cellscript_exit_codes",
    "named_failure_mode_for_rejects",
    "cycle_and_tx_size_measurements",
    "per_row_execution_objects",
    "pass_fail_status_matches",
    "transaction_context_hashes",
    "capacity_and_fee_measurements",
];

#[test]
fn ickb_diff_matrix_keeps_model_level_rows_explicit() {
    let matrix = read_matrix();
    assert_eq!(matrix["schema"], "cellscript-ickb-diff-matrix-v1");
    assert_eq!(matrix["mode"], "MODEL_LEVEL_ONLY");
    assert_eq!(matrix["equivalence_status"], "NOT_PROVEN");
    assert_eq!(matrix["production_equivalence_claim"], false);
    assert!(matrix["equivalence_evidence"].is_null());
    assert_required_evidence_list(&matrix);

    let rows = matrix["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 9);
    for row in rows {
        let scenario = row["scenario"].as_str().expect("scenario");
        let result = row["result"].as_str().expect("result");
        assert!(
            result.starts_with("model-"),
            "{scenario} must not be reported as behavioural equivalence without executed CKB VM evidence"
        );
        assert!(row["original_ickb_expected"].as_str().is_some(), "{scenario}");
        assert!(row["cellscript_expected"].as_str().is_some(), "{scenario}");
        assert_eq!(row["evidence_level"], "MODEL", "{scenario}");
        assert_eq!(row["ckb_vm_execution"], false, "{scenario}");
        assert_eq!(row["original_ickb_executed"], false, "{scenario}");
        if row["original_ickb_expected"] == "fail" || row["cellscript_expected"] == "fail" {
            assert!(
                row["failure_mode"].as_str().is_some_and(|mode| !mode.is_empty()),
                "{scenario} must bind rejects to a named failure mode"
            );
        } else {
            assert!(row["failure_mode"].is_null(), "{scenario}");
        }
    }
    validate_production_equivalence_gate(&matrix).expect("model-level matrix should be accepted as not-proven");
}

#[test]
fn ickb_production_equivalence_claim_requires_executed_evidence() {
    let mut matrix = read_matrix();
    matrix["mode"] = Value::String("EXECUTED_CKB_VM_DIFF".to_string());
    matrix["equivalence_status"] = Value::String("PROVEN".to_string());
    matrix["production_equivalence_claim"] = Value::Bool(true);

    let errors = validate_production_equivalence_gate(&matrix).expect_err("production claim must require executed evidence");
    assert!(
        errors.iter().any(|error| error.contains("equivalence_evidence")),
        "missing top-level evidence should be reported: {errors:?}"
    );
    assert!(
        errors.iter().any(|error| error.contains("row valid deposit phase 1")),
        "missing row execution evidence should be reported: {errors:?}"
    );
    assert!(
        errors.iter().any(|error| error.contains("model-level row")),
        "model-level rows must not satisfy production equivalence: {errors:?}"
    );
}

fn read_matrix() -> Value {
    let path = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("benchmarks").join("ickb_diff").join("matrix.json");
    let content = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("failed to read {path}: {err}"));
    serde_json::from_str(&content).unwrap_or_else(|err| panic!("failed to parse {path}: {err}"))
}

fn assert_required_evidence_list(matrix: &Value) {
    let evidence = matrix["required_evidence_for_equivalence"].as_array().expect("required_evidence_for_equivalence");
    for required in REQUIRED_EQUIVALENCE_EVIDENCE {
        assert!(
            evidence.iter().any(|item| item.as_str() == Some(required)),
            "missing required production equivalence evidence marker {required}"
        );
    }
}

fn validate_production_equivalence_gate(matrix: &Value) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    assert_required_evidence_list(matrix);

    let claims_equivalence = matrix["production_equivalence_claim"].as_bool().unwrap_or(false)
        || matrix["equivalence_status"].as_str() == Some("PROVEN")
        || matrix["mode"].as_str() == Some("EXECUTED_CKB_VM_DIFF");

    if !claims_equivalence {
        if matrix["equivalence_status"].as_str() != Some("NOT_PROVEN") {
            errors.push("non-production matrix must use equivalence_status=NOT_PROVEN".to_string());
        }
        if matrix["mode"].as_str() != Some("MODEL_LEVEL_ONLY") {
            errors.push("non-production matrix must use mode=MODEL_LEVEL_ONLY".to_string());
        }
        if matrix["production_equivalence_claim"].as_bool() != Some(false) {
            errors.push("non-production matrix must set production_equivalence_claim=false".to_string());
        }
        return if errors.is_empty() { Ok(()) } else { Err(errors) };
    }

    if matrix["mode"].as_str() != Some("EXECUTED_CKB_VM_DIFF") {
        errors.push("production equivalence requires mode=EXECUTED_CKB_VM_DIFF".to_string());
    }
    if matrix["equivalence_status"].as_str() != Some("PROVEN") {
        errors.push("production equivalence requires equivalence_status=PROVEN".to_string());
    }
    if matrix["production_equivalence_claim"].as_bool() != Some(true) {
        errors.push("production equivalence requires production_equivalence_claim=true".to_string());
    }

    match matrix["equivalence_evidence"].as_object() {
        Some(evidence) => {
            for field in REQUIRED_EQUIVALENCE_EVIDENCE {
                if !evidence.get(field).is_some_and(non_empty_json_value) {
                    errors.push(format!("equivalence_evidence missing non-empty {field}"));
                }
            }
        }
        None => errors.push("equivalence_evidence object is required for production equivalence".to_string()),
    }

    for row in matrix["rows"].as_array().into_iter().flatten() {
        let scenario = row["scenario"].as_str().unwrap_or("<unknown>");
        if row["evidence_level"].as_str() == Some("MODEL") || row["result"].as_str().is_some_and(|result| result.starts_with("model-"))
        {
            errors.push(format!("row {scenario} is still a model-level row"));
        }
        if row["ckb_vm_execution"].as_bool() != Some(true) {
            errors.push(format!("row {scenario} lacks CKB VM execution"));
        }
        if row["original_ickb_executed"].as_bool() != Some(true) {
            errors.push(format!("row {scenario} lacks original iCKB execution"));
        }
        validate_execution_object(row, scenario, &mut errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_execution_object(row: &Value, scenario: &str, errors: &mut Vec<String>) {
    let Some(execution) = row["execution"].as_object() else {
        errors.push(format!("row {scenario} missing execution object"));
        return;
    };
    for field in [
        "fixture_sha256",
        "original_ickb_binary_sha256",
        "cellscript_artifact_sha256",
        "ckb_vm_or_testtool_version",
        "original_ickb_exit_code",
        "cellscript_exit_code",
        "original_cycles",
        "cellscript_cycles",
        "tx_size_bytes",
    ] {
        if !execution.get(field).is_some_and(non_empty_json_value) {
            errors.push(format!("row {scenario} execution missing non-empty {field}"));
        }
    }
    if row["original_ickb_expected"] == "fail" || row["cellscript_expected"] == "fail" {
        match execution.get("failure_mode").and_then(Value::as_str) {
            Some(mode) if !mode.is_empty() => {}
            _ => errors.push(format!("row {scenario} reject case missing execution.failure_mode")),
        }
    }
}

fn non_empty_json_value(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(value) => !value.is_empty(),
        Value::Array(values) => !values.is_empty(),
        Value::Object(values) => !values.is_empty(),
        Value::Bool(_) | Value::Number(_) => true,
    }
}
