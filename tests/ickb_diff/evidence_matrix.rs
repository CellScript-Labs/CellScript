#[test]
fn ickb_diff_matrix_structure_and_model_rows_valid() {
    let matrix = read_matrix();
    assert_eq!(matrix["schema"], "cellscript-ickb-diff-matrix-v1");
    assert_eq!(matrix["mode"], "EXECUTED_CKB_VM_DIFF");
    assert_eq!(matrix["equivalence_status"], "PROVEN");
    assert_eq!(matrix["production_equivalence_claim"], true);
    assert!(matrix["equivalence_evidence"].is_object());
    assert_required_evidence_list(&matrix);
    assert_original_binary_evidence_covers_rows(&matrix);
    assert_remaining_model_blockers(&matrix);
    assert_retired_model_assumptions(&matrix);
    assert_supporting_evidence_rows_are_not_claim_rows(&matrix);

    let rows = matrix["rows"].as_array().expect("rows");
    assert!(rows.len() >= 76, "matrix should retain the executed differential iCKB rows");

    // Validate each row based on its evidence level.
    let mut seen_scenarios = std::collections::BTreeSet::new();
    for row in rows {
        let scenario = row["scenario"].as_str().expect("scenario");
        assert!(seen_scenarios.insert(scenario), "duplicate matrix scenario: {scenario}");
        let evidence_level = row["evidence_level"].as_str().expect("evidence_level");
        match evidence_level {
            "DIFFERENTIAL_CKB_VM_EXECUTED" => {
                let result = row["result"].as_str().expect("result");
                assert!(
                    result.starts_with("differential-"),
                    "{scenario} DIFFERENTIAL_CKB_VM_EXECUTED result must start with differential-"
                );
                assert_eq!(row["ckb_vm_execution"], true, "{scenario}");
                assert_eq!(row["original_ickb_executed"], true, "{scenario}");
                assert_eq!(row["full_differential"], true, "{scenario}");
                assert!(row["original_ickb_expected"].as_str().is_some(), "{scenario} must declare original_ickb_expected");
                assert!(row["cellscript_expected"].as_str().is_some(), "{scenario} must declare cellscript_expected");
                let mut errors = Vec::new();
                validate_execution_object(row, scenario, &mut errors);
                assert!(errors.is_empty(), "{scenario} execution evidence is incomplete: {errors:#?}");
            }
            other => panic!("{scenario} has unexpected evidence_level: {other}"),
        }
    }
    validate_production_equivalence_gate(&matrix).expect("matrix should satisfy the selected executed iCKB equivalence gate");
}

#[test]
fn ickb_production_equivalence_claim_requires_executed_evidence() {
    let mut matrix = read_matrix();
    matrix["mode"] = Value::String("EXECUTED_CKB_VM_DIFF".to_string());
    matrix["equivalence_status"] = Value::String("PROVEN".to_string());
    matrix["production_equivalence_claim"] = Value::Bool(true);
    matrix["equivalence_evidence"] = Value::Null;
    matrix["non_executable_model_assumptions"] = matrix["retired_model_assumptions"].clone();
    let supporting = matrix["supporting_evidence"].as_array().expect("supporting_evidence");
    let mut rows = matrix["rows"].as_array().expect("rows").clone();
    rows.push(supporting[0].clone());
    let mut row_without_execution = rows[0].clone();
    row_without_execution["execution"] = Value::Null;
    rows.push(row_without_execution);
    matrix["rows"] = Value::Array(rows);

    let errors = validate_production_equivalence_gate(&matrix).expect_err("production claim must require executed evidence");
    assert!(
        errors.iter().any(|error| error.contains("equivalence_evidence")),
        "missing top-level evidence should be reported: {errors:?}"
    );
    assert!(
        errors.iter().any(|error| error.contains("non_executable_model_assumptions")),
        "non-executable model assumptions should block production equivalence: {errors:?}"
    );
    assert!(
        errors.iter().any(|error| error.contains("lacks original iCKB execution")),
        "non-differential rows should still block production equivalence: {errors:?}"
    );
    assert!(
        errors.iter().any(|error| error.contains("missing execution object")),
        "rows without per-row execution evidence must not satisfy production equivalence: {errors:?}"
    );
}

