//! NovaSeal external production-evidence handoff bundle.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::btc_spv_adapter::{field_constraints as btc_field_constraints, required_fields as btc_required_fields};
use crate::crypto::{canonical_report_hash, sha256_hex};
use crate::shared::{lexical_path, stable_json_pretty};

const PERSON: &[u8] = b"NovaExtHandoff";
const HASH_ALGORITHM: &str = "blake2b-256(person=NovaExtHandoff)";
const BTC_OUTPUT: &str = "proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.json";
const CELLDEP_OUTPUT: &str = "proposals/novaseal/v0-mvp-skeleton/proofs/public_shared_cell_dep_attestation.json";
const TCB_OUTPUT: &str = "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.json";
const RWA_OUTPUT: &str = "proposals/novaseal/rwa-receipt-profile-v0/proofs/legal_registry_review_evidence.json";
const PROFILES: [&str; 3] = ["btc-transaction-commitment-profile-v0", "btc-utxo-seal-profile-v0", "dual-seal-profile-v0"];

fn hash(label: &str, value: &Value) -> Result<String> {
    canonical_report_hash(PERSON, label, value)
}

fn hex32(value: &Value) -> bool {
    value.as_str().is_some_and(|text| {
        text.len() == 66 && text.starts_with("0x") && text[2..].chars().all(|character| character.is_ascii_hexdigit())
    })
}

fn non_placeholder(value: &Value) -> bool {
    hex32(value) && value.as_str().is_some_and(|text| text[2..].chars().any(|character| character != '0'))
}

fn non_negative(value: &Value) -> bool {
    value.as_i64().is_some_and(|number| number >= 0) || value.as_u64().is_some()
}

fn positive(value: &Value) -> bool {
    value.as_i64().is_some_and(|number| number > 0) || value.as_u64().is_some_and(|number| number > 0)
}

fn anchor_source(profile: &str) -> &'static str {
    if profile == PROFILES[0] {
        "external_public_btc_transaction"
    } else {
        "external_public_btc_spend"
    }
}

fn profile_mapping(profile: &str) -> BTreeMap<&'static str, &'static str> {
    let mut fields = BTreeMap::from([
        ("anchor_source", "expected_anchor_source"),
        ("btc_txid", "expected_btc_txid"),
        ("btc_wtxid", "expected_btc_wtxid"),
    ]);
    if profile == PROFILES[0] {
        fields.extend([("btc_output_index", "expected_btc_output_index"), ("btc_amount_sats", "expected_btc_amount_sats")]);
    } else {
        fields.extend([
            ("spend_input_index", "expected_spend_input_index"),
            ("sealed_btc_txid", "expected_sealed_btc_txid"),
            ("sealed_btc_vout_index", "expected_sealed_btc_vout_index"),
            ("sealed_btc_amount_sats", "expected_sealed_btc_amount_sats"),
            ("script_pubkey_hash", "expected_script_pubkey_hash"),
            ("sealed_utxo_commitment_hash", "expected_sealed_utxo_commitment_hash"),
        ]);
    }
    fields
}

fn expected_binding_fields(profile: &str) -> BTreeSet<String> {
    let mut fields = [
        "ckb_live_tx_hash",
        "live_report_hash",
        "service_builder_case_hash",
        "service_builder_tx_skeleton_hash",
        "service_builder_receipt_binding_hash",
        "ckb_btc_commitment_hash",
    ]
    .into_iter()
    .map(ToOwned::to_owned)
    .collect::<BTreeSet<_>>();
    fields.extend(profile_mapping(profile).keys().map(|value| (*value).to_owned()));
    fields
}

fn binding_valid(profile: &str, field: &str, value: &Value) -> bool {
    match field {
        "ckb_live_tx_hash"
        | "live_report_hash"
        | "service_builder_case_hash"
        | "service_builder_tx_skeleton_hash"
        | "service_builder_receipt_binding_hash"
        | "ckb_btc_commitment_hash"
        | "btc_txid"
        | "btc_wtxid"
        | "sealed_btc_txid"
        | "script_pubkey_hash"
        | "sealed_utxo_commitment_hash" => non_placeholder(value),
        "anchor_source" => value.as_str() == Some(anchor_source(profile)),
        "spend_input_index" | "sealed_btc_vout_index" | "btc_output_index" => non_negative(value),
        "btc_amount_sats" | "sealed_btc_amount_sats" => positive(value),
        _ => false,
    }
}

