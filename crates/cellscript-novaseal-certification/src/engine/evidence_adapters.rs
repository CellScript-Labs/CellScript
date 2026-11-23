use super::bitcoin_anchors::{btc_anchor_source_production_eligible, expected_btc_spv_scenario, handoff_expected_bindings_exact};
use super::provenance::source_tree_hash;
use super::{
    exact_object_keys, exact_string_map, exact_string_set, external_evidence_handoff_reference_hash, is_git_commit_hash, is_hex32,
    json_array_strings, json_object_string_map, json_pointer_i64, json_pointer_str, json_pointer_u64, normalize_hex,
    novaseal_btc_spv_adapter_report_hash, novaseal_external_attestation_report_hash, novaseal_handoff_report_hash,
    object_values_all_true, BTC_SPV_EVIDENCE_ADAPTER, EXPECTED_BTC_SPV_ADAPTER_PUBLIC_FIELDS, EXPECTED_BTC_SPV_EVIDENCE_PROFILES,
    EXPECTED_BTC_SPV_FIELD_CONSTRAINTS, EXPECTED_BTC_SPV_PROFILE_SCENARIOS, EXPECTED_BTC_TX_COMMITMENT_PROFILE,
    EXPECTED_BTC_UTXO_SEAL_PROFILE, EXPECTED_DUAL_SEAL_PROFILE, EXPECTED_EXTERNAL_TCB_EXPECTED_VALUE_FIELDS,
    EXPECTED_EXTERNAL_TCB_FIELD_CONSTRAINTS, EXPECTED_EXTERNAL_TCB_REQUIRED_FIELDS, EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE,
    EXPECTED_NOVASEAL_CELLDEP_DEP_TYPE, EXPECTED_NOVASEAL_CELLDEP_HASH_TYPE, EXPECTED_NOVASEAL_RELEASE_VERSION,
    EXPECTED_PUBLIC_BTC_SPV_HANDOFF_FIELDS, EXPECTED_PUBLIC_CELLDEP_EXPECTED_VALUE_FIELDS, EXPECTED_PUBLIC_CELLDEP_FIELD_CONSTRAINTS,
    EXPECTED_PUBLIC_CELLDEP_REQUIRED_FIELDS, EXPECTED_RWA_LEGAL_REVIEW_EXPECTED_VALUE_FIELDS,
    EXPECTED_RWA_LEGAL_REVIEW_FIELD_CONSTRAINTS, EXPECTED_RWA_LEGAL_REVIEW_REQUIRED_FIELDS, EXPECTED_RWA_LEGAL_REVIEW_SCOPE,
    EXPECTED_RWA_RECEIPT_PROFILE, EXTERNAL_ATTESTATION_ADAPTER, EXTERNAL_TCB_ATTESTATION, NOVASEAL_HANDOFF_HASH_ALGORITHM,
    PUBLIC_BTC_SPV_EVIDENCE, PUBLIC_CELLDEP_ATTESTATION, RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, RWA_LEGAL_REVIEW_SOURCE_HASH_PATHS,
};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[cfg(test)]
pub(super) fn validate_btc_spv_evidence_adapter_detail(report: &Value) -> Value {
    validate_btc_spv_evidence_adapter_detail_with_sources(report, None, None)
}

