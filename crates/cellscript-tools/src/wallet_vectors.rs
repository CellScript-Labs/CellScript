//! NovaSeal wallet-signing vector generator.

use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};

use crate::crypto::{bytes32, ckb_blake2b256, decode_hex0x, hex0x, personalized_blake2b256};
use crate::shared::{python_json_pretty, python_path};

const PACKED_HASH_DOMAIN: &[u8] = b"CellScriptPackedHashV0\0";
const VECTOR_PERSON: &[u8] = b"NovaSealWalletV0";
const CKB: u64 = 100_000_000;
const COLLATERAL_AMOUNT: u64 = 1_000 * CKB;
const PRINCIPAL_AMOUNT: u64 = 700 * CKB;
const FIXED_FEE_AMOUNT: u64 = 30 * CKB;
const EXPIRY_TIMEPOINT: u64 = 200;

static ZERO_HASH: LazyLock<String> = LazyLock::new(|| format!("0x{}", "00".repeat(32)));
static BORROWER_AUTHORITY: LazyLock<String> = LazyLock::new(|| format!("0x{}", "11".repeat(32)));
static LENDER_AUTHORITY: LazyLock<String> = LazyLock::new(|| format!("0x{}", "22".repeat(32)));

fn stable_hash(label: &str, value: &str) -> Result<String> {
    Ok(hex0x(&personalized_blake2b256(VECTOR_PERSON, &[label.as_bytes(), b"\0", value.as_bytes()])?))
}

fn uint(value: u64, size: usize) -> Result<Vec<u8>> {
    if size > 8 || (size < 8 && value >= (1_u64 << (size * 8))) {
        bail!("{value} does not fit u{}", size * 8);
    }
    Ok(value.to_le_bytes()[..size].to_vec())
}

fn packed_hash(type_name: &str, packed: &[u8]) -> Result<(String, String)> {
    let length = u32::try_from(packed.len()).context("wallet packed value exceeds u32")?;
    let mut preimage = Vec::with_capacity(PACKED_HASH_DOMAIN.len() + type_name.len() + 1 + 4 + packed.len());
    preimage.extend_from_slice(PACKED_HASH_DOMAIN);
    preimage.extend_from_slice(type_name.as_bytes());
    preimage.push(0);
    preimage.extend_from_slice(&length.to_le_bytes());
    preimage.extend_from_slice(packed);
    Ok((hex0x(&preimage), hex0x(&ckb_blake2b256(&preimage)?)))
}

fn encoded(type_name: &str, packed: Vec<u8>) -> Result<Value> {
    let (preimage, digest) = packed_hash(type_name, &packed)?;
    Ok(json!({
        "type": type_name,
        "hex": hex0x(&packed),
        "hash_preimage_hex": preimage,
        "digest_blake2b_256": digest,
    }))
}

fn field_map(encoded: &Value) -> Map<String, Value> {
    let mut result = Map::new();
    let Some(fields) = encoded.get("fields").and_then(Value::as_array) else {
        return result;
    };
    for field in fields {
        let Some(name) = field.get("name").and_then(Value::as_str) else {
            continue;
        };
        if let Some(value) = field.get("value") {
            result.insert(name.to_string(), value.clone());
        } else if matches!(field.get("type").and_then(Value::as_str), Some("Byte32" | "Hash")) {
            result.insert(name.to_string(), field.get("hex").cloned().unwrap_or(Value::Null));
        } else if field.get("type").and_then(Value::as_str) == Some("OutPoint") {
            let components = field.get("components").and_then(Value::as_array);
            let component = |wanted: &str| {
                components.and_then(|items| items.iter().find(|item| item.get("name").and_then(Value::as_str) == Some(wanted)))
            };
            result.insert(
                name.to_string(),
                json!({
                    "tx_hash": component("tx_hash").and_then(|item| item.get("hex")).cloned().unwrap_or(Value::Null),
                    "index": component("index").and_then(|item| item.get("value")).cloned().unwrap_or(Value::Null),
                }),
            );
        } else if let Some(nested) = field.get("nested") {
            result.insert(name.to_string(), Value::Object(field_map(nested)));
        }
    }
    result
}

fn required_str<'value>(value: &'value Value, key: &str) -> Result<&'value str> {
    value.get(key).and_then(Value::as_str).with_context(|| format!("wallet value is missing string field {key}"))
}

