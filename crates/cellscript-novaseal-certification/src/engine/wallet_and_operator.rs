use super::bitcoin_anchors::{
    expected_public_btc_anchor_pointer, expected_public_btc_commitment_hash_pointer, public_btc_anchor_shape_matches_profile,
};
use super::report_files::json_load_path;
use super::{
    is_hex32, is_hex_bytes, is_real_tx_hash, json_array_strings, json_pointer_bool, json_pointer_i64, json_pointer_str,
    novaseal_profile_operator_report_hash, novaseal_service_builder_report_hash, object_values_all_true,
    EXPECTED_AGREEMENT_WALLET_ACTIONS, EXPECTED_PROFILE_OPERATOR_FIXTURES,
};
use crate::error::Result;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(super) fn validate_wallet_vector_detail(wallet: &Value) -> Value {
    let vectors = wallet.get("vectors").and_then(Value::as_array).cloned().unwrap_or_default();
    let agreement_vectors = vectors
        .iter()
        .filter(|vector| json_pointer_str(vector, "/suite") == Some("novaseal-agreement-profile-v0"))
        .cloned()
        .collect::<Vec<_>>();
    let mut by_action: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for vector in &agreement_vectors {
        if let Some(action) = json_pointer_str(vector, "/action") {
            by_action.entry(action.to_string()).or_default().push(vector.clone());
        }
    }

    let mut action_checks = Map::new();
    for (action, expected) in EXPECTED_AGREEMENT_WALLET_ACTIONS {
        let matches = by_action.get(*action).cloned().unwrap_or_default();
        let vector = matches.first().cloned().unwrap_or(Value::Null);
        let display = vector.get("wallet_display").cloned().unwrap_or(Value::Null);
        let packed = json_pointer_str(&vector, "/signed_intent_packed_hex");
        let byte_len = packed.and_then(|value| is_hex_bytes(value).then_some((value.len() - 2) / 2));
        action_checks.insert(
            (*action).to_string(),
            json!({
                "exactly_one_vector": matches.len() == 1,
                "status_passed": json_pointer_str(&vector, "/status") == Some("passed"),
                "signed_type": json_pointer_str(&vector, "/signed_type") == Some("NovaAgreementSignedIntentV0"),
                "fixed_width_signed_intent_259_bytes": byte_len == Some(259),
                "bip340_message_hash": json_pointer_str(&vector, "/bip340_message_hash").is_some_and(is_hex32),
                "expected_receipt_hash": json_pointer_str(&vector, "/expected_receipt_hash").is_some_and(is_hex32),
                "canonical_envelope_hash_displayed": json_pointer_str(&display, "/canonical_envelope_hash").is_some_and(is_hex32),
                "payout_commitment_hash_displayed": json_pointer_str(&display, "/payout_commitment_hash").is_some_and(is_hex32),
                "agreement_id_displayed": json_pointer_str(&display, "/agreement_id").is_some_and(is_hex32),
                "terms_hash_displayed": json_pointer_str(&display, "/terms_hash").is_some_and(is_hex32),
                "borrower_authority_displayed": json_pointer_str(&display, "/borrower_authority_hash").is_some_and(is_hex32),
                "lender_authority_displayed": json_pointer_str(&display, "/lender_authority_hash").is_some_and(is_hex32),
                "signers_match": json_array_strings(&vector, "/signers") == expected.signers,
                "status_transition_match": json_pointer_i64(&display, "/old_status") == Some(expected.old_status)
                    && json_pointer_i64(&display, "/new_status") == Some(expected.new_status),
                "nonce_transition_match": json_pointer_i64(&display, "/old_nonce") == Some(expected.old_nonce)
                    && json_pointer_i64(&display, "/new_nonce") == Some(expected.new_nonce),
                "terminal_amount_positive": json_pointer_i64(&display, "/terminal_amount_shannons").is_some_and(|amount| amount > 0),
            }),
        );
    }

    let actions_present = by_action.keys().cloned().collect::<BTreeSet<_>>();
    let expected_actions = EXPECTED_AGREEMENT_WALLET_ACTIONS.iter().map(|(name, _)| (*name).to_string()).collect::<BTreeSet<_>>();
    let checks = json!({
        "wallet_report_passed": json_pointer_str(wallet, "/status") == Some("passed"),
        "summary_counts_match": json_pointer_i64(wallet, "/summary/agreement_vectors") == Some(3)
            && json_pointer_i64(wallet, "/summary/core_vectors").unwrap_or_default() >= 6
            && json_pointer_i64(wallet, "/summary/matched") == json_pointer_i64(wallet, "/summary/total"),
        "exact_agreement_actions": actions_present == expected_actions,
        "agreement_action_details": action_checks.values().all(|row| object_values_all_true(Some(row))),
    });
    json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "actions": action_checks,
        "expected_actions": expected_actions.into_iter().collect::<Vec<_>>(),
        "agreement_vector_count": agreement_vectors.len(),
    })
}

