use super::{
    CKB_STANDARD_COMPAT_MANIFEST_SCHEMA, CKB_STANDARD_FIXTURE_SCHEMA, ICKB_CLAIM_MANIFEST_SCHEMA, ICKB_DIFF_EVIDENCE_LEVEL,
    ICKB_DIFF_MATRIX_SCHEMA, ICKB_REQUIRED_HARDENING_EVIDENCE, ICKB_REQUIRED_PRODUCTION_EVIDENCE,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(super) fn ckb_fixture_manifest_report(manifest: &serde_json::Value, base_dir: &Path, manifest_bytes: &[u8]) -> serde_json::Value {
    let mut issues = Vec::<String>::new();
    let mut rows = Vec::<serde_json::Value>::new();
    let manifest_hash = crate::hex_encode(&crate::ckb_blake2b256(manifest_bytes));

    match manifest["schema"].as_str() {
        Some(CKB_STANDARD_COMPAT_MANIFEST_SCHEMA) => {}
        Some(ICKB_CLAIM_MANIFEST_SCHEMA) => return ickb_claim_manifest_report(manifest, base_dir, manifest_bytes),
        got => {
            issues.push(format!(
                "manifest schema must be {CKB_STANDARD_COMPAT_MANIFEST_SCHEMA} or {ICKB_CLAIM_MANIFEST_SCHEMA}, got {}",
                got.unwrap_or("<missing>")
            ));
            return ckb_fixture_report_json(manifest, manifest_hash, 0, rows, issues);
        }
    }

    let Some(suites) = manifest["suites"].as_array() else {
        issues.push("manifest suites must be an array".to_string());
        return ckb_fixture_report_json(manifest, manifest_hash, 0, rows, issues);
    };

    for suite in suites {
        validate_ckb_fixture_suite(suite, base_dir, &mut rows, &mut issues);
    }

    ckb_fixture_report_json(manifest, manifest_hash, suites.len(), rows, issues)
}

fn ickb_claim_manifest_report(manifest: &serde_json::Value, base_dir: &Path, manifest_bytes: &[u8]) -> serde_json::Value {
    let mut issues = Vec::<String>::new();
    let mut rows = Vec::<serde_json::Value>::new();
    let manifest_hash = crate::hex_encode(&crate::ckb_blake2b256(manifest_bytes));

    if manifest["schema"].as_str() != Some(ICKB_CLAIM_MANIFEST_SCHEMA) {
        issues.push(format!(
            "iCKB claim manifest schema must be {ICKB_CLAIM_MANIFEST_SCHEMA}, got {}",
            manifest["schema"].as_str().unwrap_or("<missing>")
        ));
    }

    let matrix_path = match manifest["matrix_path"].as_str() {
        Some(path) if !path.is_empty() => base_dir.join(path),
        _ => {
            issues.push("iCKB claim manifest matrix_path must be a non-empty string".to_string());
            return ckb_fixture_report_json(manifest, manifest_hash, 0, rows, issues);
        }
    };

    let matrix_bytes = match std::fs::read(&matrix_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            issues.push(format!("failed to read iCKB matrix {}: {err}", matrix_path.display()));
            return ckb_fixture_report_json(manifest, manifest_hash, 0, rows, issues);
        }
    };
    let matrix: serde_json::Value = match serde_json::from_slice(&matrix_bytes) {
        Ok(matrix) => matrix,
        Err(err) => {
            issues.push(format!("failed to parse iCKB matrix {}: {err}", matrix_path.display()));
            return ckb_fixture_report_json(manifest, manifest_hash, 0, rows, issues);
        }
    };

    validate_ickb_claim_matrix(&matrix, &mut issues);

    let mut matrix_rows = BTreeMap::<String, &serde_json::Value>::new();
    if let Some(active_rows) = matrix["rows"].as_array() {
        for row in active_rows {
            if let Some(scenario) = row["scenario"].as_str() {
                if matrix_rows.insert(scenario.to_string(), row).is_some() {
                    issues.push(format!("iCKB matrix contains duplicate scenario {scenario}"));
                }
            } else {
                issues.push("iCKB matrix row missing scenario".to_string());
            }
        }
    } else {
        issues.push("iCKB matrix rows must be an array".to_string());
    }

    let default_production = manifest.get("default_production_evidence");
    let default_hardening = manifest.get("default_hardening");
    let Some(families) = manifest["families"].as_array() else {
        issues.push("iCKB claim manifest families must be an array".to_string());
        return ckb_fixture_report_json(manifest, manifest_hash, 0, rows, issues);
    };

    for family in families {
        validate_ickb_claim_family(family, &matrix_rows, default_production, default_hardening, &mut rows, &mut issues);
    }

    ckb_fixture_report_json(manifest, manifest_hash, families.len(), rows, issues)
}