pub(super) fn validate_btc_spv_evidence_adapter_detail_with_sources(
    report: &Value,
    service_builder: Option<&Value>,
    public_btc_spv_template: Option<&Value>,
) -> Value {
    let cases = report.get("cases").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut by_profile: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for case in &cases {
        if let Some(profile) = json_pointer_str(case, "/profile") {
            by_profile.entry(profile.to_string()).or_default().push(case.clone());
        }
    }
    let expected_profiles = EXPECTED_BTC_SPV_EVIDENCE_PROFILES.iter().map(|profile| (*profile).to_string()).collect::<BTreeSet<_>>();
    let actual_profiles =
        cases.iter().filter_map(|case| json_pointer_str(case, "/profile").map(ToString::to_string)).collect::<BTreeSet<_>>();
    let service_builder_cases = service_builder.and_then(|builder| builder.get("cases")).and_then(Value::as_array);
    let service_builder_source_required = service_builder.is_some();

    let mut case_checks = Map::new();
    for expected_profile in EXPECTED_BTC_SPV_EVIDENCE_PROFILES {
        let matches = by_profile.get(*expected_profile).cloned().unwrap_or_default();
        let case = matches.first().cloned().unwrap_or(Value::Null);
        let required_fields = json_array_strings(&case, "/request/required_public_fields");
        let external_inputs = json_array_strings(&case, "/request/required_external_inputs");
        let required_public_fields_complete =
            EXPECTED_BTC_SPV_ADAPTER_PUBLIC_FIELDS.iter().all(|field| required_fields.iter().any(|actual| actual == field));
        let service_builder_case = service_builder_cases.and_then(|builder_cases| {
            builder_cases.iter().find(|builder_case| json_pointer_str(builder_case, "/profile") == Some(*expected_profile))
        });
        let expected_service_builder_case_hash =
            service_builder_case.map(|builder_case| novaseal_btc_spv_adapter_report_hash("service_builder_case", builder_case));
        let checks = json!({
            "exactly_one_case": matches.len() == 1,
            "status_passed": json_pointer_str(&case, "/status") == Some("passed"),
            "request_profile_matches": json_pointer_str(&case, "/request/profile") == Some(*expected_profile),
            "scenario_matches_expected": json_pointer_str(&case, "/request/scenario") == expected_btc_spv_scenario(expected_profile),
            "minimum_confirmations_at_least_six": json_pointer_i64(&case, "/request/minimum_confirmations").unwrap_or_default() >= 6,
            "public_btc_spv_external_input_named": external_inputs.iter().any(|value| value == "public_btc_spv_evidence"),
            "ckb_live_tx_hash": json_pointer_str(&case, "/request/ckb_live_tx_hash").is_some_and(is_hex32),
            "live_report_hash": json_pointer_str(&case, "/request/live_report_hash").is_some_and(is_hex32),
            "service_builder_case_present_in_current_report": !service_builder_source_required || service_builder_case.is_some(),
            "service_builder_case_hash": json_pointer_str(&case, "/request/service_builder_case_hash").is_some_and(is_hex32),
            "service_builder_case_hash_matches_current_report": if service_builder_source_required {
                expected_service_builder_case_hash
                    .as_deref()
                    .is_some_and(|expected| json_pointer_str(&case, "/request/service_builder_case_hash") == Some(expected))
            } else {
                true
            },
            "service_builder_tx_skeleton_hash": json_pointer_str(&case, "/request/service_builder_tx_skeleton_hash").is_some_and(is_hex32),
            "service_builder_tx_skeleton_hash_matches_current_report": if service_builder_source_required {
                service_builder_case
                    .and_then(|builder_case| json_pointer_str(builder_case, "/response/tx_skeleton_hash"))
                    .is_some_and(|expected| json_pointer_str(&case, "/request/service_builder_tx_skeleton_hash") == Some(expected))
            } else {
                true
            },
            "service_builder_receipt_binding_hash": json_pointer_str(&case, "/request/service_builder_receipt_binding_hash").is_some_and(is_hex32),
            "service_builder_receipt_binding_hash_matches_current_report": if service_builder_source_required {
                service_builder_case
                    .and_then(|builder_case| json_pointer_str(builder_case, "/response/receipt_binding_hash"))
                    .is_some_and(|expected| json_pointer_str(&case, "/request/service_builder_receipt_binding_hash") == Some(expected))
            } else {
                true
            },
            "ckb_live_tx_hash_matches_current_report": if service_builder_source_required {
                service_builder_case
                    .and_then(|builder_case| json_pointer_str(builder_case, "/request/required_live_inputs/live_devnet_tx_hash"))
                    .is_some_and(|expected| json_pointer_str(&case, "/request/ckb_live_tx_hash") == Some(expected))
            } else {
                true
            },
            "live_report_hash_matches_current_report": if service_builder_source_required {
                service_builder_case
                    .and_then(|builder_case| json_pointer_str(builder_case, "/request/required_live_inputs/live_report_hash"))
                    .is_some_and(|expected| json_pointer_str(&case, "/request/live_report_hash") == Some(expected))
            } else {
                true
            },
            "expected_anchor_source_production_eligible": json_pointer_str(&case, "/request/expected_anchor_source")
                .is_some_and(|source| btc_anchor_source_production_eligible(expected_profile, source)),
            "local_anchor_source_present": json_pointer_str(&case, "/request/local_anchor_source").is_some_and(|source| !source.is_empty()),
            "ckb_btc_commitment_hash": json_pointer_str(&case, "/request/ckb_btc_commitment_hash").is_some_and(is_hex32),
            "ckb_btc_commitment_hash_matches_current_report": if service_builder_source_required {
                service_builder_case
                    .and_then(|builder_case| {
                        json_pointer_str(builder_case, "/request/required_live_inputs/public_btc_anchor/ckb_btc_commitment_hash")
                    })
                    .is_some_and(|expected| json_pointer_str(&case, "/request/ckb_btc_commitment_hash") == Some(expected))
            } else {
                true
            },
            "expected_btc_txid_present": json_pointer_str(&case, "/request/expected_btc_txid").is_some_and(is_hex32),
            "expected_btc_wtxid_present": json_pointer_str(&case, "/request/expected_btc_wtxid").is_some_and(is_hex32),
            "expected_output_fields_present": *expected_profile != EXPECTED_BTC_TX_COMMITMENT_PROFILE
                || (json_pointer_i64(&case, "/request/expected_btc_output_index").is_some_and(|value| value >= 0)
                    && json_pointer_i64(&case, "/request/expected_btc_amount_sats").is_some_and(|value| value > 0)),
            "expected_utxo_fields_present": *expected_profile != EXPECTED_BTC_UTXO_SEAL_PROFILE
                || (json_pointer_str(&case, "/request/expected_sealed_btc_txid").is_some_and(is_hex32)
                    && json_pointer_i64(&case, "/request/expected_sealed_btc_vout_index").is_some_and(|value| value >= 0)
                    && json_pointer_i64(&case, "/request/expected_sealed_btc_amount_sats").is_some_and(|value| value > 0)
                    && json_pointer_str(&case, "/request/expected_script_pubkey_hash").is_some_and(is_hex32)
                    && json_pointer_i64(&case, "/request/expected_spend_input_index").is_some_and(|value| value >= 0)
                    && json_pointer_str(&case, "/request/expected_sealed_utxo_commitment_hash").is_some_and(is_hex32)),
            "expected_dual_sealed_utxo_fields_present": *expected_profile != EXPECTED_DUAL_SEAL_PROFILE
                || (json_pointer_str(&case, "/request/expected_sealed_btc_txid").is_some_and(is_hex32)
                    && json_pointer_i64(&case, "/request/expected_sealed_btc_vout_index").is_some_and(|value| value >= 0)
                    && json_pointer_i64(&case, "/request/expected_sealed_btc_amount_sats").is_some_and(|value| value > 0)
                    && json_pointer_str(&case, "/request/expected_script_pubkey_hash").is_some_and(is_hex32)
                    && json_pointer_i64(&case, "/request/expected_spend_input_index").is_some_and(|value| value >= 0)
                    && json_pointer_str(&case, "/request/expected_sealed_utxo_commitment_hash").is_some_and(is_hex32)),
            "template_case_hash": json_pointer_str(&case, "/request/template_case_hash").is_some_and(is_hex32),
            "required_public_fields_complete": required_public_fields_complete,
            "required_public_fields_exact": exact_string_set(&required_fields, EXPECTED_BTC_SPV_ADAPTER_PUBLIC_FIELDS),
            "field_constraints_exact": exact_string_map(
                case.pointer("/request/field_constraints").unwrap_or(&Value::Null),
                EXPECTED_BTC_SPV_FIELD_CONSTRAINTS,
            ),
            "fixture_checks_passed": object_values_all_true(case.get("checks")),
        });
        case_checks.insert((*expected_profile).to_string(), checks);
    }

    let expected_service_builder_report_hash =
        service_builder.map(|builder| novaseal_btc_spv_adapter_report_hash("service_builder_report", builder));
    let expected_public_btc_spv_template_hash =
        public_btc_spv_template.map(|template| novaseal_btc_spv_adapter_report_hash("public_btc_spv_template", template));
    let checks = json!({
        "report_passed": json_pointer_str(report, "/status") == Some("passed"),
        "schema_current": json_pointer_str(report, "/schema") == Some("novaseal-btc-spv-evidence-adapter-v0.1"),
        "adapter_status_request_ready": json_pointer_str(report, "/adapter_status") == Some("request_ready_external_evidence_required"),
        "service_builder_report_hash": json_pointer_str(report, "/source_service_builder_report_hash").is_some_and(is_hex32),
        "service_builder_report_hash_matches_current_report": expected_service_builder_report_hash
            .as_deref()
            .is_none_or(|expected| json_pointer_str(report, "/source_service_builder_report_hash") == Some(expected)),
        "public_btc_spv_template_hash": json_pointer_str(report, "/source_public_btc_spv_template_hash").is_some_and(is_hex32),
        "public_btc_spv_template_hash_matches_current_template": expected_public_btc_spv_template_hash
            .as_deref()
            .is_none_or(|expected| json_pointer_str(report, "/source_public_btc_spv_template_hash") == Some(expected)),
        "production_output_named": json_pointer_str(report, "/production_output") == Some(PUBLIC_BTC_SPV_EVIDENCE),
        "summary_counts_match": json_pointer_i64(report, "/summary/total") == Some(EXPECTED_BTC_SPV_EVIDENCE_PROFILES.len() as i64)
            && json_pointer_i64(report, "/summary/matched") == json_pointer_i64(report, "/summary/total"),
        "exact_profiles": actual_profiles == expected_profiles,
        "case_details": case_checks.values().all(|row| object_values_all_true(Some(row))),
    });

    json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "cases": case_checks,
        "expected_profiles": expected_profiles.into_iter().collect::<Vec<_>>(),
        "case_count": cases.len(),
        "production_boundary": json_pointer_str(report, "/production_boundary"),
    })
}

