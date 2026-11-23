use super::package_sources::{canonical_schema_hash, expected_files, manifest_metadata, read_cell_sources, toml_str};
use super::report_files::json_load_path;
use super::{
    find_actions, json_pointer_bool, json_pointer_str, object_values_all_true, rel, value_is_present, BTC_TX_COMMITMENT_MANIFEST,
    BTC_TX_COMMITMENT_ROOT, BTC_UTXO_SEAL_MANIFEST, BTC_UTXO_SEAL_ROOT, CANONICAL_SCHEMA, DUAL_SEAL_MANIFEST, DUAL_SEAL_ROOT,
    EXPECTED_BTC_TX_COMMITMENT_DOCS, EXPECTED_BTC_TX_COMMITMENT_FIXTURES, EXPECTED_BTC_TX_COMMITMENT_INVARIANTS,
    EXPECTED_BTC_TX_COMMITMENT_PROFILE, EXPECTED_BTC_TX_COMMITMENT_SCHEMA_FILES, EXPECTED_BTC_UTXO_SEAL_DOCS,
    EXPECTED_BTC_UTXO_SEAL_FIXTURES, EXPECTED_BTC_UTXO_SEAL_INVARIANTS, EXPECTED_BTC_UTXO_SEAL_PROFILE,
    EXPECTED_BTC_UTXO_SEAL_SCHEMA_FILES, EXPECTED_CERTIFICATION_INVARIANTS, EXPECTED_CERTIFICATION_PLUGIN, EXPECTED_DUAL_SEAL_DOCS,
    EXPECTED_DUAL_SEAL_FIXTURES, EXPECTED_DUAL_SEAL_INVARIANTS, EXPECTED_DUAL_SEAL_PROFILE, EXPECTED_DUAL_SEAL_SCHEMA_FILES,
    EXPECTED_FIBER_CANDIDATE_DOCS, EXPECTED_FIBER_CANDIDATE_FIXTURES, EXPECTED_FIBER_CANDIDATE_INVARIANTS,
    EXPECTED_FIBER_CANDIDATE_PROFILE, EXPECTED_FIBER_CANDIDATE_SCHEMA_FILES, EXPECTED_FUNGIBLE_XUDT_DOCS,
    EXPECTED_FUNGIBLE_XUDT_FIXTURES, EXPECTED_FUNGIBLE_XUDT_INVARIANTS, EXPECTED_FUNGIBLE_XUDT_PROFILE,
    EXPECTED_FUNGIBLE_XUDT_SCHEMA_FILES, EXPECTED_NOVASEAL_CANONICAL_SCHEMA, EXPECTED_PROFILE_CERTIFICATION_GATE,
    EXPECTED_RWA_RECEIPT_DOCS, EXPECTED_RWA_RECEIPT_FIXTURES, EXPECTED_RWA_RECEIPT_INVARIANTS, EXPECTED_RWA_RECEIPT_PROFILE,
    EXPECTED_RWA_RECEIPT_SCHEMA_FILES, FIBER_CANDIDATE_MANIFEST, FIBER_CANDIDATE_ROOT, FIBER_NODE_EXPERIMENTS, FUNGIBLE_XUDT_MANIFEST,
    FUNGIBLE_XUDT_ROOT, PROFILE_OPERATOR_FIXTURES, REQUIRED_BTC_TX_COMMITMENT_SOURCE_PATTERNS, REQUIRED_BTC_UTXO_SEAL_SOURCE_PATTERNS,
    REQUIRED_DUAL_SEAL_SOURCE_PATTERNS, REQUIRED_FIBER_CANDIDATE_SOURCE_PATTERNS, REQUIRED_FUNGIBLE_XUDT_SOURCE_PATTERNS,
    REQUIRED_RWA_RECEIPT_SOURCE_PATTERNS, RWA_RECEIPT_MANIFEST, RWA_RECEIPT_ROOT,
};
use crate::error::Result;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::path::Path;

pub(super) fn validate_invariant_matrix(repo_root: &Path, path: &Path) -> Result<Value> {
    let payload = json_load_path(repo_root, path)?;
    let invariants = payload.get("invariants").and_then(Value::as_array).cloned().unwrap_or_default();
    let ids = invariants.iter().filter_map(|row| json_pointer_str(row, "/id").map(str::to_string)).collect::<BTreeSet<_>>();
    let required = EXPECTED_CERTIFICATION_INVARIANTS.iter().map(|value| (*value).to_string()).collect::<BTreeSet<_>>();
    let coverage_by_id = invariants
        .iter()
        .filter_map(|row| Some((json_pointer_str(row, "/id")?.to_string(), row.get("coverage").cloned().unwrap_or(Value::Null))))
        .collect::<Map<_, _>>();
    let checks = json!({
        "file_present": payload.get("missing").is_none(),
        "schema": json_pointer_str(&payload, "/schema") == Some("novaseal-agreement-invariant-matrix-v0.1"),
        "required_invariants_present": required.is_subset(&ids),
        "no_empty_coverage": ids.iter().all(|id| coverage_by_id.get(id).is_some_and(value_is_present)),
    });
    let missing = required.difference(&ids).cloned().collect::<Vec<_>>();
    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "required": required.into_iter().collect::<Vec<_>>(),
        "present": ids.into_iter().collect::<Vec<_>>(),
        "missing": missing,
        "coverage_by_id": coverage_by_id,
    }))
}