#[test]
fn ickb_claim_manifest_covers_declared_executable_branches() {
    let matrix = read_matrix();
    validate_production_equivalence_gate(&matrix).expect("matrix must remain production-equivalence proven before claim checks");
    let manifest = read_claim_manifest();
    assert_eq!(manifest["schema"], "cellscript-ickb-claim-manifest-v1");
    assert_eq!(manifest["status"], "complete-executable-claim-set");
    assert_eq!(manifest["matrix_path"], "matrix.json");

    let rows = matrix["rows"].as_array().expect("rows");
    let by_scenario =
        rows.iter().map(|row| (row["scenario"].as_str().expect("scenario").to_string(), row)).collect::<BTreeMap<_, _>>();
    let default_production = manifest["default_production_evidence"].as_object().expect("default_production_evidence");
    for required in [
        "script_group",
        "cell_deps",
        "header_deps",
        "outputs_data",
        "witnesses",
        "capacity_fee_tx_size_cycles",
        "deployment_manifest",
        "builder_plan",
    ] {
        assert!(default_production.get(required).is_some_and(non_empty_json_value), "missing production evidence {required}");
    }
    let default_hardening = manifest["default_hardening"].as_object().expect("default_hardening");
    for required in
        ["mutation_coverage", "deterministic_fuzz_seed", "normalized_fixture_generator", "max_cellscript_cycles", "max_tx_size_bytes"]
    {
        assert!(default_hardening.get(required).is_some_and(non_empty_json_value), "missing hardening evidence {required}");
    }

    let mut in_scope_branches = 0usize;
    for family in manifest["families"].as_array().expect("families") {
        let family_id = family["id"].as_str().expect("family id");
        for branch in family["branches"].as_array().expect("branches") {
            let branch_id = branch["id"].as_str().expect("branch id");
            match branch["status"].as_str().expect("branch status") {
                "in_scope" | "fixture_scoped" => {
                    in_scope_branches += 1;
                    let matched = claim_branch_scenarios(branch, &by_scenario);
                    assert!(!matched.is_empty(), "{family_id}/{branch_id} must map to differential rows");
                    for required in json_string_array(branch, "required_scenarios") {
                        assert!(by_scenario.contains_key(&required), "{family_id}/{branch_id} missing required scenario {required}");
                    }
                    for scenario in matched {
                        let row = by_scenario.get(&scenario).unwrap_or_else(|| panic!("{family_id}/{branch_id} missing {scenario}"));
                        assert_eq!(row["evidence_level"], "DIFFERENTIAL_CKB_VM_EXECUTED", "{family_id}/{branch_id}/{scenario}");
                        assert_eq!(row["ckb_vm_execution"], true, "{family_id}/{branch_id}/{scenario}");
                        assert_eq!(row["original_ickb_executed"], true, "{family_id}/{branch_id}/{scenario}");
                        assert_eq!(row["full_differential"], true, "{family_id}/{branch_id}/{scenario}");
                        if row["original_ickb_expected"] == "fail" || row["cellscript_expected"] == "fail" {
                            assert!(
                                row["failure_mode"].as_str().is_some_and(|mode| !mode.is_empty())
                                    || row["execution"]["failure_mode"].as_str().is_some_and(|mode| !mode.is_empty()),
                                "{family_id}/{branch_id}/{scenario} reject row lacks mutation/failure-mode evidence"
                            );
                        }
                        let cellscript_cycles = row["execution"]["cellscript_cycles"].as_u64().expect("cellscript_cycles");
                        assert!(
                            cellscript_cycles <= default_hardening["max_cellscript_cycles"].as_u64().expect("max cycles"),
                            "{family_id}/{branch_id}/{scenario} exceeds cycle envelope"
                        );
                        let tx_size = row["execution"]["tx_size_bytes"].as_u64().expect("tx_size_bytes");
                        assert!(
                            tx_size <= default_hardening["max_tx_size_bytes"].as_u64().expect("max tx size"),
                            "{family_id}/{branch_id}/{scenario} exceeds tx size envelope"
                        );
                    }
                    if branch["status"] == "fixture_scoped" {
                        assert!(branch["limitation"].as_str().is_some_and(|value| !value.is_empty()), "{family_id}/{branch_id}");
                    }
                }
                "retired" => {
                    assert!(branch["reason"].as_str().is_some_and(|value| !value.is_empty()), "{family_id}/{branch_id}");
                    for replacement in json_string_array(branch, "replacement_scenarios") {
                        assert!(by_scenario.contains_key(&replacement), "{family_id}/{branch_id} missing replacement {replacement}");
                    }
                }
                "out_of_scope" => {
                    assert!(branch["reason"].as_str().is_some_and(|value| !value.is_empty()), "{family_id}/{branch_id}");
                    assert!(branch["source_evidence"].as_str().is_some_and(|value| !value.is_empty()), "{family_id}/{branch_id}");
                }
                status => panic!("{family_id}/{branch_id} unsupported status {status}"),
            }
        }
    }
    assert!(in_scope_branches >= 8, "manifest should declare all executable iCKB branch families");
}

