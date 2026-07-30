//! NovaSeal service-builder fixture generator.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};

use crate::btc_anchor::public_btc_anchor_shape_matches_profile;
use crate::crypto::{canonical_report_hash, nonzero_hex32};
use crate::shared::{lexical_path, stable_json_pretty};

const REPORT_PERSON: &[u8] = b"NovaSvcBuildV0";

fn report_hash(label: &str, value: &Value) -> Result<String> {
    canonical_report_hash(REPORT_PERSON, label, value)
}

fn required<'value>(object: &'value Map<String, Value>, key: &str) -> Result<&'value Value> {
    object.get(key).with_context(|| format!("operator fixture case is missing {key}"))
}

fn required_string<'value>(object: &'value Map<String, Value>, key: &str) -> Result<&'value str> {
    required(object, key)?.as_str().with_context(|| format!("operator fixture case field {key} is not a string"))
}

fn external_inputs(profile: &str) -> Vec<&'static str> {
    let mut required = vec!["public_shared_cell_dep_attestation", "external_bip340_tcb_review_attestation"];
    if matches!(profile, "btc-transaction-commitment-profile-v0" | "btc-utxo-seal-profile-v0" | "dual-seal-profile-v0") {
        required.push("public_btc_spv_evidence");
    }
    if profile == "rwa-receipt-profile-v0" {
        required.push("legal_registry_review_evidence");
    }
    required
}

fn build_case(operator_case: &Value) -> Result<Value> {
    let operator = operator_case.as_object().context("operator fixture case is not an object")?;
    let profile = required_string(operator, "profile")?;
    let action = required_string(operator, "action")?;
    let fixture = required_string(operator, "fixture")?;
    let signers = required(operator, "signers")?.clone();
    let operator_fixture_hash = report_hash("operator_case", operator_case)?;
    let idempotency = json!([profile, action, fixture, required(operator, "signed_intent_hash")?,]);
    let required_live_inputs = json!({
        "live_report_hash": operator.get("live_report_hash").cloned().unwrap_or(Value::Null),
        "live_devnet_tx_hash": operator.get("live_devnet_tx_hash").cloned().unwrap_or(Value::Null),
        "fiber_report_hash": operator.get("fiber_report_hash").cloned().unwrap_or(Value::Null),
        "public_btc_anchor": operator.get("public_btc_anchor").cloned().unwrap_or(Value::Null),
    });
    let request = json!({
        "schema": "novaseal-service-builder-request-v0.1",
        "builder_name": "novaseal-profile-service-builder-v0",
        "profile": profile,
        "action": action,
        "fixture": fixture,
        "idempotency_key": report_hash("idempotency", &idempotency)?,
        "operator_fixture_hash": operator_fixture_hash,
        "signers": signers,
        "required_profile_inputs": {
            "source_tree_hash": required(operator, "source_tree_hash")?,
            "schema_set_hash": required(operator, "schema_set_hash")?,
            "proof_matrix_hash": required(operator, "proof_matrix_hash")?,
            "fixture_hash": required(operator, "fixture_hash")?,
        },
        "required_live_inputs": required_live_inputs,
        "production_external_inputs": external_inputs(profile),
    });
    let tx_skeleton = json!({
        "schema": "novaseal-service-builder-tx-skeleton-v0.1",
        "profile": profile,
        "action": action,
        "fixture": fixture,
        "builder_name": "novaseal-profile-service-builder-v0",
        "operator_fixture_hash": operator_fixture_hash,
        "signed_intent_hash": required(operator, "signed_intent_hash")?,
        "witness_shape_hash": required(operator, "witness_shape_hash")?,
        "source_tree_hash": required(operator, "source_tree_hash")?,
        "live_devnet_tx_hash": operator.get("live_devnet_tx_hash").cloned().unwrap_or(Value::Null),
        "public_btc_anchor": operator.get("public_btc_anchor").cloned().unwrap_or(Value::Null),
    });
    let tx_skeleton_hash = report_hash("tx_skeleton", &tx_skeleton)?;
    let receipt_binding = json!({
        "profile": profile,
        "action": action,
        "fixture": fixture,
        "signed_intent_hash": required(operator, "signed_intent_hash")?,
        "tx_skeleton_hash": tx_skeleton_hash,
        "operator_fixture_hash": operator_fixture_hash,
    });
    let builder_trace = json!({"request": request, "tx_skeleton": tx_skeleton});
    let service_queue = json!([profile, action, fixture, request["idempotency_key"]]);
    let response = json!({
        "schema": "novaseal-service-builder-response-v0.1",
        "builder_name": "novaseal-profile-service-builder-v0",
        "profile": profile,
        "action": action,
        "fixture": fixture,
        "service_queue_key": report_hash("service_queue", &service_queue)?,
        "tx_skeleton_hash": tx_skeleton_hash,
        "witness_shape_hash": required(operator, "witness_shape_hash")?,
        "signed_intent_hash": required(operator, "signed_intent_hash")?,
        "bip340_message_hash": required(operator, "bip340_message_hash")?,
        "receipt_binding_hash": report_hash("receipt_binding", &receipt_binding)?,
        "builder_trace_hash": report_hash("builder_trace", &builder_trace)?,
    });
    let production_inputs = request["production_external_inputs"].as_array().context("production inputs are not an array")?;
    let btc_required = production_inputs.iter().any(|item| item.as_str() == Some("public_btc_spv_evidence"));
    let request_anchor = request["required_live_inputs"].get("public_btc_anchor");
    let skeleton_anchor = tx_skeleton.get("public_btc_anchor");
    let profile_inputs_valid =
        request["required_profile_inputs"].as_object().context("profile inputs are not an object")?.values().all(nonzero_hex32);
    let signed_intent = response.get("signed_intent_hash").context("response signed intent is missing")?;
    let bip340_message = response.get("bip340_message_hash").context("response BIP340 message is missing")?;
    let witness_shape = response.get("witness_shape_hash").context("response witness shape is missing")?;
    let checks = json!({
        "operator_case_passed": operator.get("status").and_then(Value::as_str) == Some("passed"),
        "request_hashes_present": profile_inputs_valid,
        "signed_intent_hash_bound": nonzero_hex32(signed_intent) && signed_intent == required(operator, "signed_intent_hash")?,
        "bip340_message_hash_bound": nonzero_hex32(bip340_message) && bip340_message == required(operator, "bip340_message_hash")?,
        "witness_shape_hash_bound": nonzero_hex32(witness_shape) && witness_shape == required(operator, "witness_shape_hash")?,
        "tx_skeleton_hash_present": response.get("tx_skeleton_hash").is_some_and(nonzero_hex32),
        "receipt_binding_hash_present": response.get("receipt_binding_hash").is_some_and(nonzero_hex32),
        "service_queue_key_present": response.get("service_queue_key").is_some_and(nonzero_hex32),
        "external_requirements_named": !production_inputs.is_empty(),
        "public_btc_anchor_bound_when_required": !btc_required || request_anchor.is_some_and(|anchor| !anchor.is_null() && anchor.as_bool() != Some(false)),
        "public_btc_anchor_shape_matches_profile": !btc_required || public_btc_anchor_shape_matches_profile(profile, request_anchor),
        "tx_skeleton_public_btc_anchor_shape_matches_profile": !btc_required || public_btc_anchor_shape_matches_profile(profile, skeleton_anchor),
    });
    let passed = checks.as_object().context("checks are not an object")?.values().all(|check| check == &Value::Bool(true));
    Ok(json!({
        "profile": profile,
        "action": action,
        "fixture": fixture,
        "status": if passed { "passed" } else { "failed" },
        "checks": checks,
        "builder_name": "novaseal-profile-service-builder-v0",
        "operator_fixture_hash": operator_fixture_hash,
        "signers": signers,
        "request": request,
        "response": response,
        "tx_skeleton": tx_skeleton,
    }))
}