pub(super) fn validate_fungible_xudt_profile_package(repo_root: &Path) -> Result<Value> {
    let root = repo_root.join(FUNGIBLE_XUDT_ROOT);
    let manifest_path = repo_root.join(FUNGIBLE_XUDT_MANIFEST);
    let manifest = if manifest_path.is_file() { Some(manifest_metadata(&manifest_path)?) } else { None };
    let metadata_str = |key: &str| manifest.as_ref().and_then(|metadata| toml_str(metadata, key));
    let source = if root.join("src").is_dir() { read_cell_sources(&root.join("src"))? } else { String::new() };
    let schema_path = repo_root.join(CANONICAL_SCHEMA);
    let schema_hash = canonical_schema_hash(&schema_path)?;
    let source_checks = REQUIRED_FUNGIBLE_XUDT_SOURCE_PATTERNS
        .iter()
        .map(|(name, pattern)| (format!("source_{name}"), Value::Bool(source.contains(pattern))))
        .collect::<Map<_, _>>();
    let actions = find_actions(&source);
    let action_names = actions.iter().map(|action| action.name.clone()).collect::<BTreeSet<_>>();
    let expected_actions = ["issue_xudt", "transfer_xudt", "settle_xudt", "nova_fungible_xudt_lifecycle"]
        .iter()
        .map(|action| (*action).to_string())
        .collect::<BTreeSet<_>>();
    let schemas = expected_files(repo_root, &root.join("schemas"), EXPECTED_FUNGIBLE_XUDT_SCHEMA_FILES)?;
    let fixtures = expected_files(repo_root, &root.join("fixtures"), EXPECTED_FUNGIBLE_XUDT_FIXTURES)?;
    let docs = expected_files(repo_root, &root.join("docs"), EXPECTED_FUNGIBLE_XUDT_DOCS)?;
    let invariant_path = root.join("proofs/invariant_matrix.json");
    let invariant_payload = if invariant_path.is_file() { json_load_path(repo_root, &invariant_path)? } else { Value::Null };
    let invariants = invariant_payload.get("invariants").and_then(Value::as_array).cloned().unwrap_or_default();
    let invariant_ids = invariants.iter().filter_map(|row| json_pointer_str(row, "/id").map(str::to_string)).collect::<BTreeSet<_>>();
    let required_invariants = EXPECTED_FUNGIBLE_XUDT_INVARIANTS.iter().map(|value| (*value).to_string()).collect::<BTreeSet<_>>();
    let coverage_by_id = invariants
        .iter()
        .filter_map(|row| Some((json_pointer_str(row, "/id")?.to_string(), row.get("coverage").cloned().unwrap_or(Value::Null))))
        .collect::<Map<_, _>>();
    let mut checks = source_checks;
    checks.extend([
        ("root_present".to_string(), Value::Bool(root.is_dir())),
        ("manifest_present".to_string(), Value::Bool(manifest_path.is_file())),
        (
            "manifest_protocol_family".to_string(),
            Value::Bool(metadata_str("protocol_family") == Some("NovaSeal")),
        ),
        ("manifest_profile".to_string(), Value::Bool(metadata_str("profile") == Some(EXPECTED_FUNGIBLE_XUDT_PROFILE))),
        (
            "manifest_conforms_to".to_string(),
            Value::Bool(metadata_str("conforms_to") == Some(EXPECTED_NOVASEAL_CANONICAL_SCHEMA)),
        ),
        (
            "manifest_canonical_schema_hash".to_string(),
            Value::Bool(metadata_str("canonical_schema_hash") == schema_hash.as_deref()),
        ),
        (
            "manifest_conformance_gate".to_string(),
            Value::Bool(metadata_str("conformance_gate") == Some(EXPECTED_PROFILE_CERTIFICATION_GATE)),
        ),
        (
            "manifest_certification_plugin".to_string(),
            Value::Bool(metadata_str("certification_plugin") == Some(EXPECTED_CERTIFICATION_PLUGIN)),
        ),
        (
            "manifest_stateful_dispatcher".to_string(),
            Value::Bool(
                metadata_str("stateful_dispatcher")
                    == Some("src/nova_fungible_xudt_lifecycle_type.cell:nova_fungible_xudt_lifecycle"),
            ),
        ),
        (
            "manifest_source_actions".to_string(),
            Value::Bool(
                metadata_str("source_actions")
                    == Some(
                        "src/nova_fungible_xudt_type.cell:issue_xudt;src/nova_fungible_xudt_type.cell:transfer_xudt;src/nova_fungible_xudt_type.cell:settle_xudt;src/nova_fungible_xudt_lifecycle_type.cell:nova_fungible_xudt_lifecycle",
                    ),
            ),
        ),
        ("expected_actions_present".to_string(), Value::Bool(expected_actions.is_subset(&action_names))),
        ("schemas_exact".to_string(), Value::Bool(json_pointer_bool(&schemas, "/exact"))),
        ("fixtures_exact".to_string(), Value::Bool(json_pointer_bool(&fixtures, "/exact"))),
        ("docs_exact".to_string(), Value::Bool(json_pointer_bool(&docs, "/exact"))),
        (
            "invariant_schema".to_string(),
            Value::Bool(json_pointer_str(&invariant_payload, "/schema") == Some("novaseal-fungible-xudt-invariant-matrix-v0.1")),
        ),
        (
            "required_invariants_present".to_string(),
            Value::Bool(required_invariants.is_subset(&invariant_ids)),
        ),
        (
            "no_empty_invariant_coverage".to_string(),
            Value::Bool(invariant_ids.iter().all(|id| coverage_by_id.get(id).is_some_and(value_is_present))),
        ),
        (
            "live_devnet_gap_explicit".to_string(),
            Value::Bool(
                coverage_by_id.get("live_devnet_lifecycle").and_then(Value::as_str)
                    == Some("target/novaseal-fungible-xudt-devnet-stateful-live.json"),
            ),
        ),
    ]);
    let missing_invariants = required_invariants.difference(&invariant_ids).cloned().collect::<Vec<_>>();
    Ok(json!({
        "schema": "novaseal-fungible-xudt-profile-package-validation-v0.1",
        "status": if object_values_all_true(Some(&Value::Object(checks.clone()))) { "passed" } else { "failed" },
        "classification": "profile-package-with-compiled-lifecycle-dispatcher-not-live-stateful-acceptance",
        "root": rel(repo_root, &root),
        "manifest": rel(repo_root, &manifest_path),
        "canonical_schema_hash": schema_hash,
        "actions": action_names.into_iter().collect::<Vec<_>>(),
        "schemas": schemas,
        "fixtures": fixtures,
        "docs": docs,
        "invariant_matrix": {
            "path": rel(repo_root, &invariant_path),
            "required": required_invariants.into_iter().collect::<Vec<_>>(),
            "present": invariant_ids.into_iter().collect::<Vec<_>>(),
            "missing": missing_invariants,
            "coverage_by_id": coverage_by_id,
        },
        "checks": checks,
        "operator_fixture_evidence": PROFILE_OPERATOR_FIXTURES,
    }))
}