fn ckb_fixture_report_json(
    manifest: &serde_json::Value,
    manifest_hash: String,
    suite_count: usize,
    rows: Vec<serde_json::Value>,
    issues: Vec<String>,
) -> serde_json::Value {
    let is_ickb_claim = manifest["schema"].as_str() == Some(ICKB_CLAIM_MANIFEST_SCHEMA);
    let evidence_execution_level = if is_ickb_claim { serde_json::json!(ICKB_DIFF_EVIDENCE_LEVEL) } else { serde_json::Value::Null };
    let required_executable_gate =
        if is_ickb_claim { serde_json::json!("cargo test --locked -p cellscript --test ickb_diff") } else { serde_json::Value::Null };
    serde_json::json!({
        "schema": "cellscript-ckb-fixture-verification-v0.17",
        "manifest_schema": manifest["schema"].as_str().unwrap_or("unknown"),
        "manifest_status": manifest["status"].as_str().unwrap_or("unknown"),
        "manifest_hash": manifest_hash,
        "execution_level": if is_ickb_claim { "DIFFERENTIAL_CKB_VM_MANIFEST" } else { "MODEL" },
        "ckb_vm_execution": false,
        "committed_ckb_vm_evidence": is_ickb_claim,
        "evidence_execution_level": evidence_execution_level,
        "required_executable_gate": required_executable_gate,
        "suite_count": suite_count,
        "fixture_count": rows.len(),
        "status": if issues.is_empty() { "ok" } else { "failed" },
        "issue_count": issues.len(),
        "issues": issues,
        "fixtures": rows,
        "vm_execution_note": if is_ickb_claim {
            "This command does not execute CKB VM. It validates the iCKB claim manifest against committed dual-side CKB VM differential rows, production evidence envelopes, and the required executable Rust gate."
        } else {
            "This command validates transaction-shape model fixtures only; it does not execute CKB VM or prove production compatibility."
        },
    })
}

fn validate_ickb_claim_matrix(matrix: &serde_json::Value, issues: &mut Vec<String>) {
    if matrix["schema"].as_str() != Some(ICKB_DIFF_MATRIX_SCHEMA) {
        issues.push(format!(
            "iCKB matrix schema must be {ICKB_DIFF_MATRIX_SCHEMA}, got {}",
            matrix["schema"].as_str().unwrap_or("<missing>")
        ));
    }
    if matrix["mode"].as_str() != Some("EXECUTED_CKB_VM_DIFF") {
        issues.push("iCKB matrix mode must be EXECUTED_CKB_VM_DIFF".to_string());
    }
    if matrix["equivalence_status"].as_str() != Some("PROVEN") {
        issues.push("iCKB matrix equivalence_status must be PROVEN".to_string());
    }
    if matrix["production_equivalence_claim"].as_bool() != Some(true) {
        issues.push("iCKB matrix production_equivalence_claim must be true".to_string());
    }
    if matrix["remaining_model_blockers"].as_array().is_none_or(|blockers| !blockers.is_empty()) {
        issues.push("iCKB matrix remaining_model_blockers must be empty".to_string());
    }
    if matrix["non_executable_model_assumptions"].as_array().is_none_or(|assumptions| !assumptions.is_empty()) {
        issues.push("iCKB matrix non_executable_model_assumptions must be empty".to_string());
    }
}

fn validate_ickb_claim_family(
    family: &serde_json::Value,
    matrix_rows: &BTreeMap<String, &serde_json::Value>,
    default_production: Option<&serde_json::Value>,
    default_hardening: Option<&serde_json::Value>,
    rows: &mut Vec<serde_json::Value>,
    issues: &mut Vec<String>,
) {
    let family_id = family["id"].as_str().unwrap_or("<missing-family>");
    if family["id"].as_str().is_none_or(str::is_empty) {
        issues.push("iCKB claim family id must be a non-empty string".to_string());
    }
    let Some(branches) = family["branches"].as_array() else {
        issues.push(format!("iCKB claim family {family_id} branches must be an array"));
        return;
    };

    for branch in branches {
        validate_ickb_claim_branch(family_id, branch, matrix_rows, default_production, default_hardening, rows, issues);
    }
}

