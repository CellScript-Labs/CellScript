use super::{json_array_strings, json_pointer_bool, json_pointer_str};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub(super) fn build_profile_production_completeness(
    profile_certification: &Value,
    stateful_acceptance: &Value,
    public_attestation: &Value,
    external_review: &Value,
    btc_spv_evidence: &Value,
    rwa_legal_registry_review: &Value,
) -> Value {
    let public_cell_dep_passed = json_pointer_str(public_attestation, "/status") == Some("passed");
    let external_tcb_passed = json_pointer_str(external_review, "/status") == Some("passed");
    let public_btc_spv_passed = json_pointer_str(btc_spv_evidence, "/status") == Some("passed");
    let rwa_legal_passed = json_pointer_str(rwa_legal_registry_review, "/status") == Some("passed");
    let fiber_endpoint_passed = json_pointer_str(stateful_acceptance, "/external_endpoint_coverage/fiber/status") == Some("passed");

    let common_external = [
        ("public_shared_cell_dep_attestation", public_cell_dep_passed),
        ("external_bip340_tcb_review_attestation", external_tcb_passed),
    ];
    let btc_external = [
        ("public_shared_cell_dep_attestation", public_cell_dep_passed),
        ("external_bip340_tcb_review_attestation", external_tcb_passed),
        ("public_btc_spv_evidence", public_btc_spv_passed),
    ];
    let rwa_external = [
        ("public_shared_cell_dep_attestation", public_cell_dep_passed),
        ("external_bip340_tcb_review_attestation", external_tcb_passed),
        ("rwa_legal_registry_review_evidence", rwa_legal_passed),
    ];
    let fiber_external = [
        ("public_shared_cell_dep_attestation", public_cell_dep_passed),
        ("external_bip340_tcb_review_attestation", external_tcb_passed),
        ("fiber_node_workflow_execution", fiber_endpoint_passed),
    ];

    let rows = vec![
        profile_production_row(
            "novaseal-core-v0",
            "core",
            json_pointer_bool(stateful_acceptance, "/profile_coverage/checks/core_profile_live_stateful"),
            &common_external,
            "target/novaseal-devnet-stateful-acceptance.json#/profile_coverage/checks/core_profile_live_stateful",
        ),
        profile_production_row(
            "agreement-profile-v0",
            "object",
            json_pointer_bool(stateful_acceptance, "/profile_coverage/checks/agreement_profile_live_stateful")
                && json_pointer_bool(profile_certification, "/local_checks/conformance_gate_passed"),
            &common_external,
            "target/novaseal-devnet-stateful-acceptance.json#/profile_coverage/checks/agreement_profile_live_stateful",
        ),
        profile_production_row(
            "fungible-xudt-profile-v0",
            "object",
            json_pointer_str(profile_certification, "/planned_profile_packages/fungible_xudt/status") == Some("passed")
                && json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/fungible_xudt_value_flow_live"),
            &common_external,
            "target/novaseal-fungible-xudt-devnet-stateful-live.json",
        ),
        profile_production_row(
            "rwa-receipt-profile-v0",
            "object",
            json_pointer_str(profile_certification, "/planned_profile_packages/rwa_receipt/status") == Some("passed")
                && json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/rwa_receipt_lifecycle_live"),
            &rwa_external,
            "target/novaseal-rwa-receipt-devnet-stateful-live.json + proposals/novaseal/rwa-receipt-profile-v0/proofs/legal_registry_review_evidence.json",
        ),
        profile_production_row(
            "btc-transaction-commitment-profile-v0",
            "seal",
            json_pointer_str(profile_certification, "/planned_profile_packages/btc_tx_commitment/status") == Some("passed")
                && json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/btc_transaction_commitment_transition_live"),
            &btc_external,
            "target/novaseal-btc-transaction-commitment-devnet-stateful-live.json + proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.json",
        ),
        profile_production_row(
            "btc-utxo-seal-profile-v0",
            "seal",
            json_pointer_str(profile_certification, "/planned_profile_packages/btc_utxo_seal/status") == Some("passed")
                && json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/btc_utxo_seal_closure_live"),
            &btc_external,
            "target/novaseal-btc-utxo-seal-devnet-stateful-live.json + proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.json",
        ),
        profile_production_row(
            "dual-seal-profile-v0",
            "seal",
            json_pointer_str(profile_certification, "/planned_profile_packages/dual_seal/status") == Some("passed")
                && json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/dual_seal_finality_live"),
            &btc_external,
            "target/novaseal-dual-seal-devnet-stateful-live.json + proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.json",
        ),
        profile_production_row(
            "fiber-candidate-profile-v0",
            "application",
            json_pointer_str(profile_certification, "/planned_profile_packages/fiber_candidate/status") == Some("passed")
                && json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/fiber_candidate_path_live"),
            &fiber_external,
            "target/novaseal-fiber-candidate-devnet-stateful-live.json + target/novaseal-fiber-node-experiments.json",
        ),
    ];

    let local_complete = rows.iter().all(|row| json_pointer_str(row, "/local_status") == Some("passed"));
    let production_complete = rows.iter().all(|row| json_pointer_str(row, "/status") == Some("passed"));
    let external_required = rows.iter().any(|row| json_pointer_str(row, "/status") == Some("external_required"));
    let status = if production_complete {
        "passed"
    } else if local_complete && external_required {
        "external_required"
    } else {
        "failed"
    };
    let missing_external_evidence = rows
        .iter()
        .flat_map(|row| json_array_strings(row, "/missing_external_evidence"))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let failed_profiles = rows
        .iter()
        .filter(|row| json_pointer_str(row, "/status") != Some("passed"))
        .filter_map(|row| json_pointer_str(row, "/profile").map(str::to_string))
        .collect::<Vec<_>>();

    json!({
        "schema": "novaseal-all-profiles-production-completeness-v0.1",
        "status": status,
        "local_complete": local_complete,
        "production_complete": production_complete,
        "missing_external_evidence": missing_external_evidence,
        "failed_profiles": failed_profiles,
        "profiles": rows,
        "boundary": "Each NovaSeal profile must have local package/lifecycle evidence and every profile-specific external production proof before production_ready can be true.",
    })
}

fn profile_production_row(
    profile: &str,
    category: &str,
    local_passed: bool,
    external_requirements: &[(&str, bool)],
    evidence: &str,
) -> Value {
    let missing_external_evidence = external_requirements
        .iter()
        .filter(|(_, passed)| !*passed)
        .map(|(name, _)| Value::String((*name).to_string()))
        .collect::<Vec<_>>();
    let external_requirements =
        external_requirements.iter().map(|(name, passed)| json!({"name": name, "passed": passed})).collect::<Vec<_>>();
    let status = if !local_passed {
        "failed"
    } else if missing_external_evidence.is_empty() {
        "passed"
    } else {
        "external_required"
    };

    json!({
        "profile": profile,
        "category": category,
        "status": status,
        "local_status": if local_passed { "passed" } else { "failed" },
        "external_requirements": external_requirements,
        "missing_external_evidence": missing_external_evidence,
        "evidence": evidence,
    })
}