fn wallet_record(
    suite: &str,
    name: &str,
    action: &str,
    signers: &[&str],
    signed_intent: &Value,
    display: Value,
    expected_receipt_hash: Value,
) -> Result<Value> {
    let preimage = required_str(signed_intent, "hash_preimage_hex")?;
    let message = required_str(signed_intent, "digest_blake2b_256")?;
    let recomputed = hex0x(&ckb_blake2b256(&decode_hex0x(preimage)?)?);
    Ok(json!({
        "suite": suite,
        "name": name,
        "action": action,
        "signers": signers,
        "status": if recomputed == message { "passed" } else { "failed" },
        "bip340_message_hash": message,
        "signed_type": required_str(signed_intent, "type")?,
        "signed_intent_packed_hex": required_str(signed_intent, "hex")?,
        "signed_intent_hash_preimage_hex": preimage,
        "molecule_fixed_equivalent_hex": required_str(signed_intent, "hex")?,
        "molecule_profile": "fixed-width CellScript schema; equivalent to declared-field concatenation for these v0 structs",
        "expected_receipt_hash": expected_receipt_hash,
        "wallet_display": display,
    }))
}

fn first_truthy(values: impl IntoIterator<Item = Value>) -> Value {
    values
        .into_iter()
        .find(|value| match value {
            Value::Null => false,
            Value::Bool(value) => *value,
            Value::String(value) => !value.is_empty(),
            Value::Array(value) => !value.is_empty(),
            Value::Object(value) => !value.is_empty(),
            Value::Number(value) => value.as_f64().is_some_and(|number| number != 0.0),
        })
        .unwrap_or(Value::Null)
}

fn core_vectors(path: &Path) -> Result<Vec<Value>> {
    let payload: Value = serde_json::from_slice(&fs::read(path).with_context(|| format!("failed to read {}", path.display()))?)
        .with_context(|| format!("{} is not valid JSON", path.display()))?;
    let mut vectors = Vec::new();
    for vector in payload.get("vectors").and_then(Value::as_array).into_iter().flatten() {
        let encoded_value = vector.get("encoded").cloned().unwrap_or_else(|| json!({}));
        let Some(resolved) = encoded_value.get("resolved").and_then(Value::as_object) else {
            continue;
        };
        let signed_candidate = resolved.get("signed_intent").filter(|value| value.is_object()).cloned().or_else(|| {
            first_truthy([
                resolved.get("resolved_intent").cloned().unwrap_or(Value::Null),
                encoded_value.get("intent").cloned().unwrap_or(Value::Null),
            ])
            .is_object()
            .then(|| {
                first_truthy([
                    resolved.get("resolved_intent").cloned().unwrap_or(Value::Null),
                    encoded_value.get("intent").cloned().unwrap_or(Value::Null),
                ])
            })
        });
        let Some(mut signed_intent) = signed_candidate else {
            continue;
        };
        if signed_intent.get("hash_preimage_hex").is_none_or(Value::is_null)
            && let Some(packed_hex) = signed_intent.get("hex").and_then(Value::as_str)
        {
            let type_name = signed_intent.get("type").and_then(Value::as_str).unwrap_or("NovaSealIntentV0");
            let (preimage, digest) = packed_hash(type_name, &decode_hex0x(packed_hex)?)?;
            let object = signed_intent.as_object_mut().context("signed intent is not an object")?;
            object.insert("hash_preimage_hex".to_string(), Value::String(preimage));
            object.insert("digest_blake2b_256".to_string(), Value::String(digest));
        }
        let signed_fields = signed_intent.get("fields").and_then(Value::as_array);
        let core = if signed_fields.and_then(|fields| fields.first()).and_then(|field| field.get("nested")).is_some() {
            field_map(&signed_fields.expect("checked above")[0]["nested"])
        } else {
            field_map(&signed_intent)
        };
        let old_cell = field_map(encoded_value.get("old_cell").unwrap_or(&Value::Null));
        let display = json!({
            "protocol": "NovaSeal Core v0",
            "fixture": vector.get("fixture").cloned().unwrap_or(Value::Null),
            "action": core.get("action").cloned().unwrap_or(Value::Null),
            "terminal_path": core.get("terminal_path").cloned().unwrap_or(Value::Null),
            "btc_authority_hash": old_cell.get("btc_authority_hash").cloned().unwrap_or(Value::Null),
            "btc_authority_hash_semantics": "legacy field name; for NovaSeal v0 this equals the 32-byte BIP340 x-only public key and is not a CKB recipient lock hash or payout script identifier",
            "old_cell": core.get("old_cell").cloned().unwrap_or(Value::Null),
            "old_state_hash": core.get("old_state_hash").cloned().unwrap_or(Value::Null),
            "new_state_hash": core.get("new_state_hash").cloned().unwrap_or(Value::Null),
            "old_nonce": core.get("old_nonce").cloned().unwrap_or(Value::Null),
            "new_nonce": core.get("new_nonce").cloned().unwrap_or(Value::Null),
            "expiry": core.get("expiry").cloned().unwrap_or(Value::Null),
            "policy_hash": core.get("policy_hash").cloned().unwrap_or(Value::Null),
        });
        let expected_receipt = first_truthy([
            field_map(&signed_intent).get("expected_receipt_hash").cloned().unwrap_or(Value::Null),
            resolved.get("resolved_receipt_hash").cloned().unwrap_or(Value::Null),
            vector.pointer("/hashes/resolved_receipt_hash").cloned().unwrap_or(Value::Null),
        ]);
        let name =
            first_truthy([vector.get("name").cloned().unwrap_or(Value::Null), vector.get("fixture").cloned().unwrap_or(Value::Null)]);
        vectors.push(wallet_record(
            "novaseal-core-v0",
            &match name {
                Value::String(value) => value,
                other => other.to_string(),
            },
            "key_auth_transition",
            &["btc_authority"],
            &signed_intent,
            display,
            expected_receipt,
        )?);
    }
    Ok(vectors)
}