pub(super) fn validate_rwa_receipt_profile_package(repo_root: &Path) -> Result<Value> {
    let root = repo_root.join(RWA_RECEIPT_ROOT);
    let manifest_path = repo_root.join(RWA_RECEIPT_MANIFEST);
    let manifest = if manifest_path.is_file() { Some(manifest_metadata(&manifest_path)?) } else { None };
    let metadata_str = |key: &str| manifest.as_ref().and_then(|metadata| toml_str(metadata, key));
    let source = if root.join("src").is_dir() { read_cell_sources(&root.join("src"))? } else { String::new() };
    let schema_path = repo_root.join(CANONICAL_SCHEMA);
    let schema_hash = canonical_schema_hash(&schema_path)?;
    let source_checks = REQUIRED_RWA_RECEIPT_SOURCE_PATTERNS
        .iter()
        .map(|(name, pattern)| (format!("source_{name}"), Value::Bool(source.contains(pattern))))
        .collect::<Map<_, _>>();
    let actions = find_actions(&source);
    let action_names = actions.iter().map(|action| action.name.clone()).collect::<BTreeSet<_>>();
    let expected_actions = ["materialize_rwa_receipt", "claim_rwa_receipt", "settle_rwa_receipt", "nova_rwa_receipt_lifecycle"]
        .iter()
        .map(|action| (*action).to_string())
        .collect::<BTreeSet<_>>();
    let schemas = expected_files(repo_root, &root.join("schemas"), EXPECTED_RWA_RECEIPT_SCHEMA_FILES)?;
    let fixtures = expected_files(repo_root, &root.join("fixtures"), EXPECTED_RWA_RECEIPT_FIXTURES)?;
    let docs = expected_files(repo_root, &root.join("docs"), EXPECTED_RWA_RECEIPT_DOCS)?;
    let invariant_path = root.join("proofs/invariant_matrix.json");
    let invariant_payload = if invariant_path.is_file() { json_load_path(repo_root, &invariant_path)? } else { Value::Null };
    let invariants = invariant_payload.get("invariants").and_then(Value::as_array).cloned().unwrap_or_default();
    let invariant_ids = invariants.iter().filter_map(|row| json_pointer_str(row, "/id").map(str::to_string)).collect::<BTreeSet<_>>();
    let required_invariants = EXPECTED_RWA_RECEIPT_INVARIANTS.iter().map(|value| (*value).to_string()).collect::<BTreeSet<_>>();
    let coverage_by_id = invariants
        .iter()
        .filter_map(|row| Some((json_pointer_str(row, "/id")?.to_string(), row.get("coverage").cloned().unwrap_or(Value::Null))))
        .collect::<Map<_, _>>();
    let mut checks = source_checks;
    checks.extend([
        ("root_present".to_string(), Value::Bool(root.is_dir())),
        ("manifest_present".to_string(), Value::Bool(manifest_path.is_file())),
        (
            "manifest_protocol_family".to_string(),
            Value::Bool(metadata_str("protocol_family") == Some("NovaSeal")),
        ),
        ("manifest_profile".to_string(), Value::Bool(metadata_str("profile") == Some(EXPECTED_RWA_RECEIPT_PROFILE))),
        (
            "manifest_conforms_to".to_string(),
            Value::Bool(metadata_str("conforms_to") == Some(EXPECTED_NOVASEAL_CANONICAL_SCHEMA)),
        ),
        (
            "manifest_canonical_schema_hash".to_string(),
            Value::Bool(metadata_str("canonical_schema_hash") == schema_hash.as_deref()),
        ),
        (
            "manifest_conformance_gate".to_string(),
            Value::Bool(metadata_str("conformance_gate") == Some(EXPECTED_PROFILE_CERTIFICATION_GATE)),
        ),
        (
            "manifest_certification_plugin".to_string(),
            Value::Bool(metadata_str("certification_plugin") == Some(EXPECTED_CERTIFICATION_PLUGIN)),
        ),
        (
            "manifest_stateful_dispatcher".to_string(),
            Value::Bool(
                metadata_str("stateful_dispatcher") == Some("src/nova_rwa_receipt_lifecycle_type.cell:nova_rwa_receipt_lifecycle"),
            ),
        ),
        (
            "manifest_source_actions".to_string(),
            Value::Bool(
                metadata_str("source_actions")
                    == Some(
                        "src/nova_rwa_receipt_type.cell:materialize_rwa_receipt;src/nova_rwa_receipt_type.cell:claim_rwa_receipt;src/nova_rwa_receipt_type.cell:settle_rwa_receipt;src/nova_rwa_receipt_lifecycle_type.cell:nova_rwa_receipt_lifecycle",
                    ),
            ),
        ),
        ("expected_actions_present".to_string(), Value::Bool(expected_actions.is_subset(&action_names))),
        ("schemas_exact".to_string(), Value::Bool(json_pointer_bool(&schemas, "/exact"))),
        ("fixtures_exact".to_string(), Value::Bool(json_pointer_bool(&fixtures, "/exact"))),
        ("docs_exact".to_string(), Value::Bool(json_pointer_bool(&docs, "/exact"))),
        (
            "invariant_schema".to_string(),
            Value::Bool(json_pointer_str(&invariant_payload, "/schema") == Some("novaseal-rwa-receipt-invariant-matrix-v0.1")),
        ),
        (
            "required_invariants_present".to_string(),
            Value::Bool(required_invariants.is_subset(&invariant_ids)),
        ),
        (
            "no_empty_invariant_coverage".to_string(),
            Value::Bool(invariant_ids.iter().all(|id| coverage_by_id.get(id).is_some_and(value_is_present))),
        ),
        (
            "live_devnet_gap_explicit".to_string(),
            Value::Bool(
                coverage_by_id.get("live_devnet_lifecycle").and_then(Value::as_str)
                    == Some("target/novaseal-rwa-receipt-devnet-stateful-live.json"),
            ),
        ),
    ]);
    let missing_invariants = required_invariants.difference(&invariant_ids).cloned().collect::<Vec<_>>();
    Ok(json!({
        "schema": "novaseal-rwa-receipt-profile-package-validation-v0.1",
        "status": if object_values_all_true(Some(&Value::Object(checks.clone()))) { "passed" } else { "failed" },
        "classification": "profile-package-with-compiled-lifecycle-dispatcher-not-live-stateful-acceptance",
        "root": rel(repo_root, &root),
        "manifest": rel(repo_root, &manifest_path),
        "canonical_schema_hash": schema_hash,
        "actions": action_names.into_iter().collect::<Vec<_>>(),
        "schemas": schemas,
        "fixtures": fixtures,
        "docs": docs,
        "invariant_matrix": {
            "path": rel(repo_root, &invariant_path),
            "required": required_invariants.into_iter().collect::<Vec<_>>(),
            "present": invariant_ids.into_iter().collect::<Vec<_>>(),
            "missing": missing_invariants,
            "coverage_by_id": coverage_by_id,
        },
        "checks": checks,
        "operator_fixture_evidence": PROFILE_OPERATOR_FIXTURES,
        "remaining_acceptance_gap": "legal/registry review evidence is still required before rwa_receipt_lifecycle can make production RWA title or registry claims",
    }))
}

