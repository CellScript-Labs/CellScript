use super::evidence_adapters::{
    validate_btc_spv_evidence_adapter_detail_with_sources, validate_external_attestation_adapter_detail_with_sources,
    validate_external_evidence_handoff_detail,
};
use super::external_evidence::{validate_attestation_templates, validate_security_audit_coverage};
use super::package_sources::expected_files;
use super::profile_packages::{
    validate_btc_tx_commitment_profile_package, validate_btc_utxo_seal_profile_package, validate_dual_seal_profile_package,
    validate_fiber_candidate_profile_package, validate_fungible_xudt_profile_package, validate_invariant_matrix,
    validate_rwa_receipt_profile_package,
};
use super::report_files::json_load_path;
use super::wallet_and_operator::{
    validate_profile_operator_fixture_detail, validate_service_builder_fixture_detail, validate_wallet_lock_alignment_detail,
    validate_wallet_vector_detail,
};
use super::{
    agreement_live_evidence, json_pointer_bool, json_pointer_str, normalize_hex, object_values_all_true, AGREEMENT_ROOT,
    EXPECTED_AGREEMENT_FIXTURES, EXPECTED_AGREEMENT_PROFILE, EXPECTED_AGREEMENT_SCHEMA_FILES, EXPECTED_NOVASEAL_CANONICAL_SCHEMA,
    EXPECTED_PROFILE_CERTIFICATION_GATE, EXTERNAL_TCB_ATTESTATION_TEMPLATE, FIBER_NODE_EXPERIMENTS, PUBLIC_BTC_SPV_EVIDENCE_TEMPLATE,
    PUBLIC_CELLDEP_ATTESTATION_TEMPLATE,
};
use crate::error::Result;
use serde_json::{json, Value};
use std::path::Path;

pub(super) struct ProfileCertificationInputs<'a> {
    pub(super) repo_root: &'a Path,
    pub(super) agreement_conformance: &'a Value,
    pub(super) agreement_manifest: &'a Value,
    pub(super) core_security: &'a Value,
    pub(super) wallet: &'a Value,
    pub(super) wallet_alignment: &'a Value,
    pub(super) profile_operator_fixtures: &'a Value,
    pub(super) service_builder_fixtures: &'a Value,
    pub(super) btc_spv_evidence_adapter: &'a Value,
    pub(super) external_attestation_adapter: &'a Value,
    pub(super) external_evidence_handoff: &'a Value,
    pub(super) stateful_acceptance: &'a Value,
    pub(super) tcb: &'a Value,
    pub(super) public_attestation: &'a Value,
    pub(super) external_review: &'a Value,
    pub(super) btc_spv_evidence: &'a Value,
    pub(super) rwa_legal_registry_review: &'a Value,
}

