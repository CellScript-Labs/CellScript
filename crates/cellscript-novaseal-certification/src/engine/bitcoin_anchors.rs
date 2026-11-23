use super::{
    exact_object_keys, is_hex32, is_real_tx_hash, json_pointer_str, json_pointer_u64, normalize_hex, placeholder_hash,
    EXPECTED_BTC_SPV_EVIDENCE_PROFILES, EXPECTED_BTC_SPV_PROFILE_SCENARIOS, EXPECTED_BTC_TX_COMMITMENT_PROFILE,
    EXPECTED_BTC_UTXO_SEAL_PROFILE, EXPECTED_DUAL_SEAL_PROFILE,
};
use serde_json::Value;

pub(super) fn expected_btc_spv_scenario(profile: &str) -> Option<&'static str> {
    EXPECTED_BTC_SPV_PROFILE_SCENARIOS
        .iter()
        .find_map(|(expected_profile, scenario)| (*expected_profile == profile).then_some(*scenario))
}

pub(super) fn expected_public_btc_anchor_pointer(profile: &str) -> Option<&'static str> {
    match profile {
        EXPECTED_BTC_TX_COMMITMENT_PROFILE => Some("/commit_transaction/public_btc_anchor"),
        EXPECTED_BTC_UTXO_SEAL_PROFILE => Some("/close_utxo_seal/public_btc_anchor"),
        EXPECTED_DUAL_SEAL_PROFILE => Some("/finalize_dual_seal/public_btc_anchor"),
        _ => None,
    }
}

pub(super) fn expected_public_btc_commitment_hash_pointer(profile: &str) -> Option<&'static str> {
    match profile {
        EXPECTED_BTC_TX_COMMITMENT_PROFILE => Some("/commit_transaction/btc_tx_commitment_hash"),
        EXPECTED_BTC_UTXO_SEAL_PROFILE => Some("/close_utxo_seal/closure_commitment_hash"),
        EXPECTED_DUAL_SEAL_PROFILE => Some("/finalize_dual_seal/btc_closure_commitment_hash"),
        _ => None,
    }
}

pub(super) fn public_btc_anchor_shape_matches_profile(profile: &str, anchor: Option<&Value>) -> bool {
    let Some(anchor) = anchor else {
        return false;
    };
    match profile {
        EXPECTED_BTC_TX_COMMITMENT_PROFILE => {
            exact_object_keys(
                anchor,
                &["kind", "anchor_source", "btc_txid", "btc_wtxid", "btc_output_index", "btc_amount_sats", "ckb_btc_commitment_hash"],
            ) && json_pointer_str(anchor, "/kind") == Some("btc_transaction_commitment")
                && json_pointer_str(anchor, "/anchor_source").is_some_and(|source| btc_anchor_source_matches_profile(profile, source))
                && json_pointer_str(anchor, "/btc_txid").is_some_and(is_real_tx_hash)
                && json_pointer_str(anchor, "/btc_wtxid").is_some_and(is_real_tx_hash)
                && json_pointer_u64(anchor, "/btc_output_index").is_some()
                && json_pointer_u64(anchor, "/btc_amount_sats").is_some_and(|amount| amount > 0)
                && json_pointer_str(anchor, "/ckb_btc_commitment_hash").is_some_and(is_real_tx_hash)
        }
        EXPECTED_BTC_UTXO_SEAL_PROFILE => {
            exact_object_keys(
                anchor,
                &[
                    "kind",
                    "anchor_source",
                    "sealed_btc_txid",
                    "sealed_btc_vout_index",
                    "sealed_btc_amount_sats",
                    "script_pubkey_hash",
                    "btc_txid",
                    "btc_wtxid",
                    "spend_input_index",
                    "ckb_btc_commitment_hash",
                    "sealed_utxo_commitment_hash",
                ],
            ) && json_pointer_str(anchor, "/kind") == Some("btc_utxo_spend")
                && json_pointer_str(anchor, "/anchor_source").is_some_and(|source| btc_anchor_source_matches_profile(profile, source))
                && json_pointer_str(anchor, "/sealed_btc_txid").is_some_and(is_real_tx_hash)
                && json_pointer_u64(anchor, "/sealed_btc_vout_index").is_some()
                && json_pointer_u64(anchor, "/sealed_btc_amount_sats").is_some_and(|amount| amount > 0)
                && json_pointer_str(anchor, "/script_pubkey_hash").is_some_and(is_real_tx_hash)
                && json_pointer_str(anchor, "/btc_txid").is_some_and(is_real_tx_hash)
                && json_pointer_str(anchor, "/btc_wtxid").is_some_and(is_real_tx_hash)
                && json_pointer_u64(anchor, "/spend_input_index").is_some()
                && json_pointer_str(anchor, "/ckb_btc_commitment_hash").is_some_and(is_real_tx_hash)
                && json_pointer_str(anchor, "/sealed_utxo_commitment_hash").is_some_and(is_real_tx_hash)
        }
        EXPECTED_DUAL_SEAL_PROFILE => {
            exact_object_keys(
                anchor,
                &[
                    "kind",
                    "anchor_source",
                    "sealed_btc_txid",
                    "sealed_btc_vout_index",
                    "sealed_btc_amount_sats",
                    "script_pubkey_hash",
                    "btc_txid",
                    "btc_wtxid",
                    "spend_input_index",
                    "ckb_btc_commitment_hash",
                    "sealed_utxo_commitment_hash",
                ],
            ) && json_pointer_str(anchor, "/kind") == Some("dual_seal_btc_closure")
                && json_pointer_str(anchor, "/anchor_source").is_some_and(|source| btc_anchor_source_matches_profile(profile, source))
                && json_pointer_str(anchor, "/sealed_btc_txid").is_some_and(is_real_tx_hash)
                && json_pointer_u64(anchor, "/sealed_btc_vout_index").is_some()
                && json_pointer_u64(anchor, "/sealed_btc_amount_sats").is_some_and(|amount| amount > 0)
                && json_pointer_str(anchor, "/script_pubkey_hash").is_some_and(is_real_tx_hash)
                && json_pointer_str(anchor, "/btc_txid").is_some_and(is_real_tx_hash)
                && json_pointer_str(anchor, "/btc_wtxid").is_some_and(is_real_tx_hash)
                && json_pointer_u64(anchor, "/spend_input_index").is_some()
                && json_pointer_str(anchor, "/ckb_btc_commitment_hash").is_some_and(is_real_tx_hash)
                && json_pointer_str(anchor, "/sealed_utxo_commitment_hash").is_some_and(is_real_tx_hash)
        }
        _ => false,
    }
}