fn encode_native_payout(
    action: u64,
    role: u64,
    recipient: &str,
    amount: u64,
    terms_hash: &str,
    agreement_id: &str,
    nonce: u64,
) -> Result<Value> {
    let mut packed = Vec::new();
    packed.extend(uint(action, 1)?);
    packed.extend(bytes32(agreement_id)?);
    packed.extend(uint(role, 1)?);
    packed.extend(bytes32(recipient)?);
    packed.extend(uint(0, 1)?);
    packed.extend(bytes32(&ZERO_HASH)?);
    packed.extend(uint(amount, 8)?);
    packed.extend(bytes32(terms_hash)?);
    packed.extend(uint(nonce, 8)?);
    encoded("NativeCkbPayoutV0", packed)
}

#[allow(clippy::too_many_arguments)]
fn encode_agreement_intent_core(
    action: u64,
    agreement_id: &str,
    terms_hash: &str,
    old_status: u64,
    new_status: u64,
    old_nonce: u64,
    new_nonce: u64,
    terminal_amount: u64,
    payout_commitment_hash: &str,
) -> Result<Value> {
    let mut packed = Vec::new();
    packed.extend(uint(action, 1)?);
    packed.extend(bytes32(agreement_id)?);
    packed.extend(bytes32(terms_hash)?);
    packed.extend(bytes32(&BORROWER_AUTHORITY)?);
    packed.extend(bytes32(&LENDER_AUTHORITY)?);
    packed.extend(uint(old_status, 1)?);
    packed.extend(uint(new_status, 1)?);
    packed.extend(uint(old_nonce, 8)?);
    packed.extend(uint(new_nonce, 8)?);
    packed.extend(uint(terminal_amount, 8)?);
    packed.extend(bytes32(payout_commitment_hash)?);
    packed.extend(uint(EXPIRY_TIMEPOINT, 8)?);
    encoded("NovaAgreementIntentCoreV0", packed)
}

#[allow(clippy::too_many_arguments)]
fn encode_canonical_envelope(
    action: u64,
    agreement_id: &str,
    terms_hash: &str,
    old_state_commitment: &str,
    new_state_commitment: &str,
    old_nonce: u64,
    new_nonce: u64,
    authority_hash: &str,
    profile_body_hash: &str,
    payout_commitment_hash: &str,
) -> Result<Value> {
    let mut packed = Vec::new();
    packed.extend(bytes32(agreement_id)?);
    packed.extend(bytes32(terms_hash)?);
    packed.extend(uint(action, 1)?);
    packed.extend(uint(action, 1)?);
    packed.extend(bytes32(agreement_id)?);
    packed.extend(bytes32(old_state_commitment)?);
    packed.extend(bytes32(new_state_commitment)?);
    packed.extend(uint(old_nonce, 8)?);
    packed.extend(uint(new_nonce, 8)?);
    packed.extend(uint(EXPIRY_TIMEPOINT, 8)?);
    packed.extend(bytes32(authority_hash)?);
    packed.extend(bytes32(profile_body_hash)?);
    packed.extend(bytes32(payout_commitment_hash)?);
    encoded("NovaSealCanonicalEnvelopeV0", packed)
}