fn validate_ickb_claim_branch(
    family_id: &str,
    branch: &serde_json::Value,
    matrix_rows: &BTreeMap<String, &serde_json::Value>,
    default_production: Option<&serde_json::Value>,
    default_hardening: Option<&serde_json::Value>,
    rows: &mut Vec<serde_json::Value>,
    issues: &mut Vec<String>,
) {
    let branch_id = branch["id"].as_str().unwrap_or("<missing-branch>");
    if branch["id"].as_str().is_none_or(str::is_empty) {
        issues.push(format!("iCKB claim family {family_id} has branch with missing id"));
    }
    let status = branch["status"].as_str().unwrap_or("<missing-status>");
    let mut matched = ickb_claim_branch_scenarios(branch, matrix_rows);

    match status {
        "in_scope" | "fixture_scoped" => {
            validate_ickb_required_scenarios(family_id, branch_id, branch, matrix_rows, &mut matched, issues);
            if matched.is_empty() {
                issues.push(format!("iCKB claim branch {family_id}/{branch_id} has no matching matrix rows"));
            }
            for scenario in &matched {
                if let Some(row) = matrix_rows.get(scenario) {
                    validate_ickb_claim_row(family_id, branch_id, scenario, row, issues);
                }
            }

            let production = branch.get("production_evidence").or(default_production);
            validate_ickb_evidence_object(
                "production_evidence",
                &ICKB_REQUIRED_PRODUCTION_EVIDENCE,
                production,
                family_id,
                branch_id,
                issues,
            );
            let hardening = branch.get("hardening").or(default_hardening);
            validate_ickb_evidence_object("hardening", &ICKB_REQUIRED_HARDENING_EVIDENCE, hardening, family_id, branch_id, issues);
            validate_ickb_claim_thresholds(family_id, branch_id, hardening, &matched, matrix_rows, issues);
            if status == "fixture_scoped" && branch["limitation"].as_str().is_none_or(str::is_empty) {
                issues.push(format!("iCKB fixture-scoped branch {family_id}/{branch_id} must declare limitation"));
            }
        }
        "retired" => {
            if branch["reason"].as_str().is_none_or(str::is_empty) {
                issues.push(format!("iCKB retired branch {family_id}/{branch_id} must declare reason"));
            }
            let replacements = json_string_array(branch, "replacement_scenarios");
            if replacements.is_empty() {
                issues.push(format!("iCKB retired branch {family_id}/{branch_id} must declare replacement_scenarios"));
            }
            for scenario in replacements {
                if !matrix_rows.contains_key(&scenario) {
                    issues.push(format!("iCKB retired branch {family_id}/{branch_id} replacement scenario is missing: {scenario}"));
                }
            }
        }
        "out_of_scope" => {
            if branch["reason"].as_str().is_none_or(str::is_empty) {
                issues.push(format!("iCKB out-of-scope branch {family_id}/{branch_id} must declare reason"));
            }
            if branch["source_evidence"].as_str().is_none_or(str::is_empty) {
                issues.push(format!("iCKB out-of-scope branch {family_id}/{branch_id} must declare source_evidence"));
            }
        }
        other => issues.push(format!("iCKB claim branch {family_id}/{branch_id} has unsupported status {other}")),
    }

    rows.push(serde_json::json!({
        "family": family_id,
        "branch": branch_id,
        "status": status,
        "matched_rows": matched.len(),
        "reject_rows": matched.iter().filter(|scenario| {
            matrix_rows
                .get(*scenario)
                .is_some_and(|row| row["original_ickb_expected"].as_str() == Some("fail") || row["cellscript_expected"].as_str() == Some("fail"))
        }).count(),
        "evidence_level": if matched.is_empty() { "DECLARATIVE" } else { ICKB_DIFF_EVIDENCE_LEVEL },
    }));
}

fn ickb_claim_branch_scenarios(branch: &serde_json::Value, matrix_rows: &BTreeMap<String, &serde_json::Value>) -> BTreeSet<String> {
    let mut matched = BTreeSet::new();
    let excludes = json_string_array(branch, "exclude_scenario_prefixes");
    for scenario in json_string_array(branch, "evidence_scenarios") {
        matched.insert(scenario);
    }
    for prefix in json_string_array(branch, "evidence_scenario_prefixes") {
        for scenario in matrix_rows.keys() {
            if scenario.starts_with(&prefix) && !excludes.iter().any(|exclude| scenario.starts_with(exclude)) {
                matched.insert(scenario.clone());
            }
        }
    }
    matched
}

