//! NovaSeal profile-operator fixture generator.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::btc_anchor::public_btc_anchor_shape_matches_profile;
use crate::crypto::{canonical_report_hash, ckb_blake2b256, hex0x, sha256_hex};
use crate::shared::{lexical_path, stable_json_compact, stable_json_pretty};

const REPORT_PERSON: &[u8] = b"NovaProfileFxV0";
const PACKED_DOMAIN: &[u8] = b"NovaSealProfileOperatorFixtureV0\0";

#[derive(Clone, Copy)]
struct ActionCase {
    action: &'static str,
    fixture: &'static str,
    signers: &'static [&'static str],
    tx_pointer: Option<&'static str>,
}

#[derive(Clone, Copy)]
struct ProfileCase {
    profile: &'static str,
    root: &'static str,
    signed_type: &'static str,
    live_report: Option<&'static str>,
    public_btc_anchor: Option<&'static str>,
    fiber_report: Option<&'static str>,
    external_boundary: Option<&'static str>,
    cases: &'static [ActionCase],
}

const FUNGIBLE_CASES: &[ActionCase] = &[
    ActionCase { action: "issue_xudt", fixture: "issue_valid.json", signers: &["issuer"], tx_pointer: Some("/issue/commit/tx_hash") },
    ActionCase {
        action: "transfer_xudt",
        fixture: "transfer_valid.json",
        signers: &["holder"],
        tx_pointer: Some("/transfer/commit/tx_hash"),
    },
    ActionCase {
        action: "settle_xudt",
        fixture: "settle_valid.json",
        signers: &["holder"],
        tx_pointer: Some("/settle/commit/tx_hash"),
    },
];

const RWA_CASES: &[ActionCase] = &[
    ActionCase {
        action: "materialize_rwa_receipt",
        fixture: "materialize_valid.json",
        signers: &["issuer"],
        tx_pointer: Some("/materialize/commit/tx_hash"),
    },
    ActionCase {
        action: "claim_rwa_receipt",
        fixture: "claim_valid.json",
        signers: &["holder"],
        tx_pointer: Some("/claim/commit/tx_hash"),
    },
    ActionCase {
        action: "settle_rwa_receipt",
        fixture: "settle_valid.json",
        signers: &["issuer", "holder"],
        tx_pointer: Some("/settle/commit/tx_hash"),
    },
];

const BTC_TRANSACTION_CASES: &[ActionCase] = &[ActionCase {
    action: "commit_btc_transaction_transition",
    fixture: "commit_transaction_valid.json",
    signers: &["committer"],
    tx_pointer: Some("/commit_transaction/commit/tx_hash"),
}];

const BTC_UTXO_CASES: &[ActionCase] = &[ActionCase {
    action: "close_btc_utxo_seal",
    fixture: "close_utxo_seal_valid.json",
    signers: &["owner"],
    tx_pointer: Some("/close_utxo_seal/commit/tx_hash"),
}];

const DUAL_SEAL_CASES: &[ActionCase] = &[ActionCase {
    action: "finalize_dual_seal",
    fixture: "finalize_dual_seal_valid.json",
    signers: &["btc_owner", "ckb_authority"],
    tx_pointer: Some("/finalize_dual_seal/commit/tx_hash"),
}];

const FIBER_CASES: &[ActionCase] = &[ActionCase {
    action: "settle_fiber_candidate",
    fixture: "settle_fiber_candidate_valid.json",
    signers: &["operator"],
    tx_pointer: Some("/settle_fiber_candidate/commit/tx_hash"),
}];

