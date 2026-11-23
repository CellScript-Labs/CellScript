use super::fiber_provenance::{
    bruno_compatibility_patch_contract, fiber_repo_git_provenance, relative_file_array_all_exist, relative_file_exists,
};
use super::provenance::provenance_summary;
use super::{
    exact_string_set, is_git_commit_hash, json_array_strings, json_pointer_bool, json_pointer_bool_opt, json_pointer_i64,
    json_pointer_str, negative_case_matched, object_values_all_true, tx_hash_value_is_real, value_is_present,
    EXPECTED_BTC_SPV_EVIDENCE_PROFILES, EXPECTED_FIBER_NODE_EXECUTION_SCHEMA, EXPECTED_FIBER_NODE_PROFILES,
    EXPECTED_FIBER_REPO_ORIGIN, EXPECTED_FIBER_WORKFLOWS, FIBER_NODE_EXPERIMENTS,
};
use crate::error::Result;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

pub(super) fn live_planned_profile_summary(
    repo_root: &Path,
    report: Option<&Value>,
    source_paths: &[&str],
    tx_hashes: &[(&str, &str)],
    required_bools: &[(&str, &str)],
    negative_cases: &[(&str, &str)],
) -> Result<Value> {
    let expected_tx_hashes = tx_hashes.iter().map(|(name, pointer)| json!({"name": name, "pointer": pointer})).collect::<Vec<_>>();
    let required_live_checks =
        required_bools.iter().map(|(name, pointer)| json!({"name": name, "pointer": pointer})).collect::<Vec<_>>();
    let required_negative_cases = negative_cases.iter().map(|(name, key)| json!({"name": name, "key": key})).collect::<Vec<_>>();

    let Some(report) = report else {
        return Ok(json!({
            "present": false,
            "expected_tx_hashes": expected_tx_hashes,
            "required_live_checks": required_live_checks,
            "required_negative_cases": required_negative_cases,
            "required_live_checks_passed": false,
        }));
    };
    if report.get("_invalid_json").is_some() {
        return Ok(json!({
            "present": true,
            "valid_json": false,
            "error": report.get("_invalid_json"),
            "expected_tx_hashes": expected_tx_hashes,
            "required_live_checks": required_live_checks,
            "required_negative_cases": required_negative_cases,
            "required_live_checks_passed": false,
        }));
    }

    let provenance = provenance_summary(report, repo_root, source_paths)?;
    let mut tx_hash_summary = Map::new();
    for (name, pointer) in tx_hashes {
        tx_hash_summary.insert((*name).to_string(), json_pointer_str(report, pointer).map(Value::from).unwrap_or(Value::Null));
    }

    let mut live_checks = Map::new();
    for (name, pointer) in required_bools {
        live_checks.insert((*name).to_string(), Value::Bool(json_pointer_bool(report, pointer)));
    }

    let mut negative_checks = Map::new();
    for (name, key) in negative_cases {
        negative_checks.insert((*name).to_string(), negative_case_matched(report, key).map(Value::Bool).unwrap_or(Value::Null));
    }

    let status_passed = json_pointer_str(report, "/status") == Some("passed");
    let rpc_executed = json_pointer_bool(report, "/live_devnet_rpc_executed");
    let lifecycle_executed = json_pointer_bool(report, "/stateful_lifecycle_executed");
    let provenance_freshness_matched = json_pointer_bool(&provenance, "/freshness_matched");
    let tx_hashes_present = tx_hash_summary.values().all(tx_hash_value_is_real);
    let required_bools_passed = live_checks.values().all(|value| value.as_bool() == Some(true));
    let negative_cases_passed = negative_checks.values().all(|value| value.as_bool() == Some(true));
    let required_live_checks_passed = status_passed
        && rpc_executed
        && lifecycle_executed
        && provenance_freshness_matched
        && tx_hashes_present
        && required_bools_passed
        && negative_cases_passed;

    Ok(json!({
        "present": true,
        "valid_json": true,
        "status": json_pointer_str(report, "/status"),
        "live_devnet_rpc_executed": rpc_executed,
        "stateful_lifecycle_executed": lifecycle_executed,
        "provenance": provenance,
        "provenance_freshness_matched": provenance_freshness_matched,
        "expected_tx_hashes": expected_tx_hashes,
        "required_live_checks": required_live_checks,
        "required_negative_cases": required_negative_cases,
        "tx_hashes": tx_hash_summary,
        "live_checks": live_checks,
        "negative_cases": negative_checks,
        "required_live_checks_passed": required_live_checks_passed,
    }))
}

