use super::bitcoin_anchors::{expected_btc_spv_scenario, handoff_expected_bindings_exact};
use super::bitcoin_proofs::{validate_btc_spv_case_proof, validate_btc_transaction_binding};
use super::evidence_adapters::{handoff_case, handoff_case_expected_values};
use super::provenance::{git_commit, source_tree_hash, source_tree_hash_with_options};
use super::report_files::{json_load, json_load_path};
use super::report_origins::{
    contains_local_only_token, contains_placeholder_token, is_external_identity, is_https_report_uri, is_public_network,
};
use super::timestamps::{is_utc_date, is_utc_date_not_future, is_utc_timestamp_z, is_utc_timestamp_z_not_future};
use super::{
    exact_object_keys, exact_string_set, external_evidence_handoff_reference_hash, is_git_commit_hash, is_hex32, is_hex_bytes_len,
    json_array_strings, json_object_string_map, json_pointer_bool, json_pointer_i64, json_pointer_str, normalize_hex,
    object_values_all_true, parse_out_point, placeholder_hash, rel, value_is_present, AGREEMENT_ROOT, BIP340_TCB_SOURCE_HASH_PATHS,
    CORE_ROOT, EXPECTED_BTC_SPV_EVIDENCE_PROFILES, EXPECTED_BTC_SPV_PROFILE_SCENARIOS, EXPECTED_EXTERNAL_REQUEST_HANDOFF_FIELDS,
    EXPECTED_EXTERNAL_TCB_EXPECTED_VALUE_FIELDS, EXPECTED_EXTERNAL_TCB_REVIEW_ATTESTATION_FIELDS, EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE,
    EXPECTED_NOVASEAL_CELLDEP_DEP_TYPE, EXPECTED_NOVASEAL_CELLDEP_HASH_TYPE, EXPECTED_NOVASEAL_RELEASE_VERSION,
    EXPECTED_PUBLIC_BTC_SPV_CASE_FIELDS, EXPECTED_PUBLIC_BTC_SPV_CELLDEP_FIELDS, EXPECTED_PUBLIC_BTC_SPV_EVIDENCE_FIELDS,
    EXPECTED_PUBLIC_BTC_SPV_MERKLE_PROOF_FIELDS, EXPECTED_PUBLIC_BTC_SPV_SOURCE_SERVICE_FIELDS,
    EXPECTED_PUBLIC_CELLDEP_ATTESTATION_FIELDS, EXPECTED_PUBLIC_CELLDEP_EXPECTED_VALUE_FIELDS, EXPECTED_PUBLIC_CELLDEP_RELEASE_FIELDS,
    EXPECTED_PUBLIC_CELLDEP_RUNTIME_VERIFIER_FIELDS, EXPECTED_RWA_LEGAL_REVIEW_EVIDENCE_FIELDS,
    EXPECTED_RWA_LEGAL_REVIEW_EXPECTED_VALUE_FIELDS, EXPECTED_RWA_LEGAL_REVIEW_REGISTRY_FIELDS, EXPECTED_RWA_LEGAL_REVIEW_SCOPE,
    EXPECTED_RWA_RECEIPT_PROFILE, EXTERNAL_EVIDENCE_HANDOFF, EXTERNAL_TCB_ATTESTATION_TEMPLATE, NOVASEAL_HANDOFF_HASH_ALGORITHM,
    PUBLIC_BTC_SPV_EVIDENCE_TEMPLATE, PUBLIC_CELLDEP_ATTESTATION_TEMPLATE, RWA_LEGAL_REGISTRY_REVIEW_TEMPLATE,
    RWA_LEGAL_REVIEW_SOURCE_HASH_PATHS, VERIFIER_ROOT,
};
use crate::error::Result;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(super) fn validate_btc_spv_evidence(repo_root: &Path, rel_path: &str, external_evidence_handoff: &Value) -> Result<Value> {
    let path = repo_root.join(rel_path);
    if !path.is_file() {
        return Ok(json!({
            "status": "external_required",
            "reason": "missing public BTC SPV evidence",
            "required_report": rel_path,
            "template": PUBLIC_BTC_SPV_EVIDENCE_TEMPLATE,
            "required_handoff": EXTERNAL_EVIDENCE_HANDOFF,
            "required_profiles": EXPECTED_BTC_SPV_EVIDENCE_PROFILES,
        }));
    }
    let payload = json_load(repo_root, rel_path)?;
    let handoff_hash = external_evidence_handoff_reference_hash(external_evidence_handoff);
    let handoff_case = handoff_case(external_evidence_handoff, "public_btc_spv_evidence").unwrap_or(&Value::Null);
    let handoff_required_profiles = json_array_strings(handoff_case, "/required_profiles");
    let handoff_expected_scenarios = json_object_string_map(handoff_case.get("expected_scenarios").unwrap_or(&Value::Null));
    let handoff_expected_bindings = handoff_case.get("expected_case_bindings").unwrap_or(&Value::Null);
    let cases = payload.get("cases").and_then(Value::as_array).cloned().unwrap_or_default();
    let covered_profile_list =
        cases.iter().filter_map(|case| json_pointer_str(case, "/profile").map(str::to_string)).collect::<Vec<_>>();
    let covered_scenarios = cases
        .iter()
        .filter_map(|case| {
            let profile = json_pointer_str(case, "/profile")?;
            let scenario = json_pointer_str(case, "/scenario")?;
            Some((profile.to_string(), scenario.to_string()))
        })
        .collect::<BTreeMap<_, _>>();
    let covered_profiles = covered_profile_list.iter().cloned().collect::<BTreeSet<_>>();
    let required_profiles = EXPECTED_BTC_SPV_EVIDENCE_PROFILES.iter().map(|profile| (*profile).to_string()).collect::<BTreeSet<_>>();
    let mut case_checks = Map::new();
    for profile in EXPECTED_BTC_SPV_EVIDENCE_PROFILES {
        let Some(case) = cases.iter().find(|case| json_pointer_str(case, "/profile") == Some(*profile)) else {
            case_checks.insert((*profile).to_string(), json!({"present": false}));
            continue;
        };
        let cell_dep = case.get("spv_client_cell_dep").unwrap_or(&Value::Null);
        let source_service = case.get("source_service").unwrap_or(&Value::Null);
        let merkle_proof = case.get("btc_merkle_proof").unwrap_or(&Value::Null);
        let out_point = parse_out_point(json_pointer_str(cell_dep, "/out_point"));
        let hash_type = json_pointer_str(cell_dep, "/hash_type");
        let confirmations = json_pointer_i64(case, "/confirmations").unwrap_or_default();
        let minimum_confirmations = json_pointer_i64(case, "/minimum_confirmations").unwrap_or_default();
        let expected_binding = handoff_expected_bindings.get(*profile).unwrap_or(&Value::Null);
        let proof_checks = validate_btc_spv_case_proof(case, confirmations);
        let tx_checks = validate_btc_transaction_binding(profile, case, expected_binding);
        let mut checks = Map::new();
        macro_rules! check {
            ($name:literal, $value:expr) => {
                checks.insert($name.to_string(), Value::Bool($value));
            };
        }
        check!("present", true);
        check!("fields_exact", exact_object_keys(case, EXPECTED_PUBLIC_BTC_SPV_CASE_FIELDS));
        check!("scenario_matches_expected", json_pointer_str(case, "/scenario") == expected_btc_spv_scenario(profile));
        check!(
            "scenario_matches_handoff",
            handoff_expected_scenarios
                .get(*profile)
                .is_some_and(|scenario| json_pointer_str(case, "/scenario") == Some(scenario.as_str()))
        );
        check!("ckb_live_tx_hash_valid", json_pointer_str(case, "/ckb_live_tx_hash").is_some_and(is_hex32));
        check!("ckb_live_tx_hash_non_placeholder", !placeholder_hash(json_pointer_str(case, "/ckb_live_tx_hash")));
        check!(
            "ckb_live_tx_hash_matches_handoff",
            normalize_hex(json_pointer_str(case, "/ckb_live_tx_hash")).as_deref()
                == normalize_hex(json_pointer_str(expected_binding, "/ckb_live_tx_hash")).as_deref()
        );
        check!("live_report_hash_valid", json_pointer_str(case, "/live_report_hash").is_some_and(is_hex32));
        check!("live_report_hash_non_placeholder", !placeholder_hash(json_pointer_str(case, "/live_report_hash")));
        check!(
            "live_report_hash_matches_handoff",
            normalize_hex(json_pointer_str(case, "/live_report_hash")).as_deref()
                == normalize_hex(json_pointer_str(expected_binding, "/live_report_hash")).as_deref()
        );
        check!("service_builder_case_hash_valid", json_pointer_str(case, "/service_builder_case_hash").is_some_and(is_hex32));
        check!("service_builder_case_hash_non_placeholder", !placeholder_hash(json_pointer_str(case, "/service_builder_case_hash")));
        check!(
            "service_builder_case_hash_matches_handoff",
            normalize_hex(json_pointer_str(case, "/service_builder_case_hash")).as_deref()
                == normalize_hex(json_pointer_str(expected_binding, "/service_builder_case_hash")).as_deref()
        );
        check!(
            "service_builder_tx_skeleton_hash_valid",
            json_pointer_str(case, "/service_builder_tx_skeleton_hash").is_some_and(is_hex32)
        );
        check!(
            "service_builder_tx_skeleton_hash_non_placeholder",
            !placeholder_hash(json_pointer_str(case, "/service_builder_tx_skeleton_hash"))
        );
        check!(
            "service_builder_tx_skeleton_hash_matches_handoff",
            normalize_hex(json_pointer_str(case, "/service_builder_tx_skeleton_hash")).as_deref()
                == normalize_hex(json_pointer_str(expected_binding, "/service_builder_tx_skeleton_hash")).as_deref()
        );
        check!(
            "service_builder_receipt_binding_hash_valid",
            json_pointer_str(case, "/service_builder_receipt_binding_hash").is_some_and(is_hex32)
        );
        check!(
            "service_builder_receipt_binding_hash_non_placeholder",
            !placeholder_hash(json_pointer_str(case, "/service_builder_receipt_binding_hash"))
        );
        check!(
            "service_builder_receipt_binding_hash_matches_handoff",
            normalize_hex(json_pointer_str(case, "/service_builder_receipt_binding_hash")).as_deref()
                == normalize_hex(json_pointer_str(expected_binding, "/service_builder_receipt_binding_hash")).as_deref()
        );
        check!("ckb_btc_commitment_hash_valid", json_pointer_str(case, "/ckb_btc_commitment_hash").is_some_and(is_hex32));
        check!("ckb_btc_commitment_hash_non_placeholder", !placeholder_hash(json_pointer_str(case, "/ckb_btc_commitment_hash")));
        check!(
            "ckb_btc_commitment_hash_matches_handoff",
            normalize_hex(json_pointer_str(case, "/ckb_btc_commitment_hash")).as_deref()
                == normalize_hex(json_pointer_str(expected_binding, "/ckb_btc_commitment_hash")).as_deref()
        );
        check!("btc_txid_valid", json_pointer_str(case, "/btc_txid").is_some_and(is_hex32));
        check!("btc_txid_non_placeholder", !placeholder_hash(json_pointer_str(case, "/btc_txid")));
        check!(
            "btc_txid_matches_handoff_when_bound",
            json_pointer_str(expected_binding, "/btc_txid").is_none_or(|expected| {
                normalize_hex(json_pointer_str(case, "/btc_txid")).as_deref() == normalize_hex(Some(expected)).as_deref()
            })
        );
        check!("btc_wtxid_valid", json_pointer_str(case, "/btc_wtxid").is_some_and(is_hex32));
        check!("btc_wtxid_non_placeholder", !placeholder_hash(json_pointer_str(case, "/btc_wtxid")));
        check!(
            "btc_wtxid_matches_handoff_when_bound",
            json_pointer_str(expected_binding, "/btc_wtxid").is_none_or(|expected| {
                normalize_hex(json_pointer_str(case, "/btc_wtxid")).as_deref() == normalize_hex(Some(expected)).as_deref()
            })
        );
        check!("btc_tx_hex_valid", json_pointer_bool(&tx_checks, "/btc_tx_hex_valid"));
        check!("btc_txid_matches_tx_hex", json_pointer_bool(&tx_checks, "/btc_txid_matches_tx_hex"));
        check!("btc_wtxid_matches_tx_hex", json_pointer_bool(&tx_checks, "/btc_wtxid_matches_tx_hex"));
        check!("btc_transaction_binding_fields_exact", json_pointer_bool(&tx_checks, "/binding_fields_exact"));
        check!("btc_transaction_binding_kind_matches_profile", json_pointer_bool(&tx_checks, "/binding_kind_matches_profile"));
        check!("btc_transaction_binding_matches_handoff", json_pointer_bool(&tx_checks, "/binding_matches_handoff"));
        check!("btc_transaction_output_matches_anchor", json_pointer_bool(&tx_checks, "/transaction_output_matches_anchor"));
        check!("btc_utxo_spend_input_matches_anchor", json_pointer_bool(&tx_checks, "/utxo_spend_input_matches_anchor"));
        check!("btc_utxo_sealed_tx_matches_anchor", json_pointer_bool(&tx_checks, "/utxo_sealed_tx_matches_anchor"));
        check!(
            "btc_utxo_sealed_utxo_commitment_matches_tuple",
            json_pointer_bool(&tx_checks, "/utxo_sealed_utxo_commitment_matches_tuple")
        );
        check!("btc_dual_spend_input_matches_anchor", json_pointer_bool(&tx_checks, "/dual_spend_input_matches_anchor"));
        check!("btc_dual_sealed_tx_matches_anchor", json_pointer_bool(&tx_checks, "/dual_sealed_tx_matches_anchor"));
        check!(
            "btc_dual_sealed_utxo_commitment_matches_tuple",
            json_pointer_bool(&tx_checks, "/dual_sealed_utxo_commitment_matches_tuple")
        );
        check!("btc_block_hash_valid", json_pointer_str(case, "/btc_block_hash").is_some_and(is_hex32));
        check!("btc_block_hash_non_placeholder", !placeholder_hash(json_pointer_str(case, "/btc_block_hash")));
        check!("btc_block_header_valid", json_pointer_str(case, "/btc_block_header").is_some_and(|value| is_hex_bytes_len(value, 80)));
        check!("btc_block_hash_matches_header", json_pointer_bool(&proof_checks, "/block_hash_matches_header"));
        check!("btc_merkle_proof_fields_exact", exact_object_keys(merkle_proof, EXPECTED_PUBLIC_BTC_SPV_MERKLE_PROOF_FIELDS));
        check!("btc_merkle_proof_tx_index_non_negative", json_pointer_i64(merkle_proof, "/tx_index").is_some_and(|value| value >= 0));
        check!("btc_merkle_proof_branch_valid", json_pointer_bool(&proof_checks, "/merkle_branch_valid"));
        check!("btc_merkle_proof_merkle_root_valid", json_pointer_str(merkle_proof, "/merkle_root").is_some_and(is_hex32));
        check!("btc_merkle_proof_merkle_root_non_placeholder", !placeholder_hash(json_pointer_str(merkle_proof, "/merkle_root")));
        check!("btc_merkle_root_matches_header", json_pointer_bool(&proof_checks, "/merkle_root_matches_header"));
        check!("btc_merkle_branch_verifies_txid", json_pointer_bool(&proof_checks, "/merkle_branch_verifies_txid"));
        check!("btc_confirmations_match_heights", json_pointer_bool(&proof_checks, "/confirmations_match_heights"));
        check!("spv_proof_hash_valid", json_pointer_str(case, "/spv_proof_hash").is_some_and(is_hex32));
        check!("spv_proof_hash_non_placeholder", !placeholder_hash(json_pointer_str(case, "/spv_proof_hash")));
        check!("spv_proof_hash_matches_material", json_pointer_bool(&proof_checks, "/proof_hash_matches_material"));
        check!("minimum_confirmations_at_least_six", minimum_confirmations >= 6);
        check!("confirmations_meet_minimum", confirmations >= minimum_confirmations && minimum_confirmations >= 6);
        check!("spv_client_cell_dep_fields_exact", exact_object_keys(cell_dep, EXPECTED_PUBLIC_BTC_SPV_CELLDEP_FIELDS));
        check!("spv_client_cell_dep_out_point_valid", json_pointer_bool(&out_point, "/valid"));
        check!("spv_client_cell_dep_out_point_non_placeholder", !placeholder_hash(json_pointer_str(&out_point, "/tx_hash")));
        check!("spv_client_cell_dep_data_hash_valid", json_pointer_str(cell_dep, "/data_hash").is_some_and(is_hex32));
        check!("spv_client_cell_dep_data_hash_non_placeholder", !placeholder_hash(json_pointer_str(cell_dep, "/data_hash")));
        check!("spv_client_cell_dep_dep_type", json_pointer_str(cell_dep, "/dep_type") == Some("code"));
        check!("spv_client_cell_dep_hash_type", matches!(hash_type, Some("data" | "data1" | "type")));
        check!("source_service_fields_exact", exact_object_keys(source_service, EXPECTED_PUBLIC_BTC_SPV_SOURCE_SERVICE_FIELDS));
        check!("source_service_name_present", source_service.get("name").is_some_and(value_is_present));
        check!("source_service_name_identity", json_pointer_str(source_service, "/name").is_some_and(is_external_identity));
        check!("source_service_commit_40_hex", json_pointer_str(source_service, "/commit").is_some_and(is_git_commit_hash));
        check!("source_service_report_hash_valid", json_pointer_str(source_service, "/report_hash").is_some_and(is_hex32));
        check!("source_service_report_hash_non_placeholder", !placeholder_hash(json_pointer_str(source_service, "/report_hash")));
        case_checks.insert((*profile).to_string(), Value::Object(checks));
    }
    let case_checks_passed = case_checks.values().all(|checks| object_values_all_true(Some(checks)));
    let checks = json!({
        "schema": json_pointer_str(&payload, "/schema") == Some("novaseal-public-btc-spv-evidence-v0.1"),
        "top_level_fields_exact": exact_object_keys(&payload, EXPECTED_PUBLIC_BTC_SPV_EVIDENCE_FIELDS),
        "status_attested": json_pointer_str(&payload, "/status") == Some("attested"),
        "network_public": json_pointer_str(&payload, "/network").is_some_and(is_public_network),
        "evidence_provider_present": payload.get("evidence_provider").is_some_and(value_is_present),
        "evidence_provider_identity": json_pointer_str(&payload, "/evidence_provider").is_some_and(is_external_identity),
        "generated_at_present": payload.get("generated_at").is_some_and(value_is_present),
        "generated_at_utc_timestamp": json_pointer_str(&payload, "/generated_at").is_some_and(is_utc_timestamp_z),
        "generated_at_not_future": json_pointer_str(&payload, "/generated_at").is_some_and(is_utc_timestamp_z_not_future),
        "request_handoff_fields_exact": exact_object_keys(payload.get("request_handoff").unwrap_or(&Value::Null), EXPECTED_EXTERNAL_REQUEST_HANDOFF_FIELDS),
        "request_handoff_bundle_path": json_pointer_str(&payload, "/request_handoff/bundle") == Some(EXTERNAL_EVIDENCE_HANDOFF),
        "request_handoff_bundle_hash_matches_current": normalize_hex(json_pointer_str(&payload, "/request_handoff/bundle_hash")).as_deref()
            == Some(handoff_hash.as_str()),
        "request_handoff_bundle_hash_algorithm": json_pointer_str(&payload, "/request_handoff/bundle_hash_algorithm")
            == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM),
        "request_handoff_group": json_pointer_str(&payload, "/request_handoff/group") == Some("public_btc_spv_evidence"),
        "handoff_required_profiles_exact": exact_string_set(&handoff_required_profiles, EXPECTED_BTC_SPV_EVIDENCE_PROFILES),
        "handoff_expected_scenarios_exact": handoff_expected_scenarios == EXPECTED_BTC_SPV_PROFILE_SCENARIOS
            .iter()
            .map(|(profile, scenario)| ((*profile).to_string(), (*scenario).to_string()))
            .collect::<BTreeMap<_, _>>(),
        "handoff_expected_case_bindings_exact": handoff_expected_bindings_exact(handoff_expected_bindings),
        "required_profiles_field_exact": exact_string_set(&json_array_strings(&payload, "/required_profiles"), EXPECTED_BTC_SPV_EVIDENCE_PROFILES),
        "required_profiles_match_handoff": json_array_strings(&payload, "/required_profiles") == handoff_required_profiles,
        "required_profiles_covered_exact": exact_string_set(&covered_profile_list, EXPECTED_BTC_SPV_EVIDENCE_PROFILES),
        "covered_scenarios_match_handoff": covered_scenarios == handoff_expected_scenarios,
        "case_checks_passed": case_checks_passed,
    });
    let missing_profiles = required_profiles.difference(&covered_profiles).cloned().collect::<Vec<_>>();
    let extra_profiles = covered_profiles.difference(&required_profiles).cloned().collect::<Vec<_>>();
    Ok(json!({
        "schema": "novaseal-public-btc-spv-evidence-validation-v0.1",
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "path": rel(repo_root, &path),
        "required_profiles": EXPECTED_BTC_SPV_EVIDENCE_PROFILES,
        "covered_profiles": covered_profiles.into_iter().collect::<Vec<_>>(),
        "missing_profiles": missing_profiles,
        "extra_profiles": extra_profiles,
        "checks": checks,
        "case_checks": case_checks,
        "evidence": payload,
    }))
}