fn build_report(operator_fixtures: &Value) -> Result<Value> {
    let cases = operator_fixtures
        .get("cases")
        .and_then(Value::as_array)
        .map(|cases| cases.iter().map(build_case).collect::<Result<Vec<_>>>())
        .transpose()?
        .unwrap_or_default();
    let profiles: BTreeSet<&str> = cases.iter().filter_map(|case| case.get("profile").and_then(Value::as_str)).collect();
    let passed = !cases.is_empty() && cases.iter().all(|case| case.get("status").and_then(Value::as_str) == Some("passed"));
    let matched = cases.iter().filter(|case| case.get("status").and_then(Value::as_str) == Some("passed")).count();
    Ok(json!({
        "schema": "novaseal-service-builder-fixtures-v0.1",
        "status": if passed { "passed" } else { "failed" },
        "builder_name": "novaseal-profile-service-builder-v0",
        "source_operator_fixture_report": "target/novaseal-profile-operator-fixtures.json",
        "source_operator_fixture_report_hash": report_hash("operator_report", operator_fixtures)?,
        "fixture_boundary": "builder fixtures model reproducible service request/response hashes for local profile evidence; public BTC SPV, public CellDep, external TCB, and legal registry evidence remain production inputs",
        "summary": {
            "total": cases.len(),
            "matched": matched,
            "profile_count": profiles.len(),
            "profiles": profiles,
        },
        "profiles": profiles,
        "cases": cases,
    }))
}

pub fn run(root: &Path, operator_fixtures: Option<&Path>, output: Option<&Path>, pretty: bool) -> Result<i32> {
    let default_operator = root.join("target/novaseal-profile-operator-fixtures.json");
    let default_output = root.join("target/novaseal-service-builder-fixtures.json");
    let operator_path = lexical_path(operator_fixtures.unwrap_or(&default_operator));
    let output_path = lexical_path(output.unwrap_or(&default_output));
    let operator: Value =
        serde_json::from_slice(&fs::read(&operator_path).with_context(|| format!("failed to read {}", operator_path.display()))?)
            .with_context(|| format!("{} is not valid JSON", operator_path.display()))?;
    let report = build_report(&operator)?;
    let parent = output_path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    fs::write(&output_path, format!("{}\n", stable_json_pretty(&report)?))
        .with_context(|| format!("failed to write {}", output_path.display()))?;
    if pretty {
        println!(
            "wrote {} status={} profiles={} cases={}",
            output_path.display(),
            report["status"].as_str().unwrap_or("failed"),
            report["summary"]["profile_count"].as_u64().unwrap_or(0),
            report["summary"]["total"].as_u64().unwrap_or(0),
        );
    }
    Ok(if report["status"] == "passed" { 0 } else { 1 })
}