pub(super) fn validate_btc_tx_commitment_profile_package(repo_root: &Path) -> Result<Value> {
    let root = repo_root.join(BTC_TX_COMMITMENT_ROOT);
    let manifest_path = repo_root.join(BTC_TX_COMMITMENT_MANIFEST);
    let manifest = if manifest_path.is_file() { Some(manifest_metadata(&manifest_path)?) } else { None };
    let metadata_str = |key: &str| manifest.as_ref().and_then(|metadata| toml_str(metadata, key));
    let source = if root.join("src").is_dir() { read_cell_sources(&root.join("src"))? } else { String::new() };
    let schema_path = repo_root.join(CANONICAL_SCHEMA);
    let schema_hash = canonical_schema_hash(&schema_path)?;
    let source_checks = REQUIRED_BTC_TX_COMMITMENT_SOURCE_PATTERNS
        .iter()
        .map(|(name, pattern)| (format!("source_{name}"), Value::Bool(source.contains(pattern))))
        .collect::<Map<_, _>>();
    let actions = find_actions(&source);
    let action_names = actions.iter().map(|action| action.name.clone()).collect::<BTreeSet<_>>();
    let expected_actions = ["commit_btc_transaction_transition", "nova_btc_transaction_commitment_lifecycle"]
        .iter()
        .map(|action| (*action).to_string())
        .collect::<BTreeSet<_>>();
    let schemas = expected_files(repo_root, &root.join("schemas"), EXPECTED_BTC_TX_COMMITMENT_SCHEMA_FILES)?;
    let fixtures = expected_files(repo_root, &root.join("fixtures"), EXPECTED_BTC_TX_COMMITMENT_FIXTURES)?;
    let docs = expected_files(repo_root, &root.join("docs"), EXPECTED_BTC_TX_COMMITMENT_DOCS)?;
    let invariant_path = root.join("proofs/invariant_matrix.json");
    let invariant_payload = if invariant_path.is_file() { json_load_path(repo_root, &invariant_path)? } else { Value::Null };
    let invariants = invariant_payload.get("invariants").and_then(Value::as_array).cloned().unwrap_or_default();
    let invariant_ids = invariants.iter().filter_map(|row| json_pointer_str(row, "/id").map(str::to_string)).collect::<BTreeSet<_>>();
    let required_invariants = EXPECTED_BTC_TX_COMMITMENT_INVARIANTS.iter().map(|value| (*value).to_string()).collect::<BTreeSet<_>>();
    let coverage_by_id = invariants
        .iter()
        .filter_map(|row| Some((json_pointer_str(row, "/id")?.to_string(), row.get("coverage").cloned().unwrap_or(Value::Null))))
        .collect::<Map<_, _>>();
    let mut checks = source_checks;
    checks.extend([
        ("root_present".to_string(), Value::Bool(root.is_dir())),
        ("manifest_present".to_string(), Value::Bool(manifest_path.is_file())),
        ("manifest_protocol_family".to_string(), Value::Bool(metadata_str("protocol_family") == Some("NovaSeal"))),
        ("manifest_profile".to_string(), Value::Bool(metadata_str("profile") == Some(EXPECTED_BTC_TX_COMMITMENT_PROFILE))),
        ("manifest_conforms_to".to_string(), Value::Bool(metadata_str("conforms_to") == Some(EXPECTED_NOVASEAL_CANONICAL_SCHEMA))),
        ("manifest_canonical_schema_hash".to_string(), Value::Bool(metadata_str("canonical_schema_hash") == schema_hash.as_deref())),
        (
            "manifest_conformance_gate".to_string(),
            Value::Bool(metadata_str("conformance_gate") == Some(EXPECTED_PROFILE_CERTIFICATION_GATE)),
        ),
        (
            "manifest_certification_plugin".to_string(),
            Value::Bool(metadata_str("certification_plugin") == Some(EXPECTED_CERTIFICATION_PLUGIN)),
        ),
        (
            "manifest_stateful_dispatcher".to_string(),
            Value::Bool(
                metadata_str("stateful_dispatcher")
                    == Some("src/nova_btc_transaction_commitment_type.cell:nova_btc_transaction_commitment_lifecycle"),
            ),
        ),
        (
            "manifest_btc_public_verification_gap".to_string(),
            Value::Bool(
                metadata_str("btc_public_verification")
                    == Some("live-devnet-tuple-bound; public/mainnet SPV-or-indexer attestation required for BTC-finality claims"),
            ),
        ),
        (
            "manifest_source_actions".to_string(),
            Value::Bool(
                metadata_str("source_actions")
                    == Some(
                        "src/nova_btc_transaction_commitment_type.cell:commit_btc_transaction_transition;src/nova_btc_transaction_commitment_type.cell:nova_btc_transaction_commitment_lifecycle",
                    ),
            ),
        ),
        ("expected_actions_present".to_string(), Value::Bool(expected_actions.is_subset(&action_names))),
        ("schemas_exact".to_string(), Value::Bool(json_pointer_bool(&schemas, "/exact"))),
        ("fixtures_exact".to_string(), Value::Bool(json_pointer_bool(&fixtures, "/exact"))),
        ("docs_exact".to_string(), Value::Bool(json_pointer_bool(&docs, "/exact"))),
        (
            "invariant_schema".to_string(),
            Value::Bool(
                json_pointer_str(&invariant_payload, "/schema") == Some("novaseal-btc-transaction-commitment-invariant-matrix-v0.1"),
            ),
        ),
        ("required_invariants_present".to_string(), Value::Bool(required_invariants.is_subset(&invariant_ids))),
        (
            "no_empty_invariant_coverage".to_string(),
            Value::Bool(invariant_ids.iter().all(|id| coverage_by_id.get(id).is_some_and(value_is_present))),
        ),
        (
            "live_devnet_gap_explicit".to_string(),
            Value::Bool(
                coverage_by_id.get("live_devnet_lifecycle").and_then(Value::as_str)
                    == Some("target/novaseal-btc-transaction-commitment-devnet-stateful-live.json"),
            ),
        ),
        (
            "btc_public_verification_gap_explicit".to_string(),
            Value::Bool(
                coverage_by_id.get("btc_public_verification").and_then(Value::as_str)
                    == Some("external-public-mainnet-spv-or-indexer-evidence-required"),
            ),
        ),
    ]);
    let missing_invariants = required_invariants.difference(&invariant_ids).cloned().collect::<Vec<_>>();
    Ok(json!({
        "schema": "novaseal-btc-transaction-commitment-profile-package-validation-v0.1",
        "status": if object_values_all_true(Some(&Value::Object(checks.clone()))) { "passed" } else { "failed" },
        "classification": "profile-package-evidence-not-btc-finality-or-live-stateful-acceptance",
        "root": rel(repo_root, &root),
        "manifest": rel(repo_root, &manifest_path),
        "canonical_schema_hash": schema_hash,
        "actions": action_names.into_iter().collect::<Vec<_>>(),
        "schemas": schemas,
        "fixtures": fixtures,
        "docs": docs,
        "invariant_matrix": {
            "path": rel(repo_root, &invariant_path),
            "required": required_invariants.into_iter().collect::<Vec<_>>(),
            "present": invariant_ids.into_iter().collect::<Vec<_>>(),
            "missing": missing_invariants,
            "coverage_by_id": coverage_by_id,
        },
        "checks": checks,
        "remaining_acceptance_gap": "public BTC SPV evidence is still required before btc_transaction_commitment_transition can make production BTC-finality claims",
    }))
}