pub(super) fn validate_public_attestation(
    repo_root: &Path,
    rel_path: &str,
    artifact_hash: Option<&str>,
    tcb_repo_commit: Option<&str>,
    external_evidence_handoff: &Value,
) -> Result<Value> {
    let path = repo_root.join(rel_path);
    if !path.exists() {
        return Ok(json!({
            "status": "external_required",
            "reason": "missing public/shared CellDep attestation",
            "required_report": rel_path,
            "required_handoff": EXTERNAL_EVIDENCE_HANDOFF,
        }));
    }
    let payload = json_load_path(repo_root, &path)?;
    let handoff_hash = external_evidence_handoff_reference_hash(external_evidence_handoff);
    let verifier = payload.get("runtime_verifier").cloned().unwrap_or(Value::Null);
    let release = payload.get("release").cloned().unwrap_or(Value::Null);
    let handoff_expected_values =
        handoff_case_expected_values(external_evidence_handoff, "public_shared_cell_dep_attestation").unwrap_or(&Value::Null);
    let parsed = parse_out_point(json_pointer_str(&verifier, "/out_point"));
    let checks = json!({
        "schema": json_pointer_str(&payload, "/schema") == Some("novaseal-public-shared-cell-dep-attestation-v0.1"),
        "top_level_fields_exact": exact_object_keys(&payload, EXPECTED_PUBLIC_CELLDEP_ATTESTATION_FIELDS),
        "status": json_pointer_str(&payload, "/status") == Some("attested"),
        "network_public": json_pointer_str(&payload, "/network").is_some_and(is_public_network),
        "attested_at_utc_timestamp": json_pointer_str(&payload, "/attested_at").is_some_and(is_utc_timestamp_z),
        "attested_at_not_future": json_pointer_str(&payload, "/attested_at").is_some_and(is_utc_timestamp_z_not_future),
        "attestor_identity": json_pointer_str(&payload, "/attestor").is_some_and(is_external_identity),
        "release_fields_exact": exact_object_keys(&release, EXPECTED_PUBLIC_CELLDEP_RELEASE_FIELDS),
        "release_package": json_pointer_str(&release, "/package") == Some("novaseal"),
        "release_version_present": release.get("version").is_some_and(value_is_present),
        "release_version_matches_expected": json_pointer_str(&release, "/version") == Some(EXPECTED_NOVASEAL_RELEASE_VERSION),
        "release_manifest_commit_present": json_pointer_str(&release, "/manifest_commit").is_some_and(is_git_commit_hash),
        "release_manifest_commit_matches_tcb": json_pointer_str(&release, "/manifest_commit") == tcb_repo_commit,
        "request_handoff_fields_exact": exact_object_keys(payload.get("request_handoff").unwrap_or(&Value::Null), EXPECTED_EXTERNAL_REQUEST_HANDOFF_FIELDS),
        "request_handoff_bundle_path": json_pointer_str(&payload, "/request_handoff/bundle") == Some(EXTERNAL_EVIDENCE_HANDOFF),
        "request_handoff_bundle_hash_matches_current": normalize_hex(json_pointer_str(&payload, "/request_handoff/bundle_hash")).as_deref()
            == Some(handoff_hash.as_str()),
        "request_handoff_bundle_hash_algorithm": json_pointer_str(&payload, "/request_handoff/bundle_hash_algorithm")
            == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM),
        "request_handoff_group": json_pointer_str(&payload, "/request_handoff/group") == Some("public_shared_cell_dep_attestation"),
        "handoff_expected_values_exact": exact_object_keys(handoff_expected_values, EXPECTED_PUBLIC_CELLDEP_EXPECTED_VALUE_FIELDS),
        "release_package_matches_handoff": json_pointer_str(&release, "/package")
            == json_pointer_str(handoff_expected_values, "/release.package"),
        "release_version_matches_handoff": json_pointer_str(&release, "/version")
            == json_pointer_str(handoff_expected_values, "/release.version"),
        "release_manifest_commit_matches_handoff": json_pointer_str(&release, "/manifest_commit")
            == json_pointer_str(handoff_expected_values, "/release.manifest_commit"),
        "runtime_verifier_fields_exact": exact_object_keys(&verifier, EXPECTED_PUBLIC_CELLDEP_RUNTIME_VERIFIER_FIELDS),
        "artifact_hash_valid": normalize_hex(json_pointer_str(&verifier, "/artifact_hash")).as_deref().is_some_and(is_hex32),
        "artifact_hash_non_placeholder": !placeholder_hash(normalize_hex(json_pointer_str(&verifier, "/artifact_hash")).as_deref()),
        "artifact_hash": normalize_hex(json_pointer_str(&verifier, "/artifact_hash")).as_deref() == artifact_hash,
        "artifact_hash_matches_handoff": normalize_hex(json_pointer_str(&verifier, "/artifact_hash")).as_deref()
            == normalize_hex(json_pointer_str(handoff_expected_values, "/artifact_hash")).as_deref(),
        "data_hash_non_placeholder": !placeholder_hash(normalize_hex(json_pointer_str(&verifier, "/data_hash")).as_deref()),
        "out_point_valid": json_pointer_bool(&parsed, "/valid"),
        "out_point_non_placeholder": !placeholder_hash(json_pointer_str(&parsed, "/tx_hash")),
        "dep_type": json_pointer_str(&verifier, "/dep_type") == Some(EXPECTED_NOVASEAL_CELLDEP_DEP_TYPE),
        "dep_type_matches_handoff": json_pointer_str(&verifier, "/dep_type")
            == json_pointer_str(handoff_expected_values, "/runtime_verifier.dep_type"),
        "hash_type": matches!(json_pointer_str(&verifier, "/hash_type"), Some("data" | "data1" | "type")),
        "hash_type_matches_expected": json_pointer_str(&verifier, "/hash_type") == Some(EXPECTED_NOVASEAL_CELLDEP_HASH_TYPE),
        "hash_type_matches_handoff": json_pointer_str(&verifier, "/hash_type")
            == json_pointer_str(handoff_expected_values, "/runtime_verifier.hash_type"),
        "data_hash_valid": normalize_hex(json_pointer_str(&verifier, "/data_hash")).as_deref().is_some_and(is_hex32),
        "verifier_id": json_pointer_str(&verifier, "/verifier_id") == Some("btc.bip340.v0"),
        "verifier_id_matches_handoff": json_pointer_str(&verifier, "/verifier_id")
            == json_pointer_str(handoff_expected_values, "/runtime_verifier.verifier_id"),
        "ipc_abi": json_pointer_str(&verifier, "/ipc_abi") == Some("cellscript-btc-bip340-ipc-v0"),
        "ipc_abi_matches_handoff": json_pointer_str(&verifier, "/ipc_abi")
            == json_pointer_str(handoff_expected_values, "/runtime_verifier.ipc_abi"),
    });
    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "attestation": payload,
    }))
}