#[cfg(test)]
pub(super) fn validate_external_attestation_adapter_detail(report: &Value) -> Value {
    validate_external_attestation_adapter_detail_with_sources(report, None, None, None)
}

pub(super) fn validate_external_attestation_adapter_detail_with_sources(
    report: &Value,
    tcb_review: Option<&Value>,
    public_cell_dep_template: Option<&Value>,
    external_tcb_template: Option<&Value>,
) -> Value {
    let cases = report.get("cases").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut by_name: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for case in &cases {
        if let Some(name) = json_pointer_str(case, "/name") {
            by_name.entry(name.to_string()).or_default().push(case.clone());
        }
    }
    let expected = [
        (
            "public_shared_cell_dep_attestation",
            PUBLIC_CELLDEP_ATTESTATION,
            "novaseal-public-shared-cell-dep-attestation-v0.1",
            "attested",
        ),
        (
            "external_bip340_tcb_review_attestation",
            EXTERNAL_TCB_ATTESTATION,
            "novaseal-bip340-external-tcb-review-attestation-v0.1",
            "accepted",
        ),
    ];
    let expected_names = expected.iter().map(|(name, _, _, _)| (*name).to_string()).collect::<BTreeSet<_>>();
    let actual_names =
        cases.iter().filter_map(|case| json_pointer_str(case, "/name").map(ToString::to_string)).collect::<BTreeSet<_>>();
    let expected_tcb_artifact_hash =
        tcb_review.and_then(|tcb| normalize_hex(json_pointer_str(tcb, "/runtime_artifact/artifact_hash")));
    let expected_tcb_artifact_hash_algorithm =
        tcb_review.and_then(|tcb| json_pointer_str(tcb, "/runtime_artifact/artifact_hash_algorithm"));
    let expected_tcb_source_tree_hash =
        tcb_review.and_then(|tcb| normalize_hex(json_pointer_str(tcb, "/source_inventory/source_tree_sha256")));
    let expected_tcb_repo_commit = tcb_review.and_then(|tcb| json_pointer_str(tcb, "/repo_commit"));

    let mut case_checks = Map::new();
    for (name, production_output, template_schema, required_status) in expected {
        let matches = by_name.get(name).cloned().unwrap_or_default();
        let case = matches.first().cloned().unwrap_or(Value::Null);
        let required_fields = json_array_strings(&case, "/request/required_public_fields");
        let expected_required_fields = if name == "public_shared_cell_dep_attestation" {
            EXPECTED_PUBLIC_CELLDEP_REQUIRED_FIELDS
        } else {
            EXPECTED_EXTERNAL_TCB_REQUIRED_FIELDS
        };
        let expected_field_constraints = if name == "public_shared_cell_dep_attestation" {
            EXPECTED_PUBLIC_CELLDEP_FIELD_CONSTRAINTS
        } else {
            EXPECTED_EXTERNAL_TCB_FIELD_CONSTRAINTS
        };
        let expected_template_hash = if name == "public_shared_cell_dep_attestation" {
            public_cell_dep_template.map(|template| novaseal_external_attestation_report_hash("public_celldep_template", template))
        } else {
            external_tcb_template.map(|template| novaseal_external_attestation_report_hash("external_tcb_template", template))
        };
        let checks = json!({
            "exactly_one_case": matches.len() == 1,
            "status_passed": json_pointer_str(&case, "/status") == Some("passed"),
            "production_output_matches": json_pointer_str(&case, "/request/production_output") == Some(production_output),
            "template_schema_matches": json_pointer_str(&case, "/request/template_schema") == Some(template_schema),
            "template_hash": json_pointer_str(&case, "/request/template_hash").is_some_and(is_hex32),
            "template_hash_matches_current_template": expected_template_hash
                .as_deref()
                .is_none_or(|expected| json_pointer_str(&case, "/request/template_hash") == Some(expected)),
            "verifier_id_current": json_pointer_str(&case, "/request/verifier_id") == Some("btc.bip340.v0"),
            "ipc_abi_current": json_pointer_str(&case, "/request/ipc_abi") == Some("cellscript-btc-bip340-ipc-v0"),
            "required_status_matches": json_pointer_str(&case, "/request/required_status") == Some(required_status),
            "required_fields_complete": expected_required_fields.iter().all(|field| required_fields.iter().any(|actual| actual == field)),
            "required_fields_exact": exact_string_set(&required_fields, expected_required_fields),
            "field_constraints_exact": exact_string_map(
                case.pointer("/request/field_constraints").unwrap_or(&Value::Null),
                expected_field_constraints,
            ),
            "artifact_hash_present": json_pointer_str(&case, "/request/expected_artifact_hash").is_some_and(is_hex32),
            "expected_release_package_current": name != "public_shared_cell_dep_attestation"
                || json_pointer_str(&case, "/request/expected_release_package") == Some("novaseal"),
            "expected_release_version_current": name != "public_shared_cell_dep_attestation"
                || json_pointer_str(&case, "/request/expected_release_version") == Some(EXPECTED_NOVASEAL_RELEASE_VERSION),
            "expected_dep_type_current": name != "public_shared_cell_dep_attestation"
                || json_pointer_str(&case, "/request/expected_dep_type") == Some(EXPECTED_NOVASEAL_CELLDEP_DEP_TYPE),
            "expected_hash_type_current": name != "public_shared_cell_dep_attestation"
                || json_pointer_str(&case, "/request/expected_hash_type") == Some(EXPECTED_NOVASEAL_CELLDEP_HASH_TYPE),
            "expected_release_manifest_commit_present": name != "public_shared_cell_dep_attestation"
                || json_pointer_str(&case, "/request/expected_release_manifest_commit").is_some_and(is_git_commit_hash),
            "expected_release_manifest_commit_matches_current_tcb": name != "public_shared_cell_dep_attestation"
                || expected_tcb_repo_commit
                    .is_none_or(|expected| json_pointer_str(&case, "/request/expected_release_manifest_commit") == Some(expected)),
            "expected_review_scope_exact": name != "external_bip340_tcb_review_attestation"
                || exact_string_set(&json_array_strings(&case, "/request/expected_review_scope"), EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE),
            "expected_artifact_hash_matches_current_tcb": expected_tcb_artifact_hash
                .as_deref()
                .is_none_or(|expected| {
                    normalize_hex(json_pointer_str(&case, "/request/expected_artifact_hash")).as_deref() == Some(expected)
                }),
            "template_artifact_hash_matches_current_tcb": expected_tcb_artifact_hash
                .as_deref()
                .is_none_or(|expected| {
                    normalize_hex(json_pointer_str(&case, "/request/template_artifact_hash")).as_deref() == Some(expected)
                }),
            "expected_source_tree_sha256_matches_current_tcb": name != "external_bip340_tcb_review_attestation"
                || expected_tcb_source_tree_hash
                    .as_deref()
                    .is_none_or(|expected| {
                        normalize_hex(json_pointer_str(&case, "/request/expected_source_tree_sha256")).as_deref() == Some(expected)
                    }),
            "template_source_tree_sha256_matches_current_tcb": name != "external_bip340_tcb_review_attestation"
                || expected_tcb_source_tree_hash
                    .as_deref()
                    .is_none_or(|expected| {
                        normalize_hex(json_pointer_str(&case, "/request/template_source_tree_sha256")).as_deref() == Some(expected)
                    }),
            "expected_artifact_hash_algorithm_matches_current_tcb": name == "public_shared_cell_dep_attestation"
                || expected_tcb_artifact_hash_algorithm
                    .is_none_or(|expected| json_pointer_str(&case, "/request/expected_artifact_hash_algorithm") == Some(expected)),
            "artifact_hash_algorithm_matches_tcb": name == "public_shared_cell_dep_attestation"
                || (
                    json_pointer_str(&case, "/request/expected_artifact_hash_algorithm") == Some("sha256")
                        && json_pointer_str(&case, "/request/template_artifact_hash_algorithm")
                            == json_pointer_str(&case, "/request/expected_artifact_hash_algorithm")
                ),
            "fixture_checks_passed": object_values_all_true(case.get("checks")),
        });
        case_checks.insert(name.to_string(), checks);
    }

    let expected_tcb_review_hash = tcb_review.map(|tcb| novaseal_external_attestation_report_hash("tcb_review", tcb));
    let expected_public_cell_dep_template_hash =
        public_cell_dep_template.map(|template| novaseal_external_attestation_report_hash("public_celldep_template", template));
    let expected_external_tcb_template_hash =
        external_tcb_template.map(|template| novaseal_external_attestation_report_hash("external_tcb_template", template));
    let checks = json!({
        "report_passed": json_pointer_str(report, "/status") == Some("passed"),
        "schema_current": json_pointer_str(report, "/schema") == Some("novaseal-external-attestation-adapter-v0.1"),
        "adapter_status_request_ready": json_pointer_str(report, "/adapter_status") == Some("request_ready_external_attestations_required"),
        "source_tcb_review_hash": json_pointer_str(report, "/source_tcb_review_hash").is_some_and(is_hex32),
        "source_tcb_review_hash_matches_current_report": expected_tcb_review_hash
            .as_deref()
            .is_none_or(|expected| json_pointer_str(report, "/source_tcb_review_hash") == Some(expected)),
        "source_public_cell_dep_template_hash": json_pointer_str(report, "/source_public_cell_dep_template_hash").is_some_and(is_hex32),
        "source_public_cell_dep_template_hash_matches_current_template": expected_public_cell_dep_template_hash
            .as_deref()
            .is_none_or(|expected| json_pointer_str(report, "/source_public_cell_dep_template_hash") == Some(expected)),
        "source_external_tcb_template_hash": json_pointer_str(report, "/source_external_tcb_template_hash").is_some_and(is_hex32),
        "source_external_tcb_template_hash_matches_current_template": expected_external_tcb_template_hash
            .as_deref()
            .is_none_or(|expected| json_pointer_str(report, "/source_external_tcb_template_hash") == Some(expected)),
        "summary_counts_match": json_pointer_i64(report, "/summary/total") == Some(expected_names.len() as i64)
            && json_pointer_i64(report, "/summary/matched") == json_pointer_i64(report, "/summary/total"),
        "exact_attestations": actual_names == expected_names,
        "case_details": case_checks.values().all(|row| object_values_all_true(Some(row))),
    });

    json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "cases": case_checks,
        "expected_attestations": expected_names.into_iter().collect::<Vec<_>>(),
        "case_count": cases.len(),
        "production_boundary": json_pointer_str(report, "/production_boundary"),
    })
}

