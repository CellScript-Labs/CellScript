"""Shared NovaSeal BTC public-anchor shape checks."""

from __future__ import annotations

from typing import Any


def _is_nonzero_hex32(value: Any) -> bool:
    if not isinstance(value, str) or not value.startswith("0x") or len(value) != 66:
        return False
    try:
        raw = bytes.fromhex(value[2:])
    except ValueError:
        return False
    return any(byte != 0 for byte in raw)


def _is_non_negative_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value >= 0


def _is_positive_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value > 0


def _exact_keys(value: dict[str, Any], keys: list[str]) -> bool:
    return set(value.keys()) == set(keys)


def public_btc_anchor_shape_matches_profile(profile: str, anchor: Any) -> bool:
    if not isinstance(anchor, dict):
        return False
    if profile == "btc-transaction-commitment-profile-v0":
        return (
            _exact_keys(
                anchor,
                [
                    "kind",
                    "anchor_source",
                    "btc_txid",
                    "btc_wtxid",
                    "btc_output_index",
                    "btc_amount_sats",
                    "ckb_btc_commitment_hash",
                ],
            )
            and anchor.get("kind") == "btc_transaction_commitment"
            and isinstance(anchor.get("anchor_source"), str)
            and bool(anchor.get("anchor_source"))
            and _is_nonzero_hex32(anchor.get("btc_txid"))
            and _is_nonzero_hex32(anchor.get("btc_wtxid"))
            and _is_non_negative_int(anchor.get("btc_output_index"))
            and _is_positive_int(anchor.get("btc_amount_sats"))
            and _is_nonzero_hex32(anchor.get("ckb_btc_commitment_hash"))
        )
    if profile in {"btc-utxo-seal-profile-v0", "dual-seal-profile-v0"}:
        expected_kind = {
            "btc-utxo-seal-profile-v0": "btc_utxo_spend",
            "dual-seal-profile-v0": "dual_seal_btc_closure",
        }[profile]
        return (
            _exact_keys(
                anchor,
                [
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
            )
            and anchor.get("kind") == expected_kind
            and isinstance(anchor.get("anchor_source"), str)
            and bool(anchor.get("anchor_source"))
            and _is_nonzero_hex32(anchor.get("sealed_btc_txid"))
            and _is_non_negative_int(anchor.get("sealed_btc_vout_index"))
            and _is_positive_int(anchor.get("sealed_btc_amount_sats"))
            and _is_nonzero_hex32(anchor.get("script_pubkey_hash"))
            and _is_nonzero_hex32(anchor.get("btc_txid"))
            and _is_nonzero_hex32(anchor.get("btc_wtxid"))
            and _is_non_negative_int(anchor.get("spend_input_index"))
            and _is_nonzero_hex32(anchor.get("ckb_btc_commitment_hash"))
            and _is_nonzero_hex32(anchor.get("sealed_utxo_commitment_hash"))
        )
    return False