pub(super) fn validate_external_review(
    repo_root: &Path,
    rel_path: &str,
    artifact_hash: Option<&str>,
    source_tree_hash: Option<&str>,
    external_evidence_handoff: &Value,
) -> Result<Value> {
    let path = repo_root.join(rel_path);
    if !path.exists() {
        return Ok(json!({
            "status": "external_required",
            "reason": "missing external BIP340 TCB review attestation",
            "required_report": rel_path,
            "required_handoff": EXTERNAL_EVIDENCE_HANDOFF,
        }));
    }
    let payload = json_load_path(repo_root, &path)?;
    let handoff_hash = external_evidence_handoff_reference_hash(external_evidence_handoff);
    let handoff_expected_values =
        handoff_case_expected_values(external_evidence_handoff, "external_bip340_tcb_review_attestation").unwrap_or(&Value::Null);
    let checks = json!({
        "schema": json_pointer_str(&payload, "/schema") == Some("novaseal-bip340-external-tcb-review-attestation-v0.1"),
        "top_level_fields_exact": exact_object_keys(&payload, EXPECTED_EXTERNAL_TCB_REVIEW_ATTESTATION_FIELDS),
        "status": json_pointer_str(&payload, "/status") == Some("accepted"),
        "request_handoff_fields_exact": exact_object_keys(payload.get("request_handoff").unwrap_or(&Value::Null), EXPECTED_EXTERNAL_REQUEST_HANDOFF_FIELDS),
        "request_handoff_bundle_path": json_pointer_str(&payload, "/request_handoff/bundle") == Some(EXTERNAL_EVIDENCE_HANDOFF),
        "request_handoff_bundle_hash_matches_current": normalize_hex(json_pointer_str(&payload, "/request_handoff/bundle_hash")).as_deref()
            == Some(handoff_hash.as_str()),
        "request_handoff_bundle_hash_algorithm": json_pointer_str(&payload, "/request_handoff/bundle_hash_algorithm")
            == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM),
        "request_handoff_group": json_pointer_str(&payload, "/request_handoff/group") == Some("external_bip340_tcb_review_attestation"),
        "handoff_expected_values_exact": exact_object_keys(handoff_expected_values, EXPECTED_EXTERNAL_TCB_EXPECTED_VALUE_FIELDS),
        "artifact_hash_valid": normalize_hex(json_pointer_str(&payload, "/artifact_hash")).as_deref().is_some_and(is_hex32),
        "artifact_hash_non_placeholder": !placeholder_hash(normalize_hex(json_pointer_str(&payload, "/artifact_hash")).as_deref()),
        "artifact_hash": normalize_hex(json_pointer_str(&payload, "/artifact_hash")).as_deref() == artifact_hash,
        "artifact_hash_matches_handoff": normalize_hex(json_pointer_str(&payload, "/artifact_hash")).as_deref()
            == normalize_hex(json_pointer_str(handoff_expected_values, "/artifact_hash")).as_deref(),
        "artifact_hash_algorithm": json_pointer_str(&payload, "/artifact_hash_algorithm") == Some("sha256"),
        "artifact_hash_algorithm_matches_handoff": json_pointer_str(&payload, "/artifact_hash_algorithm")
            == json_pointer_str(handoff_expected_values, "/artifact_hash_algorithm"),
        "source_tree_sha256_valid": normalize_hex(json_pointer_str(&payload, "/source_tree_sha256")).as_deref().is_some_and(is_hex32),
        "source_tree_sha256_non_placeholder": !placeholder_hash(normalize_hex(json_pointer_str(&payload, "/source_tree_sha256")).as_deref()),
        "source_tree_sha256_matches_current_tcb": normalize_hex(json_pointer_str(&payload, "/source_tree_sha256")).as_deref() == source_tree_hash,
        "source_tree_sha256_matches_handoff": normalize_hex(json_pointer_str(&payload, "/source_tree_sha256")).as_deref()
            == normalize_hex(json_pointer_str(handoff_expected_values, "/source_tree_sha256")).as_deref(),
        "verifier_id": json_pointer_str(&payload, "/verifier_id") == Some("btc.bip340.v0"),
        "verifier_id_matches_handoff": json_pointer_str(&payload, "/verifier_id")
            == json_pointer_str(handoff_expected_values, "/verifier_id"),
        "ipc_abi": json_pointer_str(&payload, "/ipc_abi") == Some("cellscript-btc-bip340-ipc-v0"),
        "ipc_abi_matches_handoff": json_pointer_str(&payload, "/ipc_abi")
            == json_pointer_str(handoff_expected_values, "/ipc_abi"),
        "reviewer_present": json_pointer_str(&payload, "/reviewer").is_some_and(|value| !value.is_empty()),
        "reviewer_identity": json_pointer_str(&payload, "/reviewer").is_some_and(is_external_identity),
        "review_date_present": json_pointer_str(&payload, "/review_date").is_some_and(|value| !value.is_empty()),
        "review_date_utc_date": json_pointer_str(&payload, "/review_date").is_some_and(is_utc_date),
        "review_date_not_future": json_pointer_str(&payload, "/review_date").is_some_and(is_utc_date_not_future),
        "report_uri_https": json_pointer_str(&payload, "/report_uri").is_some_and(is_https_report_uri),
        "review_scope_exact": exact_string_set(&json_array_strings(&payload, "/review_scope"), EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE),
        "review_scope_matches_handoff": json_array_strings(&payload, "/review_scope")
            == json_array_strings(handoff_expected_values, "/review_scope"),
    });
    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "attestation": payload,
    }))
}