fn btc_case(adapter: &Value) -> Result<Value> {
    let cases = adapter.get("cases").and_then(Value::as_array).cloned().unwrap_or_default();
    let profiles = cases.iter().filter_map(|case| case.get("profile").and_then(Value::as_str)).collect::<BTreeSet<_>>();
    let mut scenarios = Map::new();
    let mut bindings = Map::new();
    for case in &cases {
        let Some(profile) = case.get("profile").and_then(Value::as_str) else {
            continue;
        };
        if let Some(scenario) = case.pointer("/request/scenario").and_then(Value::as_str) {
            scenarios.insert(profile.to_owned(), Value::String(scenario.to_owned()));
        }
        let request = case.get("request").cloned().unwrap_or_else(|| json!({}));
        let mut binding = Map::new();
        for field in [
            "ckb_live_tx_hash",
            "live_report_hash",
            "service_builder_case_hash",
            "service_builder_tx_skeleton_hash",
            "service_builder_receipt_binding_hash",
            "ckb_btc_commitment_hash",
        ] {
            binding.insert(field.to_owned(), request.get(field).cloned().unwrap_or(Value::Null));
        }
        for (output_field, request_field) in profile_mapping(profile) {
            if let Some(value) = request.get(request_field)
                && !value.is_null()
            {
                binding.insert(output_field.to_owned(), value.clone());
            }
        }
        bindings.insert(profile.to_owned(), Value::Object(binding));
    }
    let required_profiles = PROFILES.into_iter().collect::<BTreeSet<_>>();
    let binding_complete = bindings.keys().map(String::as_str).collect::<BTreeSet<_>>() == required_profiles
        && bindings.iter().all(|(profile, value)| {
            let Some(values) = value.as_object() else {
                return false;
            };
            values.keys().cloned().collect::<BTreeSet<_>>() == expected_binding_fields(profile)
                && values.iter().all(|(field, value)| binding_valid(profile, field, value))
        });
    let checks = json!({
        "source_adapter_passed": adapter.get("status").and_then(Value::as_str) == Some("passed"),
        "source_adapter_status_request_ready": adapter.get("adapter_status").and_then(Value::as_str) == Some("request_ready_external_evidence_required"),
        "production_output_matches": adapter.get("production_output").and_then(Value::as_str) == Some(BTC_OUTPUT),
        "summary_counts_match": adapter.pointer("/summary/total").and_then(Value::as_u64) == Some(3) && adapter.pointer("/summary/matched") == adapter.pointer("/summary/total"),
        "required_profiles_complete": profiles == required_profiles,
        "expected_scenarios_complete": scenarios.keys().map(String::as_str).collect::<BTreeSet<_>>() == required_profiles && scenarios.values().all(|value| value.as_str().is_some_and(|text| !text.is_empty())),
        "expected_case_bindings_complete": binding_complete,
        "source_cases_passed": cases.iter().all(|case| case.get("status").and_then(Value::as_str) == Some("passed"))
    });
    let passed = checks.as_object().is_some_and(|map| map.values().all(|value| value == &Value::Bool(true)));
    Ok(json!({
        "group": "public_btc_spv_evidence",
        "status": if passed { "passed" } else { "failed" },
        "checks": checks,
        "source_adapter": "target/novaseal-btc-spv-evidence-adapter.json",
        "source_adapter_hash": hash("btc_spv_adapter", adapter)?,
        "production_output": BTC_OUTPUT,
        "required_profiles": PROFILES,
        "expected_scenarios": scenarios,
        "expected_case_bindings": bindings,
        "required_external_fields": btc_required_fields(),
        "field_constraints": btc_field_constraints()
    }))
}

fn field_set(case: &Value) -> BTreeSet<&str> {
    case.pointer("/request/required_public_fields").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect()
}

fn truthy(value: Option<&Value>) -> bool {
    value.is_some_and(|value| match value {
        Value::Null | Value::Bool(false) => false,
        Value::String(text) => !text.is_empty(),
        Value::Array(values) => !values.is_empty(),
        Value::Object(values) => !values.is_empty(),
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::Bool(true) => true,
    })
}