const PROFILE_CASES: &[ProfileCase] = &[
    ProfileCase {
        profile: "fungible-xudt-profile-v0",
        root: "proposals/novaseal/fungible-xudt-profile-v0",
        signed_type: "NovaFungibleXudtSignedIntentV0",
        live_report: Some("target/novaseal-fungible-xudt-devnet-stateful-live.json"),
        public_btc_anchor: None,
        fiber_report: None,
        external_boundary: None,
        cases: FUNGIBLE_CASES,
    },
    ProfileCase {
        profile: "rwa-receipt-profile-v0",
        root: "proposals/novaseal/rwa-receipt-profile-v0",
        signed_type: "NovaRwaReceiptSignedIntentV0",
        live_report: Some("target/novaseal-rwa-receipt-devnet-stateful-live.json"),
        public_btc_anchor: None,
        fiber_report: None,
        external_boundary: None,
        cases: RWA_CASES,
    },
    ProfileCase {
        profile: "btc-transaction-commitment-profile-v0",
        root: "proposals/novaseal/btc-transaction-commitment-profile-v0",
        signed_type: "NovaBtcTransactionCommitmentSignedIntentV0",
        live_report: Some("target/novaseal-btc-transaction-commitment-devnet-stateful-live.json"),
        public_btc_anchor: Some("/commit_transaction/public_btc_anchor"),
        fiber_report: None,
        external_boundary: None,
        cases: BTC_TRANSACTION_CASES,
    },
    ProfileCase {
        profile: "btc-utxo-seal-profile-v0",
        root: "proposals/novaseal/btc-utxo-seal-profile-v0",
        signed_type: "NovaBtcUtxoSealSignedIntentV0",
        live_report: Some("target/novaseal-btc-utxo-seal-devnet-stateful-live.json"),
        public_btc_anchor: Some("/close_utxo_seal/public_btc_anchor"),
        fiber_report: None,
        external_boundary: None,
        cases: BTC_UTXO_CASES,
    },
    ProfileCase {
        profile: "dual-seal-profile-v0",
        root: "proposals/novaseal/dual-seal-profile-v0",
        signed_type: "NovaDualSealSignedIntentV0",
        live_report: Some("target/novaseal-dual-seal-devnet-stateful-live.json"),
        public_btc_anchor: Some("/finalize_dual_seal/public_btc_anchor"),
        fiber_report: None,
        external_boundary: None,
        cases: DUAL_SEAL_CASES,
    },
    ProfileCase {
        profile: "fiber-candidate-profile-v0",
        root: "proposals/novaseal/fiber-candidate-profile-v0",
        signed_type: "NovaFiberCandidateSignedIntentV0",
        live_report: Some("target/novaseal-fiber-candidate-devnet-stateful-live.json"),
        public_btc_anchor: None,
        fiber_report: Some("target/novaseal-fiber-node-experiments.json"),
        external_boundary: None,
        cases: FIBER_CASES,
    },
];

fn report_hash(label: &str, value: &Value) -> Result<String> {
    canonical_report_hash(REPORT_PERSON, label, value)
}

fn read_json(path: &Path) -> Result<Value> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("failed to read {}", path.display()))?)
        .with_context(|| format!("{} is not valid JSON", path.display()))
}

fn json_file_hash(path: &Path) -> Result<String> {
    let label = path.file_name().and_then(|name| name.to_str()).context("JSON file name is not UTF-8")?;
    report_hash(label, &read_json(path)?)
}

fn matching_files(directory: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory).with_context(|| format!("failed to read {}", directory.display()))? {
        let candidate = entry?.path();
        if candidate.extension().and_then(|value| value.to_str()) == Some(extension) {
            files.push(candidate);
        }
    }
    files.sort();
    Ok(files)
}

fn file_set_hash(root: &Path, paths: &[PathBuf]) -> Result<String> {
    let mut entries = Vec::new();
    for candidate in paths {
        if candidate.is_symlink() || !candidate.is_file() {
            continue;
        }
        let relative =
            candidate.strip_prefix(root).with_context(|| format!("{} is outside {}", candidate.display(), root.display()))?;
        entries.push(json!({
            "path": relative.to_string_lossy(),
            "sha256": sha256_hex(&fs::read(candidate).with_context(|| format!("failed to read {}", candidate.display()))?),
        }));
    }
    report_hash("file_set", &Value::Array(entries))
}

fn packed_hash(type_name: &str, packed: &[u8]) -> Result<(String, String)> {
    let length = u32::try_from(packed.len()).context("packed operator fixture exceeds u32")?;
    let mut preimage = Vec::with_capacity(PACKED_DOMAIN.len() + type_name.len() + 1 + 4 + packed.len());
    preimage.extend_from_slice(PACKED_DOMAIN);
    preimage.extend_from_slice(type_name.as_bytes());
    preimage.push(0);
    preimage.extend_from_slice(&length.to_le_bytes());
    preimage.extend_from_slice(packed);
    Ok((hex0x(&preimage), hex0x(&ckb_blake2b256(&preimage)?)))
}

