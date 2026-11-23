use super::{
    exact_object_keys, hex_bytes, hex_bytes_exact, is_hex32, json_array_strings, json_pointer_i64, json_pointer_str, json_pointer_u64,
    normalize_hex, placeholder_hash, BitcoinTxInput, BitcoinTxOutput, BitcoinTxSummary, EXPECTED_BTC_TX_COMMITMENT_PROFILE,
    EXPECTED_BTC_UTXO_SEAL_PROFILE, EXPECTED_DUAL_SEAL_PROFILE,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub(super) fn validate_btc_transaction_binding(profile: &str, case: &Value, expected_binding: &Value) -> Value {
    let binding = case.get("btc_transaction_binding").unwrap_or(&Value::Null);
    let tx = json_pointer_str(case, "/btc_tx_hex").and_then(parse_bitcoin_tx_hex);
    let sealed_tx = json_pointer_str(binding, "/sealed_btc_tx_hex").and_then(parse_bitcoin_tx_hex);
    let txid_matches =
        tx.as_ref().is_some_and(|tx| normalize_hex(json_pointer_str(case, "/btc_txid")).as_deref() == Some(tx.txid.as_str()));
    let wtxid_matches =
        tx.as_ref().is_some_and(|tx| normalize_hex(json_pointer_str(case, "/btc_wtxid")).as_deref() == Some(tx.wtxid.as_str()));

    json!({
        "btc_tx_hex_valid": tx.is_some(),
        "btc_txid_matches_tx_hex": txid_matches,
        "btc_wtxid_matches_tx_hex": wtxid_matches,
        "binding_fields_exact": btc_transaction_binding_fields_exact(profile, binding),
        "binding_kind_matches_profile": btc_transaction_binding_kind_matches_profile(profile, binding),
        "binding_matches_handoff": btc_binding_fields_match_handoff(profile, binding, expected_binding),
        "transaction_output_matches_anchor": profile != EXPECTED_BTC_TX_COMMITMENT_PROFILE
            || btc_transaction_output_matches_anchor(binding, tx.as_ref(), expected_binding),
        "utxo_spend_input_matches_anchor": profile != EXPECTED_BTC_UTXO_SEAL_PROFILE
            || btc_utxo_spend_input_matches_anchor(binding, tx.as_ref(), expected_binding),
        "utxo_sealed_tx_matches_anchor": profile != EXPECTED_BTC_UTXO_SEAL_PROFILE
            || btc_utxo_sealed_tx_matches_anchor(binding, sealed_tx.as_ref(), expected_binding),
        "utxo_sealed_utxo_commitment_matches_tuple": profile != EXPECTED_BTC_UTXO_SEAL_PROFILE
            || btc_sealed_utxo_commitment_matches_tuple(binding, expected_binding),
        "dual_spend_input_matches_anchor": profile != EXPECTED_DUAL_SEAL_PROFILE
            || btc_dual_spend_input_matches_anchor(binding, tx.as_ref(), expected_binding),
        "dual_sealed_tx_matches_anchor": profile != EXPECTED_DUAL_SEAL_PROFILE
            || btc_utxo_sealed_tx_matches_anchor(binding, sealed_tx.as_ref(), expected_binding),
        "dual_sealed_utxo_commitment_matches_tuple": profile != EXPECTED_DUAL_SEAL_PROFILE
            || btc_sealed_utxo_commitment_matches_tuple(binding, expected_binding),
    })
}

pub(super) fn btc_transaction_binding_fields_exact(profile: &str, binding: &Value) -> bool {
    match profile {
        EXPECTED_BTC_TX_COMMITMENT_PROFILE => exact_object_keys(binding, &["kind", "btc_output_index", "btc_amount_sats"]),
        EXPECTED_BTC_UTXO_SEAL_PROFILE | EXPECTED_DUAL_SEAL_PROFILE => exact_object_keys(
            binding,
            &[
                "kind",
                "spend_input_index",
                "sealed_btc_txid",
                "sealed_btc_vout_index",
                "sealed_btc_amount_sats",
                "script_pubkey_hash",
                "sealed_btc_tx_hex",
                "sealed_utxo_commitment_hash",
            ],
        ),
        _ => false,
    }
}

pub(super) fn btc_transaction_binding_kind_matches_profile(profile: &str, binding: &Value) -> bool {
    let expected = match profile {
        EXPECTED_BTC_TX_COMMITMENT_PROFILE => "btc_transaction_output",
        EXPECTED_BTC_UTXO_SEAL_PROFILE => "btc_utxo_spend",
        EXPECTED_DUAL_SEAL_PROFILE => "dual_seal_btc_closure",
        _ => return false,
    };
    json_pointer_str(binding, "/kind") == Some(expected)
}

fn btc_binding_fields_match_handoff(profile: &str, binding: &Value, expected_binding: &Value) -> bool {
    match profile {
        EXPECTED_BTC_TX_COMMITMENT_PROFILE => {
            json_pointer_u64(binding, "/btc_output_index") == json_pointer_u64(expected_binding, "/btc_output_index")
                && json_pointer_u64(binding, "/btc_amount_sats") == json_pointer_u64(expected_binding, "/btc_amount_sats")
        }
        EXPECTED_BTC_UTXO_SEAL_PROFILE | EXPECTED_DUAL_SEAL_PROFILE => {
            btc_sealed_binding_fields_match_handoff(binding, expected_binding)
        }
        _ => false,
    }
}

fn btc_sealed_binding_fields_match_handoff(binding: &Value, expected_binding: &Value) -> bool {
    json_pointer_u64(binding, "/spend_input_index") == json_pointer_u64(expected_binding, "/spend_input_index")
        && normalize_hex(json_pointer_str(binding, "/sealed_btc_txid")).as_deref()
            == normalize_hex(json_pointer_str(expected_binding, "/sealed_btc_txid")).as_deref()
        && json_pointer_u64(binding, "/sealed_btc_vout_index") == json_pointer_u64(expected_binding, "/sealed_btc_vout_index")
        && json_pointer_u64(binding, "/sealed_btc_amount_sats") == json_pointer_u64(expected_binding, "/sealed_btc_amount_sats")
        && normalize_hex(json_pointer_str(binding, "/script_pubkey_hash")).as_deref()
            == normalize_hex(json_pointer_str(expected_binding, "/script_pubkey_hash")).as_deref()
        && normalize_hex(json_pointer_str(binding, "/sealed_utxo_commitment_hash")).as_deref()
            == normalize_hex(json_pointer_str(expected_binding, "/sealed_utxo_commitment_hash")).as_deref()
}

pub(super) fn btc_transaction_output_matches_anchor(binding: &Value, tx: Option<&BitcoinTxSummary>, expected_binding: &Value) -> bool {
    let Some(tx) = tx else {
        return false;
    };
    let Some(index) = json_pointer_u64(binding, "/btc_output_index") else {
        return false;
    };
    let Some(expected_index) = json_pointer_u64(expected_binding, "/btc_output_index") else {
        return false;
    };
    let Some(expected_amount) = json_pointer_u64(expected_binding, "/btc_amount_sats") else {
        return false;
    };
    index == expected_index && tx.outputs.get(index as usize).is_some_and(|output| output.amount_sats == expected_amount)
}

pub(super) fn btc_utxo_spend_input_matches_anchor(binding: &Value, tx: Option<&BitcoinTxSummary>, expected_binding: &Value) -> bool {
    let Some(tx) = tx else {
        return false;
    };
    let Some(index) = json_pointer_u64(binding, "/spend_input_index") else {
        return false;
    };
    let Some(expected_index) = json_pointer_u64(expected_binding, "/spend_input_index") else {
        return false;
    };
    let Some(expected_vout) = json_pointer_u64(expected_binding, "/sealed_btc_vout_index") else {
        return false;
    };
    let expected_txid = normalize_hex(json_pointer_str(expected_binding, "/sealed_btc_txid"));
    index == expected_index
        && tx
            .inputs
            .get(index as usize)
            .is_some_and(|input| Some(input.prev_txid.as_str()) == expected_txid.as_deref() && input.prev_vout == expected_vout)
}

pub(super) fn btc_utxo_sealed_tx_matches_anchor(
    binding: &Value,
    sealed_tx: Option<&BitcoinTxSummary>,
    expected_binding: &Value,
) -> bool {
    let Some(sealed_tx) = sealed_tx else {
        return false;
    };
    let Some(index) = json_pointer_u64(binding, "/sealed_btc_vout_index") else {
        return false;
    };
    let Some(expected_amount) = json_pointer_u64(expected_binding, "/sealed_btc_amount_sats") else {
        return false;
    };
    let expected_txid = normalize_hex(json_pointer_str(expected_binding, "/sealed_btc_txid"));
    let expected_script_hash = normalize_hex(json_pointer_str(expected_binding, "/script_pubkey_hash"));
    Some(sealed_tx.txid.as_str()) == expected_txid.as_deref()
        && sealed_tx.outputs.get(index as usize).is_some_and(|output| {
            let actual_script_hash = format!("0x{}", hex::encode(crate::ckb_blake2b256(&output.script_pubkey)));
            output.amount_sats == expected_amount && Some(actual_script_hash.as_str()) == expected_script_hash.as_deref()
        })
}

pub(super) fn btc_dual_spend_input_matches_anchor(binding: &Value, tx: Option<&BitcoinTxSummary>, expected_binding: &Value) -> bool {
    btc_utxo_spend_input_matches_anchor(binding, tx, expected_binding)
}

fn btc_sealed_utxo_commitment_matches_tuple(binding: &Value, expected_binding: &Value) -> bool {
    let Some(txid) = json_pointer_str(binding, "/sealed_btc_txid") else {
        return false;
    };
    let Some(vout) = json_pointer_u64(binding, "/sealed_btc_vout_index") else {
        return false;
    };
    let Some(amount) = json_pointer_u64(binding, "/sealed_btc_amount_sats") else {
        return false;
    };
    let Some(script_hash) = json_pointer_str(binding, "/script_pubkey_hash") else {
        return false;
    };
    let Some(actual) = btc_sealed_utxo_commitment_hash(txid, vout, amount, script_hash) else {
        return false;
    };
    Some(actual.as_str()) == normalize_hex(json_pointer_str(binding, "/sealed_utxo_commitment_hash")).as_deref()
        && Some(actual.as_str()) == normalize_hex(json_pointer_str(expected_binding, "/sealed_utxo_commitment_hash")).as_deref()
}

pub(super) fn btc_sealed_utxo_commitment_hash(txid: &str, vout: u64, amount: u64, script_hash: &str) -> Option<String> {
    let txid = normalize_hex(Some(txid)).and_then(|value| hex_bytes_exact(&value, 32))?;
    let script_hash = normalize_hex(Some(script_hash)).and_then(|value| hex_bytes_exact(&value, 32))?;
    let vout = u32::try_from(vout).ok()?;
    let mut packed = Vec::with_capacity(76);
    packed.extend_from_slice(&txid);
    packed.extend_from_slice(&vout.to_le_bytes());
    packed.extend_from_slice(&amount.to_le_bytes());
    packed.extend_from_slice(&script_hash);
    cellscript_packed_hash_hex("BtcUtxoCommitmentV0", &packed)
}

fn cellscript_packed_hash_hex(type_name: &str, packed: &[u8]) -> Option<String> {
    let packed_len = u32::try_from(packed.len()).ok()?;
    let mut preimage = Vec::with_capacity(b"CellScriptPackedHashV0\0".len() + type_name.len() + 1 + 4 + packed.len());
    preimage.extend_from_slice(b"CellScriptPackedHashV0\0");
    preimage.extend_from_slice(type_name.as_bytes());
    preimage.push(0);
    preimage.extend_from_slice(&packed_len.to_le_bytes());
    preimage.extend_from_slice(packed);
    Some(format!("0x{}", hex::encode(crate::ckb_blake2b256(&preimage))))
}

pub(super) fn parse_bitcoin_tx_hex(value: &str) -> Option<BitcoinTxSummary> {
    parse_bitcoin_tx(&hex_bytes(value)?)
}

pub(super) fn parse_bitcoin_tx(bytes: &[u8]) -> Option<BitcoinTxSummary> {
    let mut cursor = 0usize;
    let version = read_slice(bytes, &mut cursor, 4)?.to_vec();
    let mut segwit = false;
    if bytes.get(cursor) == Some(&0) {
        match bytes.get(cursor + 1).copied() {
            Some(0x01) => {
                segwit = true;
                cursor += 2;
            }
            Some(flag) if flag != 0 => return None,
            _ => {}
        }
    }
    let input_count_start = cursor;
    let input_count = read_varint(bytes, &mut cursor)?;
    if input_count == 0 {
        return None;
    }
    let input_count_bytes = bytes.get(input_count_start..cursor)?.to_vec();
    let inputs_start = cursor;
    let mut inputs = Vec::new();
    for _ in 0..input_count {
        let prev_hash = read_slice(bytes, &mut cursor, 32)?;
        let prev_txid = bitcoin_display_hash_from_internal(prev_hash);
        let prev_vout = u64::from(read_u32_le(bytes, &mut cursor)?);
        let script_len = read_varint(bytes, &mut cursor)?;
        read_slice(bytes, &mut cursor, usize::try_from(script_len).ok()?)?;
        read_slice(bytes, &mut cursor, 4)?;
        inputs.push(BitcoinTxInput { prev_txid, prev_vout });
    }
    let inputs_bytes = bytes.get(inputs_start..cursor)?.to_vec();
    let output_count_start = cursor;
    let output_count = read_varint(bytes, &mut cursor)?;
    if output_count == 0 {
        return None;
    }
    let output_count_bytes = bytes.get(output_count_start..cursor)?.to_vec();
    let outputs_start = cursor;
    let mut outputs = Vec::new();
    for _ in 0..output_count {
        let amount_sats = read_u64_le(bytes, &mut cursor)?;
        let script_len = read_varint(bytes, &mut cursor)?;
        let script_pubkey = read_slice(bytes, &mut cursor, usize::try_from(script_len).ok()?)?.to_vec();
        outputs.push(BitcoinTxOutput { amount_sats, script_pubkey });
    }
    let outputs_bytes = bytes.get(outputs_start..cursor)?.to_vec();
    if segwit {
        for _ in 0..input_count {
            let item_count = read_varint(bytes, &mut cursor)?;
            for _ in 0..item_count {
                let item_len = read_varint(bytes, &mut cursor)?;
                read_slice(bytes, &mut cursor, usize::try_from(item_len).ok()?)?;
            }
        }
    }
    let lock_time = read_slice(bytes, &mut cursor, 4)?.to_vec();
    if cursor != bytes.len() {
        return None;
    }
    let mut stripped = Vec::new();
    stripped.extend_from_slice(&version);
    stripped.extend_from_slice(&input_count_bytes);
    stripped.extend_from_slice(&inputs_bytes);
    stripped.extend_from_slice(&output_count_bytes);
    stripped.extend_from_slice(&outputs_bytes);
    stripped.extend_from_slice(&lock_time);
    Some(BitcoinTxSummary {
        txid: bitcoin_display_hash(&stripped),
        wtxid: if segwit { bitcoin_display_hash(bytes) } else { bitcoin_display_hash(&stripped) },
        inputs,
        outputs,
    })
}

fn read_slice<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> Option<&'a [u8]> {
    let end = cursor.checked_add(len)?;
    let slice = bytes.get(*cursor..end)?;
    *cursor = end;
    Some(slice)
}