#[allow(clippy::too_many_arguments)]
fn encode_agreement_receipt_commitment(
    action: u64,
    agreement_id: &str,
    terms_hash: &str,
    old_status: u64,
    new_status: u64,
    terminal_amount: u64,
    old_nonce: u64,
    new_nonce: u64,
    intent_core_hash: &str,
    payout_commitment_hash: &str,
) -> Result<Value> {
    let mut packed = Vec::new();
    packed.extend(uint(action, 1)?);
    packed.extend(bytes32(agreement_id)?);
    packed.extend(uint(old_status, 1)?);
    packed.extend(uint(new_status, 1)?);
    packed.extend(bytes32(terms_hash)?);
    packed.extend(bytes32(&BORROWER_AUTHORITY)?);
    packed.extend(bytes32(&LENDER_AUTHORITY)?);
    packed.extend(uint(terminal_amount, 8)?);
    packed.extend(uint(old_nonce, 8)?);
    packed.extend(uint(new_nonce, 8)?);
    packed.extend(bytes32(intent_core_hash)?);
    packed.extend(bytes32(payout_commitment_hash)?);
    encoded("NovaAgreementReceiptCommitmentV0", packed)
}

fn encode_agreement_signed_intent(core: &Value, canonical_envelope_hash: &str, expected_receipt_hash: &str) -> Result<Value> {
    let mut packed = decode_hex0x(required_str(core, "hex")?)?;
    packed.extend(bytes32(canonical_envelope_hash)?);
    packed.extend(bytes32(expected_receipt_hash)?);
    encoded("NovaAgreementSignedIntentV0", packed)
}

#[allow(clippy::too_many_arguments)]
fn agreement_case(
    name: &str,
    action: u64,
    old_status: u64,
    new_status: u64,
    old_nonce: u64,
    new_nonce: u64,
    terminal_amount: u64,
    signers: &[&str],
) -> Result<Value> {
    let agreement_id = stable_hash("agreement_id", "mvb-starter-v0")?;
    let terms_hash = stable_hash("terms_hash", "ckb-ckb-fixed-fee-v0")?;
    let payout_hash = if action == 0 {
        required_str(
            &encode_native_payout(action, 0, &BORROWER_AUTHORITY, PRINCIPAL_AMOUNT, &terms_hash, &agreement_id, 0)?,
            "digest_blake2b_256",
        )?
        .to_string()
    } else if action == 1 {
        let lender =
            encode_native_payout(action, 1, &LENDER_AUTHORITY, PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT, &terms_hash, &agreement_id, 1)?;
        let borrower = encode_native_payout(action, 2, &BORROWER_AUTHORITY, COLLATERAL_AMOUNT, &terms_hash, &agreement_id, 1)?;
        let mut packed = Vec::new();
        packed.extend(bytes32(required_str(&lender, "digest_blake2b_256")?)?);
        packed.extend(bytes32(required_str(&borrower, "digest_blake2b_256")?)?);
        packed_hash("RepayPayoutCommitmentV0", &packed)?.1
    } else {
        required_str(
            &encode_native_payout(action, 3, &LENDER_AUTHORITY, COLLATERAL_AMOUNT, &terms_hash, &agreement_id, 1)?,
            "digest_blake2b_256",
        )?
        .to_string()
    };
    let core = encode_agreement_intent_core(
        action,
        &agreement_id,
        &terms_hash,
        old_status,
        new_status,
        old_nonce,
        new_nonce,
        terminal_amount,
        &payout_hash,
    )?;
    let receipt = encode_agreement_receipt_commitment(
        action,
        &agreement_id,
        &terms_hash,
        old_status,
        new_status,
        terminal_amount,
        old_nonce,
        new_nonce,
        required_str(&core, "digest_blake2b_256")?,
        &payout_hash,
    )?;
    let authority_hash = if action == 2 { &*LENDER_AUTHORITY } else { &*BORROWER_AUTHORITY };
    let previous = if action == 0 { ZERO_HASH.clone() } else { stable_hash("previous_receipt_hash", "agreement-active-v0")? };
    let canonical = encode_canonical_envelope(
        action,
        &agreement_id,
        &terms_hash,
        &previous,
        required_str(&receipt, "digest_blake2b_256")?,
        old_nonce,
        new_nonce,
        authority_hash,
        required_str(&core, "digest_blake2b_256")?,
        &payout_hash,
    )?;
    let signed = encode_agreement_signed_intent(
        &core,
        required_str(&canonical, "digest_blake2b_256")?,
        required_str(&receipt, "digest_blake2b_256")?,
    )?;
    let action_name = match action {
        0 => "originate_agreement",
        1 => "repay_before_expiry",
        2 => "claim_after_expiry",
        _ => bail!("unsupported agreement action {action}"),
    };
    wallet_record(
        "novaseal-agreement-profile-v0",
        name,
        action_name,
        signers,
        &signed,
        json!({
            "protocol": "NovaSeal Agreement Profile v0",
            "action": action_name,
            "agreement_id": agreement_id,
            "terms_hash": terms_hash,
            "borrower_authority_hash": &*BORROWER_AUTHORITY,
            "lender_authority_hash": &*LENDER_AUTHORITY,
            "old_status": old_status,
            "new_status": new_status,
            "old_nonce": old_nonce,
            "new_nonce": new_nonce,
            "terminal_amount_shannons": terminal_amount,
            "canonical_envelope_hash": required_str(&canonical, "digest_blake2b_256")?,
            "payout_commitment_hash": payout_hash,
            "expiry_timepoint": EXPIRY_TIMEPOINT,
        }),
        receipt.get("digest_blake2b_256").cloned().unwrap_or(Value::Null),
    )
}

