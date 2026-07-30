//! Rust port of the NovaSeal external attestation request adapter.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::crypto::canonical_report_hash;
use crate::shared::{python_json_pretty, python_path};

const PERSON: &[u8] = b"NovaExtAttReqV0";

fn read_json(path: &Path) -> Result<Value> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("failed to read {}", path.display()))?)
        .with_context(|| format!("failed to decode {}", path.display()))
}

fn hash(label: &str, value: &Value) -> Result<String> {
    canonical_report_hash(PERSON, label, value)
}

fn present(value: &Value) -> bool {
    !value.is_null()
        && value.as_str() != Some("")
        && !value.as_array().is_some_and(Vec::is_empty)
        && !value.as_object().is_some_and(serde_json::Map::is_empty)
}

fn public_case(template: &Value, tcb: &Value) -> Result<Value> {
    let verifier = template.get("runtime_verifier").cloned().unwrap_or_else(|| json!({}));
    let release = template.get("release").cloned().unwrap_or_else(|| json!({}));
    let runtime = tcb.get("runtime_artifact").cloned().unwrap_or_else(|| json!({}));
    let required_fields = json!([
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
        "request_handoff.group"
    ]);
    let request = json!({
        "attestation_type": "public_shared_cell_dep_attestation",
        "production_output": "proposals/novaseal/v0-mvp-skeleton/proofs/public_shared_cell_dep_attestation.json",
        "template_schema": template.get("schema").cloned().unwrap_or(Value::Null),
        "template_hash": hash("public_celldep_template", template)?,
        "required_public_fields": required_fields,
        "field_constraints": {
            "network": "explicit public CKB mainnet/testnet name; placeholders and local/devnet/regtest/simnet/private/fake labels are rejected",
            "attested_at": "UTC timestamp in YYYY-MM-DDTHH:MM:SSZ form; future timestamps are rejected",
            "attestor": "real independent release signer or deployer identity; placeholder, first-party NovaSeal/CellScript/a19q3, local/devnet/fake/internal, example, and unknown tokens are rejected",
            "release.package": "novaseal",
            "release.version": "exact NovaSeal release version 0.0.1-v0-mvp",
            "release.manifest_commit": "40-character hex source commit matching the reviewed TCB repo_commit",
            "runtime_verifier.verifier_id": "btc.bip340.v0",
            "runtime_verifier.ipc_abi": "cellscript-btc-bip340-ipc-v0",
            "runtime_verifier.out_point": "0x-prefixed 32-byte CKB transaction hash plus numeric output index",
            "runtime_verifier.data_hash": "0x-prefixed 32-byte non-placeholder CellDep data hash",
            "runtime_verifier.dep_type": "code",
            "runtime_verifier.hash_type": "data1",
            "runtime_verifier.artifact_hash": "0x-prefixed 32-byte non-placeholder BIP340 runtime verifier artifact hash",
            "request_handoff.bundle": "target/novaseal-external-evidence-handoff-bundle.json",
            "request_handoff.bundle_hash": "0x-prefixed 32-byte hash of the NovaSeal external evidence handoff bundle",
            "request_handoff.bundle_hash_algorithm": "blake2b-256(person=NovaExtHandoff)",
            "request_handoff.group": "public_shared_cell_dep_attestation"
        },
        "verifier_id": verifier.get("verifier_id").cloned().unwrap_or(Value::Null),
        "ipc_abi": verifier.get("ipc_abi").cloned().unwrap_or(Value::Null),
        "expected_artifact_hash": runtime.get("artifact_hash").filter(|value| value.as_str().is_some_and(|text| !text.is_empty())).or_else(|| verifier.get("artifact_hash")).cloned().unwrap_or(Value::Null),
        "expected_release_package": release.get("package").cloned().unwrap_or(Value::Null),
        "expected_release_version": release.get("version").cloned().unwrap_or(Value::Null),
        "expected_release_manifest_commit": tcb.get("repo_commit").cloned().unwrap_or(Value::Null),
        "expected_dep_type": verifier.get("dep_type").cloned().unwrap_or(Value::Null),
        "expected_hash_type": verifier.get("hash_type").cloned().unwrap_or(Value::Null),
        "template_artifact_hash": verifier.get("artifact_hash").cloned().unwrap_or(Value::Null),
        "required_status": "attested",
        "network_must_not_equal": "local-devnet"
    });
    let release_keys = release.as_object().map(|map| map.keys().map(String::as_str).collect::<BTreeSet<_>>()).unwrap_or_default();
    let checks = json!({
        "template_schema_current": request["template_schema"] == "novaseal-public-shared-cell-dep-attestation-v0.1",
        "template_status_attested": template.get("status").and_then(Value::as_str) == Some("attested"),
        "release_fields_current": release_keys == BTreeSet::from(["package", "version", "manifest_commit"]),
        "release_package_current": release.get("package").and_then(Value::as_str) == Some("novaseal"),
        "release_version_current": release.get("version").and_then(Value::as_str) == Some("0.0.1-v0-mvp"),
        "release_manifest_commit_present": release.get("manifest_commit").is_some_and(present),
        "expected_release_manifest_commit_present": present(&request["expected_release_manifest_commit"]),
        "verifier_id_current": request["verifier_id"] == "btc.bip340.v0",
        "ipc_abi_current": request["ipc_abi"] == "cellscript-btc-bip340-ipc-v0",
        "dep_type_current": request["expected_dep_type"] == "code",
        "hash_type_current": request["expected_hash_type"] == "data1",
        "artifact_hash_matches_tcb": request["template_artifact_hash"] == request["expected_artifact_hash"],
        "required_fields_complete": request["required_public_fields"].as_array().is_some_and(|fields| fields.len() == 17)
    });
    let passed = checks.as_object().is_some_and(|map| map.values().all(|value| value == &Value::Bool(true)));
    Ok(
        json!({ "name": "public_shared_cell_dep_attestation", "status": if passed { "passed" } else { "failed" }, "checks": checks, "request": request }),
    )
}