pub(super) fn validate_rwa_legal_registry_review(
    repo_root: &Path,
    rel_path: &str,
    external_evidence_handoff: &Value,
) -> Result<Value> {
    let source_hash = source_tree_hash(repo_root, RWA_LEGAL_REVIEW_SOURCE_HASH_PATHS)?;
    let expected_source_hash = normalize_hex(json_pointer_str(&source_hash, "/sha256"));
    let path = repo_root.join(rel_path);
    if !path.exists() {
        return Ok(json!({
            "status": "external_required",
            "reason": "missing RWA legal/registry review evidence",
            "required_report": rel_path,
            "required_template": RWA_LEGAL_REGISTRY_REVIEW_TEMPLATE,
            "required_handoff": EXTERNAL_EVIDENCE_HANDOFF,
            "expected_profile_source_tree_sha256": expected_source_hash,
        }));
    }
    let payload = json_load_path(repo_root, &path)?;
    let registry = payload.get("registry").cloned().unwrap_or(Value::Null);
    let handoff_hash = external_evidence_handoff_reference_hash(external_evidence_handoff);
    let handoff_expected_values =
        handoff_case_expected_values(external_evidence_handoff, "rwa_legal_registry_review_evidence").unwrap_or(&Value::Null);
    let checks = json!({
        "schema": json_pointer_str(&payload, "/schema") == Some("novaseal-rwa-legal-registry-review-evidence-v0.1"),
        "top_level_fields_exact": exact_object_keys(&payload, EXPECTED_RWA_LEGAL_REVIEW_EVIDENCE_FIELDS),
        "status": json_pointer_str(&payload, "/status") == Some("accepted"),
        "profile": json_pointer_str(&payload, "/profile") == Some(EXPECTED_RWA_RECEIPT_PROFILE),
        "profile_matches_handoff": json_pointer_str(&payload, "/profile") == json_pointer_str(handoff_expected_values, "/profile"),
        "request_handoff_fields_exact": exact_object_keys(payload.get("request_handoff").unwrap_or(&Value::Null), EXPECTED_EXTERNAL_REQUEST_HANDOFF_FIELDS),
        "request_handoff_bundle_path": json_pointer_str(&payload, "/request_handoff/bundle") == Some(EXTERNAL_EVIDENCE_HANDOFF),
        "request_handoff_bundle_hash_matches_current": normalize_hex(json_pointer_str(&payload, "/request_handoff/bundle_hash")).as_deref()
            == Some(handoff_hash.as_str()),
        "request_handoff_bundle_hash_algorithm": json_pointer_str(&payload, "/request_handoff/bundle_hash_algorithm")
            == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM),
        "request_handoff_group": json_pointer_str(&payload, "/request_handoff/group") == Some("rwa_legal_registry_review_evidence"),
        "handoff_expected_values_exact": exact_object_keys(handoff_expected_values, EXPECTED_RWA_LEGAL_REVIEW_EXPECTED_VALUE_FIELDS),
        "reviewer_present": json_pointer_str(&payload, "/reviewer").is_some_and(|value| !value.is_empty()),
        "reviewer_identity": json_pointer_str(&payload, "/reviewer").is_some_and(is_external_identity),
        "review_date_present": json_pointer_str(&payload, "/review_date").is_some_and(|value| !value.is_empty()),
        "review_date_utc_date": json_pointer_str(&payload, "/review_date").is_some_and(is_utc_date),
        "review_date_not_future": json_pointer_str(&payload, "/review_date").is_some_and(is_utc_date_not_future),
        "report_uri_https": json_pointer_str(&payload, "/report_uri").is_some_and(is_https_report_uri),
        "review_scope_exact": exact_string_set(&json_array_strings(&payload, "/review_scope"), EXPECTED_RWA_LEGAL_REVIEW_SCOPE),
        "review_scope_matches_handoff": json_array_strings(&payload, "/review_scope")
            == json_array_strings(handoff_expected_values, "/review_scope"),
        "registry_fields_exact": exact_object_keys(&registry, EXPECTED_RWA_LEGAL_REVIEW_REGISTRY_FIELDS),
        "registry_authority_identity": json_pointer_str(&registry, "/authority").is_some_and(is_external_identity),
        "registry_jurisdiction_present": json_pointer_str(&registry, "/jurisdiction").is_some_and(|value| {
            value_is_present(&Value::String(value.to_string()))
                && !contains_placeholder_token(value)
                && !contains_local_only_token(value)
        }),
        "registry_report_hash_valid": normalize_hex(json_pointer_str(&registry, "/registry_report_hash")).as_deref().is_some_and(is_hex32),
        "registry_report_hash_non_placeholder": !placeholder_hash(normalize_hex(json_pointer_str(&registry, "/registry_report_hash")).as_deref()),
        "profile_source_tree_sha256_valid": normalize_hex(json_pointer_str(&payload, "/profile_source_tree_sha256")).as_deref().is_some_and(is_hex32),
        "profile_source_tree_sha256_non_placeholder": !placeholder_hash(normalize_hex(json_pointer_str(&payload, "/profile_source_tree_sha256")).as_deref()),
        "profile_source_tree_sha256_matches_current": normalize_hex(json_pointer_str(&payload, "/profile_source_tree_sha256")) == expected_source_hash,
        "profile_source_tree_sha256_matches_handoff": normalize_hex(json_pointer_str(&payload, "/profile_source_tree_sha256"))
            == normalize_hex(json_pointer_str(handoff_expected_values, "/profile_source_tree_sha256")),
    });
    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "evidence": payload,
        "profile_source_tree": source_hash,
    }))
}