fn btc_anchor_source_matches_profile(profile: &str, source: &str) -> bool {
    match profile {
        EXPECTED_BTC_TX_COMMITMENT_PROFILE => matches!(source, "local_deterministic_fixture" | "external_public_btc_transaction"),
        EXPECTED_BTC_UTXO_SEAL_PROFILE | EXPECTED_DUAL_SEAL_PROFILE => {
            matches!(source, "local_deterministic_fixture" | "external_public_btc_spend")
        }
        _ => false,
    }
}

pub(super) fn btc_anchor_source_production_eligible(profile: &str, source: &str) -> bool {
    match profile {
        EXPECTED_BTC_TX_COMMITMENT_PROFILE => source == "external_public_btc_transaction",
        EXPECTED_BTC_UTXO_SEAL_PROFILE | EXPECTED_DUAL_SEAL_PROFILE => source == "external_public_btc_spend",
        _ => false,
    }
}

pub(super) fn handoff_expected_bindings_exact(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    if object.len() != EXPECTED_BTC_SPV_EVIDENCE_PROFILES.len() {
        return false;
    }
    let expected_hash_fields = [
        "ckb_live_tx_hash",
        "live_report_hash",
        "service_builder_case_hash",
        "service_builder_tx_skeleton_hash",
        "service_builder_receipt_binding_hash",
        "ckb_btc_commitment_hash",
    ];
    EXPECTED_BTC_SPV_EVIDENCE_PROFILES.iter().all(|profile| {
        object.get(*profile).is_some_and(|binding| {
            let profile_fields: &[&str] = match *profile {
                EXPECTED_BTC_TX_COMMITMENT_PROFILE => {
                    &["anchor_source", "btc_txid", "btc_wtxid", "btc_output_index", "btc_amount_sats"]
                }
                EXPECTED_BTC_UTXO_SEAL_PROFILE => &[
                    "anchor_source",
                    "btc_txid",
                    "btc_wtxid",
                    "spend_input_index",
                    "sealed_btc_txid",
                    "sealed_btc_vout_index",
                    "sealed_btc_amount_sats",
                    "script_pubkey_hash",
                    "sealed_utxo_commitment_hash",
                ],
                EXPECTED_DUAL_SEAL_PROFILE => &[
                    "anchor_source",
                    "btc_txid",
                    "btc_wtxid",
                    "spend_input_index",
                    "sealed_btc_txid",
                    "sealed_btc_vout_index",
                    "sealed_btc_amount_sats",
                    "script_pubkey_hash",
                    "sealed_utxo_commitment_hash",
                ],
                _ => return false,
            };
            let allowed_fields = expected_hash_fields.iter().chain(profile_fields.iter()).copied().collect::<Vec<_>>();
            exact_object_keys(binding, &allowed_fields)
                && expected_hash_fields.iter().all(|field| {
                    normalize_hex(json_pointer_str(binding, &format!("/{field}"))).as_deref().is_some_and(is_hex32)
                        && !placeholder_hash(normalize_hex(json_pointer_str(binding, &format!("/{field}"))).as_deref())
                })
                && profile_fields.iter().all(|field| btc_binding_expected_field_valid(profile, binding, field))
        })
    })
}

fn btc_binding_expected_field_valid(profile: &str, binding: &Value, field: &str) -> bool {
    match field {
        "anchor_source" => {
            json_pointer_str(binding, "/anchor_source").is_some_and(|source| btc_anchor_source_production_eligible(profile, source))
        }
        "btc_txid" | "btc_wtxid" | "sealed_btc_txid" | "script_pubkey_hash" | "sealed_utxo_commitment_hash" => {
            normalize_hex(json_pointer_str(binding, &format!("/{field}"))).as_deref().is_some_and(is_hex32)
                && !placeholder_hash(normalize_hex(json_pointer_str(binding, &format!("/{field}"))).as_deref())
        }
        "btc_output_index" | "spend_input_index" | "sealed_btc_vout_index" => {
            json_pointer_u64(binding, &format!("/{field}")).is_some()
        }
        "btc_amount_sats" | "sealed_btc_amount_sats" => {
            json_pointer_u64(binding, &format!("/{field}")).is_some_and(|amount| amount > 0)
        }
        _ => false,
    }
}