pub(super) fn validate_external_evidence_handoff_detail(
    repo_root: &Path,
    report: &Value,
    btc_spv_adapter: &Value,
    external_attestation_adapter: &Value,
) -> Value {
    let cases = report.get("cases").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut by_group: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for case in &cases {
        if let Some(group) = json_pointer_str(case, "/group") {
            by_group.entry(group.to_string()).or_default().push(case.clone());
        }
    }

    let expected = [
        ("public_btc_spv_evidence", PUBLIC_BTC_SPV_EVIDENCE),
        ("public_shared_cell_dep_attestation", PUBLIC_CELLDEP_ATTESTATION),
        ("external_bip340_tcb_review_attestation", EXTERNAL_TCB_ATTESTATION),
        ("rwa_legal_registry_review_evidence", RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE),
    ];
    let expected_groups = expected.iter().map(|(group, _)| (*group).to_string()).collect::<BTreeSet<_>>();
    let expected_outputs = expected.iter().map(|(_, output)| (*output).to_string()).collect::<BTreeSet<_>>();
    let actual_groups =
        cases.iter().filter_map(|case| json_pointer_str(case, "/group").map(ToString::to_string)).collect::<BTreeSet<_>>();
    let actual_outputs = json_array_strings(report, "/production_outputs").into_iter().collect::<BTreeSet<_>>();
    let expected_btc_spv_adapter_hash = novaseal_handoff_report_hash("btc_spv_adapter", btc_spv_adapter);
    let expected_external_attestation_adapter_hash =
        novaseal_handoff_report_hash("external_attestation_adapter", external_attestation_adapter);
    let expected_handoff_bundle_hash = external_evidence_handoff_reference_hash(report);
    let expected_btc_spv_scenarios = btc_spv_adapter_expected_scenarios(btc_spv_adapter);
    let expected_btc_spv_case_bindings = btc_spv_adapter_expected_case_bindings(btc_spv_adapter);
    let expected_public_manifest_commit = adapter_case_request_str(
        external_attestation_adapter,
        "public_shared_cell_dep_attestation",
        "/request/expected_release_manifest_commit",
    );
    let expected_public_release_package = adapter_case_request_str(
        external_attestation_adapter,
        "public_shared_cell_dep_attestation",
        "/request/expected_release_package",
    );
    let expected_public_release_version = adapter_case_request_str(
        external_attestation_adapter,
        "public_shared_cell_dep_attestation",
        "/request/expected_release_version",
    );
    let expected_public_artifact_hash = adapter_case_request_str(
        external_attestation_adapter,
        "public_shared_cell_dep_attestation",
        "/request/expected_artifact_hash",
    );
    let expected_public_dep_type =
        adapter_case_request_str(external_attestation_adapter, "public_shared_cell_dep_attestation", "/request/expected_dep_type");
    let expected_public_hash_type =
        adapter_case_request_str(external_attestation_adapter, "public_shared_cell_dep_attestation", "/request/expected_hash_type");
    let expected_public_ipc_abi =
        adapter_case_request_str(external_attestation_adapter, "public_shared_cell_dep_attestation", "/request/ipc_abi");
    let expected_public_verifier_id =
        adapter_case_request_str(external_attestation_adapter, "public_shared_cell_dep_attestation", "/request/verifier_id");
    let expected_external_tcb_artifact_hash = adapter_case_request_str(
        external_attestation_adapter,
        "external_bip340_tcb_review_attestation",
        "/request/expected_artifact_hash",
    );
    let expected_external_tcb_artifact_hash_algorithm = adapter_case_request_str(
        external_attestation_adapter,
        "external_bip340_tcb_review_attestation",
        "/request/expected_artifact_hash_algorithm",
    );
    let expected_external_tcb_ipc_abi =
        adapter_case_request_str(external_attestation_adapter, "external_bip340_tcb_review_attestation", "/request/ipc_abi");
    let expected_external_tcb_verifier_id =
        adapter_case_request_str(external_attestation_adapter, "external_bip340_tcb_review_attestation", "/request/verifier_id");
    let expected_external_tcb_source_tree_hash = adapter_case_request_str(
        external_attestation_adapter,
        "external_bip340_tcb_review_attestation",
        "/request/expected_source_tree_sha256",
    );
    let expected_external_tcb_review_scope = adapter_case_request_strings(
        external_attestation_adapter,
        "external_bip340_tcb_review_attestation",
        "/request/expected_review_scope",
    );
    let expected_rwa_profile_source_hash = source_tree_hash(repo_root, RWA_LEGAL_REVIEW_SOURCE_HASH_PATHS)
        .ok()
        .and_then(|value| normalize_hex(json_pointer_str(&value, "/sha256")));

    let mut case_checks = Map::new();
    for (group, production_output) in expected {
        let matches = by_group.get(group).cloned().unwrap_or_default();
        let case = matches.first().cloned().unwrap_or(Value::Null);
        let required_external_fields = json_array_strings(&case, "/required_external_fields");
        let required_profiles = json_array_strings(&case, "/required_profiles");
        let expected_btc_profiles =
            EXPECTED_BTC_SPV_EVIDENCE_PROFILES.iter().map(|profile| (*profile).to_string()).collect::<Vec<_>>();
        let expected_source_hash = if group == "public_btc_spv_evidence" {
            expected_btc_spv_adapter_hash.as_str()
        } else {
            expected_external_attestation_adapter_hash.as_str()
        };
        let expected_source_adapter =
            if group == "public_btc_spv_evidence" { BTC_SPV_EVIDENCE_ADAPTER } else { EXTERNAL_ATTESTATION_ADAPTER };
        let expected_required_external_fields = match group {
            "public_btc_spv_evidence" => EXPECTED_PUBLIC_BTC_SPV_HANDOFF_FIELDS,
            "public_shared_cell_dep_attestation" => EXPECTED_PUBLIC_CELLDEP_REQUIRED_FIELDS,
            "rwa_legal_registry_review_evidence" => EXPECTED_RWA_LEGAL_REVIEW_REQUIRED_FIELDS,
            _ => EXPECTED_EXTERNAL_TCB_REQUIRED_FIELDS,
        };
        let expected_field_constraints = match group {
            "public_btc_spv_evidence" => EXPECTED_BTC_SPV_FIELD_CONSTRAINTS,
            "public_shared_cell_dep_attestation" => EXPECTED_PUBLIC_CELLDEP_FIELD_CONSTRAINTS,
            "rwa_legal_registry_review_evidence" => EXPECTED_RWA_LEGAL_REVIEW_FIELD_CONSTRAINTS,
            _ => EXPECTED_EXTERNAL_TCB_FIELD_CONSTRAINTS,
        };
        let expected_scenarios_match_source_adapter = group != "public_btc_spv_evidence"
            || (json_object_string_map(case.get("expected_scenarios").unwrap_or(&Value::Null)) == expected_btc_spv_scenarios
                && EXPECTED_BTC_SPV_PROFILE_SCENARIOS
                    .iter()
                    .all(|(profile, scenario)| expected_btc_spv_scenarios.get(*profile).is_some_and(|actual| actual == *scenario)));
        let expected_case_bindings_match_source_adapter = group != "public_btc_spv_evidence"
            || (case.get("expected_case_bindings").is_some_and(handoff_expected_bindings_exact)
                && case.get("expected_case_bindings") == Some(&expected_btc_spv_case_bindings));
        let expected_values_match_source_adapter = match group {
            "public_shared_cell_dep_attestation" => {
                exact_object_keys(case.get("expected_values").unwrap_or(&Value::Null), EXPECTED_PUBLIC_CELLDEP_EXPECTED_VALUE_FIELDS)
                    && expected_public_manifest_commit.is_some_and(is_git_commit_hash)
                    && expected_public_release_package == Some("novaseal")
                    && expected_public_release_version == Some(EXPECTED_NOVASEAL_RELEASE_VERSION)
                    && expected_public_artifact_hash.is_some_and(is_hex32)
                    && expected_public_dep_type == Some(EXPECTED_NOVASEAL_CELLDEP_DEP_TYPE)
                    && expected_public_hash_type == Some(EXPECTED_NOVASEAL_CELLDEP_HASH_TYPE)
                    && expected_public_ipc_abi == Some("cellscript-btc-bip340-ipc-v0")
                    && expected_public_verifier_id == Some("btc.bip340.v0")
                    && json_pointer_str(&case, "/expected_values/artifact_hash") == expected_public_artifact_hash
                    && json_pointer_str(&case, "/expected_values/release.package") == expected_public_release_package
                    && json_pointer_str(&case, "/expected_values/release.version") == expected_public_release_version
                    && json_pointer_str(&case, "/expected_values/release.manifest_commit") == expected_public_manifest_commit
                    && json_pointer_str(&case, "/expected_values/runtime_verifier.dep_type") == expected_public_dep_type
                    && json_pointer_str(&case, "/expected_values/runtime_verifier.hash_type") == expected_public_hash_type
                    && json_pointer_str(&case, "/expected_values/runtime_verifier.ipc_abi") == expected_public_ipc_abi
                    && json_pointer_str(&case, "/expected_values/runtime_verifier.verifier_id") == expected_public_verifier_id
            }
            "external_bip340_tcb_review_attestation" => {
                exact_object_keys(case.get("expected_values").unwrap_or(&Value::Null), EXPECTED_EXTERNAL_TCB_EXPECTED_VALUE_FIELDS)
                    && expected_external_tcb_artifact_hash.is_some_and(is_hex32)
                    && expected_external_tcb_source_tree_hash.is_some_and(is_hex32)
                    && expected_external_tcb_ipc_abi == Some("cellscript-btc-bip340-ipc-v0")
                    && expected_external_tcb_verifier_id == Some("btc.bip340.v0")
                    && exact_string_set(&expected_external_tcb_review_scope, EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE)
                    && json_pointer_str(&case, "/expected_values/artifact_hash") == expected_external_tcb_artifact_hash
                    && json_pointer_str(&case, "/expected_values/artifact_hash_algorithm")
                        == expected_external_tcb_artifact_hash_algorithm
                    && json_pointer_str(&case, "/expected_values/ipc_abi") == expected_external_tcb_ipc_abi
                    && json_pointer_str(&case, "/expected_values/verifier_id") == expected_external_tcb_verifier_id
                    && json_array_strings(&case, "/expected_values/review_scope") == expected_external_tcb_review_scope
                    && json_pointer_str(&case, "/expected_values/source_tree_sha256") == expected_external_tcb_source_tree_hash
            }
            "rwa_legal_registry_review_evidence" => {
                exact_object_keys(case.get("expected_values").unwrap_or(&Value::Null), EXPECTED_RWA_LEGAL_REVIEW_EXPECTED_VALUE_FIELDS)
                    && expected_rwa_profile_source_hash.as_deref().is_some_and(is_hex32)
                    && json_pointer_str(&case, "/expected_values/profile") == Some(EXPECTED_RWA_RECEIPT_PROFILE)
                    && exact_string_set(&json_array_strings(&case, "/expected_values/review_scope"), EXPECTED_RWA_LEGAL_REVIEW_SCOPE)
                    && normalize_hex(json_pointer_str(&case, "/expected_values/profile_source_tree_sha256"))
                        == expected_rwa_profile_source_hash
            }
            _ => true,
        };
        let checks = json!({
            "exactly_one_case": matches.len() == 1,
            "status_passed": json_pointer_str(&case, "/status") == Some("passed"),
            "production_output_matches": json_pointer_str(&case, "/production_output") == Some(production_output),
            "source_adapter_path_matches_current": json_pointer_str(&case, "/source_adapter") == Some(expected_source_adapter),
            "source_adapter_hash_matches_current": json_pointer_str(&case, "/source_adapter_hash") == Some(expected_source_hash),
            "required_external_fields_complete": expected_required_external_fields
                .iter()
                .all(|field| required_external_fields.iter().any(|actual| actual == field)),
            "required_external_fields_exact": exact_string_set(&required_external_fields, expected_required_external_fields),
            "field_constraints_exact": exact_string_map(
                case.get("field_constraints").unwrap_or(&Value::Null),
                expected_field_constraints,
            ),
            "expected_scenarios_match_source_adapter": expected_scenarios_match_source_adapter,
            "expected_case_bindings_match_source_adapter": expected_case_bindings_match_source_adapter,
            "expected_values_match_source_adapter": expected_values_match_source_adapter,
            "btc_profiles_complete": group != "public_btc_spv_evidence"
                || required_profiles == expected_btc_profiles,
            "fixture_checks_passed": object_values_all_true(case.get("checks")),
        });
        case_checks.insert(group.to_string(), checks);
    }

    let checks = json!({
        "report_passed": json_pointer_str(report, "/status") == Some("passed"),
        "schema_current": json_pointer_str(report, "/schema") == Some("novaseal-external-evidence-handoff-bundle-v0.1"),
        "handoff_status_request_ready": json_pointer_str(report, "/handoff_status") == Some("request_bundle_ready_external_evidence_required"),
        "source_btc_spv_adapter_path_matches_current": json_pointer_str(report, "/source_btc_spv_adapter") == Some(BTC_SPV_EVIDENCE_ADAPTER),
        "source_external_attestation_adapter_path_matches_current": json_pointer_str(report, "/source_external_attestation_adapter")
            == Some(EXTERNAL_ATTESTATION_ADAPTER),
        "source_btc_spv_adapter_hash_matches_current": json_pointer_str(report, "/source_btc_spv_adapter_hash")
            == Some(expected_btc_spv_adapter_hash.as_str()),
        "source_external_attestation_adapter_hash_matches_current": json_pointer_str(report, "/source_external_attestation_adapter_hash")
            == Some(expected_external_attestation_adapter_hash.as_str()),
        "bundle_hash_matches_reference": normalize_hex(json_pointer_str(report, "/bundle_hash")).as_deref()
            == Some(expected_handoff_bundle_hash.as_str()),
        "bundle_hash_algorithm": json_pointer_str(report, "/bundle_hash_algorithm") == Some(NOVASEAL_HANDOFF_HASH_ALGORITHM),
        "summary_counts_match": json_pointer_i64(report, "/summary/total") == Some(expected_groups.len() as i64)
            && json_pointer_i64(report, "/summary/matched") == json_pointer_i64(report, "/summary/total"),
        "exact_groups": actual_groups == expected_groups,
        "exact_production_outputs": actual_outputs == expected_outputs,
        "case_details": case_checks.values().all(|row| object_values_all_true(Some(row))),
    });

    json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "cases": case_checks,
        "expected_groups": expected_groups.into_iter().collect::<Vec<_>>(),
        "expected_production_outputs": expected_outputs.into_iter().collect::<Vec<_>>(),
        "case_count": cases.len(),
        "production_boundary": json_pointer_str(report, "/production_boundary"),
    })
}