pub(super) fn validate_attestation_templates(
    repo_root: &Path,
    artifact_hash: Option<&str>,
    artifact_hash_algorithm: Option<&str>,
    expected_tcb_source_tree_hash: Option<&str>,
) -> Result<Value> {
    let public_path = repo_root.join(PUBLIC_CELLDEP_ATTESTATION_TEMPLATE);
    let external_path = repo_root.join(EXTERNAL_TCB_ATTESTATION_TEMPLATE);
    let btc_spv_path = repo_root.join(PUBLIC_BTC_SPV_EVIDENCE_TEMPLATE);
    let rwa_legal_path = repo_root.join(RWA_LEGAL_REGISTRY_REVIEW_TEMPLATE);
    let public_payload = if public_path.is_file() { Some(json_load_path(repo_root, &public_path)?) } else { None };
    let external_payload = if external_path.is_file() { Some(json_load_path(repo_root, &external_path)?) } else { None };
    let btc_spv_payload = if btc_spv_path.is_file() { Some(json_load_path(repo_root, &btc_spv_path)?) } else { None };
    let rwa_legal_payload = if rwa_legal_path.is_file() { Some(json_load_path(repo_root, &rwa_legal_path)?) } else { None };
    let public = public_payload.as_ref().unwrap_or(&Value::Null);
    let external = external_payload.as_ref().unwrap_or(&Value::Null);
    let btc_spv = btc_spv_payload.as_ref().unwrap_or(&Value::Null);
    let rwa_legal = rwa_legal_payload.as_ref().unwrap_or(&Value::Null);
    let public_release = public.get("release").unwrap_or(&Value::Null);
    let public_verifier = public.get("runtime_verifier").unwrap_or(&Value::Null);
    let public_handoff = public.get("request_handoff").unwrap_or(&Value::Null);
    let external_handoff = external.get("request_handoff").unwrap_or(&Value::Null);
    let btc_spv_handoff = btc_spv.get("request_handoff").unwrap_or(&Value::Null);
    let rwa_legal_handoff = rwa_legal.get("request_handoff").unwrap_or(&Value::Null);
    let rwa_legal_registry = rwa_legal.get("registry").unwrap_or(&Value::Null);
    let rwa_source_hash = source_tree_hash(repo_root, RWA_LEGAL_REVIEW_SOURCE_HASH_PATHS)?;
    let btc_spv_profiles = btc_spv
        .get("required_profiles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    let expected_btc_spv_profiles = EXPECTED_BTC_SPV_EVIDENCE_PROFILES.iter().copied().collect::<BTreeSet<_>>();
    let checks = Value::Object(
        [
            ("public_template_present", public_path.is_file()),
            ("external_template_present", external_path.is_file()),
            ("btc_spv_template_present", btc_spv_path.is_file()),
            ("rwa_legal_template_present", rwa_legal_path.is_file()),
            ("public_schema", json_pointer_str(public, "/schema") == Some("novaseal-public-shared-cell-dep-attestation-v0.1")),
            ("external_schema", json_pointer_str(external, "/schema") == Some("novaseal-bip340-external-tcb-review-attestation-v0.1")),
            ("btc_spv_schema", json_pointer_str(btc_spv, "/schema") == Some("novaseal-public-btc-spv-evidence-v0.1")),
            ("rwa_legal_schema", json_pointer_str(rwa_legal, "/schema") == Some("novaseal-rwa-legal-registry-review-evidence-v0.1")),
            ("public_top_level_fields_exact", exact_object_keys(public, EXPECTED_PUBLIC_CELLDEP_ATTESTATION_FIELDS)),
            ("public_release_fields_exact", exact_object_keys(public_release, EXPECTED_PUBLIC_CELLDEP_RELEASE_FIELDS)),
            ("public_release_package", json_pointer_str(public_release, "/package") == Some("novaseal")),
            (
                "public_release_version_current",
                json_pointer_str(public_release, "/version") == Some(EXPECTED_NOVASEAL_RELEASE_VERSION),
            ),
            ("public_release_manifest_commit_present", public_release.get("manifest_commit").is_some_and(value_is_present)),
            ("public_request_handoff_fields_exact", exact_object_keys(public_handoff, EXPECTED_EXTERNAL_REQUEST_HANDOFF_FIELDS)),
            ("public_request_handoff_bundle_path", json_pointer_str(public_handoff, "/bundle") == Some(EXTERNAL_EVIDENCE_HANDOFF)),
            (
                "public_request_handoff_hash_algorithm",
                json_pointer_str(public_handoff, "/bundle_hash_algorithm") == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM),
            ),
            ("public_request_handoff_group", json_pointer_str(public_handoff, "/group") == Some("public_shared_cell_dep_attestation")),
            (
                "public_runtime_verifier_fields_exact",
                exact_object_keys(public_verifier, EXPECTED_PUBLIC_CELLDEP_RUNTIME_VERIFIER_FIELDS),
            ),
            ("external_top_level_fields_exact", exact_object_keys(external, EXPECTED_EXTERNAL_TCB_REVIEW_ATTESTATION_FIELDS)),
            ("external_request_handoff_fields_exact", exact_object_keys(external_handoff, EXPECTED_EXTERNAL_REQUEST_HANDOFF_FIELDS)),
            ("external_request_handoff_bundle_path", json_pointer_str(external_handoff, "/bundle") == Some(EXTERNAL_EVIDENCE_HANDOFF)),
            (
                "external_request_handoff_hash_algorithm",
                json_pointer_str(external_handoff, "/bundle_hash_algorithm") == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM),
            ),
            (
                "external_request_handoff_group",
                json_pointer_str(external_handoff, "/group") == Some("external_bip340_tcb_review_attestation"),
            ),
            ("external_artifact_hash_algorithm", json_pointer_str(external, "/artifact_hash_algorithm") == Some("sha256")),
            (
                "external_artifact_hash_algorithm_matches_current_tcb",
                json_pointer_str(external, "/artifact_hash_algorithm") == artifact_hash_algorithm,
            ),
            ("btc_spv_top_level_fields_exact", exact_object_keys(btc_spv, EXPECTED_PUBLIC_BTC_SPV_EVIDENCE_FIELDS)),
            ("btc_spv_request_handoff_fields_exact", exact_object_keys(btc_spv_handoff, EXPECTED_EXTERNAL_REQUEST_HANDOFF_FIELDS)),
            ("btc_spv_request_handoff_bundle_path", json_pointer_str(btc_spv_handoff, "/bundle") == Some(EXTERNAL_EVIDENCE_HANDOFF)),
            (
                "btc_spv_request_handoff_hash_algorithm",
                json_pointer_str(btc_spv_handoff, "/bundle_hash_algorithm") == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM),
            ),
            ("btc_spv_request_handoff_group", json_pointer_str(btc_spv_handoff, "/group") == Some("public_btc_spv_evidence")),
            ("btc_spv_required_profiles_exact", btc_spv_profiles == expected_btc_spv_profiles),
            ("rwa_legal_top_level_fields_exact", exact_object_keys(rwa_legal, EXPECTED_RWA_LEGAL_REVIEW_EVIDENCE_FIELDS)),
            ("rwa_legal_registry_fields_exact", exact_object_keys(rwa_legal_registry, EXPECTED_RWA_LEGAL_REVIEW_REGISTRY_FIELDS)),
            ("rwa_legal_request_handoff_fields_exact", exact_object_keys(rwa_legal_handoff, EXPECTED_EXTERNAL_REQUEST_HANDOFF_FIELDS)),
            (
                "rwa_legal_request_handoff_bundle_path",
                json_pointer_str(rwa_legal_handoff, "/bundle") == Some(EXTERNAL_EVIDENCE_HANDOFF),
            ),
            (
                "rwa_legal_request_handoff_hash_algorithm",
                json_pointer_str(rwa_legal_handoff, "/bundle_hash_algorithm") == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM),
            ),
            (
                "rwa_legal_request_handoff_group",
                json_pointer_str(rwa_legal_handoff, "/group") == Some("rwa_legal_registry_review_evidence"),
            ),
            ("rwa_legal_profile", json_pointer_str(rwa_legal, "/profile") == Some(EXPECTED_RWA_RECEIPT_PROFILE)),
            (
                "rwa_legal_review_scope_exact",
                exact_string_set(&json_array_strings(rwa_legal, "/review_scope"), EXPECTED_RWA_LEGAL_REVIEW_SCOPE),
            ),
            (
                "rwa_legal_profile_source_tree_hash_matches_current",
                normalize_hex(json_pointer_str(rwa_legal, "/profile_source_tree_sha256")).as_deref()
                    == normalize_hex(json_pointer_str(&rwa_source_hash, "/sha256")).as_deref(),
            ),
            (
                "public_template_network_not_local_devnet",
                json_pointer_str(public, "/network").is_some_and(|network| !network.is_empty() && network != "local-devnet"),
            ),
            (
                "public_artifact_hash_matches_current_tcb",
                normalize_hex(json_pointer_str(public_verifier, "/artifact_hash")).as_deref() == artifact_hash,
            ),
            ("public_dep_type", json_pointer_str(public_verifier, "/dep_type") == Some(EXPECTED_NOVASEAL_CELLDEP_DEP_TYPE)),
            ("public_hash_type", matches!(json_pointer_str(public_verifier, "/hash_type"), Some("data" | "data1" | "type"))),
            (
                "public_hash_type_matches_expected",
                json_pointer_str(public_verifier, "/hash_type") == Some(EXPECTED_NOVASEAL_CELLDEP_HASH_TYPE),
            ),
            (
                "external_artifact_hash_matches_current_tcb",
                normalize_hex(json_pointer_str(external, "/artifact_hash")).as_deref() == artifact_hash,
            ),
            (
                "external_source_tree_hash_matches_current_tcb",
                normalize_hex(json_pointer_str(external, "/source_tree_sha256")).as_deref() == expected_tcb_source_tree_hash,
            ),
            ("public_verifier_id", json_pointer_str(public_verifier, "/verifier_id") == Some("btc.bip340.v0")),
            ("external_verifier_id", json_pointer_str(external, "/verifier_id") == Some("btc.bip340.v0")),
            ("public_ipc_abi", json_pointer_str(public_verifier, "/ipc_abi") == Some("cellscript-btc-bip340-ipc-v0")),
            ("external_ipc_abi", json_pointer_str(external, "/ipc_abi") == Some("cellscript-btc-bip340-ipc-v0")),
        ]
        .into_iter()
        .map(|(key, passed)| (key.to_string(), Value::Bool(passed)))
        .collect(),
    );
    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "expected_artifact_hash": artifact_hash,
        "expected_artifact_hash_algorithm": artifact_hash_algorithm,
        "expected_source_tree_sha256": expected_tcb_source_tree_hash,
        "checks": checks,
        "templates": {
            "public_shared_cell_dep": rel(repo_root, &public_path),
            "external_bip340_tcb_review": rel(repo_root, &external_path),
            "public_btc_spv_evidence": rel(repo_root, &btc_spv_path),
            "rwa_legal_registry_review_evidence": rel(repo_root, &rwa_legal_path),
        },
    }))
}