pub(super) fn validate_wallet_lock_alignment_detail(alignment: &Value) -> Value {
    let fixtures = alignment.get("fixtures").and_then(Value::as_array).cloned().unwrap_or_default();
    let fixture_count = json_pointer_i64(alignment, "/summary/fixtures");
    let fixture_details_passed = fixtures.iter().all(|fixture| {
        json_pointer_bool(fixture, "/canonical_vs_current_lock_digest_match")
            && json_pointer_str(fixture, "/current_lock_message_rule")
                == Some("hash_blake2b_packed(NovaSealSignedIntentV0 { core, expected_receipt_hash })")
            && json_pointer_str(fixture, "/canonical_wallet_message_rule") == Some("signed_intent_hash_after_resolved_receipt")
            && json_pointer_bool(fixture, "/canonical_wallet_positive/self_verified")
            && json_pointer_bool(fixture, "/current_lock_compat_positive/self_verified")
            && json_pointer_bool(fixture, "/cross_check/canonical_signature_accepts_current_lock_digest")
            && json_pointer_bool(fixture, "/cross_check/current_lock_signature_accepts_canonical_digest")
            && json_pointer_i64(fixture, "/resolved_intent_size_bytes") == Some(254)
    });
    let checks = json!({
        "schema_current": json_pointer_str(alignment, "/schema") == Some("novaseal-wallet-signing-alignment-v0.2"),
        "classification_current": json_pointer_str(alignment, "/classification")
            == Some("wallet_signing_vectors_and_lock_digest_alignment_probe"),
        "exact_fixture_count": fixture_count == Some(11) && fixtures.len() == 11,
        "source_model_uses_packed_intent": json_pointer_bool(alignment, "/source_digest_model/all_required_snippets_present")
            && json_pointer_bool(alignment, "/source_digest_model/state_type_uses_packed_signed_intent_hash")
            && json_pointer_bool(alignment, "/source_digest_model/state_type_verifier_uses_signed_intent_hash")
            && json_pointer_bool(alignment, "/source_digest_model/package_lock_uses_packed_digest")
            && json_pointer_bool(alignment, "/source_digest_model/standalone_lock_uses_packed_digest")
            && !json_pointer_bool(alignment, "/source_digest_model/legacy_domain_hash_visible"),
        "summary_alignment_ready": json_pointer_bool(alignment, "/summary/wallet_lock_alignment_ready"),
        "production_wallet_ready": json_pointer_bool(alignment, "/summary/production_wallet_ready"),
        "summary_digest_counts_match": json_pointer_i64(alignment, "/summary/current_lock_digest_matches_canonical") == fixture_count
            && json_pointer_i64(alignment, "/summary/current_lock_digest_mismatches") == Some(0),
        "summary_signature_counts_match": json_pointer_i64(alignment, "/summary/canonical_wallet_vectors_self_verified") == fixture_count
            && json_pointer_i64(alignment, "/summary/current_lock_compat_vectors_self_verified") == fixture_count
            && json_pointer_i64(alignment, "/summary/canonical_wallet_signatures_accepted_by_current_lock_digest") == fixture_count
            && json_pointer_i64(alignment, "/summary/current_lock_signatures_accepted_by_canonical_wallet_digest") == fixture_count,
        "fixture_details_passed": fixture_details_passed,
    });
    json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "summary": alignment.get("summary").cloned().unwrap_or(Value::Null),
        "source_digest_model": alignment.get("source_digest_model").cloned().unwrap_or(Value::Null),
    })
}