fn json_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|number| number != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

fn optional_json(root: &Path, relative: Option<&str>) -> Result<Option<Value>> {
    let Some(relative) = relative else {
        return Ok(None);
    };
    let candidate = root.join(relative);
    if candidate.is_file() {
        Ok(Some(read_json(&candidate)?))
    } else {
        Ok(None)
    }
}

fn pointer(value: Option<&Value>, pointer: Option<&str>) -> Value {
    value.zip(pointer).and_then(|(value, pointer)| value.pointer(pointer)).cloned().unwrap_or(Value::Null)
}

fn build_case(root: &Path, profile: &ProfileCase, action_case: &ActionCase) -> Result<Value> {
    let profile_root = root.join(profile.root);
    let fixture_path = profile_root.join("fixtures").join(action_case.fixture);
    let fixture = read_json(&fixture_path)?;
    let source_hash = file_set_hash(root, &matching_files(&profile_root.join("src"), "cell")?)?;
    let schema_hash = file_set_hash(root, &matching_files(&profile_root.join("schemas"), "schema")?)?;
    let proof_hash = json_file_hash(&profile_root.join("proofs/invariant_matrix.json"))?;
    let live_report = optional_json(root, profile.live_report)?;
    let fiber_report = optional_json(root, profile.fiber_report)?;
    let live_tx_hash = pointer(live_report.as_ref(), action_case.tx_pointer);
    let public_btc_anchor = pointer(live_report.as_ref(), profile.public_btc_anchor);
    let public_btc_required =
        matches!(profile.profile, "btc-transaction-commitment-profile-v0" | "btc-utxo-seal-profile-v0" | "dual-seal-profile-v0");
    let signers = action_case.signers;
    let display = json!({
        "profile": profile.profile,
        "action": action_case.action,
        "fixture": action_case.fixture,
        "fixture_description": fixture.get("description").cloned().unwrap_or(Value::Null),
        "signers": signers,
        "signed_type": profile.signed_type,
        "source_tree_hash": source_hash,
        "schema_set_hash": schema_hash,
        "proof_matrix_hash": proof_hash,
        "live_devnet_tx_hash": live_tx_hash,
        "public_btc_anchor": public_btc_anchor,
        "external_boundary": profile.external_boundary,
    });
    let signature_witnesses: Vec<String> = signers.iter().map(|signer| format!("{signer}_sig")).collect();
    let witness_shape = json!({
        "signed_intent": profile.signed_type,
        "signature_witnesses": signature_witnesses,
        "fixture_expected": fixture.get("expected").cloned().unwrap_or(Value::Null),
        "live_report": profile.live_report,
        "fiber_report": profile.fiber_report,
    });
    let live_report_hash = match (&live_report, profile.live_report) {
        (Some(report), Some(label)) => Value::String(report_hash(label, report)?),
        _ => Value::Null,
    };
    let fiber_report_hash = match (&fiber_report, profile.fiber_report) {
        (Some(report), Some(label)) => Value::String(report_hash(label, report)?),
        _ => Value::Null,
    };
    let intent_body = json!({
        "schema": "novaseal-profile-operator-intent-v0.1",
        "profile": profile.profile,
        "action": action_case.action,
        "fixture": action_case.fixture,
        "fixture_hash": json_file_hash(&fixture_path)?,
        "source_tree_hash": source_hash,
        "schema_set_hash": schema_hash,
        "proof_matrix_hash": proof_hash,
        "signers": signers,
        "witness_shape_hash": report_hash("witness_shape", &witness_shape)?,
        "live_report_hash": live_report_hash,
        "fiber_report_hash": fiber_report_hash,
        "live_tx_hash": live_tx_hash,
        "public_btc_anchor": public_btc_anchor,
        "external_boundary": profile.external_boundary,
    });
    let packed = stable_json_compact(&intent_body)?.into_bytes();
    let (preimage, digest) = packed_hash(profile.signed_type, &packed)?;
    let tx_skeleton = json!({
        "profile": profile.profile,
        "action": action_case.action,
        "fixture": action_case.fixture,
        "live_tx_hash": live_tx_hash,
        "source_tree_hash": source_hash,
        "witness_shape_hash": intent_body["witness_shape_hash"],
        "public_btc_anchor": public_btc_anchor,
    });
    let fixture_expected = fixture.get("expected").and_then(Value::as_str) == Some("accepted");
    let fixture_action = fixture.get("action").and_then(Value::as_str) == Some(action_case.action);
    let live_passed = live_report.as_ref().and_then(|report| report.get("status")).and_then(Value::as_str) == Some("passed")
        || profile.external_boundary == Some("package_fixture_only_external_btc_and_ckb_finality_required");
    let fiber_passed = fiber_report.as_ref().is_none_or(|report| {
        !json_truthy(report) || report.pointer("/workflow_coverage/all_required_workflows_executed_passed") == Some(&Value::Bool(true))
    });
    let anchor_present = !public_btc_required || json_truthy(&public_btc_anchor);
    let anchor_shape = !public_btc_required || public_btc_anchor_shape_matches_profile(profile.profile, Some(&public_btc_anchor));
    let checks = json!({
        "fixture_expected_accepted": fixture_expected,
        "fixture_action_matches": fixture_action,
        "live_status_passed_or_external_boundary": live_passed,
        "fiber_execution_passed_when_required": fiber_passed,
        "public_btc_anchor_present_when_required": anchor_present,
        "public_btc_anchor_shape_matches_profile": anchor_shape,
    });
    let passed = checks.as_object().context("operator checks are not an object")?.values().all(|check| check == &Value::Bool(true));
    Ok(json!({
        "profile": profile.profile,
        "action": action_case.action,
        "fixture": action_case.fixture,
        "status": if passed { "passed" } else { "failed" },
        "checks": checks,
        "signers": signers,
        "signed_type": profile.signed_type,
        "signed_intent_hash": digest,
        "signed_intent_hash_preimage_hex": preimage,
        "signed_intent_body_hex": hex0x(&packed),
        "bip340_message_hash": digest,
        "witness_shape_hash": intent_body["witness_shape_hash"],
        "tx_skeleton_hash": report_hash("tx_skeleton", &tx_skeleton)?,
        "fixture_hash": intent_body["fixture_hash"],
        "source_tree_hash": source_hash,
        "schema_set_hash": schema_hash,
        "proof_matrix_hash": proof_hash,
        "live_report_hash": intent_body["live_report_hash"],
        "fiber_report_hash": intent_body["fiber_report_hash"],
        "live_devnet_tx_hash": live_tx_hash,
        "public_btc_anchor": public_btc_anchor,
        "wallet_display": display,
        "operator_witness_shape": witness_shape,
    }))
}