fn validate_ickb_required_scenarios(
    family_id: &str,
    branch_id: &str,
    branch: &serde_json::Value,
    matrix_rows: &BTreeMap<String, &serde_json::Value>,
    matched: &mut BTreeSet<String>,
    issues: &mut Vec<String>,
) {
    for scenario in json_string_array(branch, "required_scenarios") {
        if matrix_rows.contains_key(&scenario) {
            matched.insert(scenario);
        } else {
            issues.push(format!("iCKB claim branch {family_id}/{branch_id} required scenario is missing: {scenario}"));
        }
    }
}

fn validate_ickb_claim_row(family_id: &str, branch_id: &str, scenario: &str, row: &serde_json::Value, issues: &mut Vec<String>) {
    if row["evidence_level"].as_str() != Some(ICKB_DIFF_EVIDENCE_LEVEL) {
        issues.push(format!(
            "iCKB claim branch {family_id}/{branch_id} scenario {scenario} must have evidence_level={ICKB_DIFF_EVIDENCE_LEVEL}"
        ));
    }
    if row["ckb_vm_execution"].as_bool() != Some(true)
        || row["original_ickb_executed"].as_bool() != Some(true)
        || row["full_differential"].as_bool() != Some(true)
    {
        issues.push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} is not a full dual-side VM row"));
    }
    let original = row["original_ickb_expected"].as_str();
    let cellscript = row["cellscript_expected"].as_str();
    if original != cellscript {
        issues.push(format!(
            "iCKB claim branch {family_id}/{branch_id} scenario {scenario} expectation mismatch original={original:?} cellscript={cellscript:?}"
        ));
    }
    if (original == Some("fail") || cellscript == Some("fail"))
        && row["failure_mode"].as_str().is_none_or(str::is_empty)
        && row["execution"]["failure_mode"].as_str().is_none_or(str::is_empty)
    {
        issues.push(format!("iCKB claim branch {family_id}/{branch_id} reject scenario {scenario} lacks named failure mode"));
    }
    for field in ["tx_size_bytes", "occupied_capacity_shannons", "fee_shannons"] {
        if !row["execution"].get(field).is_some_and(ckb_fixture_non_empty_json_value) {
            issues.push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} execution missing {field}"));
        }
    }
    if !row["execution"]["normalized_fixture"].is_object() {
        issues.push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} missing normalized_fixture"));
    }
    validate_ickb_claim_execution_object(family_id, branch_id, scenario, row, issues);
}

