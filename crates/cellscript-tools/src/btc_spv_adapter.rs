//! Rust port of the NovaSeal public BTC SPV evidence adapter request.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::crypto::canonical_report_hash;
use crate::shared::{lexical_path, stable_json_pretty};

const PERSON: &[u8] = b"NovaBtcSpvReqV0";
const PROFILES: [&str; 3] = ["btc-transaction-commitment-profile-v0", "btc-utxo-seal-profile-v0", "dual-seal-profile-v0"];

fn scenario(profile: &str) -> &'static str {
    match profile {
        "btc-transaction-commitment-profile-v0" => "btc-transaction-commitment-transition",
        "btc-utxo-seal-profile-v0" => "btc-utxo-seal-closure",
        _ => "dual-seal-finality",
    }
}

fn production_anchor(profile: &str) -> &'static str {
    if profile == "btc-transaction-commitment-profile-v0" {
        "external_public_btc_transaction"
    } else {
        "external_public_btc_spend"
    }
}

fn hash(label: &str, value: &Value) -> Result<String> {
    canonical_report_hash(PERSON, label, value)
}

fn hex32(value: &Value) -> bool {
    value.as_str().is_some_and(|text| {
        text.len() == 66 && text.starts_with("0x") && text[2..].chars().all(|character| character.is_ascii_hexdigit())
    })
}

fn non_negative(value: &Value) -> bool {
    value.as_i64().is_some_and(|number| number >= 0) || value.as_u64().is_some()
}

fn positive(value: &Value) -> bool {
    value.as_i64().is_some_and(|number| number > 0) || value.as_u64().is_some_and(|number| number > 0)
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(false) => false,
        Value::String(text) => !text.is_empty(),
        Value::Array(values) => !values.is_empty(),
        Value::Object(values) => !values.is_empty(),
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::Bool(true) => true,
    }
}

pub(crate) fn required_fields() -> Value {
    json!([
        "network",
        "generated_at",
        "evidence_provider",
        "required_profiles",
        "profile",
        "scenario",
        "ckb_live_tx_hash",
        "live_report_hash",
        "service_builder_case_hash",
        "service_builder_tx_skeleton_hash",
        "service_builder_receipt_binding_hash",
        "ckb_btc_commitment_hash",
        "btc_txid",
        "btc_wtxid",
        "btc_tx_hex",
        "btc_block_hash",
        "btc_block_header",
        "btc_merkle_proof.tx_index",
        "btc_merkle_proof.merkle_branch",
        "btc_merkle_proof.merkle_root",
        "btc_merkle_proof.block_height",
        "btc_merkle_proof.observed_tip_height",
        "btc_transaction_binding.kind",
        "btc_transaction_binding.btc_output_index",
        "btc_transaction_binding.btc_amount_sats",
        "btc_transaction_binding.spend_input_index",
        "btc_transaction_binding.sealed_btc_txid",
        "btc_transaction_binding.sealed_btc_vout_index",
        "btc_transaction_binding.sealed_btc_amount_sats",
        "btc_transaction_binding.script_pubkey_hash",
        "btc_transaction_binding.sealed_btc_tx_hex",
        "btc_transaction_binding.sealed_utxo_commitment_hash",
        "spv_proof_hash",
        "minimum_confirmations",
        "confirmations",
        "spv_client_cell_dep.out_point",
        "spv_client_cell_dep.data_hash",
        "spv_client_cell_dep.dep_type",
        "spv_client_cell_dep.hash_type",
        "source_service.name",
        "source_service.commit",
        "source_service.report_hash",
        "request_handoff.bundle",
        "request_handoff.bundle_hash",
        "request_handoff.bundle_hash_algorithm",
        "request_handoff.group"
    ])
}