pub(super) fn fiber_node_execution_summary(repo_root: &Path, report: Option<&Value>) -> Value {
    let Some(report) = report else {
        return json!({
            "present": false,
            "status": "missing",
            "discovery_ready": false,
            "all_required_workflows_executed_passed": false,
            "required_report": FIBER_NODE_EXPERIMENTS,
        });
    };
    if report.get("_invalid_json").is_some() {
        return json!({
            "present": true,
            "valid_json": false,
            "error": report.get("_invalid_json"),
            "discovery_ready": false,
            "all_required_workflows_executed_passed": false,
            "required_report": FIBER_NODE_EXPERIMENTS,
        });
    }

    let workflows = report.get("workflows").and_then(Value::as_array).cloned().unwrap_or_default();
    let workflow_suites =
        workflows.iter().filter_map(|workflow| json_pointer_str(workflow, "/suite").map(str::to_string)).collect::<Vec<_>>();
    let workflow_suites_exact =
        exact_string_set(&workflow_suites, &EXPECTED_FIBER_WORKFLOWS.iter().map(|(suite, _)| *suite).collect::<Vec<_>>());
    let workflows_by_suite = workflows
        .iter()
        .filter_map(|workflow| json_pointer_str(workflow, "/suite").map(|suite| (suite.to_string(), workflow)))
        .collect::<BTreeMap<_, _>>();
    let duplicate_free_workflow_suites = workflows_by_suite.len() == workflow_suites.len();
    let fiber_repo_path = json_pointer_str(report, "/fiber_repo/path").map(Path::new);
    let fiber_repo_exists = fiber_repo_path.is_some_and(Path::is_dir);
    let fiber_repo_git = fiber_repo_git_provenance(fiber_repo_path, report);
    let fiber_repo_current_checkout_matches_report = json_pointer_bool(&fiber_repo_git, "/verified");

    let mut workflow_checks = Map::new();
    let mut failed_workflows = Vec::new();
    for (suite, expected_profiles) in EXPECTED_FIBER_WORKFLOWS {
        let Some(workflow) = workflows_by_suite.get(*suite) else {
            failed_workflows.push(Value::String((*suite).to_string()));
            workflow_checks.insert((*suite).to_string(), json!({ "present": false }));
            continue;
        };
        let mapped_profiles = json_array_strings(workflow, "/mapped_profiles");
        let evidence_files_present = workflow
            .get("evidence_files")
            .and_then(Value::as_array)
            .is_some_and(|files| !files.is_empty() && files.iter().all(|file| file.as_str().is_some_and(|file| !file.is_empty())));
        let evidence_files_exist =
            fiber_repo_path.is_some_and(|fiber_repo| relative_file_array_all_exist(fiber_repo, workflow.get("evidence_files"), true));
        let rpc_methods_present = workflow.get("rpc_methods").and_then(Value::as_array).is_some_and(|methods| {
            !methods.is_empty() && methods.iter().all(|method| method.as_str().is_some_and(|method| !method.is_empty()))
        });
        let execution_logs_present = value_is_present(workflow.pointer("/execution/stdout_log").unwrap_or(&Value::Null))
            && value_is_present(workflow.pointer("/execution/stderr_log").unwrap_or(&Value::Null));
        let execution_logs_exist = relative_file_exists(repo_root, json_pointer_str(workflow, "/execution/stdout_log"), true)
            && relative_file_exists(repo_root, json_pointer_str(workflow, "/execution/stderr_log"), false);
        let expected_command = json!(["npm", "exec", "--", "@usebruno/cli", "run", format!("e2e/{suite}"), "-r", "--env", "test"]);
        let execution_started_node = json_pointer_bool(workflow, "/execution/started_node");
        let execution_command_exact = workflow.pointer("/execution/command") == Some(&expected_command);
        let execution_returncode_zero = json_pointer_i64(workflow, "/execution/returncode") == Some(0);
        let execution_duration_positive =
            workflow.pointer("/execution/duration_seconds").and_then(Value::as_f64).is_some_and(|duration| duration > 0.0);
        let execution_fiber_repo_matches_report = json_pointer_str(workflow, "/execution/fiber_repo/path")
            .is_some_and(|path| json_pointer_str(report, "/fiber_repo/path") == Some(path))
            && json_pointer_str(workflow, "/execution/fiber_repo/origin")
                .is_some_and(|origin| json_pointer_str(report, "/fiber_repo/origin") == Some(origin))
            && json_pointer_str(workflow, "/execution/fiber_repo/branch")
                .is_some_and(|branch| json_pointer_str(report, "/fiber_repo/branch") == Some(branch))
            && json_pointer_str(workflow, "/execution/fiber_repo/commit")
                .is_some_and(|commit| is_git_commit_hash(commit) && json_pointer_str(report, "/fiber_repo/commit") == Some(commit))
            && json_pointer_bool_opt(workflow, "/execution/fiber_repo/dirty")
                .is_some_and(|dirty| json_pointer_bool_opt(report, "/fiber_repo/dirty") == Some(dirty));
        let bruno_compatibility_patch_files_exist = bruno_compatibility_patch_contract(
            repo_root,
            json_pointer_str(workflow, "/execution/bruno_cwd"),
            workflow.pointer("/execution/bruno_compatibility_patches"),
        );
        let checks = json!({
            "present": json_pointer_bool(workflow, "/present"),
            "status_passed": json_pointer_str(workflow, "/status") == Some("passed"),
            "execution_passed": json_pointer_str(workflow, "/execution/status") == Some("passed"),
            "execution_started_node": execution_started_node,
            "execution_command_exact": execution_command_exact,
            "execution_returncode_zero": execution_returncode_zero,
            "execution_duration_positive": execution_duration_positive,
            "execution_fiber_repo_matches_report": execution_fiber_repo_matches_report,
            "mapped_profiles_exact": exact_string_set(&mapped_profiles, expected_profiles),
            "expected_terms_present": object_values_all_true(workflow.get("expected_terms")),
            "evidence_files_present": evidence_files_present,
            "evidence_files_exist": evidence_files_exist,
            "rpc_methods_present": rpc_methods_present,
            "execution_logs_present": execution_logs_present,
            "execution_logs_exist": execution_logs_exist,
            "bruno_compatibility_patch_files_exist": bruno_compatibility_patch_files_exist,
        });
        if !object_values_all_true(Some(&checks)) {
            failed_workflows.push(Value::String((*suite).to_string()));
        }
        workflow_checks.insert((*suite).to_string(), checks);
    }

    let all_present = json_pointer_bool(report, "/workflow_coverage/all_required_workflows_present");
    let runnable_devnet_contract_present = json_pointer_bool(report, "/devnet_contract/runnable_devnet_contract_present");
    let all_executed_passed_reported = json_pointer_bool(report, "/workflow_coverage/all_required_workflows_executed_passed");
    let partial_execution_passed_reported = json_pointer_bool(report, "/workflow_coverage/partial_execution_passed");
    let profiles_covered = json_array_strings(report, "/profiles_covered");
    let schema_ok = json_pointer_str(report, "/schema") == Some(EXPECTED_FIBER_NODE_EXECUTION_SCHEMA);
    let status_passed = json_pointer_str(report, "/status") == Some("passed");
    let recorded_fiber_repo_provenance_verified = json_pointer_str(report, "/fiber_repo/origin") == Some(EXPECTED_FIBER_REPO_ORIGIN)
        && json_pointer_str(report, "/fiber_repo/branch").is_some_and(|branch| !branch.is_empty())
        && json_pointer_str(report, "/fiber_repo/commit").is_some_and(is_git_commit_hash)
        && !json_pointer_bool(report, "/fiber_repo/dirty");
    let clean_expected_repo = fiber_repo_exists && recorded_fiber_repo_provenance_verified;
    let required_count = json_pointer_i64(report, "/workflow_coverage/required_count");
    let present_count = json_pointer_i64(report, "/workflow_coverage/present_count");
    let executed_count = json_pointer_i64(report, "/workflow_coverage/executed_count");
    let passed_execution_count = json_pointer_i64(report, "/workflow_coverage/passed_execution_count");
    let expected_workflow_count = EXPECTED_FIBER_WORKFLOWS.len() as i64;
    let count_contract_exact = required_count == Some(expected_workflow_count)
        && present_count == Some(expected_workflow_count)
        && executed_count == Some(expected_workflow_count)
        && passed_execution_count == Some(expected_workflow_count);
    let partial_execution_contract_exact = match (required_count, executed_count, passed_execution_count) {
        (Some(required), Some(executed), Some(passed)) => executed > 0 && executed < required && passed == executed,
        _ => false,
    };
    let reported_partial_execution_semantics =
        !partial_execution_passed_reported || (!all_executed_passed_reported && partial_execution_contract_exact);
    let profiles_exact = exact_string_set(&profiles_covered, EXPECTED_FIBER_NODE_PROFILES);
    let workflow_rows_passed = workflow_suites_exact && duplicate_free_workflow_suites && failed_workflows.is_empty();
    let discovery_ready =
        schema_ok && all_present && runnable_devnet_contract_present && workflow_suites_exact && duplicate_free_workflow_suites;
    let all_executed_passed = discovery_ready
        && status_passed
        && clean_expected_repo
        && count_contract_exact
        && reported_partial_execution_semantics
        && profiles_exact
        && all_executed_passed_reported
        && workflow_rows_passed;
    let checks = json!({
        "schema_ok": schema_ok,
        "status_passed": status_passed,
        "clean_expected_fiber_repo": clean_expected_repo,
        "fiber_repo_exists": fiber_repo_exists,
        "fiber_repo_git_provenance_verified": recorded_fiber_repo_provenance_verified,
        "fiber_repo_current_checkout_matches_report": fiber_repo_current_checkout_matches_report,
        "runnable_devnet_contract_present": runnable_devnet_contract_present,
        "coverage_counts_exact": count_contract_exact,
        "reported_partial_execution_semantics": reported_partial_execution_semantics,
        "profiles_covered_exact": profiles_exact,
        "workflow_suites_exact": workflow_suites_exact,
        "duplicate_free_workflow_suites": duplicate_free_workflow_suites,
        "workflow_rows_passed": workflow_rows_passed,
        "reported_all_required_workflows_present": all_present,
        "reported_all_required_workflows_executed_passed": all_executed_passed_reported,
    });
    json!({
        "present": true,
        "valid_json": true,
        "schema": json_pointer_str(report, "/schema"),
        "status": json_pointer_str(report, "/status"),
        "fiber_repo": {
            "path": json_pointer_str(report, "/fiber_repo/path"),
            "origin": json_pointer_str(report, "/fiber_repo/origin"),
            "branch": json_pointer_str(report, "/fiber_repo/branch"),
            "commit": json_pointer_str(report, "/fiber_repo/commit"),
            "dirty": json_pointer_bool(report, "/fiber_repo/dirty"),
        },
        "fiber_repo_git_provenance": fiber_repo_git,
        "workflow_coverage": report.get("workflow_coverage").cloned().unwrap_or(Value::Null),
        "profiles_covered": report.get("profiles_covered").cloned().unwrap_or(Value::Null),
        "tooling": report.get("tooling").cloned().unwrap_or(Value::Null),
        "checks": checks,
        "workflow_checks": workflow_checks,
        "failed_workflows": failed_workflows,
        "expected_workflows": EXPECTED_FIBER_WORKFLOWS.iter().map(|(suite, _)| *suite).collect::<Vec<_>>(),
        "expected_profiles": EXPECTED_FIBER_NODE_PROFILES,
        "discovery_ready": discovery_ready,
        "partial_execution_passed": partial_execution_passed_reported && partial_execution_contract_exact && !all_executed_passed,
        "all_required_workflows_executed_passed": all_executed_passed,
        "execution_boundary": "discovery_ready is not live Fiber devnet evidence; all_required_workflows_executed_passed requires exact suite/profile coverage, clean Nervos Fiber provenance, runnable devnet tooling, and per-workflow started-node execution with the exact Bruno command, returncode 0, positive duration, and persisted logs",
        "required_report": FIBER_NODE_EXPERIMENTS,
    })
}