fn adapter_case_request_str<'a>(adapter: &'a Value, case_name: &str, pointer: &str) -> Option<&'a str> {
    adapter
        .get("cases")?
        .as_array()?
        .iter()
        .find(|case| json_pointer_str(case, "/name") == Some(case_name))
        .and_then(|case| json_pointer_str(case, pointer))
}

fn adapter_case_request_strings(adapter: &Value, case_name: &str, pointer: &str) -> Vec<String> {
    adapter
        .get("cases")
        .and_then(Value::as_array)
        .and_then(|cases| cases.iter().find(|case| json_pointer_str(case, "/name") == Some(case_name)))
        .map_or_else(Vec::new, |case| json_array_strings(case, pointer))
}

pub(super) fn handoff_case<'a>(handoff: &'a Value, group: &str) -> Option<&'a Value> {
    handoff
        .get("cases")
        .and_then(Value::as_array)
        .and_then(|cases| cases.iter().find(|case| json_pointer_str(case, "/group") == Some(group)))
}

pub(super) fn handoff_case_expected_values<'a>(handoff: &'a Value, group: &str) -> Option<&'a Value> {
    handoff_case(handoff, group).and_then(|case| case.get("expected_values"))
}

fn btc_spv_adapter_expected_scenarios(adapter: &Value) -> BTreeMap<String, String> {
    adapter
        .get("cases")
        .and_then(Value::as_array)
        .map(|cases| {
            cases
                .iter()
                .filter_map(|case| {
                    let profile = json_pointer_str(case, "/profile")?;
                    let scenario = json_pointer_str(case, "/request/scenario")?;
                    Some((profile.to_string(), scenario.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn btc_spv_adapter_expected_case_bindings(adapter: &Value) -> Value {
    let binding_fields = [
        ("ckb_live_tx_hash", "/request/ckb_live_tx_hash"),
        ("live_report_hash", "/request/live_report_hash"),
        ("service_builder_case_hash", "/request/service_builder_case_hash"),
        ("service_builder_tx_skeleton_hash", "/request/service_builder_tx_skeleton_hash"),
        ("service_builder_receipt_binding_hash", "/request/service_builder_receipt_binding_hash"),
        ("ckb_btc_commitment_hash", "/request/ckb_btc_commitment_hash"),
    ];
    let mut profiles = Map::new();
    if let Some(cases) = adapter.get("cases").and_then(Value::as_array) {
        for case in cases {
            let Some(profile) = json_pointer_str(case, "/profile") else {
                continue;
            };
            let mut binding = Map::new();
            for (field, pointer) in binding_fields {
                if let Some(value) = json_pointer_str(case, pointer) {
                    binding.insert(field.to_string(), Value::String(value.to_string()));
                }
            }
            for (field, pointer) in [
                ("anchor_source", "/request/expected_anchor_source"),
                ("btc_txid", "/request/expected_btc_txid"),
                ("btc_wtxid", "/request/expected_btc_wtxid"),
                ("sealed_btc_txid", "/request/expected_sealed_btc_txid"),
                ("script_pubkey_hash", "/request/expected_script_pubkey_hash"),
                ("sealed_utxo_commitment_hash", "/request/expected_sealed_utxo_commitment_hash"),
            ] {
                if let Some(value) = json_pointer_str(case, pointer) {
                    binding.insert(field.to_string(), Value::String(value.to_string()));
                }
            }
            for (field, pointer) in [
                ("btc_output_index", "/request/expected_btc_output_index"),
                ("btc_amount_sats", "/request/expected_btc_amount_sats"),
                ("spend_input_index", "/request/expected_spend_input_index"),
                ("sealed_btc_vout_index", "/request/expected_sealed_btc_vout_index"),
                ("sealed_btc_amount_sats", "/request/expected_sealed_btc_amount_sats"),
            ] {
                if let Some(value) = json_pointer_u64(case, pointer) {
                    binding.insert(field.to_string(), Value::Number(value.into()));
                }
            }
            profiles.insert(profile.to_string(), Value::Object(binding));
        }
    }
    Value::Object(profiles)
}