pub(super) fn validate_profile_certification(input: ProfileCertificationInputs<'_>) -> Result<Value> {
    let ProfileCertificationInputs {
        repo_root,
        agreement_conformance,
        agreement_manifest,
        core_security,
        wallet,
        wallet_alignment,
        profile_operator_fixtures,
        service_builder_fixtures,
        btc_spv_evidence_adapter,
        external_attestation_adapter,
        external_evidence_handoff,
        stateful_acceptance,
        tcb,
        public_attestation,
        external_review,
        btc_spv_evidence,
        rwa_legal_registry_review,
    } = input;
    let schema_files = expected_files(repo_root, &repo_root.join(AGREEMENT_ROOT).join("schemas"), EXPECTED_AGREEMENT_SCHEMA_FILES)?;
    let fixture_files = expected_files(repo_root, &repo_root.join(AGREEMENT_ROOT).join("fixtures"), EXPECTED_AGREEMENT_FIXTURES)?;
    let wallet_detail = validate_wallet_vector_detail(wallet);
    let wallet_alignment_detail = validate_wallet_lock_alignment_detail(wallet_alignment);
    let profile_operator_fixture_detail = validate_profile_operator_fixture_detail(repo_root, profile_operator_fixtures)?;
    let service_builder_fixture_detail = validate_service_builder_fixture_detail(service_builder_fixtures, profile_operator_fixtures);
    let public_btc_spv_template = json_load_path(repo_root, &repo_root.join(PUBLIC_BTC_SPV_EVIDENCE_TEMPLATE))?;
    let public_cell_dep_template = json_load_path(repo_root, &repo_root.join(PUBLIC_CELLDEP_ATTESTATION_TEMPLATE))?;
    let external_tcb_template = json_load_path(repo_root, &repo_root.join(EXTERNAL_TCB_ATTESTATION_TEMPLATE))?;
    let btc_spv_adapter_detail = validate_btc_spv_evidence_adapter_detail_with_sources(
        btc_spv_evidence_adapter,
        Some(service_builder_fixtures),
        Some(&public_btc_spv_template),
    );
    let external_attestation_adapter_detail = validate_external_attestation_adapter_detail_with_sources(
        external_attestation_adapter,
        Some(tcb),
        Some(&public_cell_dep_template),
        Some(&external_tcb_template),
    );
    let external_evidence_handoff_detail = validate_external_evidence_handoff_detail(
        repo_root,
        external_evidence_handoff,
        btc_spv_evidence_adapter,
        external_attestation_adapter,
    );
    let invariant_matrix = validate_invariant_matrix(repo_root, &repo_root.join(AGREEMENT_ROOT).join("proofs/invariant_matrix.json"))?;
    let fungible_xudt_profile = validate_fungible_xudt_profile_package(repo_root)?;
    let rwa_receipt_profile = validate_rwa_receipt_profile_package(repo_root)?;
    let btc_tx_commitment_profile = validate_btc_tx_commitment_profile_package(repo_root)?;
    let btc_utxo_seal_profile = validate_btc_utxo_seal_profile_package(repo_root)?;
    let dual_seal_profile = validate_dual_seal_profile_package(repo_root)?;
    let fiber_candidate_profile = validate_fiber_candidate_profile_package(repo_root)?;
    let live_evidence = agreement_live_evidence(stateful_acceptance);
    let fiber_node_experiments = stateful_acceptance
        .get("external_experiment_coverage")
        .and_then(|coverage| coverage.get("fiber_node_execution"))
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "present": false,
                "discovery_ready": false,
                "all_required_workflows_executed_passed": false,
                "required_report": FIBER_NODE_EXPERIMENTS,
            })
        });
    let artifact_hash = normalize_hex(json_pointer_str(tcb, "/runtime_artifact/artifact_hash"));
    let artifact_hash_algorithm = json_pointer_str(tcb, "/runtime_artifact/artifact_hash_algorithm");
    let source_tree_hash = normalize_hex(json_pointer_str(tcb, "/source_inventory/source_tree_sha256"));
    let attestation_templates =
        validate_attestation_templates(repo_root, artifact_hash.as_deref(), artifact_hash_algorithm, source_tree_hash.as_deref())?;
    let security_audit_coverage =
        validate_security_audit_coverage(repo_root, core_security, &invariant_matrix, &live_evidence, tcb, &attestation_templates)?;
    let docs = json!({
        "agreement_profile": repo_root.join(AGREEMENT_ROOT).join("docs/AGREEMENT_PROFILE.md").is_file(),
        "security": repo_root.join(AGREEMENT_ROOT).join("docs/SECURITY.md").is_file(),
        "audit_status": repo_root.join(AGREEMENT_ROOT).join("docs/AUDIT_STATUS.md").is_file(),
        "devnet_acceptance": repo_root.join(AGREEMENT_ROOT).join("docs/DEVNET_STATEFUL_ACCEPTANCE.md").is_file(),
    });
    let external_checks = json!({
        "public_shared_cell_dep_attested": json_pointer_str(public_attestation, "/status") == Some("passed"),
        "external_bip340_tcb_review_attested": json_pointer_str(external_review, "/status") == Some("passed"),
        "public_btc_spv_evidence_attested": json_pointer_str(btc_spv_evidence, "/status") == Some("passed"),
        "rwa_legal_registry_review_attested": json_pointer_str(rwa_legal_registry_review, "/status") == Some("passed"),
    });
    let local_checks = json!({
        "conformance_gate_passed": json_pointer_str(agreement_conformance, "/status") == Some("passed"),
        "profile_schema_set_exact": json_pointer_bool(&schema_files, "/exact"),
        "profile_fixture_set_exact": json_pointer_bool(&fixture_files, "/exact"),
        "wallet_vector_detail_passed": json_pointer_str(&wallet_detail, "/status") == Some("passed"),
        "wallet_lock_alignment_passed": json_pointer_str(&wallet_alignment_detail, "/status") == Some("passed"),
        "profile_operator_fixture_detail_passed": json_pointer_str(&profile_operator_fixture_detail, "/status") == Some("passed"),
        "service_builder_fixture_detail_passed": json_pointer_str(&service_builder_fixture_detail, "/status") == Some("passed"),
        "btc_spv_evidence_adapter_passed": json_pointer_str(&btc_spv_adapter_detail, "/status") == Some("passed"),
        "external_attestation_adapter_passed": json_pointer_str(&external_attestation_adapter_detail, "/status") == Some("passed"),
        "external_evidence_handoff_passed": json_pointer_str(&external_evidence_handoff_detail, "/status") == Some("passed"),
        "invariant_matrix_passed": json_pointer_str(&invariant_matrix, "/status") == Some("passed"),
        "live_devnet_evidence_passed": json_pointer_str(&live_evidence, "/status") == Some("passed"),
        "agreement_runtime_verifier_pin_passed": object_values_all_true(agreement_manifest.get("checks")),
        "local_bip340_tcb_review_passed": json_pointer_str(tcb, "/status").is_some_and(|status| status.starts_with("passed_local_review")),
        "external_attestation_templates_current": json_pointer_str(&attestation_templates, "/status") == Some("passed"),
        "security_audit_coverage_passed": json_pointer_str(&security_audit_coverage, "/status") == Some("passed"),
        "fungible_xudt_profile_package_passed": json_pointer_str(&fungible_xudt_profile, "/status") == Some("passed"),
        "rwa_receipt_profile_package_passed": json_pointer_str(&rwa_receipt_profile, "/status") == Some("passed"),
        "btc_tx_commitment_profile_package_passed": json_pointer_str(&btc_tx_commitment_profile, "/status") == Some("passed"),
        "btc_utxo_seal_profile_package_passed": json_pointer_str(&btc_utxo_seal_profile, "/status") == Some("passed"),
        "dual_seal_profile_package_passed": json_pointer_str(&dual_seal_profile, "/status") == Some("passed"),
        "fiber_candidate_profile_package_passed": json_pointer_str(&fiber_candidate_profile, "/status") == Some("passed"),
        "external_fiber_node_experiments_passed": json_pointer_bool(&fiber_node_experiments, "/all_required_workflows_executed_passed"),
        "required_docs_present": object_values_all_true(Some(&docs)),
    });
    let local_passed = object_values_all_true(Some(&local_checks));
    let production_statement_eligible = local_passed && object_values_all_true(Some(&external_checks));
    let production_statement_blockers = external_checks
        .as_object()
        .into_iter()
        .flat_map(|object| object.iter())
        .filter(|(_, passed)| passed.as_bool() != Some(true))
        .map(|(name, _)| Value::String(name.clone()))
        .collect::<Vec<_>>();

    Ok(json!({
        "schema": "novaseal-profile-certification-v0.1",
        "profile": EXPECTED_AGREEMENT_PROFILE,
        "conforms_to": EXPECTED_NOVASEAL_CANONICAL_SCHEMA,
        "gate": EXPECTED_PROFILE_CERTIFICATION_GATE,
        "status": if local_passed { "passed" } else { "failed" },
        "certification_level": if local_passed {
            "public_ecosystem_profile_certification_local_ready"
        } else {
            "public_ecosystem_profile_certification_failed"
        },
        "production_statement_eligible": production_statement_eligible,
        "production_statement_blockers": production_statement_blockers,
        "local_checks": local_checks,
        "external_checks": external_checks,
        "public_btc_spv_evidence": btc_spv_evidence,
        "rwa_legal_registry_review": rwa_legal_registry_review,
        "schema_files": schema_files,
        "fixture_files": fixture_files,
        "wallet_vectors": wallet_detail,
        "wallet_lock_alignment": wallet_alignment_detail,
        "profile_operator_fixtures": profile_operator_fixture_detail,
        "service_builder_fixtures": service_builder_fixture_detail,
        "btc_spv_evidence_adapter": btc_spv_adapter_detail,
        "external_attestation_adapter": external_attestation_adapter_detail,
        "external_evidence_handoff": external_evidence_handoff_detail,
        "invariant_matrix": invariant_matrix,
        "planned_profile_packages": {
            "btc_tx_commitment": btc_tx_commitment_profile,
            "btc_utxo_seal": btc_utxo_seal_profile,
            "dual_seal": dual_seal_profile,
            "fiber_candidate": fiber_candidate_profile,
            "fungible_xudt": fungible_xudt_profile,
            "rwa_receipt": rwa_receipt_profile,
        },
        "live_devnet": live_evidence,
        "external_fiber_node_experiments": fiber_node_experiments,
        "attestation_templates": attestation_templates,
        "security_audit_coverage": security_audit_coverage,
        "docs": docs,
        "design_boundary": {
            "agreement_calls_core_runtime": false,
            "canonical_constraint": "manifest canonical_schema_hash + signed canonical_envelope_hash + runtime recomputation",
            "rgb_code_vendored": false,
            "rgbplusplus_schema_dependency": false,
            "new_runtime_machinery_added": false,
        },
    }))
}