pub(super) fn validate_btc_utxo_seal_profile_package(repo_root: &Path) -> Result<Value> {
    let root = repo_root.join(BTC_UTXO_SEAL_ROOT);
    let manifest_path = repo_root.join(BTC_UTXO_SEAL_MANIFEST);
    let manifest = if manifest_path.is_file() { Some(manifest_metadata(&manifest_path)?) } else { None };
    let metadata_str = |key: &str| manifest.as_ref().and_then(|metadata| toml_str(metadata, key));
    let source = if root.join("src").is_dir() { read_cell_sources(&root.join("src"))? } else { String::new() };
    let schema_path = repo_root.join(CANONICAL_SCHEMA);
    let schema_hash = canonical_schema_hash(&schema_path)?;
    let source_checks = REQUIRED_BTC_UTXO_SEAL_SOURCE_PATTERNS
        .iter()
        .map(|(name, pattern)| (format!("source_{name}"), Value::Bool(source.contains(pattern))))
        .collect::<Map<_, _>>();
    let actions = find_actions(&source);
    let action_names = actions.iter().map(|action| action.name.clone()).collect::<BTreeSet<_>>();
    let expected_actions =
        ["close_btc_utxo_seal", "nova_btc_utxo_seal_lifecycle"].iter().map(|action| (*action).to_string()).collect::<BTreeSet<_>>();
    let schemas = expected_files(repo_root, &root.join("schemas"), EXPECTED_BTC_UTXO_SEAL_SCHEMA_FILES)?;
    let fixtures = expected_files(repo_root, &root.join("fixtures"), EXPECTED_BTC_UTXO_SEAL_FIXTURES)?;
    let docs = expected_files(repo_root, &root.join("docs"), EXPECTED_BTC_UTXO_SEAL_DOCS)?;
    let invariant_path = root.join("proofs/invariant_matrix.json");
    let invariant_payload = if invariant_path.is_file() { json_load_path(repo_root, &invariant_path)? } else { Value::Null };
    let invariants = invariant_payload.get("invariants").and_then(Value::as_array).cloned().unwrap_or_default();
    let invariant_ids = invariants.iter().filter_map(|row| json_pointer_str(row, "/id").map(str::to_string)).collect::<BTreeSet<_>>();
    let required_invariants = EXPECTED_BTC_UTXO_SEAL_INVARIANTS.iter().map(|value| (*value).to_string()).collect::<BTreeSet<_>>();
    let coverage_by_id = invariants
        .iter()
        .filter_map(|row| Some((json_pointer_str(row, "/id")?.to_string(), row.get("coverage").cloned().unwrap_or(Value::Null))))
        .collect::<Map<_, _>>();
    let mut checks = source_checks;
    checks.extend([
        ("root_present".to_string(), Value::Bool(root.is_dir())),
        ("manifest_present".to_string(), Value::Bool(manifest_path.is_file())),
        ("manifest_protocol_family".to_string(), Value::Bool(metadata_str("protocol_family") == Some("NovaSeal"))),
        ("manifest_profile".to_string(), Value::Bool(metadata_str("profile") == Some(EXPECTED_BTC_UTXO_SEAL_PROFILE))),
        ("manifest_conforms_to".to_string(), Value::Bool(metadata_str("conforms_to") == Some(EXPECTED_NOVASEAL_CANONICAL_SCHEMA))),
        ("manifest_canonical_schema_hash".to_string(), Value::Bool(metadata_str("canonical_schema_hash") == schema_hash.as_deref())),
        (
            "manifest_conformance_gate".to_string(),
            Value::Bool(metadata_str("conformance_gate") == Some(EXPECTED_PROFILE_CERTIFICATION_GATE)),
        ),
        (
            "manifest_certification_plugin".to_string(),
            Value::Bool(metadata_str("certification_plugin") == Some(EXPECTED_CERTIFICATION_PLUGIN)),
        ),
        (
            "manifest_stateful_dispatcher".to_string(),
            Value::Bool(metadata_str("stateful_dispatcher") == Some("src/nova_btc_utxo_seal_type.cell:nova_btc_utxo_seal_lifecycle")),
        ),
        (
            "manifest_btc_public_verification_gap".to_string(),
            Value::Bool(
                metadata_str("btc_public_verification")
                    == Some("live-devnet-spend-tuple-bound; public/mainnet SPV-or-indexer attestation required for BTC-spend claims"),
            ),
        ),
        (
            "manifest_source_actions".to_string(),
            Value::Bool(
                metadata_str("source_actions")
                    == Some("src/nova_btc_utxo_seal_type.cell:close_btc_utxo_seal;src/nova_btc_utxo_seal_type.cell:nova_btc_utxo_seal_lifecycle"),
            ),
        ),
        ("expected_actions_present".to_string(), Value::Bool(expected_actions.is_subset(&action_names))),
        ("schemas_exact".to_string(), Value::Bool(json_pointer_bool(&schemas, "/exact"))),
        ("fixtures_exact".to_string(), Value::Bool(json_pointer_bool(&fixtures, "/exact"))),
        ("docs_exact".to_string(), Value::Bool(json_pointer_bool(&docs, "/exact"))),
        (
            "invariant_schema".to_string(),
            Value::Bool(json_pointer_str(&invariant_payload, "/schema") == Some("novaseal-btc-utxo-seal-invariant-matrix-v0.1")),
        ),
        ("required_invariants_present".to_string(), Value::Bool(required_invariants.is_subset(&invariant_ids))),
        (
            "no_empty_invariant_coverage".to_string(),
            Value::Bool(invariant_ids.iter().all(|id| coverage_by_id.get(id).is_some_and(value_is_present))),
        ),
        (
            "live_devnet_gap_explicit".to_string(),
            Value::Bool(
                coverage_by_id.get("live_devnet_lifecycle").and_then(Value::as_str)
                    == Some("target/novaseal-btc-utxo-seal-devnet-stateful-live.json"),
            ),
        ),
        (
            "btc_public_verification_gap_explicit".to_string(),
            Value::Bool(
                coverage_by_id.get("btc_public_verification").and_then(Value::as_str)
                    == Some("external-public-mainnet-spv-or-indexer-evidence-required"),
            ),
        ),
    ]);
    let missing_invariants = required_invariants.difference(&invariant_ids).cloned().collect::<Vec<_>>();
    Ok(json!({
        "schema": "novaseal-btc-utxo-seal-profile-package-validation-v0.1",
        "status": if object_values_all_true(Some(&Value::Object(checks.clone()))) { "passed" } else { "failed" },
        "classification": "profile-package-evidence-not-btc-spend-proof-or-live-stateful-acceptance",
        "root": rel(repo_root, &root),
        "manifest": rel(repo_root, &manifest_path),
        "canonical_schema_hash": schema_hash,
        "actions": action_names.into_iter().collect::<Vec<_>>(),
        "schemas": schemas,
        "fixtures": fixtures,
        "docs": docs,
        "invariant_matrix": {
            "path": rel(repo_root, &invariant_path),
            "required": required_invariants.into_iter().collect::<Vec<_>>(),
            "present": invariant_ids.into_iter().collect::<Vec<_>>(),
            "missing": missing_invariants,
            "coverage_by_id": coverage_by_id,
        },
        "checks": checks,
        "remaining_acceptance_gap": "public BTC SPV spend-verification evidence is still required before btc_utxo_seal_closure can make production BTC-spend claims",
    }))
}