fn validate_ickb_claim_execution_object(
    family_id: &str,
    branch_id: &str,
    scenario: &str,
    row: &serde_json::Value,
    issues: &mut Vec<String>,
) {
    let Some(execution) = row["execution"].as_object() else {
        issues.push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} missing execution object"));
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
        if !execution.get(field).is_some_and(ckb_fixture_non_empty_json_value) {
            issues.push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} execution missing non-empty {field}"));
        }
    }

    for field in ["fixture_sha256", "normalized_fixture_sha256", "original_ickb_binary_sha256", "cellscript_artifact_sha256"] {
        match execution.get(field).and_then(serde_json::Value::as_str) {
            Some(hash) if ckb_fixture_is_canonical_prefixed_sha256(hash) => {}
            _ => issues.push(format!(
                "iCKB claim branch {family_id}/{branch_id} scenario {scenario} execution.{field} must be canonical 0x-prefixed SHA-256"
            )),
        }
    }

    match execution.get("transaction_context_sha256").and_then(serde_json::Value::as_object) {
        Some(hashes) => {
            for side in ["original", "cellscript"] {
                match hashes.get(side).and_then(serde_json::Value::as_str) {
                    Some(hash) if ckb_fixture_is_canonical_prefixed_sha256(hash) => {}
                    _ => issues.push(format!(
                        "iCKB claim branch {family_id}/{branch_id} scenario {scenario} transaction_context_sha256.{side} must be canonical 0x-prefixed SHA-256"
                    )),
                }
            }
        }
        None => issues.push(format!(
            "iCKB claim branch {family_id}/{branch_id} scenario {scenario} execution.transaction_context_sha256 must be an object"
        )),
    }

    if execution.get("statuses_match").and_then(serde_json::Value::as_bool) != Some(true) {
        issues.push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} execution.statuses_match must be true"));
    }

    for (side, expected_field, status_field, exit_field, cycle_field) in [
        ("original", "original_ickb_expected", "original_ickb_status", "original_ickb_exit_code", "original_cycles"),
        ("cellscript", "cellscript_expected", "cellscript_status", "cellscript_exit_code", "cellscript_cycles"),
    ] {
        let expected = row[expected_field].as_str();
        let status = execution.get(status_field).and_then(serde_json::Value::as_str);
        if expected.is_some() && status != expected {
            issues.push(format!(
                "iCKB claim branch {family_id}/{branch_id} scenario {scenario} {side} status {status:?} does not match {expected_field}={expected:?}"
            ));
        }
        if status == Some("pass") {
            if execution.get(exit_field).and_then(serde_json::Value::as_i64) != Some(0) {
                issues
                    .push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} {side} pass must have exit code 0"));
            }
            if execution.get(cycle_field).and_then(serde_json::Value::as_u64).unwrap_or(0) == 0 {
                issues.push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} {side} pass must consume cycles"));
            }
        }
        if status == Some("fail") && execution.get(exit_field).and_then(serde_json::Value::as_i64) == Some(0) {
            issues.push(format!(
                "iCKB claim branch {family_id}/{branch_id} scenario {scenario} {side} fail must have a non-zero exit code"
            ));
        }
    }

    for field in ["tx_size_bytes", "occupied_capacity_shannons"] {
        if execution.get(field).and_then(serde_json::Value::as_u64).unwrap_or(0) == 0 {
            issues.push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} execution.{field} must be positive"));
        }
    }

    if row["original_ickb_expected"] == "fail" || row["cellscript_expected"] == "fail" {
        match execution.get("failure_mode").and_then(serde_json::Value::as_str) {
            Some(mode) if !mode.is_empty() => {}
            _ => issues.push(format!(
                "iCKB claim branch {family_id}/{branch_id} scenario {scenario} reject case missing execution.failure_mode"
            )),
        }
    }
}

fn validate_ickb_evidence_object(
    label: &str,
    required: &[&str],
    object: Option<&serde_json::Value>,
    family_id: &str,
    branch_id: &str,
    issues: &mut Vec<String>,
) {
    let Some(object) = object.and_then(serde_json::Value::as_object) else {
        issues.push(format!("iCKB claim branch {family_id}/{branch_id} missing {label} object"));
        return;
    };
    for field in required {
        if !object.get(*field).is_some_and(ckb_fixture_non_empty_json_value) {
            issues.push(format!("iCKB claim branch {family_id}/{branch_id} {label} missing non-empty {field}"));
        }
    }
}

fn validate_ickb_claim_thresholds(
    family_id: &str,
    branch_id: &str,
    hardening: Option<&serde_json::Value>,
    scenarios: &BTreeSet<String>,
    matrix_rows: &BTreeMap<String, &serde_json::Value>,
    issues: &mut Vec<String>,
) {
    let max_cycles = hardening.and_then(|value| value["max_cellscript_cycles"].as_u64());
    let max_tx_size = hardening.and_then(|value| value["max_tx_size_bytes"].as_u64());
    for scenario in scenarios {
        let Some(row) = matrix_rows.get(scenario) else {
            continue;
        };
        if let (Some(max), Some(actual)) = (max_cycles, row["execution"]["cellscript_cycles"].as_u64())
            && actual > max
        {
            issues.push(format!(
                "iCKB claim branch {family_id}/{branch_id} scenario {scenario} cellscript_cycles {actual} exceeds {max}"
            ));
        }
        if let (Some(max), Some(actual)) = (max_tx_size, row["execution"]["tx_size_bytes"].as_u64())
            && actual > max
        {
            issues.push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} tx_size_bytes {actual} exceeds {max}"));
        }
    }
}

fn json_string_array(value: &serde_json::Value, key: &str) -> Vec<String> {
    value[key].as_array().into_iter().flatten().filter_map(serde_json::Value::as_str).map(ToString::to_string).collect()
}

fn ckb_fixture_non_empty_json_value(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::String(value) => !value.is_empty(),
        serde_json::Value::Array(values) => !values.is_empty(),
        serde_json::Value::Object(values) => !values.is_empty(),
        serde_json::Value::Bool(_) | serde_json::Value::Number(_) => true,
    }
}