fn external_case(template: &Value, tcb: &Value) -> Result<Value> {
    let runtime = tcb.get("runtime_artifact").cloned().unwrap_or_else(|| json!({}));
    let source = tcb.get("source_inventory").cloned().unwrap_or_else(|| json!({}));
    let request = json!({
        "attestation_type": "external_bip340_tcb_review_attestation",
        "production_output": "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.json",
        "template_schema": template.get("schema").cloned().unwrap_or(Value::Null),
        "template_hash": hash("external_tcb_template", template)?,
        "required_public_fields": ["reviewer", "review_date", "review_scope", "verifier_id", "ipc_abi", "artifact_hash", "artifact_hash_algorithm", "source_tree_sha256", "report_uri", "request_handoff.bundle", "request_handoff.bundle_hash", "request_handoff.bundle_hash_algorithm", "request_handoff.group"],
        "field_constraints": {
            "reviewer": "real external reviewer identity; placeholder, first-party NovaSeal/CellScript/a19q3, local/devnet/fake/internal, example, and unknown tokens are rejected",
            "review_date": "UTC date in YYYY-MM-DD form; future dates are rejected",
            "review_scope": "exact BIP340 verifier, RISC-V shell, IPC envelope, and artifact/CellDep pinning scope",
            "verifier_id": "btc.bip340.v0",
            "ipc_abi": "cellscript-btc-bip340-ipc-v0",
            "artifact_hash": "0x-prefixed 32-byte non-placeholder BIP340 runtime verifier artifact hash",
            "artifact_hash_algorithm": "sha256",
            "source_tree_sha256": "0x-prefixed 32-byte non-placeholder SHA-256 source tree hash",
            "report_uri": "HTTPS URI for the public review report or source-controlled review commit; example, loopback, private, and reserved hosts are rejected",
            "request_handoff.bundle": "target/novaseal-external-evidence-handoff-bundle.json",
            "request_handoff.bundle_hash": "0x-prefixed 32-byte hash of the NovaSeal external evidence handoff bundle",
            "request_handoff.bundle_hash_algorithm": "blake2b-256(person=NovaExtHandoff)",
            "request_handoff.group": "external_bip340_tcb_review_attestation"
        },
        "verifier_id": template.get("verifier_id").cloned().unwrap_or(Value::Null),
        "ipc_abi": template.get("ipc_abi").cloned().unwrap_or(Value::Null),
        "expected_artifact_hash": runtime.get("artifact_hash").cloned().unwrap_or(Value::Null),
        "template_artifact_hash": template.get("artifact_hash").cloned().unwrap_or(Value::Null),
        "expected_artifact_hash_algorithm": runtime.get("artifact_hash_algorithm").cloned().unwrap_or(Value::Null),
        "template_artifact_hash_algorithm": template.get("artifact_hash_algorithm").cloned().unwrap_or(Value::Null),
        "expected_source_tree_sha256": source.get("source_tree_sha256").cloned().unwrap_or(Value::Null),
        "template_source_tree_sha256": template.get("source_tree_sha256").cloned().unwrap_or(Value::Null),
        "expected_review_scope": template.get("review_scope").cloned().unwrap_or(Value::Null),
        "required_status": "accepted"
    });
    let expected_scope = json!([
        "BIP340 verifier core",
        "RISC-V runtime verifier shell",
        "CellScript BIP340 IPC envelope",
        "artifact hash and CellDep pinning requirements"
    ]);
    let checks = json!({
        "template_schema_current": request["template_schema"] == "novaseal-bip340-external-tcb-review-attestation-v0.1",
        "template_status_accepted": template.get("status").and_then(Value::as_str) == Some("accepted"),
        "verifier_id_current": request["verifier_id"] == "btc.bip340.v0",
        "ipc_abi_current": request["ipc_abi"] == "cellscript-btc-bip340-ipc-v0",
        "artifact_hash_matches_tcb": present(&request["expected_artifact_hash"]) && request["template_artifact_hash"] == request["expected_artifact_hash"],
        "artifact_hash_algorithm_current": template.get("artifact_hash_algorithm").and_then(Value::as_str) == Some("sha256"),
        "artifact_hash_algorithm_matches_tcb": present(&request["expected_artifact_hash_algorithm"]) && request["template_artifact_hash_algorithm"] == request["expected_artifact_hash_algorithm"],
        "source_tree_hash_matches_tcb": present(&request["expected_source_tree_sha256"]) && request["template_source_tree_sha256"] == request["expected_source_tree_sha256"],
        "review_scope_exact": template.get("review_scope") == Some(&expected_scope),
        "required_fields_complete": request["required_public_fields"].as_array().is_some_and(|fields| fields.len() == 13)
    });
    let passed = checks.as_object().is_some_and(|map| map.values().all(|value| value == &Value::Bool(true)));
    Ok(
        json!({ "name": "external_bip340_tcb_review_attestation", "status": if passed { "passed" } else { "failed" }, "checks": checks, "request": request }),
    )
}