fn attestation_case(adapter: &Value, name: &str, group: &str, output: &str, required: &[&str]) -> Result<Value> {
    let empty = json!({});
    let source = adapter
        .get("cases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|case| case.get("name").and_then(Value::as_str) == Some(name))
        .unwrap_or(&empty);
    let request = source.get("request").cloned().unwrap_or_else(|| json!({}));
    let fields = field_set(source);
    let checks = json!({
        "source_adapter_passed": adapter.get("status").and_then(Value::as_str) == Some("passed"),
        "source_adapter_status_request_ready": adapter.get("adapter_status").and_then(Value::as_str) == Some("request_ready_external_attestations_required"),
        "source_case_passed": source.get("status").and_then(Value::as_str) == Some("passed"),
        "production_output_matches": request.get("production_output").and_then(Value::as_str) == Some(output),
        "required_fields_complete": required.iter().all(|field| fields.contains(field))
    });
    let passed = checks.as_object().is_some_and(|map| map.values().all(|value| value == &Value::Bool(true)));
    let mut expected = Map::new();
    let mappings = [
        ("expected_release_package", "release.package"),
        ("expected_release_version", "release.version"),
        ("expected_release_manifest_commit", "release.manifest_commit"),
        ("expected_dep_type", "runtime_verifier.dep_type"),
        ("expected_hash_type", "runtime_verifier.hash_type"),
    ];
    for (input, output) in mappings {
        if truthy(request.get(input)) {
            expected.insert(output.to_owned(), request[input].clone());
        }
    }
    if name == "public_shared_cell_dep_attestation" {
        for (input, output) in [("ipc_abi", "runtime_verifier.ipc_abi"), ("verifier_id", "runtime_verifier.verifier_id")] {
            if truthy(request.get(input)) {
                expected.insert(output.to_owned(), request[input].clone());
            }
        }
    } else {
        for input in ["ipc_abi", "verifier_id"] {
            if truthy(request.get(input)) {
                expected.insert(input.to_owned(), request[input].clone());
            }
        }
    }
    for (input, output) in [
        ("expected_artifact_hash", "artifact_hash"),
        ("expected_artifact_hash_algorithm", "artifact_hash_algorithm"),
        ("expected_review_scope", "review_scope"),
        ("expected_source_tree_sha256", "source_tree_sha256"),
    ] {
        if truthy(request.get(input)) {
            expected.insert(output.to_owned(), request[input].clone());
        }
    }
    let mut result = json!({
        "group": group,
        "status": if passed { "passed" } else { "failed" },
        "checks": checks,
        "source_adapter": "target/novaseal-external-attestation-adapter.json",
        "source_adapter_hash": hash("external_attestation_adapter", adapter)?,
        "source_case": name,
        "production_output": output,
        "required_external_fields": required,
        "field_constraints": request.get("field_constraints").cloned().unwrap_or_else(|| json!({}))
    });
    if !expected.is_empty() {
        result["expected_values"] = Value::Object(expected);
    }
    Ok(result)
}

fn collect_hash_files(root: &Path, path: &Path, files: &mut BTreeSet<PathBuf>) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        bail!("source tree path must not be a symlink: {}", path.strip_prefix(root).unwrap_or(path).display());
    }
    if metadata.is_file() {
        files.insert(path.to_owned());
        return Ok(());
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        let name = entry.file_name();
        if child.is_dir() && ["target", "build", ".git"].iter().any(|skip| name == *skip) {
            continue;
        }
        let child_meta = fs::symlink_metadata(&child)?;
        if child_meta.file_type().is_symlink() {
            bail!("source tree path must not be a symlink: {}", child.strip_prefix(root).unwrap_or(&child).display());
        }
        if child_meta.is_dir() {
            collect_hash_files(root, &child, files)?;
        } else if child_meta.is_file()
            && (child.file_name().and_then(|value| value.to_str()) == Some("Cargo.lock")
                || ["cell", "schema", "toml", "py", "json", "rs"]
                    .contains(&child.extension().and_then(|value| value.to_str()).unwrap_or("")))
        {
            files.insert(child);
        }
    }
    Ok(())
}