pub(super) fn validate_profile_operator_fixture_detail(repo_root: &Path, report: &Value) -> Result<Value> {
    let cases = report.get("cases").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut by_profile_action: BTreeMap<(String, String), Vec<Value>> = BTreeMap::new();
    for case in &cases {
        if let (Some(profile), Some(action)) = (json_pointer_str(case, "/profile"), json_pointer_str(case, "/action")) {
            by_profile_action.entry((profile.to_string(), action.to_string())).or_default().push(case.clone());
        }
    }

    let expected_profiles =
        EXPECTED_PROFILE_OPERATOR_FIXTURES.iter().map(|fixture| fixture.profile.to_string()).collect::<BTreeSet<_>>();
    let expected_actions = EXPECTED_PROFILE_OPERATOR_FIXTURES
        .iter()
        .map(|fixture| format!("{}:{}", fixture.profile, fixture.action))
        .collect::<BTreeSet<_>>();
    let actual_profiles =
        cases.iter().filter_map(|case| json_pointer_str(case, "/profile").map(ToString::to_string)).collect::<BTreeSet<_>>();
    let actual_actions = cases
        .iter()
        .filter_map(|case| Some(format!("{}:{}", json_pointer_str(case, "/profile")?, json_pointer_str(case, "/action")?)))
        .collect::<BTreeSet<_>>();

    let mut case_checks = Map::new();
    for expected in EXPECTED_PROFILE_OPERATOR_FIXTURES {
        let matches = by_profile_action.get(&(expected.profile.to_string(), expected.action.to_string())).cloned().unwrap_or_default();
        let case = matches.first().cloned().unwrap_or(Value::Null);
        let display = case.get("wallet_display").cloned().unwrap_or(Value::Null);
        let live_report = json_load_path(repo_root, &repo_root.join(expected.live_report))?;
        let expected_live_report_hash = novaseal_profile_operator_report_hash(expected.live_report, &live_report);
        let expected_live_tx_hash = json_pointer_str(&live_report, expected.live_tx_hash_pointer);
        let public_btc_anchor_pointer = expected_public_btc_anchor_pointer(expected.profile);
        let public_btc_anchor_required = public_btc_anchor_pointer.is_some();
        let expected_public_btc_anchor = public_btc_anchor_pointer.and_then(|pointer| live_report.pointer(pointer));
        let expected_public_btc_commitment_hash =
            expected_public_btc_commitment_hash_pointer(expected.profile).and_then(|pointer| json_pointer_str(&live_report, pointer));
        let case_public_btc_anchor = case.pointer("/public_btc_anchor");
        let display_public_btc_anchor = display.pointer("/public_btc_anchor");
        let case_public_btc_commitment_hash =
            case_public_btc_anchor.and_then(|anchor| json_pointer_str(anchor, "/ckb_btc_commitment_hash"));
        let expected_fiber_report_hash = expected
            .fiber_report
            .map(|path| {
                json_load_path(repo_root, &repo_root.join(path)).map(|report| novaseal_profile_operator_report_hash(path, &report))
            })
            .transpose()?;
        let checks = json!({
            "exactly_one_fixture": matches.len() == 1,
            "status_passed": json_pointer_str(&case, "/status") == Some("passed"),
            "fixture_matches": json_pointer_str(&case, "/fixture") == Some(expected.fixture),
            "signers_match": json_array_strings(&case, "/signers") == expected.signers,
            "signed_type_named": json_pointer_str(&case, "/signed_type").is_some_and(|value| value.starts_with("Nova") && value.ends_with("SignedIntentV0")),
            "signed_intent_hash": json_pointer_str(&case, "/signed_intent_hash").is_some_and(is_hex32),
            "bip340_message_hash_matches": json_pointer_str(&case, "/bip340_message_hash") == json_pointer_str(&case, "/signed_intent_hash"),
            "signed_intent_body_hex": json_pointer_str(&case, "/signed_intent_body_hex").is_some_and(is_hex_bytes),
            "signed_intent_preimage_hex": json_pointer_str(&case, "/signed_intent_hash_preimage_hex").is_some_and(is_hex_bytes),
            "witness_shape_hash": json_pointer_str(&case, "/witness_shape_hash").is_some_and(is_hex32),
            "tx_skeleton_hash": json_pointer_str(&case, "/tx_skeleton_hash").is_some_and(is_hex32),
            "fixture_hash": json_pointer_str(&case, "/fixture_hash").is_some_and(is_hex32),
            "source_tree_hash": json_pointer_str(&case, "/source_tree_hash").is_some_and(is_hex32),
            "schema_set_hash": json_pointer_str(&case, "/schema_set_hash").is_some_and(is_hex32),
            "proof_matrix_hash": json_pointer_str(&case, "/proof_matrix_hash").is_some_and(is_hex32),
            "display_profile_matches": json_pointer_str(&display, "/profile") == Some(expected.profile),
            "display_action_matches": json_pointer_str(&display, "/action") == Some(expected.action),
            "live_evidence_present_when_required": !expected.live_required
                || json_pointer_str(&case, "/live_report_hash").is_some_and(is_hex32),
            "live_report_hash_matches_current_report": !expected.live_required
                || json_pointer_str(&case, "/live_report_hash") == Some(expected_live_report_hash.as_str()),
            "live_tx_hash_present_when_required": !expected.live_required
                || json_pointer_str(&case, "/live_devnet_tx_hash").is_some_and(is_real_tx_hash),
            "live_tx_hash_matches_current_report": !expected.live_required
                || json_pointer_str(&case, "/live_devnet_tx_hash") == expected_live_tx_hash,
            "display_live_tx_hash_matches_current_report": !expected.live_required
                || json_pointer_str(&display, "/live_devnet_tx_hash") == expected_live_tx_hash,
            "public_btc_anchor_empty_when_not_required": public_btc_anchor_required
                || case_public_btc_anchor.is_none_or(Value::is_null),
            "public_btc_anchor_present_when_required": !public_btc_anchor_required
                || case_public_btc_anchor.is_some_and(|anchor| anchor.is_object()),
            "public_btc_anchor_shape_matches_profile": !public_btc_anchor_required
                || public_btc_anchor_shape_matches_profile(expected.profile, case_public_btc_anchor),
            "public_btc_anchor_matches_current_report": !public_btc_anchor_required
                || case_public_btc_anchor == expected_public_btc_anchor,
            "public_btc_anchor_commitment_matches_current_report": !public_btc_anchor_required
                || case_public_btc_commitment_hash == expected_public_btc_commitment_hash,
            "display_public_btc_anchor_matches_case": !public_btc_anchor_required
                || display_public_btc_anchor == case_public_btc_anchor,
            "external_boundary_documented_when_not_live": expected.live_required
                || json_pointer_str(&display, "/external_boundary") == Some("package_fixture_only_external_btc_and_ckb_finality_required"),
            "fiber_execution_bound_when_required": !expected.fiber_required
                || json_pointer_str(&case, "/fiber_report_hash").is_some_and(is_hex32),
            "fiber_report_hash_matches_current_report": !expected.fiber_required
                || json_pointer_str(&case, "/fiber_report_hash") == expected_fiber_report_hash.as_deref(),
            "fixture_checks_passed": object_values_all_true(case.get("checks")),
        });
        case_checks.insert(format!("{}:{}", expected.profile, expected.action), checks);
    }

    let checks = json!({
        "report_passed": json_pointer_str(report, "/status") == Some("passed"),
        "schema_current": json_pointer_str(report, "/schema") == Some("novaseal-profile-operator-fixtures-v0.1"),
        "summary_counts_match": json_pointer_i64(report, "/summary/total") == Some(EXPECTED_PROFILE_OPERATOR_FIXTURES.len() as i64)
            && json_pointer_i64(report, "/summary/matched") == json_pointer_i64(report, "/summary/total")
            && json_pointer_i64(report, "/summary/profile_count") == Some(expected_profiles.len() as i64),
        "exact_profiles": actual_profiles == expected_profiles,
        "exact_profile_actions": actual_actions == expected_actions,
        "case_details": case_checks.values().all(|row| object_values_all_true(Some(row))),
    });

    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "cases": case_checks,
        "expected_profiles": expected_profiles.into_iter().collect::<Vec<_>>(),
        "expected_actions": expected_actions.into_iter().collect::<Vec<_>>(),
        "case_count": cases.len(),
    }))
}