fn build_report(root: &Path) -> Result<Value> {
    let mut cases = Vec::new();
    for profile in PROFILE_CASES {
        for action_case in profile.cases {
            cases.push(build_case(root, profile, action_case)?);
        }
    }
    let profiles: BTreeSet<&str> = cases.iter().filter_map(|case| case["profile"].as_str()).collect();
    let matched = cases.iter().filter(|case| case["status"] == "passed").count();
    let passed = !cases.is_empty() && matched == cases.len();
    Ok(json!({
        "schema": "novaseal-profile-operator-fixtures-v0.1",
        "status": if passed { "passed" } else { "failed" },
        "hash_algorithm": "ckb_blake2b_256",
        "signature_scheme": "BIP340 Schnorr over 32-byte signed profile intent hash",
        "fixture_boundary": "wallet/service fixtures bind declared profile actions to source, schema, invariant, witness, and live-report evidence; external BTC/CellDep/TCB attestations remain separate production gates",
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

pub fn run(root: &Path, output: Option<&Path>, pretty: bool) -> Result<i32> {
    let default_output = root.join("target/novaseal-profile-operator-fixtures.json");
    let output = lexical_path(output.unwrap_or(&default_output));
    let report = build_report(root)?;
    let parent = output.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    fs::write(&output, format!("{}\n", stable_json_pretty(&report)?))
        .with_context(|| format!("failed to write {}", output.display()))?;
    if pretty {
        println!(
            "wrote {} status={} profiles={} cases={}",
            output.display(),
            report["status"].as_str().unwrap_or("failed"),
            report["summary"]["profile_count"].as_u64().unwrap_or(0),
            report["summary"]["total"].as_u64().unwrap_or(0),
        );
    }
    Ok(if report["status"] == "passed" { 0 } else { 1 })
}