pub fn run(
    root: &Path,
    tcb_review: Option<&Path>,
    public_template: Option<&Path>,
    external_template: Option<&Path>,
    output: Option<&Path>,
    pretty: bool,
) -> Result<i32> {
    let default_tcb = root.join("target/novaseal-bip340-tcb-review.json");
    let default_public = root.join("proposals/novaseal/v0-mvp-skeleton/proofs/public_shared_cell_dep_attestation.template.json");
    let default_external = root.join("proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.template.json");
    let default_output = root.join("target/novaseal-external-attestation-adapter.json");
    let tcb = read_json(&python_path(tcb_review.unwrap_or(&default_tcb)))?;
    let public = read_json(&python_path(public_template.unwrap_or(&default_public)))?;
    let external = read_json(&python_path(external_template.unwrap_or(&default_external)))?;
    let cases = vec![public_case(&public, &tcb)?, external_case(&external, &tcb)?];
    let matched = cases.iter().filter(|case| case["status"] == "passed").count();
    let passed = matched == cases.len();
    let report = json!({
        "schema": "novaseal-external-attestation-adapter-v0.1",
        "status": if passed { "passed" } else { "failed" },
        "adapter_status": "request_ready_external_attestations_required",
        "source_tcb_review": "target/novaseal-bip340-tcb-review.json",
        "source_tcb_review_hash": hash("tcb_review", &tcb)?,
        "source_public_cell_dep_template": "proposals/novaseal/v0-mvp-skeleton/proofs/public_shared_cell_dep_attestation.template.json",
        "source_public_cell_dep_template_hash": hash("public_celldep_template", &public)?,
        "source_external_tcb_template": "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.template.json",
        "source_external_tcb_template_hash": hash("external_tcb_template", &external)?,
        "production_boundary": "This adapter proves the attestation request package is complete; it does not prove public CellDep deployment or independent external TCB review.",
        "summary": { "total": cases.len(), "matched": matched, "required_attestations": cases.iter().map(|case| case["name"].clone()).collect::<Vec<_>>() },
        "cases": cases
    });
    let output = python_path(output.unwrap_or(&default_output));
    fs::create_dir_all(output.parent().context("output path has no parent")?)?;
    fs::write(&output, format!("{}\n", python_json_pretty(&report)?))?;
    if pretty {
        println!(
            "wrote {} status={} attestations={}/{}",
            output.display(),
            report["status"].as_str().unwrap_or("failed"),
            matched,
            report["summary"]["total"]
        );
    }
    Ok(if passed { 0 } else { 1 })
}