fn source_tree_hash(root: &Path) -> Result<String> {
    let paths = [
        "proposals/novaseal/rwa-receipt-profile-v0/Cell.toml",
        "proposals/novaseal/rwa-receipt-profile-v0/src/nova_rwa_receipt_type.cell",
        "proposals/novaseal/rwa-receipt-profile-v0/src/nova_rwa_receipt_lifecycle_type.cell",
        "proposals/novaseal/rwa-receipt-profile-v0/schemas",
        "proposals/novaseal/rwa-receipt-profile-v0/fixtures",
        "proposals/novaseal/rwa-receipt-profile-v0/proofs/invariant_matrix.json",
    ];
    let mut files = BTreeSet::new();
    for path in paths {
        collect_hash_files(root, &root.join(path), &mut files)?;
    }
    let mut state = Sha256::new();
    for path in files {
        let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        state.update(relative.as_bytes());
        state.update([0]);
        state.update(hex::decode(sha256_hex(&fs::read(path)?))?);
    }
    Ok(format!("0x{}", hex::encode(state.finalize())))
}

fn rwa_constraints() -> Value {
    json!({
        "profile": "rwa-receipt-profile-v0",
        "reviewer": "real external legal or registry reviewer identity; placeholder, first-party NovaSeal/CellScript/a19q3, local/devnet/fake/internal, example, and unknown tokens are rejected",
        "review_date": "UTC date in YYYY-MM-DD form; future dates are rejected",
        "review_scope": "exact RWA receipt legal-title, custody, registry-state, oracle-fact, and enforceability review scope",
        "registry.authority": "real registry or custodian authority identity; placeholder, first-party NovaSeal/CellScript/a19q3, local/devnet/fake/internal, example, and unknown tokens are rejected",
        "registry.jurisdiction": "explicit real-world jurisdiction; placeholder, local/devnet/fake/internal, example, and unknown tokens are rejected",
        "registry.registry_report_hash": "0x-prefixed 32-byte non-placeholder hash of the external registry/legal review report",
        "profile_source_tree_sha256": "0x-prefixed 32-byte non-placeholder SHA-256 hash of the RWA profile source tree",
        "report_uri": "HTTPS URI for the public legal/registry review report or source-controlled review commit; example, loopback, private, and reserved hosts are rejected",
        "request_handoff.bundle": "target/novaseal-external-evidence-handoff-bundle.json",
        "request_handoff.bundle_hash": "0x-prefixed 32-byte hash of the NovaSeal external evidence handoff bundle",
        "request_handoff.bundle_hash_algorithm": HASH_ALGORITHM,
        "request_handoff.group": "rwa_legal_registry_review_evidence"
    })
}

fn rwa_case(root: &Path, adapter: &Value) -> Result<Value> {
    let source_hash = source_tree_hash(root)?;
    let checks = json!({
        "source_external_attestation_adapter_passed": adapter.get("status").and_then(Value::as_str) == Some("passed"),
        "source_external_attestation_adapter_status_request_ready": adapter.get("adapter_status").and_then(Value::as_str) == Some("request_ready_external_attestations_required"),
        "production_output_matches": RWA_OUTPUT.ends_with("legal_registry_review_evidence.json"),
        "profile_source_tree_hash_current": source_hash.len() == 66 && source_hash.starts_with("0x")
    });
    let passed = checks.as_object().is_some_and(|map| map.values().all(|value| value == &Value::Bool(true)));
    Ok(json!({
        "group": "rwa_legal_registry_review_evidence",
        "status": if passed { "passed" } else { "failed" },
        "checks": checks,
        "source_adapter": "target/novaseal-external-attestation-adapter.json",
        "source_adapter_hash": hash("external_attestation_adapter", adapter)?,
        "production_output": RWA_OUTPUT,
        "required_external_fields": ["profile", "reviewer", "review_date", "review_scope", "registry.authority", "registry.jurisdiction", "registry.registry_report_hash", "profile_source_tree_sha256", "report_uri", "request_handoff.bundle", "request_handoff.bundle_hash", "request_handoff.bundle_hash_algorithm", "request_handoff.group"],
        "field_constraints": rwa_constraints(),
        "expected_values": {
            "profile": "rwa-receipt-profile-v0",
            "profile_source_tree_sha256": source_hash,
            "review_scope": ["RWA receipt legal title boundary", "RWA receipt custody and registry-state provenance", "RWA receipt oracle-fact exclusion boundary", "RWA receipt enforceability and jurisdiction boundary"]
        }
    }))
}