pub(crate) fn field_constraints() -> Value {
    json!({
        "network": "explicit public mainnet/testnet name; placeholders and local/devnet/regtest/simnet/private/fake labels are rejected",
        "generated_at": "UTC timestamp in YYYY-MM-DDTHH:MM:SSZ form; future timestamps are rejected",
        "evidence_provider": "real external provider identity; placeholder, first-party NovaSeal/CellScript/a19q3, local/devnet/fake/internal, example, and unknown tokens are rejected",
        "ckb_live_tx_hash": "0x-prefixed 32-byte CKB live transaction hash matching the current NovaSeal service-builder case",
        "live_report_hash": "0x-prefixed 32-byte hash of the current NovaSeal live devnet report for this profile",
        "service_builder_case_hash": "0x-prefixed 32-byte hash of the current NovaSeal service-builder case for this profile",
        "service_builder_tx_skeleton_hash": "0x-prefixed 32-byte service-builder transaction skeleton hash for this profile",
        "service_builder_receipt_binding_hash": "0x-prefixed 32-byte service-builder receipt binding hash for this profile",
        "ckb_btc_commitment_hash": "0x-prefixed 32-byte CKB-side BTC commitment hash from the current live profile report",
        "btc_txid": "0x-prefixed 32-byte non-placeholder Bitcoin transaction id",
        "btc_wtxid": "0x-prefixed 32-byte Bitcoin witness transaction id derived from btc_tx_hex",
        "btc_tx_hex": "0x-prefixed raw Bitcoin transaction bytes whose txid/wtxid match the public evidence case",
        "btc_block_hash": "0x-prefixed 32-byte non-placeholder Bitcoin block hash anchoring the SPV proof",
        "btc_block_header": "0x-prefixed 80-byte Bitcoin block header whose double-SHA256 hash matches btc_block_hash",
        "btc_merkle_proof.tx_index": "zero-based transaction index used to orient the Merkle branch",
        "btc_merkle_proof.merkle_branch": "array of 0x-prefixed 32-byte Bitcoin sibling hashes in display order; empty only for tx_index 0 in a single-transaction block",
        "btc_merkle_proof.merkle_root": "0x-prefixed 32-byte Bitcoin Merkle root matching the block header",
        "btc_merkle_proof.block_height": "public Bitcoin block height containing btc_txid",
        "btc_merkle_proof.observed_tip_height": "public Bitcoin tip height used to compute confirmations",
        "btc_transaction_binding.kind": "profile-specific binding kind: btc_transaction_output, btc_utxo_spend, or dual_seal_btc_closure",
        "btc_transaction_binding.btc_output_index": "BTC transaction commitment output index; required for btc-transaction-commitment-profile-v0",
        "btc_transaction_binding.btc_amount_sats": "BTC transaction commitment output amount in sats; required for btc-transaction-commitment-profile-v0",
        "btc_transaction_binding.spend_input_index": "Bitcoin spend input index; required for UTXO and dual-seal closure profiles",
        "btc_transaction_binding.sealed_btc_txid": "sealed Bitcoin transaction id whose output is spent; required for btc-utxo-seal-profile-v0 and dual-seal-profile-v0",
        "btc_transaction_binding.sealed_btc_vout_index": "sealed Bitcoin output index; required for btc-utxo-seal-profile-v0 and dual-seal-profile-v0",
        "btc_transaction_binding.sealed_btc_amount_sats": "sealed Bitcoin output amount in sats; required for btc-utxo-seal-profile-v0 and dual-seal-profile-v0",
        "btc_transaction_binding.script_pubkey_hash": "0x-prefixed CKB Blake2b-256 hash of the sealed output scriptPubKey bytes; required for btc-utxo-seal-profile-v0 and dual-seal-profile-v0",
        "btc_transaction_binding.sealed_btc_tx_hex": "0x-prefixed raw sealed Bitcoin transaction bytes; required for btc-utxo-seal-profile-v0 and dual-seal-profile-v0",
        "btc_transaction_binding.sealed_utxo_commitment_hash": "0x-prefixed 32-byte CKB-side sealed UTXO commitment hash; required for btc-utxo-seal-profile-v0 and dual-seal-profile-v0",
        "spv_proof_hash": "0x-prefixed SHA-256 hash of the canonical BTC SPV proof material carried in this case",
        "minimum_confirmations": "integer confirmation floor; at least 6",
        "confirmations": "integer observed confirmations meeting minimum_confirmations",
        "spv_client_cell_dep.out_point": "0x-prefixed 32-byte CKB transaction hash plus numeric output index",
        "spv_client_cell_dep.data_hash": "0x-prefixed 32-byte non-placeholder SPV client data hash",
        "spv_client_cell_dep.dep_type": "code",
        "spv_client_cell_dep.hash_type": "data, data1, or type CKB script hash type",
        "source_service.name": "real external SPV service identity; placeholder, first-party NovaSeal/CellScript/a19q3, local/devnet/fake/internal, example, and unknown tokens are rejected",
        "source_service.commit": "40-character hex service source commit",
        "source_service.report_hash": "0x-prefixed 32-byte non-placeholder SPV service report hash",
        "request_handoff.bundle": "target/novaseal-external-evidence-handoff-bundle.json",
        "request_handoff.bundle_hash": "0x-prefixed 32-byte hash of the NovaSeal external evidence handoff bundle",
        "request_handoff.bundle_hash_algorithm": "blake2b-256(person=NovaExtHandoff)",
        "request_handoff.group": "public_btc_spv_evidence"
    })
}