pub(super) fn validate_bip340_tcb_source_inventory(repo_root: &Path, tcb: &Value) -> Result<Value> {
    let current_source = source_tree_hash_with_options(repo_root, BIP340_TCB_SOURCE_HASH_PATHS, true)?;
    let recorded_repo_commit = json_pointer_str(tcb, "/repo_commit");
    let current_repo_commit = git_commit(repo_root);
    let recorded_files = tcb
        .pointer("/source_inventory/files")
        .and_then(Value::as_array)
        .map(|files| files.iter().filter_map(|row| json_pointer_str(row, "/path").map(str::to_string)).collect::<Vec<_>>())
        .unwrap_or_default();
    let current_files = json_array_strings(&current_source, "/files");
    let checks = json!({
        "current_source_tree_valid": json_pointer_bool(&current_source, "/valid"),
        "source_tree_sha256_matches_current": normalize_hex(json_pointer_str(tcb, "/source_inventory/source_tree_sha256"))
            == normalize_hex(json_pointer_str(&current_source, "/sha256")),
        "source_tree_file_count_matches_current": json_pointer_i64(tcb, "/source_inventory/total_files")
            == json_pointer_i64(&current_source, "/file_count"),
        "source_tree_file_list_matches_current": recorded_files == current_files,
        "source_tree_has_files": json_pointer_i64(&current_source, "/file_count").is_some_and(|count| count > 0),
        "source_tree_invalid_paths_empty": json_array_strings(&current_source, "/invalid_paths").is_empty(),
        "repo_commit_present": recorded_repo_commit.is_some_and(is_git_commit_hash),
        "current_repo_commit_available": current_repo_commit.as_deref().is_some_and(is_git_commit_hash),
        "repo_commit_matches_current_head": recorded_repo_commit == current_repo_commit.as_deref(),
    });
    Ok(json!({
        "schema": "novaseal-bip340-tcb-source-inventory-validation-v0.2",
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "recorded_repo_commit": recorded_repo_commit,
        "current_repo_commit": current_repo_commit,
        "recorded_source_tree_sha256": normalize_hex(json_pointer_str(tcb, "/source_inventory/source_tree_sha256")),
        "current_source_tree": current_source,
        "recorded_files": recorded_files,
    }))
}