fn matrix_path() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("benchmarks").join("ickb_diff").join("matrix.json")
}

fn read_matrix() -> Value {
    let path = matrix_path();
    let content = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("failed to read {path}: {err}"));
    serde_json::from_str(&content).unwrap_or_else(|err| panic!("failed to parse {path}: {err}"))
}

fn claim_manifest_path() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("benchmarks").join("ickb_diff").join("claim_manifest.json")
}

fn read_claim_manifest() -> Value {
    let path = claim_manifest_path();
    let content = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("failed to read {path}: {err}"));
    serde_json::from_str(&content).unwrap_or_else(|err| panic!("failed to parse {path}: {err}"))
}

fn claim_branch_scenarios(branch: &Value, by_scenario: &BTreeMap<String, &Value>) -> BTreeSet<String> {
    let excludes = json_string_array(branch, "exclude_scenario_prefixes");
    let mut matched = BTreeSet::new();
    for scenario in json_string_array(branch, "evidence_scenarios") {
        matched.insert(scenario);
    }
    for prefix in json_string_array(branch, "evidence_scenario_prefixes") {
        for scenario in by_scenario.keys() {
            if scenario.starts_with(&prefix) && !excludes.iter().any(|exclude| scenario.starts_with(exclude)) {
                matched.insert(scenario.clone());
            }
        }
    }
    for scenario in json_string_array(branch, "required_scenarios") {
        matched.insert(scenario);
    }
    matched
}