fn ckb_fixture_is_canonical_prefixed_sha256(value: &str) -> bool {
    value
        .strip_prefix("0x")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()))
}

fn validate_ckb_fixture_suite(
    suite: &serde_json::Value,
    base_dir: &Path,
    rows: &mut Vec<serde_json::Value>,
    issues: &mut Vec<String>,
) {
    let suite_name = suite["name"].as_str().unwrap_or("<unknown>");
    let accepted = ckb_fixture_names(suite, "accepted_fixtures", issues, suite_name);
    let rejected = ckb_fixture_names(suite, "rejected_fixtures", issues, suite_name);
    let Some(files) = suite["fixture_files"].as_object() else {
        issues.push(format!("suite {suite_name} missing fixture_files object"));
        return;
    };
    for fixture_name in accepted.iter().chain(rejected.iter()) {
        let Some(file) = files.get(*fixture_name).and_then(serde_json::Value::as_str) else {
            issues.push(format!("suite {suite_name} missing fixture file mapping for {fixture_name}"));
            continue;
        };
        let path = base_dir.join(file);
        let fixture = match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(value) => value,
                Err(error) => {
                    issues.push(format!("fixture {file} failed to parse: {error}"));
                    continue;
                }
            },
            Err(error) => {
                issues.push(format!("fixture {file} failed to read: {error}"));
                continue;
            }
        };
        match validate_ckb_fixture_model(&fixture, suite_name, fixture_name, accepted.contains(fixture_name), file) {
            Ok(row) => rows.push(row),
            Err(error) => issues.push(error),
        }
    }
}

fn validate_ckb_fixture_model(
    fixture: &serde_json::Value,
    suite_name: &str,
    fixture_name: &str,
    should_accept: bool,
    file: &str,
) -> std::result::Result<serde_json::Value, String> {
    if fixture["schema"].as_str() != Some(CKB_STANDARD_FIXTURE_SCHEMA) {
        return Err(format!("fixture {file} schema must be {CKB_STANDARD_FIXTURE_SCHEMA}"));
    }
    if fixture["suite"].as_str() != Some(suite_name) {
        return Err(format!("fixture {file} suite does not match manifest suite {suite_name}"));
    }
    if fixture["fixture_name"].as_str() != Some(fixture_name) {
        return Err(format!("fixture {file} fixture_name does not match manifest key {fixture_name}"));
    }

    let shape = &fixture["transaction_shape"];
    ckb_fixture_require_array(shape, "inputs")?;
    ckb_fixture_require_array(shape, "outputs")?;
    ckb_fixture_require_array(shape, "cell_deps")?;
    ckb_fixture_validate_metadata_expectation(fixture)?;
    ckb_fixture_validate_capacity_report(fixture)?;

    let expected = fixture["expected_behavior"].as_object().ok_or_else(|| format!("fixture {file} missing expected_behavior"))?;
    let expected_exit = expected
        .get("script_exit_code")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| format!("fixture {file} missing expected_behavior.script_exit_code"))?;
    let expected_reason = expected.get("rejection_reason").and_then(serde_json::Value::as_str);

    if should_accept && fixture["status"].as_str() != Some("accepted") {
        return Err(format!("fixture {file} is listed accepted but status is not accepted"));
    }
    if !should_accept && fixture["status"].as_str() != Some("rejected") {
        return Err(format!("fixture {file} is listed rejected but status is not rejected"));
    }
    if should_accept && expected_exit != 0 {
        return Err(format!("fixture {file} accepted case has non-zero expected exit code"));
    }
    if !should_accept && expected_exit == 0 {
        return Err(format!("fixture {file} rejected case has zero expected exit code"));
    }
    if !should_accept && expected_reason.is_none() {
        return Err(format!("fixture {file} rejected case lacks expected_behavior.rejection_reason"));
    }

    let verdict = ckb_fixture_evaluate_semantics(fixture)?;
    if verdict.0 != expected_exit {
        return Err(format!("fixture {file} model exit {} disagrees with expected exit {expected_exit}", verdict.0));
    }
    if expected_exit != 0 && verdict.1.as_deref() != expected_reason {
        return Err(format!("fixture {file} model rejection {:?} disagrees with expected {:?}", verdict.1, expected_reason));
    }

    Ok(serde_json::json!({
        "suite": suite_name,
        "fixture_name": fixture_name,
        "file": file,
        "expected_status": fixture["status"].as_str().unwrap_or("unknown"),
        "execution_level": "MODEL",
        "ckb_vm_execution": false,
        "model_exit_code": verdict.0,
        "expected_exit_code": expected_exit,
        "rejection_reason": verdict.1.or_else(|| expected_reason.map(str::to_string)),
        "status": if verdict.0 == 0 { "accepted" } else { "rejected" },
    }))
}

