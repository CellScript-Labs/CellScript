use super::package_sources::{
    canonical_schema_checks, canonical_schema_hash, canonical_schema_lines, expected_files, manifest_metadata, read_cell_sources,
    toml_str,
};
use super::{
    json_pointer_bool, object_values_all_true, rel, CANONICAL_SCHEMA, CORE_ROOT, EXPECTED_AGREEMENT_CONFORMANCE_GATE,
    EXPECTED_AGREEMENT_PROFILE, EXPECTED_CERTIFICATION_PLUGIN, EXPECTED_CERTIFICATION_REPORT, EXPECTED_CORE_FIXTURES,
    EXPECTED_NOVASEAL_CANONICAL_SCHEMA, REQUIRED_AGREEMENT_CORE_PATTERNS,
};
use crate::error::Result;
use serde_json::{json, Map, Value};
use std::path::Path;

pub(super) fn validate_core_security_source(repo_root: &Path) -> Result<Value> {
    let source = read_cell_sources(&repo_root.join(CORE_ROOT).join("src"))?;
    let fixture_files = expected_files(repo_root, &repo_root.join(CORE_ROOT).join("fixtures"), EXPECTED_CORE_FIXTURES)?;
    let checks = json!({
        "fixture_set_exact": json_pointer_bool(&fixture_files, "/exact"),
        "wrong_pubkey_valid_signature_fixture_present": repo_root
            .join("proposals/novaseal/v0-mvp-skeleton/fixtures/wrong_pubkey_valid_signature_reject.json")
            .is_file(),
        "authority_hash_mapping_mismatch_fixture_present": repo_root
            .join("proposals/novaseal/v0-mvp-skeleton/fixtures/authority_hash_mapping_mismatch_reject.json")
            .is_file(),
        "authority_rotation_without_explicit_action_fixture_present": repo_root
            .join("proposals/novaseal/v0-mvp-skeleton/fixtures/authority_rotation_without_explicit_action_reject.json")
            .is_file(),
        "state_action_binds_sig_pubkey_to_old_cell_authority": source.contains("require sig.pubkey == old_cell.btc_authority_hash.0"),
        "lifecycle_binds_sig_pubkey_to_old_cell_authority": source.contains("require pubkey == old_cell.btc_authority_hash.0"),
        "lock_binds_sig_pubkey_to_cell_authority_in_both_lock_surfaces": source.matches("require sig.pubkey == cell.btc_authority_hash.0").count() >= 2,
        "core_nonce_increment_guarded": source.contains("require old_cell.nonce < U64_MAX")
            && source.contains("require ckb::cell_data_u64_le(source::input(0), 130) < U64_MAX")
            && source.contains(
                "require ckb::cell_data_u64_le(source::output(0), 130) == ckb::cell_data_u64_le(source::input(0), 130) + 1",
            ),
    });
    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "fixture_files": fixture_files,
        "security_boundary": "BIP340 verification is only authority-enforcing when the verified x-only pubkey is bound to the Cell-declared authority.",
    }))
}