pub(super) fn validate_dual_seal_profile_package(repo_root: &Path) -> Result<Value> {
    let root = repo_root.join(DUAL_SEAL_ROOT);
    let manifest_path = repo_root.join(DUAL_SEAL_MANIFEST);
    let manifest = if manifest_path.is_file() { Some(manifest_metadata(&manifest_path)?) } else { None };
    let metadata_str = |key: &str| manifest.as_ref().and_then(|metadata| toml_str(metadata, key));
    let source = if root.join("src").is_dir() { read_cell_sources(&root.join("src"))? } else { String::new() };
    let schema_path = repo_root.join(CANONICAL_SCHEMA);
    let schema_hash = canonical_schema_hash(&schema_path)?;
    let source_checks = REQUIRED_DUAL_SEAL_SOURCE_PATTERNS
        .iter()
        .map(|(name, pattern)| (format!("source_{name}"), Value::Bool(source.contains(pattern))))
        .collect::<Map<_, _>>();
    let actions = find_actions(&source);
    let action_names = actions.iter().map(|action| action.name.clone()).collect::<BTreeSet<_>>();
    let expected_actions =
        ["finalize_dual_seal", "nova_dual_seal_lifecycle"].iter().map(|action| (*action).to_string()).collect::<BTreeSet<_>>();
    let schemas = expected_files(repo_root, &root.join("schemas"), EXPECTED_DUAL_SEAL_SCHEMA_FILES)?;
    let fixtures = expected_files(repo_root, &root.join("fixtures"), EXPECTED_DUAL_SEAL_FIXTURES)?;
    let docs = expected_files(repo_root, &root.join("docs"), EXPECTED_DUAL_SEAL_DOCS)?;
    let invariant_path = root.join("proofs/invariant_matrix.json");
    let invariant_payload = if invariant_path.is_file() { json_load_path(repo_root, &invariant_path)? } else { Value::Null };
    let invariants = invariant_payload.get("invariants").and_then(Value::as_array).cloned().unwrap_or_default();
    let invariant_ids = invariants.iter().filter_map(|row| json_pointer_str(row, "/id").map(str::to_string)).collect::<BTreeSet<_>>();
    let required_invariants = EXPECTED_DUAL_SEAL_INVARIANTS.iter().map(|value| (*value).to_string()).collect::<BTreeSet<_>>();
    let coverage_by_id = invariants
        .iter()
        .filter_map(|row| Some((json_pointer_str(row, "/id")?.to_string(), row.get("coverage").cloned().unwrap_or(Value::Null))))
        .collect::<Map<_, _>>();
    let mut checks = source_checks;
    checks.extend([
        ("root_present".to_string(), Value::Bool(root.is_dir())),
        ("manifest_present".to_string(), Value::Bool(manifest_path.is_file())),
        ("manifest_protocol_family".to_string(), Value::Bool(metadata_str("protocol_family") == Some("NovaSeal"))),
        ("manifest_profile".to_string(), Value::Bool(metadata_str("profile") == Some(EXPECTED_DUAL_SEAL_PROFILE))),
        ("manifest_conforms_to".to_string(), Value::Bool(metadata_str("conforms_to") == Some(EXPECTED_NOVASEAL_CANONICAL_SCHEMA))),
        ("manifest_canonical_schema_hash".to_string(), Value::Bool(metadata_str("canonical_schema_hash") == schema_hash.as_deref())),
        (
            "manifest_conformance_gate".to_string(),
            Value::Bool(metadata_str("conformance_gate") == Some(EXPECTED_PROFILE_CERTIFICATION_GATE)),
        ),
        (
            "manifest_certification_plugin".to_string(),
            Value::Bool(metadata_str("certification_plugin") == Some(EXPECTED_CERTIFICATION_PLUGIN)),
        ),
        (
            "manifest_stateful_dispatcher".to_string(),
            Value::Bool(metadata_str("stateful_dispatcher") == Some("src/nova_dual_seal_type.cell:nova_dual_seal_lifecycle")),
        ),
        (
            "manifest_btc_public_verification_gap".to_string(),
            Value::Bool(
                metadata_str("btc_public_verification")
                    == Some("live-devnet-closure-bound; public/mainnet SPV-or-indexer attestation required for BTC-finality claims"),
            ),
        ),
        (
            "manifest_ckb_finality_gap".to_string(),
            Value::Bool(metadata_str("ckb_finality_verification") == Some("target/novaseal-dual-seal-devnet-stateful-live.json")),
        ),
        (
            "manifest_source_actions".to_string(),
            Value::Bool(
                metadata_str("source_actions")
                    == Some("src/nova_dual_seal_type.cell:finalize_dual_seal;src/nova_dual_seal_type.cell:nova_dual_seal_lifecycle"),
            ),
        ),
        ("expected_actions_present".to_string(), Value::Bool(expected_actions.is_subset(&action_names))),
        ("schemas_exact".to_string(), Value::Bool(json_pointer_bool(&schemas, "/exact"))),
        ("fixtures_exact".to_string(), Value::Bool(json_pointer_bool(&fixtures, "/exact"))),
        ("docs_exact".to_string(), Value::Bool(json_pointer_bool(&docs, "/exact"))),
        (
            "invariant_schema".to_string(),
            Value::Bool(json_pointer_str(&invariant_payload, "/schema") == Some("novaseal-dual-seal-invariant-matrix-v0.1")),
        ),
        ("required_invariants_present".to_string(), Value::Bool(required_invariants.is_subset(&invariant_ids))),
        (
            "no_empty_invariant_coverage".to_string(),
            Value::Bool(invariant_ids.iter().all(|id| coverage_by_id.get(id).is_some_and(value_is_present))),
        ),
        (
            "live_devnet_gap_explicit".to_string(),
            Value::Bool(coverage_by_id.get("live_devnet_lifecycle").and_then(Value::as_str) == Some("live-devnet-covered")),
        ),
        (
            "btc_public_verification_gap_explicit".to_string(),
            Value::Bool(
                coverage_by_id.get("btc_public_verification").and_then(Value::as_str)
                    == Some("external-public-mainnet-spv-or-indexer-evidence-required"),
            ),
        ),
        (
            "ckb_finality_gap_explicit".to_string(),
            Value::Bool(coverage_by_id.get("ckb_finality_verification").and_then(Value::as_str) == Some("live-devnet-covered")),
        ),
    ]);
    let missing_invariants = required_invariants.difference(&invariant_ids).cloned().collect::<Vec<_>>();
    Ok(json!({
        "schema": "novaseal-dual-seal-profile-package-validation-v0.1",
        "status": if object_values_all_true(Some(&Value::Object(checks.clone()))) { "passed" } else { "failed" },
        "classification": "profile-package-plus-live-ckb-stateful-evidence-not-btc-finality",
        "root": rel(repo_root, &root),
        "manifest": rel(repo_root, &manifest_path),
        "canonical_schema_hash": schema_hash,
        "actions": action_names.into_iter().collect::<Vec<_>>(),
        "schemas": schemas,
        "fixtures": fixtures,
        "docs": docs,
        "invariant_matrix": {
            "path": rel(repo_root, &invariant_path),
            "required": required_invariants.into_iter().collect::<Vec<_>>(),
            "present": invariant_ids.into_iter().collect::<Vec<_>>(),
            "missing": missing_invariants,
            "coverage_by_id": coverage_by_id,
        },
        "checks": checks,
        "remaining_acceptance_gap": "public BTC closure evidence is still required before dual_seal_finality can make production BTC-finality claims",
    }))
}