fn ckb_fixture_validate_metadata_expectation(fixture: &serde_json::Value) -> std::result::Result<(), String> {
    let metadata = fixture["metadata_expectation"].as_object().ok_or("fixture missing metadata_expectation")?;
    let proof_plan =
        metadata.get("proof_plan").and_then(serde_json::Value::as_object).ok_or("fixture missing proof_plan expectation")?;
    for key in ["trigger", "scope", "reads", "coverage", "on_chain_checked"] {
        if !proof_plan.contains_key(key) {
            return Err(format!("fixture proof_plan expectation missing {key}"));
        }
    }
    if !metadata.contains_key("codegen_coverage_status") {
        return Err("fixture metadata expectation missing codegen_coverage_status".to_string());
    }
    if fixture.get("cycle_report").is_none() {
        return Err("fixture missing cycle_report".to_string());
    }
    if fixture.get("capacity_report").is_none() {
        return Err("fixture missing capacity_report".to_string());
    }
    Ok(())
}

fn ckb_fixture_evaluate_semantics(fixture: &serde_json::Value) -> std::result::Result<(i64, Option<String>), String> {
    let shape = &fixture["transaction_shape"];
    match fixture["suite"].as_str().ok_or("missing suite")? {
        "sudt" => ckb_fixture_evaluate_amount_conservation(shape, "sudt-cell", "output_amount > input_amount; conservation violated"),
        "xudt" => {
            if ckb_fixture_any_cell_dep_name(shape, "lockup") || ckb_fixture_any_input_witness(shape, "lockup-active") {
                return Ok(ckb_fixture_reject("extension_policy_violated: lockup period not expired"));
            }
            ckb_fixture_evaluate_amount_conservation(shape, "xudt-cell", "output_amount > input_amount; conservation violated")
        }
        "acp" => {
            let first_input = ckb_fixture_first_cell(shape, "inputs")?;
            let first_output = ckb_fixture_first_cell(shape, "outputs")?;
            if ckb_fixture_cell_str(first_input, "witness").contains("wrong")
                || ckb_fixture_cell_str(first_input, "lock_script") != ckb_fixture_cell_str(first_output, "lock_script")
            {
                return Ok(ckb_fixture_reject("witness_lock_hash != args_owner_lock_hash"));
            }
            Ok(ckb_fixture_pass())
        }
        "cheque" => {
            let first_input = ckb_fixture_first_cell(shape, "inputs")?;
            let first_output = ckb_fixture_first_cell(shape, "outputs")?;
            if ckb_fixture_cell_str(first_input, "witness").contains("wrong")
                || ckb_fixture_cell_str(first_output, "lock_script").contains("wrong")
            {
                return Ok(ckb_fixture_reject("receiver_lock_hash != args_receiver_hash"));
            }
            Ok(ckb_fixture_pass())
        }
        "omnilock" => {
            if ckb_fixture_any_input_witness(shape, "invalid") {
                Ok(ckb_fixture_reject("auth_verification_failed: invalid_signature_or_wrong_method"))
            } else {
                Ok(ckb_fixture_pass())
            }
        }
        "nervosdao-since" => {
            if shape["header_deps"].as_array().into_iter().flatten().any(|header| header.as_str() == Some("mature-epoch-header")) {
                Ok(ckb_fixture_pass())
            } else {
                Ok(ckb_fixture_reject("since_not_mature: current_epoch < required_epoch"))
            }
        }
        "type-id" => {
            let type_id_outputs = shape["outputs"]
                .as_array()
                .ok_or("missing outputs")?
                .iter()
                .filter(|output| ckb_fixture_cell_str(output, "type_script").starts_with("type-id-script"))
                .count();
            if type_id_outputs > 1 {
                Ok(ckb_fixture_reject("duplicate_type_id: at-most-one-input-and-one-output-per-type-id-group"))
            } else {
                Ok(ckb_fixture_pass())
            }
        }
        other => Err(format!("unsupported compat fixture suite {other}")),
    }
}