pub(crate) fn validate_agreement_profile_conformance(
    repo_root: &Path,
    core_manifest_path: &Path,
    agreement_manifest_path: &Path,
    agreement_root: &Path,
) -> Result<Value> {
    let core_metadata = manifest_metadata(core_manifest_path)?;
    let agreement_metadata = manifest_metadata(agreement_manifest_path)?;
    let agreement_source = read_cell_sources(&agreement_root.join("src"))?;
    let schema_path = repo_root.join(CANONICAL_SCHEMA);
    let schema_hash = canonical_schema_hash(&schema_path)?;
    let schema_checks = canonical_schema_checks(&schema_path)?;
    let source_checks = REQUIRED_AGREEMENT_CORE_PATTERNS
        .iter()
        .map(|(name, pattern)| (format!("source_{name}"), Value::Bool(agreement_source.contains(pattern))))
        .collect::<Map<_, _>>();

    let mut checks = schema_checks;
    checks.extend([
        (
            "core_declares_canonical_schema".to_string(),
            Value::Bool(toml_str(&core_metadata, "canonical_schema") == Some(EXPECTED_NOVASEAL_CANONICAL_SCHEMA)),
        ),
        (
            "core_canonical_schema_hash".to_string(),
            Value::Bool(toml_str(&core_metadata, "canonical_schema_hash") == schema_hash.as_deref()),
        ),
        ("core_package_role".to_string(), Value::Bool(toml_str(&core_metadata, "package_role") == Some("canonical-example"))),
        ("core_protocol_family".to_string(), Value::Bool(toml_str(&core_metadata, "protocol_family") == Some("NovaSeal"))),
        ("profile_protocol_family".to_string(), Value::Bool(toml_str(&agreement_metadata, "protocol_family") == Some("NovaSeal"))),
        ("profile_name".to_string(), Value::Bool(toml_str(&agreement_metadata, "profile") == Some(EXPECTED_AGREEMENT_PROFILE))),
        (
            "profile_conforms_to".to_string(),
            Value::Bool(toml_str(&agreement_metadata, "conforms_to") == Some(EXPECTED_NOVASEAL_CANONICAL_SCHEMA)),
        ),
        (
            "profile_canonical_schema_hash".to_string(),
            Value::Bool(toml_str(&agreement_metadata, "canonical_schema_hash") == schema_hash.as_deref()),
        ),
        (
            "profile_conformance_gate".to_string(),
            Value::Bool(toml_str(&agreement_metadata, "conformance_gate") == Some(EXPECTED_AGREEMENT_CONFORMANCE_GATE)),
        ),
        (
            "profile_certification_plugin".to_string(),
            Value::Bool(toml_str(&agreement_metadata, "certification_plugin") == Some(EXPECTED_CERTIFICATION_PLUGIN)),
        ),
        (
            "profile_certification_report".to_string(),
            Value::Bool(toml_str(&agreement_metadata, "certification_report") == Some(EXPECTED_CERTIFICATION_REPORT)),
        ),
    ]);
    checks.extend(source_checks);

    let source_patterns = REQUIRED_AGREEMENT_CORE_PATTERNS
        .iter()
        .map(|(name, pattern)| ((*name).to_string(), Value::String((*pattern).to_string())))
        .collect::<Map<_, _>>();
    Ok(json!({
        "status": if object_values_all_true(Some(&Value::Object(checks.clone()))) { "passed" } else { "failed" },
        "conforms_to": toml_str(&agreement_metadata, "conforms_to"),
        "expected_conforms_to": EXPECTED_NOVASEAL_CANONICAL_SCHEMA,
        "canonical_schema": toml_str(&core_metadata, "canonical_schema"),
        "canonical_schema_file": rel(repo_root, &schema_path),
        "canonical_schema_hash": schema_hash,
        "canonical_schema_hash_algorithm": "sha256(normalized schema lines: comments/blank lines ignored, whitespace collapsed)",
        "canonical_schema_lines": canonical_schema_lines(&schema_path)?,
        "core_manifest": rel(repo_root, core_manifest_path),
        "profile_manifest": rel(repo_root, agreement_manifest_path),
        "checks": checks,
        "manifest": {
            "canonical_schema": toml_str(&core_metadata, "canonical_schema"),
            "canonical_schema_hash": toml_str(&core_metadata, "canonical_schema_hash"),
            "package_role": toml_str(&core_metadata, "package_role"),
            "core_protocol_family": toml_str(&core_metadata, "protocol_family"),
            "profile": toml_str(&agreement_metadata, "profile"),
            "protocol_family": toml_str(&agreement_metadata, "protocol_family"),
            "conforms_to": toml_str(&agreement_metadata, "conforms_to"),
            "profile_canonical_schema_hash": toml_str(&agreement_metadata, "canonical_schema_hash"),
            "conformance_gate": toml_str(&agreement_metadata, "conformance_gate"),
        },
        "source_patterns": source_patterns,
    }))
}