fn json_string_array(value: &Value, key: &str) -> Vec<String> {
    value[key].as_array().into_iter().flatten().filter_map(Value::as_str).map(ToString::to_string).collect()
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

fn assert_original_binary_evidence_covers_rows(matrix: &Value) {
    let summarized = matrix["equivalence_evidence"]["original_ickb_script_binary_sha256"]
        .as_array()
        .expect("original_ickb_script_binary_sha256")
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    let mut used = BTreeSet::new();
    for row in matrix["rows"].as_array().expect("rows") {
        if let Some(execution) = row["execution"].as_object() {
            for (key, value) in execution {
                let is_original_or_shared = key.starts_with("original_") || key.starts_with("shared_");
                if is_original_or_shared
                    && key.ends_with("_binary_sha256")
                    && !key.contains("patched")
                    && let Some(hash) = value.as_str()
                {
                    used.insert(hash);
                }
            }
        }
    }
    for hash in used {
        assert!(summarized.contains(hash), "top-level original binary evidence is missing row-used binary hash {hash}");
    }
}

fn assert_remaining_model_blockers(matrix: &Value) {
    let rows = matrix["rows"].as_array().expect("rows");
    let model_rows = rows
        .iter()
        .filter(|row| row["evidence_level"].as_str() == Some("MODEL"))
        .map(|row| {
            (row["scenario"].as_str().expect("MODEL row scenario"), row["failure_mode"].as_str().expect("MODEL row failure_mode"))
        })
        .collect::<Vec<_>>();
    assert_eq!(model_rows.as_slice(), &REMAINING_MODEL_BLOCKERS, "active MODEL rows must be only the unpaired blockers");

    let blockers = matrix["remaining_model_blockers"].as_array().expect("remaining_model_blockers");
    assert_eq!(blockers.len(), REMAINING_MODEL_BLOCKERS.len(), "remaining blocker registry length");
    for ((expected_scenario, expected_failure_mode), blocker) in REMAINING_MODEL_BLOCKERS.iter().zip(blockers) {
        assert_eq!(blocker["scenario"].as_str(), Some(*expected_scenario), "{expected_scenario}");
        assert_eq!(blocker["failure_mode"].as_str(), Some(*expected_failure_mode), "{expected_scenario}");
        assert_eq!(blocker["evidence_level"], "MODEL", "{expected_scenario}");
        assert_eq!(blocker["ckb_vm_execution"], false, "{expected_scenario}");
        assert!(
            blocker["blocker"].as_str().is_some_and(|value| !value.is_empty()),
            "{expected_scenario} must explain why it remains model-level"
        );
        assert!(
            blocker["required_capability"].as_str().is_some_and(|value| !value.is_empty()),
            "{expected_scenario} must name the required capability to upgrade"
        );
    }
}

fn assert_retired_model_assumptions(matrix: &Value) {
    let rows = matrix["rows"].as_array().expect("rows");
    let active_assumptions = matrix["non_executable_model_assumptions"].as_array().expect("non_executable_model_assumptions");
    assert!(active_assumptions.is_empty(), "production equivalence requires no active non-executable model assumptions");
    let assumptions = matrix["retired_model_assumptions"].as_array().expect("retired_model_assumptions");
    assert_eq!(assumptions.len(), RETIRED_MODEL_ASSUMPTIONS.len(), "retired assumption registry length");

    for ((expected_scenario, expected_failure_mode, expected_replacement), assumption) in
        RETIRED_MODEL_ASSUMPTIONS.iter().zip(assumptions)
    {
        assert_eq!(assumption["scenario"].as_str(), Some(*expected_scenario), "{expected_scenario}");
        assert_eq!(assumption["failure_mode"].as_str(), Some(*expected_failure_mode), "{expected_scenario}");
        assert_eq!(assumption["evidence_level"], "NON_EXECUTABLE_MODEL_ASSUMPTION", "{expected_scenario}");
        assert_eq!(assumption["ckb_vm_execution"], false, "{expected_scenario}");
        assert_eq!(assumption["replacement_evidence"].as_str(), Some(*expected_replacement), "{expected_scenario}");
        assert!(
            assumption["reason"].as_str().is_some_and(|value| !value.is_empty()),
            "{expected_scenario} must explain why the legacy model row is not active executable evidence"
        );
        assert!(
            rows.iter().all(|row| row["scenario"].as_str() != Some(*expected_scenario)),
            "{expected_scenario} must not remain in active matrix rows"
        );
        let replacement = rows
            .iter()
            .find(|row| row["scenario"].as_str() == Some(*expected_replacement))
            .unwrap_or_else(|| panic!("{expected_scenario} replacement evidence row is missing: {expected_replacement}"));
        assert_eq!(replacement["evidence_level"], "DIFFERENTIAL_CKB_VM_EXECUTED", "{expected_scenario}");
        assert_eq!(replacement["full_differential"], true, "{expected_scenario}");
    }
}

fn assert_supporting_evidence_rows_are_not_claim_rows(matrix: &Value) {
    let rows = matrix["rows"].as_array().expect("rows");
    assert!(
        rows.iter().all(|row| row["evidence_level"].as_str() == Some("DIFFERENTIAL_CKB_VM_EXECUTED")),
        "selected equivalence rows must all be differential rows"
    );
    let supporting = matrix["supporting_evidence"].as_array().expect("supporting_evidence");
    assert!(!supporting.is_empty(), "one-sided VM evidence should remain available as supporting evidence");
    for row in supporting {
        let scenario = row["scenario"].as_str().expect("supporting scenario");
        assert!(
            !rows.iter().any(|claim_row| claim_row["scenario"].as_str() == Some(scenario)),
            "{scenario} must not be counted as a selected equivalence row"
        );
        assert_ne!(row["evidence_level"], "DIFFERENTIAL_CKB_VM_EXECUTED", "{scenario}");
        assert_eq!(row["ckb_vm_execution"], true, "{scenario}");
        assert_eq!(row["full_differential"], false, "{scenario}");
    }
}

fn validate_production_equivalence_gate(matrix: &Value) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    assert_required_evidence_list(matrix);

    // CELL_SCRIPT_CKB_VM_EXECUTED is an intermediate level: CellScript-generated
    // script executed in CKB VM, but original iCKB not yet run.
    // It must NOT claim production equivalence or full differential.
    let has_cellscript_only_vm_evidence = matrix["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|row| row["evidence_level"].as_str() == Some("CELL_SCRIPT_CKB_VM_EXECUTED"));

    if has_cellscript_only_vm_evidence {
        // Validate that CellScript-only VM rows are honest about not being full differential.
        for row in matrix["rows"].as_array().into_iter().flatten() {
            if row["evidence_level"].as_str() == Some("CELL_SCRIPT_CKB_VM_EXECUTED") {
                if row["full_differential"].as_bool() == Some(true) {
                    let scenario = row["scenario"].as_str().unwrap_or("<unknown>");
                    errors.push(format!("row {scenario} has CELL_SCRIPT_CKB_VM_EXECUTED but claims full_differential=true"));
                }
                if row["original_ickb_executed"].as_bool() == Some(true) {
                    let scenario = row["scenario"].as_str().unwrap_or("<unknown>");
                    errors.push(format!("row {scenario} has CELL_SCRIPT_CKB_VM_EXECUTED but claims original_ickb_executed=true"));
                }
            }
        }
    }

    let claims_equivalence = matrix["production_equivalence_claim"].as_bool().unwrap_or(false)
        || matrix["equivalence_status"].as_str() == Some("PROVEN")
        || matrix["mode"].as_str() == Some("EXECUTED_CKB_VM_DIFF");

    if !claims_equivalence {
        if matrix["equivalence_status"].as_str() != Some("NOT_PROVEN") {
            errors.push("non-production matrix must use equivalence_status=NOT_PROVEN".to_string());
        }
        // CELL_SCRIPT_CKB_VM_EXECUTED, PARTIAL_CKB_VM_EXECUTION, and MODEL_LEVEL_ONLY
        // are all non-production modes
        let mode = matrix["mode"].as_str().unwrap_or("");
        if !matches!(mode, "MODEL_LEVEL_ONLY" | "CELL_SCRIPT_CKB_VM_EXECUTED" | "PARTIAL_CKB_VM_EXECUTION") {
            errors.push(
                "non-production matrix must use mode=MODEL_LEVEL_ONLY, CELL_SCRIPT_CKB_VM_EXECUTED, or PARTIAL_CKB_VM_EXECUTION"
                    .to_string(),
            );
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

    if matrix["non_executable_model_assumptions"].as_array().is_some_and(|assumptions| !assumptions.is_empty()) {
        errors.push("production equivalence requires non_executable_model_assumptions to be empty".to_string());
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
        "normalized_fixture_sha256",
        "transaction_context_sha256",
        "original_ickb_binary_sha256",
        "cellscript_artifact_sha256",
        "ckb_vm_or_testtool_version",
        "original_ickb_exit_code",
        "cellscript_exit_code",
        "original_ickb_status",
        "cellscript_status",
        "statuses_match",
        "original_cycles",
        "cellscript_cycles",
        "tx_size_bytes",
        "occupied_capacity_shannons",
        "fee_shannons",
    ] {
        if !execution.get(field).is_some_and(non_empty_json_value) {
            errors.push(format!("row {scenario} execution missing non-empty {field}"));
        }
    }

    for field in ["fixture_sha256", "normalized_fixture_sha256", "original_ickb_binary_sha256", "cellscript_artifact_sha256"] {
        match execution.get(field).and_then(Value::as_str) {
            Some(hash) if is_canonical_prefixed_sha256(hash) => {}
            _ => errors.push(format!("row {scenario} execution.{field} must be canonical 0x-prefixed SHA-256")),
        }
    }

    match execution.get("transaction_context_sha256").and_then(Value::as_object) {
        Some(hashes) => {
            for side in ["original", "cellscript"] {
                match hashes.get(side).and_then(Value::as_str) {
                    Some(hash) if is_canonical_prefixed_sha256(hash) => {}
                    _ => {
                        errors.push(format!("row {scenario} transaction_context_sha256.{side} must be canonical 0x-prefixed SHA-256"))
                    }
                }
            }
        }
        None => errors.push(format!("row {scenario} execution.transaction_context_sha256 must be an object")),
    }

    if execution.get("statuses_match").and_then(Value::as_bool) != Some(true) {
        errors.push(format!("row {scenario} execution.statuses_match must be true"));
    }
    for (side, expected_field, status_field, exit_field, cycle_field) in [
        ("original", "original_ickb_expected", "original_ickb_status", "original_ickb_exit_code", "original_cycles"),
        ("cellscript", "cellscript_expected", "cellscript_status", "cellscript_exit_code", "cellscript_cycles"),
    ] {
        let expected = row[expected_field].as_str();
        let status = execution.get(status_field).and_then(Value::as_str);
        if expected.is_some() && status != expected {
            errors.push(format!("row {scenario} {side} status {status:?} does not match {expected_field}={expected:?}"));
        }
        if status == Some("pass") {
            if execution.get(exit_field).and_then(Value::as_i64) != Some(0) {
                errors.push(format!("row {scenario} {side} pass must have exit code 0"));
            }
            if execution.get(cycle_field).and_then(Value::as_u64).unwrap_or(0) == 0 {
                errors.push(format!("row {scenario} {side} pass must consume cycles"));
            }
        }
        if status == Some("fail") && execution.get(exit_field).and_then(Value::as_i64) == Some(0) {
            errors.push(format!("row {scenario} {side} fail must have a non-zero exit code"));
        }
    }

    for field in ["tx_size_bytes", "occupied_capacity_shannons"] {
        if execution.get(field).and_then(Value::as_u64).unwrap_or(0) == 0 {
            errors.push(format!("row {scenario} execution.{field} must be positive"));
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

fn is_canonical_prefixed_sha256(value: &str) -> bool {
    value
        .strip_prefix("0x")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()))
}

// ---------------------------------------------------------------------------
// CKB VM execution test backed by ckb-testtool
// ---------------------------------------------------------------------------
//
// This test proves that a CellScript-generated script can execute under a
// real CKB script VM/syscall environment (not bare ckb-vm). It uses
// ckb-testtool's Context::verify_tx which runs scripts via ckb-script's
// ScriptVerify with full syscall support (LOAD_SCRIPT, LOAD_CELL_BY_FIELD,
// LOAD_WITNESS, LOAD_HEADER, LOAD_CELL_DATA, etc.).
//
// This is "executable evidence", NOT "equivalence evidence".
// The matrix status for these rows is CELL_SCRIPT_CKB_VM_EXECUTED,
// not EXECUTED_CKB_VM_DIFF or PROVEN.