fn agreement_vectors() -> Result<Vec<Value>> {
    Ok(vec![
        agreement_case("originate_valid", 0, 0, 1, 0, 0, PRINCIPAL_AMOUNT, &["borrower", "lender"])?,
        agreement_case("repay_before_expiry_valid", 1, 1, 2, 0, 1, PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT, &["borrower"])?,
        agreement_case("claim_after_expiry_valid", 2, 1, 3, 0, 1, COLLATERAL_AMOUNT, &["lender"])?,
    ])
}

pub fn run(root: &Path, core_vectors_path: Option<&Path>, output: Option<&Path>, pretty: bool) -> Result<i32> {
    let default_core = root.join("proposals/novaseal/v0-mvp-skeleton/target/novaseal-canonical-vectors.json");
    let default_output = root.join("target/novaseal-wallet-signing-vectors.json");
    let core_path = python_path(core_vectors_path.unwrap_or(&default_core));
    let output = python_path(output.unwrap_or(&default_output));
    let mut vectors = core_vectors(&core_path)?;
    vectors.extend(agreement_vectors()?);
    let matched = vectors.iter().filter(|vector| vector["status"] == "passed").count();
    let core_count = vectors.iter().filter(|vector| vector["suite"] == "novaseal-core-v0").count();
    let agreement_count = vectors.iter().filter(|vector| vector["suite"] == "novaseal-agreement-profile-v0").count();
    let passed = !vectors.is_empty() && matched == vectors.len();
    let payload = json!({
        "schema": "novaseal-wallet-signing-vectors-v0.1",
        "status": if passed { "passed" } else { "failed" },
        "hash_algorithm": "ckb_blake2b_256",
        "signature_scheme": "BIP340 Schnorr over 32-byte signed intent hash",
        "authority_identifier_semantics": {
            "btc_authority_hash": "legacy-named NovaSeal core field; in v0 it equals the 32-byte BIP340 x-only public key",
            "not_ckb_recipient_lock_hash": true,
            "not_payout_script_identifier": true,
            "agreement_payout_mapping": "profile/builder surface; payout recipients must not be inferred from the core BTC authority field",
        },
        "molecule_alignment": "fixed-width v0 structs use declared-field little-endian concatenation; no dynamic tables/vectors in these signing objects",
        "summary": {
            "total": vectors.len(),
            "core_vectors": core_count,
            "agreement_vectors": agreement_count,
            "matched": matched,
        },
        "vectors": vectors,
    });
    let parent = output.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    fs::write(&output, format!("{}\n", python_json_pretty(&payload)?))
        .with_context(|| format!("failed to write {}", output.display()))?;
    if pretty {
        println!(
            "wrote {} status={} total={} core={} agreement={}",
            output.display(),
            payload["status"].as_str().unwrap_or("failed"),
            payload["summary"]["total"].as_u64().unwrap_or(0),
            payload["summary"]["core_vectors"].as_u64().unwrap_or(0),
            payload["summary"]["agreement_vectors"].as_u64().unwrap_or(0),
        );
    }
    Ok(if passed { 0 } else { 1 })
}