pub(super) fn external_endpoint_coverage_summary(
    btc_spv_evidence: &Value,
    fiber_node_experiments: &Value,
    live_btc_tx_commitment: &Value,
    live_btc_utxo_seal: &Value,
    live_dual_seal: &Value,
    live_fiber_candidate: &Value,
) -> Value {
    let btc_ckb_lifecycle_passed = json_pointer_bool(live_btc_tx_commitment, "/required_live_checks_passed")
        && json_pointer_bool(live_btc_utxo_seal, "/required_live_checks_passed")
        && json_pointer_bool(live_dual_seal, "/required_live_checks_passed");
    let public_btc_spv_status = json_pointer_str(btc_spv_evidence, "/status").unwrap_or("missing");
    let public_btc_spv_passed = public_btc_spv_status == "passed";
    let public_btc_spv_external_required = public_btc_spv_status == "external_required";
    let fiber_ckb_lifecycle_passed = json_pointer_bool(live_fiber_candidate, "/required_live_checks_passed");
    let fiber_git_provenance_verified = json_pointer_bool(fiber_node_experiments, "/checks/fiber_repo_git_provenance_verified");
    let fiber_node_workflows_passed =
        json_pointer_bool(fiber_node_experiments, "/all_required_workflows_executed_passed") && fiber_git_provenance_verified;

    let btc_status = if !btc_ckb_lifecycle_passed {
        "failed"
    } else if public_btc_spv_passed {
        "passed"
    } else if public_btc_spv_external_required {
        "external_required"
    } else {
        "failed"
    };
    let fiber_status = if !fiber_ckb_lifecycle_passed {
        "failed"
    } else if fiber_node_workflows_passed {
        "passed"
    } else {
        "external_required"
    };
    let status = if btc_status == "passed" && fiber_status == "passed" {
        "passed"
    } else if matches!(btc_status, "passed" | "external_required") && matches!(fiber_status, "passed" | "external_required") {
        "external_required"
    } else {
        "failed"
    };

    json!({
        "schema": "novaseal-external-endpoint-coverage-v0.1",
        "status": status,
        "btc": {
            "status": btc_status,
            "ckb_profile_lifecycle_passed": btc_ckb_lifecycle_passed,
            "public_spv_evidence_passed": public_btc_spv_passed,
            "public_spv_evidence_status": public_btc_spv_status,
            "required_profiles": EXPECTED_BTC_SPV_EVIDENCE_PROFILES,
            "evidence": {
                "status": public_btc_spv_status,
                "required_report": json_pointer_str(btc_spv_evidence, "/required_report"),
                "reason": json_pointer_str(btc_spv_evidence, "/reason"),
                "template": json_pointer_str(btc_spv_evidence, "/template"),
            },
            "boundary": "CKB live lifecycle evidence only proves the NovaSeal BTC-facing profile transitions and BIP340 intent verification. Production requires public BTC SPV evidence for inclusion, spend validity, and confirmation depth.",
        },
        "fiber": {
            "status": fiber_status,
            "ckb_profile_lifecycle_passed": fiber_ckb_lifecycle_passed,
            "all_required_workflows_executed_passed": fiber_node_workflows_passed,
            "workflow_coverage": fiber_node_experiments.get("workflow_coverage").cloned().unwrap_or(Value::Null),
            "fiber_repo": fiber_node_experiments.get("fiber_repo").cloned().unwrap_or(Value::Null),
            "fiber_repo_git_provenance": fiber_node_experiments.get("fiber_repo_git_provenance").cloned().unwrap_or(Value::Null),
            "boundary": "Fiber coverage requires the NovaSeal CKB candidate lifecycle and the separate Nervos Fiber devnet Bruno workflow suite to pass.",
        },
        "checks": {
            "btc_ckb_lifecycle_passed": btc_ckb_lifecycle_passed,
            "public_btc_spv_evidence_passed": public_btc_spv_passed,
            "fiber_ckb_lifecycle_passed": fiber_ckb_lifecycle_passed,
            "fiber_git_provenance_verified": fiber_git_provenance_verified,
            "fiber_node_workflows_passed": fiber_node_workflows_passed,
        },
        "production_complete": status == "passed",
        "boundary": "This is stricter than local stateful CKB acceptance: BTC and Fiber external endpoint evidence is reported explicitly and production fails closed while public BTC SPV evidence is missing.",
    })
}