fn read_u32_le(bytes: &[u8], cursor: &mut usize) -> Option<u32> {
    Some(u32::from_le_bytes(read_slice(bytes, cursor, 4)?.try_into().ok()?))
}

fn read_u64_le(bytes: &[u8], cursor: &mut usize) -> Option<u64> {
    Some(u64::from_le_bytes(read_slice(bytes, cursor, 8)?.try_into().ok()?))
}

fn read_varint(bytes: &[u8], cursor: &mut usize) -> Option<u64> {
    let first = *read_slice(bytes, cursor, 1)?.first()?;
    match first {
        0x00..=0xfc => Some(u64::from(first)),
        0xfd => {
            let value: [u8; 2] = read_slice(bytes, cursor, 2)?.try_into().ok()?;
            let value = u16::from_le_bytes(value);
            (value >= 0xfd).then_some(u64::from(value))
        }
        0xfe => {
            let value = read_u32_le(bytes, cursor)?;
            (value > u32::from(u16::MAX)).then_some(u64::from(value))
        }
        0xff => {
            let value = read_u64_le(bytes, cursor)?;
            (value > u64::from(u32::MAX)).then_some(value)
        }
    }
}

pub(super) fn validate_btc_spv_case_proof(case: &Value, confirmations: i64) -> Value {
    let header = json_pointer_str(case, "/btc_block_header").and_then(|value| hex_bytes_exact(value, 80));
    let header_hash = header.as_deref().map(bitcoin_display_hash);
    let header_merkle_root = header.as_deref().map(bitcoin_header_merkle_root_display);
    let proof = case.get("btc_merkle_proof").unwrap_or(&Value::Null);
    let txid = json_pointer_str(case, "/btc_txid");
    let tx_index = json_pointer_i64(proof, "/tx_index");
    let branch = json_array_strings(proof, "/merkle_branch");
    let branch_valid = match tx_index {
        Some(0) if branch.is_empty() => true,
        Some(_) if branch.is_empty() => false,
        Some(_) => branch.iter().all(|hash| is_hex32(hash) && !placeholder_hash(Some(hash))),
        None => false,
    };
    let computed_merkle_root = txid.zip(tx_index).and_then(|(txid, tx_index)| bitcoin_merkle_root_display(txid, &branch, tx_index));
    let block_height = json_pointer_i64(proof, "/block_height");
    let observed_tip_height = json_pointer_i64(proof, "/observed_tip_height");
    let expected_confirmations = block_height.zip(observed_tip_height).and_then(|(block_height, observed_tip_height)| {
        if block_height >= 0 && observed_tip_height >= block_height {
            observed_tip_height.checked_sub(block_height)?.checked_add(1)
        } else {
            None
        }
    });
    let proof_hash = btc_spv_proof_material_hash(case);
    json!({
        "block_hash_matches_header": header_hash.as_deref()
            == normalize_hex(json_pointer_str(case, "/btc_block_hash")).as_deref(),
        "merkle_branch_valid": branch_valid,
        "merkle_root_matches_header": header_merkle_root.as_deref()
            == normalize_hex(json_pointer_str(proof, "/merkle_root")).as_deref(),
        "merkle_branch_verifies_txid": computed_merkle_root.as_deref()
            == normalize_hex(json_pointer_str(proof, "/merkle_root")).as_deref(),
        "confirmations_match_heights": expected_confirmations == Some(confirmations),
        "proof_hash_matches_material": proof_hash.as_deref()
            == normalize_hex(json_pointer_str(case, "/spv_proof_hash")).as_deref(),
    })
}

