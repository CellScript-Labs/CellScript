//! Shared NovaSeal BTC public-anchor shape checks.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::crypto::nonzero_hex32;

fn exact_keys(value: &Map<String, Value>, keys: &[&str]) -> bool {
    value.keys().map(String::as_str).collect::<BTreeSet<_>>() == keys.iter().copied().collect::<BTreeSet<_>>()
}

fn non_negative_integer(value: Option<&Value>) -> bool {
    value.and_then(Value::as_i64).is_some_and(|number| number >= 0) || value.and_then(Value::as_u64).is_some()
}

fn positive_integer(value: Option<&Value>) -> bool {
    value.and_then(Value::as_i64).is_some_and(|number| number > 0) || value.and_then(Value::as_u64).is_some_and(|number| number > 0)
}

pub fn public_btc_anchor_shape_matches_profile(profile: &str, anchor: Option<&Value>) -> bool {
    let Some(anchor) = anchor.and_then(Value::as_object) else {
        return false;
    };
    if profile == "btc-transaction-commitment-profile-v0" {
        return exact_keys(
            anchor,
            &["kind", "anchor_source", "btc_txid", "btc_wtxid", "btc_output_index", "btc_amount_sats", "ckb_btc_commitment_hash"],
        ) && anchor.get("kind").and_then(Value::as_str) == Some("btc_transaction_commitment")
            && anchor.get("anchor_source").and_then(Value::as_str).is_some_and(|source| !source.is_empty())
            && anchor.get("btc_txid").is_some_and(nonzero_hex32)
            && anchor.get("btc_wtxid").is_some_and(nonzero_hex32)
            && non_negative_integer(anchor.get("btc_output_index"))
            && positive_integer(anchor.get("btc_amount_sats"))
            && anchor.get("ckb_btc_commitment_hash").is_some_and(nonzero_hex32);
    }
    if matches!(profile, "btc-utxo-seal-profile-v0" | "dual-seal-profile-v0") {
        let expected_kind = if profile == "btc-utxo-seal-profile-v0" { "btc_utxo_spend" } else { "dual_seal_btc_closure" };
        return exact_keys(
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
        ) && anchor.get("kind").and_then(Value::as_str) == Some(expected_kind)
            && anchor.get("anchor_source").and_then(Value::as_str).is_some_and(|source| !source.is_empty())
            && anchor.get("sealed_btc_txid").is_some_and(nonzero_hex32)
            && non_negative_integer(anchor.get("sealed_btc_vout_index"))
            && positive_integer(anchor.get("sealed_btc_amount_sats"))
            && anchor.get("script_pubkey_hash").is_some_and(nonzero_hex32)
            && anchor.get("btc_txid").is_some_and(nonzero_hex32)
            && anchor.get("btc_wtxid").is_some_and(nonzero_hex32)
            && non_negative_integer(anchor.get("spend_input_index"))
            && anchor.get("ckb_btc_commitment_hash").is_some_and(nonzero_hex32)
            && anchor.get("sealed_utxo_commitment_hash").is_some_and(nonzero_hex32);
    }
    false
}