pub(super) fn validate_fiber_candidate_profile_package(repo_root: &Path) -> Result<Value> {
    let root = repo_root.join(FIBER_CANDIDATE_ROOT);
    let manifest_path = repo_root.join(FIBER_CANDIDATE_MANIFEST);
    let manifest = if manifest_path.is_file() { Some(manifest_metadata(&manifest_path)?) } else { None };
    let metadata_str = |key: &str| manifest.as_ref().and_then(|metadata| toml_str(metadata, key));
    let source = if root.join("src").is_dir() { read_cell_sources(&root.join("src"))? } else { String::new() };
    let schema_path = repo_root.join(CANONICAL_SCHEMA);
    let schema_hash = canonical_schema_hash(&schema_path)?;
    let source_checks = REQUIRED_FIBER_CANDIDATE_SOURCE_PATTERNS
        .iter()
        .map(|(name, pattern)| (format!("source_{name}"), Value::Bool(source.contains(pattern))))
        .collect::<Map<_, _>>();
    let actions = find_actions(&source);
    let action_names = actions.iter().map(|action| action.name.clone()).collect::<BTreeSet<_>>();
    let expected_actions = ["settle_fiber_candidate", "nova_fiber_candidate_lifecycle"]
        .iter()
        .map(|action| (*action).to_string())
        .collect::<BTreeSet<_>>();
    let schemas = expected_files(repo_root, &root.join("schemas"), EXPECTED_FIBER_CANDIDATE_SCHEMA_FILES)?;
    let fixtures = expected_files(repo_root, &root.join("fixtures"), EXPECTED_FIBER_CANDIDATE_FIXTURES)?;
    let docs = expected_files(repo_root, &root.join("docs"), EXPECTED_FIBER_CANDIDATE_DOCS)?;
    let audit_doc = std::fs::read_to_string(root.join("docs/AUDIT_STATUS.md")).unwrap_or_default();
    let devnet_doc = std::fs::read_to_string(root.join("docs/DEVNET_STATEFUL_ACCEPTANCE.md")).unwrap_or_default();
    let security_doc = std::fs::read_to_string(root.join("docs/SECURITY.md")).unwrap_or_default();
    let invariant_path = root.join("proofs/invariant_matrix.json");
    let invariant_payload = if invariant_path.is_file() { json_load_path(repo_root, &invariant_path)? } else { Value::Null };
    let invariants = invariant_payload.get("invariants").and_then(Value::as_array).cloned().unwrap_or_default();
    let invariant_ids = invariants.iter().filter_map(|row| json_pointer_str(row, "/id").map(str::to_string)).collect::<BTreeSet<_>>();
    let required_invariants = EXPECTED_FIBER_CANDIDATE_INVARIANTS.iter().map(|value| (*value).to_string()).collect::<BTreeSet<_>>();
    let coverage_by_id = invariants
        .iter()
        .filter_map(|row| Some((json_pointer_str(row, "/id")?.to_string(), row.get("coverage").cloned().unwrap_or(Value::Null))))
        .collect::<Map<_, _>>();
    let mut checks = source_checks;
    checks.extend([
        ("root_present".to_string(), Value::Bool(root.is_dir())),
        ("manifest_present".to_string(), Value::Bool(manifest_path.is_file())),
        ("manifest_protocol_family".to_string(), Value::Bool(metadata_str("protocol_family") == Some("NovaSeal"))),
        ("manifest_profile".to_string(), Value::Bool(metadata_str("profile") == Some(EXPECTED_FIBER_CANDIDATE_PROFILE))),
        ("manifest_conforms_to".to_string(), Value::Bool(metadata_str("conforms_to") == Some(EXPECTED_NOVASEAL_CANONICAL_SCHEMA))),
        ("manifest_canonical_schema_hash".to_string(), Value::Bool(metadata_str("canonical_schema_hash") == schema_hash.as_deref())),
        (
            "manifest_conformance_gate".to_string(),
            Value::Bool(metadata_str("conformance_gate") == Some(EXPECTED_PROFILE_CERTIFICATION_GATE)),
        ),
        (
            "manifest_certification_plugin".to_string(),
            Value::Bool(metadata_str("certification_plugin") == Some(EXPECTED_CERTIFICATION_PLUGIN)),
        ),
        (
            "manifest_stateful_dispatcher".to_string(),
            Value::Bool(metadata_str("stateful_dispatcher") == Some("src/nova_fiber_candidate_type.cell:nova_fiber_candidate_lifecycle")),
        ),
        (
            "manifest_fiber_execution_gap".to_string(),
            Value::Bool(metadata_str("fiber_execution") == Some(FIBER_NODE_EXPERIMENTS)),
        ),
        (
            "manifest_source_actions".to_string(),
            Value::Bool(
                metadata_str("source_actions")
                    == Some("src/nova_fiber_candidate_type.cell:settle_fiber_candidate;src/nova_fiber_candidate_type.cell:nova_fiber_candidate_lifecycle"),
            ),
        ),
        ("expected_actions_present".to_string(), Value::Bool(expected_actions.is_subset(&action_names))),
        ("schemas_exact".to_string(), Value::Bool(json_pointer_bool(&schemas, "/exact"))),
        ("fixtures_exact".to_string(), Value::Bool(json_pointer_bool(&fixtures, "/exact"))),
        ("docs_exact".to_string(), Value::Bool(json_pointer_bool(&docs, "/exact"))),
        (
            "docs_fiber_execution_claim_current".to_string(),
            Value::Bool(
                audit_doc.contains("live-fiber-node-execution-evidence")
                    && audit_doc.contains("Fiber workflow discovery | live-fiber-workflow-suite-evidence")
                    && !audit_doc.contains("pending-fiber-node-suite-execution")
                    && !audit_doc.contains("discovery-ready-live-not-run")
                    && devnet_doc.contains("all required Fiber workflow suites")
                    && devnet_doc.contains("executed and passed")
                    && !devnet_doc.contains("workflow execution remains pending")
                    && security_doc.contains("external Fiber workflow execution evidence is present")
                    && security_doc.contains("does not verify Fiber HTLCs, routes, liquidity, fees, or revocations"),
            ),
        ),
        (
            "invariant_schema".to_string(),
            Value::Bool(json_pointer_str(&invariant_payload, "/schema") == Some("novaseal-fiber-candidate-invariant-matrix-v0.1")),
        ),
        ("required_invariants_present".to_string(), Value::Bool(required_invariants.is_subset(&invariant_ids))),
        (
            "no_empty_invariant_coverage".to_string(),
            Value::Bool(invariant_ids.iter().all(|id| coverage_by_id.get(id).is_some_and(value_is_present))),
        ),
        (
            "live_devnet_gap_explicit".to_string(),
            Value::Bool(
                coverage_by_id.get("live_devnet_lifecycle").and_then(Value::as_str)
                    == Some("target/novaseal-fiber-candidate-devnet-stateful-live.json"),
            ),
        ),
        (
            "fiber_execution_gap_explicit".to_string(),
            Value::Bool(coverage_by_id.get("fiber_execution").and_then(Value::as_str) == Some(FIBER_NODE_EXPERIMENTS)),
        ),
    ]);
    let missing_invariants = required_invariants.difference(&invariant_ids).cloned().collect::<Vec<_>>();
    Ok(json!({
        "schema": "novaseal-fiber-candidate-profile-package-validation-v0.1",
        "status": if object_values_all_true(Some(&Value::Object(checks.clone()))) { "passed" } else { "failed" },
        "classification": "profile-package-with-live-stateful-and-fiber-node-execution-evidence",
        "root": rel(repo_root, &root),
        "manifest": rel(repo_root, &manifest_path),
        "canonical_schema_hash": schema_hash,
        "actions": action_names.into_iter().collect::<Vec<_>>(),
        "schemas": schemas,
        "fixtures": fixtures,
        "docs": docs,
        "invariant_matrix": {
            "path": rel(repo_root, &invariant_path),
            "required": required_invariants.into_iter().collect::<Vec<_>>(),
            "present": invariant_ids.into_iter().collect::<Vec<_>>(),
            "missing": missing_invariants,
            "coverage_by_id": coverage_by_id,
        },
        "checks": checks,
        "operator_fixture_evidence": PROFILE_OPERATOR_FIXTURES,
    }))
}