pub(super) fn btc_spv_proof_material_hash(case: &Value) -> Option<String> {
    let material = json!({
        "btc_txid": json_pointer_str(case, "/btc_txid")?,
        "btc_wtxid": json_pointer_str(case, "/btc_wtxid")?,
        "btc_tx_hex": json_pointer_str(case, "/btc_tx_hex")?,
        "btc_transaction_binding": case.get("btc_transaction_binding")?,
        "btc_block_hash": json_pointer_str(case, "/btc_block_hash")?,
        "btc_block_header": json_pointer_str(case, "/btc_block_header")?,
        "btc_merkle_proof": case.get("btc_merkle_proof")?,
    });
    Some(format!("0x{}", hex::encode(Sha256::digest(cellscript_canonical_json::canonical_json_string(&material).as_bytes()))))
}

pub(super) fn bitcoin_merkle_root_display(txid_display: &str, branch_display: &[String], tx_index: i64) -> Option<String> {
    let mut index = u64::try_from(tx_index).ok()?;
    let mut current = bitcoin_internal_hash_from_display(txid_display)?;
    for sibling in branch_display {
        let sibling = bitcoin_internal_hash_from_display(sibling)?;
        let mut preimage = Vec::with_capacity(64);
        if index & 1 == 0 {
            preimage.extend_from_slice(&current);
            preimage.extend_from_slice(&sibling);
        } else {
            preimage.extend_from_slice(&sibling);
            preimage.extend_from_slice(&current);
        }
        current = bitcoin_double_sha256(&preimage);
        index >>= 1;
    }
    Some(bitcoin_display_hash_from_internal(&current))
}

fn bitcoin_header_merkle_root_display(header: &[u8]) -> String {
    bitcoin_display_hash_from_internal(&header[36..68])
}

pub(super) fn bitcoin_display_hash(header: &[u8]) -> String {
    let digest = bitcoin_double_sha256(header);
    bitcoin_display_hash_from_internal(&digest)
}

fn bitcoin_display_hash_from_internal(hash: &[u8]) -> String {
    let mut display = hash.to_vec();
    display.reverse();
    format!("0x{}", hex::encode(display))
}

pub(super) fn bitcoin_internal_hash_from_display(value: &str) -> Option<[u8; 32]> {
    let mut bytes = hex_bytes_exact(value, 32)?;
    bytes.reverse();
    bytes.try_into().ok()
}

fn bitcoin_double_sha256(bytes: &[u8]) -> [u8; 32] {
    let first = Sha256::digest(bytes);
    Sha256::digest(first).into()
}