fn ckb_fixture_evaluate_amount_conservation(
    shape: &serde_json::Value,
    cell_type: &str,
    reason: &str,
) -> std::result::Result<(i64, Option<String>), String> {
    let input_sum = ckb_fixture_amount_sum(shape, "inputs", cell_type)?;
    let output_sum = ckb_fixture_amount_sum(shape, "outputs", cell_type)?;
    if output_sum > input_sum {
        Ok(ckb_fixture_reject(reason))
    } else {
        Ok(ckb_fixture_pass())
    }
}

fn ckb_fixture_amount_sum(shape: &serde_json::Value, side: &str, cell_type: &str) -> std::result::Result<u128, String> {
    shape[side]
        .as_array()
        .ok_or_else(|| format!("missing transaction_shape.{side}"))?
        .iter()
        .filter(|cell| ckb_fixture_cell_str(cell, "type") == cell_type)
        .try_fold(0u128, |total, cell| Ok(total + ckb_fixture_little_endian_u128(ckb_fixture_cell_str(cell, "data"))?))
}

fn ckb_fixture_little_endian_u128(hex_value: &str) -> std::result::Result<u128, String> {
    let bytes = hex_value.strip_prefix("0x").unwrap_or(hex_value);
    if bytes.is_empty() {
        return Ok(0);
    }
    if !bytes.len().is_multiple_of(2) {
        return Err(format!("odd-length hex amount {hex_value}"));
    }
    let raw = hex::decode(bytes).map_err(|err| format!("invalid hex amount {hex_value}: {err}"))?;
    if raw.len() > 16 {
        return Err(format!("amount data exceeds u128 width: {} bytes", raw.len()));
    }
    let mut padded = [0u8; 16];
    padded[..raw.len()].copy_from_slice(&raw);
    Ok(u128::from_le_bytes(padded))
}

fn ckb_fixture_validate_capacity_report(fixture: &serde_json::Value) -> std::result::Result<(), String> {
    let reported = fixture["capacity_report"]["occupied_capacity_shannons"]
        .as_u64()
        .ok_or("capacity_report missing occupied_capacity_shannons")?;
    let output_capacity = fixture["transaction_shape"]["outputs"]
        .as_array()
        .ok_or("missing outputs")?
        .iter()
        .map(|output| output["capacity_shannons"].as_u64().ok_or("output missing capacity_shannons"))
        .try_fold(0u64, |total, value| value.map(|value| total.saturating_add(value)))?;
    if reported > output_capacity {
        return Err(format!("capacity report occupied capacity {reported} exceeds output capacity {output_capacity}"));
    }
    Ok(())
}

fn ckb_fixture_names<'a>(suite: &'a serde_json::Value, key: &str, issues: &mut Vec<String>, suite_name: &str) -> BTreeSet<&'a str> {
    match suite[key].as_array() {
        Some(values) => values.iter().filter_map(serde_json::Value::as_str).collect(),
        None => {
            issues.push(format!("suite {suite_name} missing {key} array"));
            BTreeSet::new()
        }
    }
}

fn ckb_fixture_require_array(value: &serde_json::Value, key: &str) -> std::result::Result<(), String> {
    value[key].as_array().map(|_| ()).ok_or_else(|| format!("missing transaction_shape.{key}"))
}

fn ckb_fixture_first_cell<'a>(shape: &'a serde_json::Value, side: &str) -> std::result::Result<&'a serde_json::Value, String> {
    shape[side].as_array().and_then(|cells| cells.first()).ok_or_else(|| format!("missing first transaction_shape.{side} cell"))
}

fn ckb_fixture_cell_str<'a>(cell: &'a serde_json::Value, field: &str) -> &'a str {
    cell[field].as_str().unwrap_or("")
}

fn ckb_fixture_any_cell_dep_name(shape: &serde_json::Value, needle: &str) -> bool {
    shape["cell_deps"].as_array().into_iter().flatten().any(|dep| dep["name"].as_str().is_some_and(|name| name.contains(needle)))
}

fn ckb_fixture_any_input_witness(shape: &serde_json::Value, needle: &str) -> bool {
    shape["inputs"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|input| input["witness"].as_str().is_some_and(|witness| witness.contains(needle)))
}

fn ckb_fixture_pass() -> (i64, Option<String>) {
    (0, None)
}

fn ckb_fixture_reject(reason: &str) -> (i64, Option<String>) {
    (1, Some(reason.to_string()))
}