pub(super) fn validate_security_audit_coverage(
    repo_root: &Path,
    core_security: &Value,
    invariant_matrix: &Value,
    live_evidence: &Value,
    tcb: &Value,
    attestation_templates: &Value,
) -> Result<Value> {
    let agreement_security = std::fs::read_to_string(repo_root.join(AGREEMENT_ROOT).join("docs/SECURITY.md")).unwrap_or_default();
    let agreement_audit = std::fs::read_to_string(repo_root.join(AGREEMENT_ROOT).join("docs/AUDIT_STATUS.md")).unwrap_or_default();
    let riscv_shell_doc = std::fs::read_to_string(repo_root.join(CORE_ROOT).join("docs/RISCV_VERIFIER_SHELL.md")).unwrap_or_default();
    let riscv_main = std::fs::read_to_string(repo_root.join(VERIFIER_ROOT).join("../novaseal_btc_verifier_riscv/src/main.rs"))
        .or_else(|_| std::fs::read_to_string(repo_root.join(CORE_ROOT).join("verifier/novaseal_btc_verifier_riscv/src/main.rs")))
        .unwrap_or_default();
    let unsafe_hits = tcb.pointer("/source_inventory/unsafe_hits").and_then(Value::as_array).cloned().unwrap_or_default();
    let review_hits = tcb.pointer("/source_inventory/review_hits").and_then(Value::as_array).cloned().unwrap_or_default();
    let unsafe_surface_isolated = unsafe_hits.iter().all(|hit| {
        json_pointer_str(hit, "/path").is_some_and(|path| {
            path.ends_with("Cargo.toml")
                || path == "proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier_riscv/src/main.rs"
        })
    });
    let unsafe_block_count = riscv_main.matches("unsafe {").count();
    let safety_comment_count = riscv_main.matches("// SAFETY:").count();
    let local_tcb_gates = tcb.get("local_review_gates").and_then(Value::as_array).cloned().unwrap_or_default();
    let local_tcb_gates_passed =
        !local_tcb_gates.is_empty() && local_tcb_gates.iter().all(|gate| json_pointer_str(gate, "/status") == Some("passed"));
    let tcb_source_inventory = validate_bip340_tcb_source_inventory(repo_root, tcb)?;
    let checks = json!({
        "agreement_security_sections_present": agreement_security.contains("## Implemented Guards")
            && agreement_security.contains("## Not Implemented")
            && agreement_security.contains("## Risk Posture"),
        "agreement_audit_status_sections_present": agreement_audit.contains("## Claim Classification")
            && agreement_audit.contains("## Fixture Honesty")
            && agreement_audit.contains("## Production Statement Boundary"),
        "core_authority_binding_security_passed": json_pointer_str(core_security, "/status") == Some("passed"),
        "agreement_invariant_matrix_passed": json_pointer_str(invariant_matrix, "/status") == Some("passed"),
        "live_negative_cases_rejected": json_pointer_bool(live_evidence, "/checks/negative_cases_rejected"),
        "live_valid_paths_exercised": json_pointer_bool(live_evidence, "/checks/valid_originate_repay_claim_live"),
        "local_bip340_tcb_review_passed": json_pointer_str(tcb, "/status").is_some_and(|status| status.starts_with("passed_local_review")),
        "local_bip340_tcb_gates_passed": local_tcb_gates_passed,
        "tcb_source_inventory_present": json_pointer_str(tcb, "/source_inventory/source_tree_sha256").is_some()
            && json_pointer_i64(tcb, "/source_inventory/total_files").is_some(),
        "tcb_source_inventory_matches_current": json_pointer_str(&tcb_source_inventory, "/status") == Some("passed"),
        "tcb_review_hits_empty": review_hits.is_empty(),
        "unsafe_boundary_documented": riscv_shell_doc.contains("## Unsafe Boundary")
            && riscv_shell_doc.contains("syscall register ABI only"),
        "unsafe_surface_isolated": unsafe_surface_isolated,
        "unsafe_blocks_have_safety_comments": unsafe_block_count > 0 && safety_comment_count >= unsafe_block_count,
        "external_attestation_templates_current": json_pointer_str(attestation_templates, "/status") == Some("passed"),
        "production_blockers_explicit": agreement_security.contains("public/shared CellDep")
            && agreement_security.contains("public BTC SPV")
            && agreement_security.contains("RWA legal/registry review")
            && agreement_security.contains("external BIP340")
            && agreement_audit.contains("external production attestations, public BTC SPV evidence, and RWA legal/registry review evidence still required"),
    });
    Ok(json!({
        "schema": "novaseal-security-audit-coverage-v0.1",
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "unsafe_inventory": {
            "unsafe_hit_count": unsafe_hits.len(),
            "review_hit_count": review_hits.len(),
            "unsafe_block_count": unsafe_block_count,
            "safety_comment_count": safety_comment_count,
            "boundary": "RISC-V verifier shell syscall ABI only; no raw pointer dereference, transmute, mutable static, or C FFI memory access is accepted by this local audit gate.",
        },
        "tcb_source_inventory": tcb_source_inventory,
        "residual_production_blockers": [
            "public/shared CellDep pinning attestation",
            "public BTC SPV evidence for BTC-facing profiles",
            "RWA legal/registry review evidence",
            "external BIP340 runtime verifier TCB review attestation",
        ],
    }))
}