fn find_profile<'a>(cases: Option<&'a Vec<Value>>, profile: &str) -> Option<&'a Value> {
    cases?.iter().find(|case| case.get("profile").and_then(Value::as_str) == Some(profile))
}

fn profile_cases(service: &Value, template: &Value) -> Result<Vec<Value>> {
    let builder_cases = service.get("cases").and_then(Value::as_array);
    let template_cases = template.get("cases").and_then(Value::as_array);
    let mut cases = Vec::new();
    for profile in PROFILES {
        let builder = find_profile(builder_cases, profile);
        let template_case = find_profile(template_cases, profile);
        let external_inputs = builder
            .and_then(|case| case.pointer("/request/production_external_inputs"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let live_inputs = builder.and_then(|case| case.pointer("/request/required_live_inputs")).cloned().unwrap_or_else(|| json!({}));
        let anchor = live_inputs.get("public_btc_anchor").filter(|value| value.is_object()).cloned().unwrap_or_else(|| json!({}));
        let builder_value = builder.cloned().unwrap_or(Value::Null);
        let template_value = template_case.cloned().unwrap_or(Value::Null);
        let request = json!({
            "profile": profile,
            "scenario": template_case.and_then(|case| case.get("scenario")).cloned().unwrap_or(Value::Null),
            "minimum_confirmations": template_case.and_then(|case| case.get("minimum_confirmations")).cloned().unwrap_or(Value::from(6)),
            "required_public_fields": required_fields(),
            "field_constraints": field_constraints(),
            "required_external_inputs": external_inputs,
            "ckb_live_tx_hash": live_inputs.get("live_devnet_tx_hash").cloned().unwrap_or(Value::Null),
            "live_report_hash": live_inputs.get("live_report_hash").cloned().unwrap_or(Value::Null),
            "service_builder_case_hash": hash("service_builder_case", &builder_value)?,
            "service_builder_tx_skeleton_hash": builder.and_then(|case| case.pointer("/response/tx_skeleton_hash")).cloned().unwrap_or(Value::Null),
            "service_builder_receipt_binding_hash": builder.and_then(|case| case.pointer("/response/receipt_binding_hash")).cloned().unwrap_or(Value::Null),
            "local_anchor_source": anchor.get("anchor_source").cloned().unwrap_or(Value::Null),
            "expected_anchor_source": production_anchor(profile),
            "ckb_btc_commitment_hash": anchor.get("ckb_btc_commitment_hash").cloned().unwrap_or(Value::Null),
            "expected_btc_txid": anchor.get("btc_txid").cloned().unwrap_or(Value::Null),
            "expected_btc_wtxid": anchor.get("btc_wtxid").cloned().unwrap_or(Value::Null),
            "expected_btc_output_index": anchor.get("btc_output_index").cloned().unwrap_or(Value::Null),
            "expected_btc_amount_sats": anchor.get("btc_amount_sats").cloned().unwrap_or(Value::Null),
            "expected_sealed_btc_txid": anchor.get("sealed_btc_txid").cloned().unwrap_or(Value::Null),
            "expected_sealed_btc_vout_index": anchor.get("sealed_btc_vout_index").cloned().unwrap_or(Value::Null),
            "expected_sealed_btc_amount_sats": anchor.get("sealed_btc_amount_sats").cloned().unwrap_or(Value::Null),
            "expected_script_pubkey_hash": anchor.get("script_pubkey_hash").cloned().unwrap_or(Value::Null),
            "expected_spend_input_index": anchor.get("spend_input_index").cloned().unwrap_or(Value::Null),
            "expected_sealed_utxo_commitment_hash": anchor.get("sealed_utxo_commitment_hash").cloned().unwrap_or(Value::Null),
            "template_case_hash": hash("template_case", &template_value)?
        });
        let transaction = profile == PROFILES[0];
        let utxo = profile == PROFILES[1];
        let dual = profile == PROFILES[2];
        let utxo_fields = hex32(&request["expected_sealed_btc_txid"])
            && non_negative(&request["expected_sealed_btc_vout_index"])
            && positive(&request["expected_sealed_btc_amount_sats"])
            && hex32(&request["expected_script_pubkey_hash"])
            && non_negative(&request["expected_spend_input_index"])
            && hex32(&request["expected_sealed_utxo_commitment_hash"]);
        let checks = json!({
            "service_builder_case_present": builder.is_some(),
            "template_case_present": template_case.is_some(),
            "scenario_matches_required_profile": request["scenario"] == scenario(profile),
            "public_btc_spv_external_input_named": request["required_external_inputs"].as_array().is_some_and(|items| items.iter().any(|item| item == "public_btc_spv_evidence")),
            "minimum_confirmations_at_least_six": non_negative(&request["minimum_confirmations"]) && request["minimum_confirmations"].as_u64().unwrap_or(0) >= 6,
            "live_binding_hashes_present": hex32(&request["ckb_live_tx_hash"]) && hex32(&request["live_report_hash"]),
            "service_builder_hashes_present": hex32(&request["service_builder_tx_skeleton_hash"]) && hex32(&request["service_builder_receipt_binding_hash"]),
            "expected_anchor_source_production_eligible": request["expected_anchor_source"] == production_anchor(profile),
            "local_anchor_source_present": truthy(&request["local_anchor_source"]),
            "ckb_btc_commitment_hash_present": hex32(&request["ckb_btc_commitment_hash"]),
            "expected_btc_txid_present": hex32(&request["expected_btc_txid"]),
            "expected_btc_wtxid_present": hex32(&request["expected_btc_wtxid"]),
            "expected_output_fields_present": !transaction || (non_negative(&request["expected_btc_output_index"]) && positive(&request["expected_btc_amount_sats"])),
            "expected_utxo_fields_present": !utxo || utxo_fields,
            "expected_dual_sealed_utxo_fields_present": !dual || utxo_fields,
            "required_public_fields_complete": request["required_public_fields"].as_array().is_some_and(|fields| fields.len() == 46)
        });
        let passed = checks.as_object().is_some_and(|map| map.values().all(|value| value == &Value::Bool(true)));
        cases.push(
            json!({ "profile": profile, "status": if passed { "passed" } else { "failed" }, "checks": checks, "request": request }),
        );
    }
    Ok(cases)
}

pub fn run(root: &Path, service_builder: Option<&Path>, template: Option<&Path>, output: Option<&Path>, pretty: bool) -> Result<i32> {
    let default_service = root.join("target/novaseal-service-builder-fixtures.json");
    let default_template = root.join("proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.template.json");
    let default_output = root.join("target/novaseal-btc-spv-evidence-adapter.json");
    let service = serde_json::from_slice::<Value>(&fs::read(lexical_path(service_builder.unwrap_or(&default_service)))?)?;
    let template = serde_json::from_slice::<Value>(&fs::read(lexical_path(template.unwrap_or(&default_template)))?)?;
    let cases = profile_cases(&service, &template)?;
    let matched = cases.iter().filter(|case| case["status"] == "passed").count();
    let passed = matched == cases.len();
    let report = json!({
        "schema": "novaseal-btc-spv-evidence-adapter-v0.1",
        "status": if passed { "passed" } else { "failed" },
        "adapter_status": "request_ready_external_evidence_required",
        "source_service_builder_report": "target/novaseal-service-builder-fixtures.json",
        "source_service_builder_report_hash": hash("service_builder_report", &service)?,
        "source_public_btc_spv_template": "proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.template.json",
        "source_public_btc_spv_template_hash": hash("public_btc_spv_template", &template)?,
        "production_output": "proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.json",
        "production_boundary": "This adapter proves the request contract is complete; it does not prove BTC inclusion, spend validity, confirmation depth, or public SPV client deployment.",
        "summary": { "total": cases.len(), "matched": matched, "required_profiles": PROFILES },
        "cases": cases
    });
    let output = lexical_path(output.unwrap_or(&default_output));
    fs::create_dir_all(output.parent().context("output path has no parent")?)?;
    fs::write(&output, format!("{}\n", stable_json_pretty(&report)?))?;
    if pretty {
        println!(
            "wrote {} status={} profiles={}/{}",
            output.display(),
            report["status"].as_str().unwrap_or("failed"),
            matched,
            report["summary"]["total"]
        );
    }
    Ok(if passed { 0 } else { 1 })
}