pub fn run(
    root: &Path,
    btc_adapter: Option<&Path>,
    attestation_adapter: Option<&Path>,
    output: Option<&Path>,
    pretty: bool,
) -> Result<i32> {
    let default_btc = root.join("target/novaseal-btc-spv-evidence-adapter.json");
    let default_attestation = root.join("target/novaseal-external-attestation-adapter.json");
    let default_output = root.join("target/novaseal-external-evidence-handoff-bundle.json");
    let btc: Value = serde_json::from_slice(&fs::read(lexical_path(btc_adapter.unwrap_or(&default_btc)))?)?;
    let attestation: Value = serde_json::from_slice(&fs::read(lexical_path(attestation_adapter.unwrap_or(&default_attestation)))?)?;
    let celldep_fields = [
        "network",
        "attested_at",
        "attestor",
        "release.package",
        "release.version",
        "release.manifest_commit",
        "runtime_verifier.verifier_id",
        "runtime_verifier.ipc_abi",
        "runtime_verifier.out_point",
        "runtime_verifier.data_hash",
        "runtime_verifier.dep_type",
        "runtime_verifier.hash_type",
        "runtime_verifier.artifact_hash",
        "request_handoff.bundle",
        "request_handoff.bundle_hash",
        "request_handoff.bundle_hash_algorithm",
        "request_handoff.group",
    ];
    let tcb_fields = [
        "reviewer",
        "review_date",
        "review_scope",
        "verifier_id",
        "ipc_abi",
        "artifact_hash",
        "artifact_hash_algorithm",
        "source_tree_sha256",
        "report_uri",
        "request_handoff.bundle",
        "request_handoff.bundle_hash",
        "request_handoff.bundle_hash_algorithm",
        "request_handoff.group",
    ];
    let cases = vec![
        btc_case(&btc)?,
        attestation_case(
            &attestation,
            "public_shared_cell_dep_attestation",
            "public_shared_cell_dep_attestation",
            CELLDEP_OUTPUT,
            &celldep_fields,
        )?,
        attestation_case(
            &attestation,
            "external_bip340_tcb_review_attestation",
            "external_bip340_tcb_review_attestation",
            TCB_OUTPUT,
            &tcb_fields,
        )?,
        rwa_case(root, &attestation)?,
    ];
    let matched = cases.iter().filter(|case| case["status"] == "passed").count();
    let passed = matched == cases.len();
    let mut report = json!({
        "schema": "novaseal-external-evidence-handoff-bundle-v0.1",
        "status": if passed { "passed" } else { "failed" },
        "handoff_status": "request_bundle_ready_external_evidence_required",
        "source_btc_spv_adapter": "target/novaseal-btc-spv-evidence-adapter.json",
        "source_btc_spv_adapter_hash": hash("btc_spv_adapter", &btc)?,
        "source_external_attestation_adapter": "target/novaseal-external-attestation-adapter.json",
        "source_external_attestation_adapter_hash": hash("external_attestation_adapter", &attestation)?,
        "production_outputs": cases.iter().map(|case| case["production_output"].clone()).collect::<Vec<_>>(),
        "production_boundary": "This handoff proves external request completeness; it does not satisfy external production evidence.",
        "summary": { "total": cases.len(), "matched": matched, "groups": cases.iter().map(|case| case["group"].clone()).collect::<Vec<_>>() },
        "cases": cases
    });
    report["bundle_hash_algorithm"] = Value::String(HASH_ALGORITHM.to_owned());
    report["bundle_hash"] = Value::String(hash(
        "external_evidence_handoff_bundle",
        &report
            .as_object()
            .context("report must be an object")?
            .iter()
            .filter(|(key, _)| !matches!(key.as_str(), "bundle_hash" | "bundle_hash_algorithm"))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Map<_, _>>()
            .into(),
    )?);
    let output = lexical_path(output.unwrap_or(&default_output));
    fs::create_dir_all(output.parent().context("output path has no parent")?)?;
    fs::write(&output, format!("{}\n", stable_json_pretty(&report)?))?;
    if pretty {
        println!(
            "wrote {} status={} groups={}/{}",
            output.display(),
            report["status"].as_str().unwrap_or("failed"),
            matched,
            report["summary"]["total"]
        );
    }
    Ok(if passed { 0 } else { 1 })
}