pub(super) fn validate_service_builder_fixture_detail(report: &Value, operator_fixtures: &Value) -> Value {
    let cases = report.get("cases").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut by_profile_action: BTreeMap<(String, String), Vec<Value>> = BTreeMap::new();
    for case in &cases {
        if let (Some(profile), Some(action)) = (json_pointer_str(case, "/profile"), json_pointer_str(case, "/action")) {
            by_profile_action.entry((profile.to_string(), action.to_string())).or_default().push(case.clone());
        }
    }
    let operator_cases = operator_fixtures.get("cases").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut operator_by_profile_action: BTreeMap<(String, String), Vec<Value>> = BTreeMap::new();
    for case in &operator_cases {
        if let (Some(profile), Some(action)) = (json_pointer_str(case, "/profile"), json_pointer_str(case, "/action")) {
            operator_by_profile_action.entry((profile.to_string(), action.to_string())).or_default().push(case.clone());
        }
    }

    let expected_profiles =
        EXPECTED_PROFILE_OPERATOR_FIXTURES.iter().map(|fixture| fixture.profile.to_string()).collect::<BTreeSet<_>>();
    let expected_actions = EXPECTED_PROFILE_OPERATOR_FIXTURES
        .iter()
        .map(|fixture| format!("{}:{}", fixture.profile, fixture.action))
        .collect::<BTreeSet<_>>();
    let actual_profiles =
        cases.iter().filter_map(|case| json_pointer_str(case, "/profile").map(ToString::to_string)).collect::<BTreeSet<_>>();
    let actual_actions = cases
        .iter()
        .filter_map(|case| Some(format!("{}:{}", json_pointer_str(case, "/profile")?, json_pointer_str(case, "/action")?)))
        .collect::<BTreeSet<_>>();
    let expected_operator_report_hash = novaseal_service_builder_report_hash("operator_report", operator_fixtures);

    let mut case_checks = Map::new();
    for expected in EXPECTED_PROFILE_OPERATOR_FIXTURES {
        let matches = by_profile_action.get(&(expected.profile.to_string(), expected.action.to_string())).cloned().unwrap_or_default();
        let case = matches.first().cloned().unwrap_or(Value::Null);
        let operator_matches =
            operator_by_profile_action.get(&(expected.profile.to_string(), expected.action.to_string())).cloned().unwrap_or_default();
        let operator_case = operator_matches.first().cloned().unwrap_or(Value::Null);
        let expected_operator_case_hash = novaseal_service_builder_report_hash("operator_case", &operator_case);
        let public_btc_anchor_required = expected_public_btc_anchor_pointer(expected.profile).is_some();
        let operator_public_btc_anchor = operator_case.pointer("/public_btc_anchor");
        let request_public_btc_anchor = case.pointer("/request/required_live_inputs/public_btc_anchor");
        let tx_skeleton_public_btc_anchor = case.pointer("/tx_skeleton/public_btc_anchor");
        let checks = json!({
            "exactly_one_fixture": matches.len() == 1,
            "exactly_one_operator_fixture": operator_matches.len() == 1,
            "status_passed": json_pointer_str(&case, "/status") == Some("passed"),
            "builder_name": json_pointer_str(&case, "/builder_name") == Some("novaseal-profile-service-builder-v0"),
            "fixture_matches": json_pointer_str(&case, "/fixture") == Some(expected.fixture),
            "signers_match": json_array_strings(&case, "/signers") == expected.signers,
            "operator_fixture_hash": json_pointer_str(&case, "/operator_fixture_hash").is_some_and(is_hex32),
            "operator_fixture_hash_matches_current_operator_case": json_pointer_str(&case, "/operator_fixture_hash")
                == Some(expected_operator_case_hash.as_str()),
            "request_schema": json_pointer_str(&case, "/request/schema") == Some("novaseal-service-builder-request-v0.1"),
            "request_profile_matches": json_pointer_str(&case, "/request/profile") == Some(expected.profile),
            "request_action_matches": json_pointer_str(&case, "/request/action") == Some(expected.action),
            "request_signers_match": json_array_strings(&case, "/request/signers") == expected.signers,
            "request_idempotency_key": json_pointer_str(&case, "/request/idempotency_key").is_some_and(is_hex32),
            "request_operator_hash_matches": json_pointer_str(&case, "/request/operator_fixture_hash") == json_pointer_str(&case, "/operator_fixture_hash"),
            "request_profile_hashes_present": json_pointer_str(&case, "/request/required_profile_inputs/source_tree_hash").is_some_and(is_hex32)
                && json_pointer_str(&case, "/request/required_profile_inputs/schema_set_hash").is_some_and(is_hex32)
                && json_pointer_str(&case, "/request/required_profile_inputs/proof_matrix_hash").is_some_and(is_hex32)
                && json_pointer_str(&case, "/request/required_profile_inputs/fixture_hash").is_some_and(is_hex32),
            "live_inputs_present_when_required": !expected.live_required
                || (json_pointer_str(&case, "/request/required_live_inputs/live_report_hash").is_some_and(is_hex32)
                    && json_pointer_str(&case, "/request/required_live_inputs/live_devnet_tx_hash").is_some_and(is_real_tx_hash)),
            "live_inputs_match_operator_fixture": !expected.live_required
                || (json_pointer_str(&case, "/request/required_live_inputs/live_report_hash")
                    == json_pointer_str(&operator_case, "/live_report_hash")
                    && json_pointer_str(&case, "/request/required_live_inputs/live_devnet_tx_hash")
                        == json_pointer_str(&operator_case, "/live_devnet_tx_hash")),
            "fiber_input_present_when_required": !expected.fiber_required
                || json_pointer_str(&case, "/request/required_live_inputs/fiber_report_hash").is_some_and(is_hex32),
            "fiber_input_matches_operator_fixture": !expected.fiber_required
                || json_pointer_str(&case, "/request/required_live_inputs/fiber_report_hash")
                    == json_pointer_str(&operator_case, "/fiber_report_hash"),
            "public_btc_anchor_input_empty_when_not_required": public_btc_anchor_required
                || request_public_btc_anchor.is_none_or(Value::is_null),
            "public_btc_anchor_input_present_when_required": !public_btc_anchor_required
                || request_public_btc_anchor.is_some_and(|anchor| anchor.is_object()),
            "public_btc_anchor_input_shape_matches_profile": !public_btc_anchor_required
                || public_btc_anchor_shape_matches_profile(expected.profile, request_public_btc_anchor),
            "public_btc_anchor_input_matches_operator_fixture": !public_btc_anchor_required
                || request_public_btc_anchor == operator_public_btc_anchor,
            "external_inputs_named": !json_array_strings(&case, "/request/production_external_inputs").is_empty(),
            "response_schema": json_pointer_str(&case, "/response/schema") == Some("novaseal-service-builder-response-v0.1"),
            "response_profile_matches": json_pointer_str(&case, "/response/profile") == Some(expected.profile),
            "response_action_matches": json_pointer_str(&case, "/response/action") == Some(expected.action),
            "response_service_queue_key": json_pointer_str(&case, "/response/service_queue_key").is_some_and(is_hex32),
            "response_tx_skeleton_hash": json_pointer_str(&case, "/response/tx_skeleton_hash").is_some_and(is_hex32),
            "response_witness_shape_hash": json_pointer_str(&case, "/response/witness_shape_hash").is_some_and(is_hex32),
            "response_signed_intent_hash": json_pointer_str(&case, "/response/signed_intent_hash").is_some_and(is_hex32),
            "response_bip340_message_hash_matches": json_pointer_str(&case, "/response/bip340_message_hash")
                == json_pointer_str(&case, "/response/signed_intent_hash"),
            "response_receipt_binding_hash": json_pointer_str(&case, "/response/receipt_binding_hash").is_some_and(is_hex32),
            "response_builder_trace_hash": json_pointer_str(&case, "/response/builder_trace_hash").is_some_and(is_hex32),
            "tx_skeleton_schema": json_pointer_str(&case, "/tx_skeleton/schema") == Some("novaseal-service-builder-tx-skeleton-v0.1"),
            "tx_skeleton_operator_hash_matches": json_pointer_str(&case, "/tx_skeleton/operator_fixture_hash")
                == json_pointer_str(&case, "/operator_fixture_hash"),
            "tx_skeleton_public_btc_anchor_empty_when_not_required": public_btc_anchor_required
                || tx_skeleton_public_btc_anchor.is_none_or(Value::is_null),
            "tx_skeleton_public_btc_anchor_present_when_required": !public_btc_anchor_required
                || tx_skeleton_public_btc_anchor.is_some_and(|anchor| anchor.is_object()),
            "tx_skeleton_public_btc_anchor_shape_matches_profile": !public_btc_anchor_required
                || public_btc_anchor_shape_matches_profile(expected.profile, tx_skeleton_public_btc_anchor),
            "tx_skeleton_public_btc_anchor_matches_operator_fixture": !public_btc_anchor_required
                || tx_skeleton_public_btc_anchor == operator_public_btc_anchor,
            "fixture_checks_passed": object_values_all_true(case.get("checks")),
        });
        case_checks.insert(format!("{}:{}", expected.profile, expected.action), checks);
    }

    let checks = json!({
        "report_passed": json_pointer_str(report, "/status") == Some("passed"),
        "schema_current": json_pointer_str(report, "/schema") == Some("novaseal-service-builder-fixtures-v0.1"),
        "builder_name": json_pointer_str(report, "/builder_name") == Some("novaseal-profile-service-builder-v0"),
        "source_operator_fixture_report_hash": json_pointer_str(report, "/source_operator_fixture_report_hash").is_some_and(is_hex32),
        "source_operator_fixture_report_hash_matches_current_report": json_pointer_str(report, "/source_operator_fixture_report_hash")
            == Some(expected_operator_report_hash.as_str()),
        "summary_counts_match": json_pointer_i64(report, "/summary/total") == Some(EXPECTED_PROFILE_OPERATOR_FIXTURES.len() as i64)
            && json_pointer_i64(report, "/summary/matched") == json_pointer_i64(report, "/summary/total")
            && json_pointer_i64(report, "/summary/profile_count") == Some(expected_profiles.len() as i64),
        "exact_profiles": actual_profiles == expected_profiles,
        "exact_profile_actions": actual_actions == expected_actions,
        "case_details": case_checks.values().all(|row| object_values_all_true(Some(row))),
    });

    json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "cases": case_checks,
        "expected_profiles": expected_profiles.into_iter().collect::<Vec<_>>(),
        "expected_actions": expected_actions.into_iter().collect::<Vec<_>>(),
        "case_count": cases.len(),
    })
}
