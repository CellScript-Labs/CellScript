use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::ckb_devnet::{
    always_success_dep, always_success_lock, ckb_hash, ckb_hash_hex, deploy_code, funding_cells, hex0x, provenance, resolve_ckb_bin,
    schnorr_sign, transaction, u16_bytes, u32_bytes, u64_bytes, u8_bytes, xonly_pubkey, CkbDevnet, RECEIPT_CAPACITY, SHANNONS,
    STATE_CAPACITY, TEST_AUX_RAND, TEST_SECRET_KEY, ZERO_HASH,
};
use crate::shared::{python_json_default, python_json_pretty};

const VERSION: u64 = 0;
const ASSET_KIND_CKB: u64 = 0;
const EARLY_CLOSE_FIXED_FEE: u64 = 0;
const STATUS_OFFERED: u64 = 0;
const STATUS_ACTIVE: u64 = 1;
const STATUS_REPAID: u64 = 2;
const STATUS_DEFAULTED: u64 = 3;
const PATH_ORIGINATE: u64 = 0;
const PATH_REPAY: u64 = 1;
const PATH_CLAIM: u64 = 2;
const PAYOUT_BORROWER_PRINCIPAL: u64 = 0;
const PAYOUT_LENDER_REPAYMENT: u64 = 1;
const PAYOUT_BORROWER_COLLATERAL_RETURN: u64 = 2;
const PAYOUT_LENDER_DEFAULT_CLAIM: u64 = 3;
const PAYOUT_CAPACITY_BASE: u64 = 300 * SHANNONS;
const LENDER_SECRET: [u8; 32] = [0x11; 32];
const LENDER_AUX: [u8; 32] = [0x24; 32];

type Hash = [u8; 32];

#[derive(Clone)]
struct Terms {
    agreement_id: Hash,
    terms_hash: Hash,
    borrower: Hash,
    lender: Hash,
    collateral_kind: u64,
    collateral_hash: Hash,
    collateral_amount: u64,
    principal_kind: u64,
    principal_hash: Hash,
    principal_amount: u64,
    fixed_fee: u64,
    start: u64,
    expiry: u64,
    early_close: u64,
}

#[derive(Clone)]
struct Active {
    agreement_id: Hash,
    terms_hash: Hash,
    borrower: Hash,
    lender: Hash,
    collateral_kind: u64,
    collateral_hash: Hash,
    collateral_amount: u64,
    principal_kind: u64,
    principal_hash: Hash,
    principal_amount: u64,
    fixed_fee: u64,
    expiry: u64,
    status: u64,
    latest_receipt: Hash,
    nonce: u64,
}

#[derive(Clone)]
struct Payout {
    action: u64,
    agreement_id: Hash,
    role: u64,
    recipient: Hash,
    asset_kind: u64,
    asset_hash: Hash,
    amount: u64,
    terms_hash: Hash,
    nonce: u64,
}

struct OriginMaterial {
    terms_data: Vec<u8>,
    active: Active,
    active_data: Vec<u8>,
    payout_data: Vec<u8>,
    receipt_data: Vec<u8>,
    signed_intent: Vec<u8>,
    signed_intent_hash: Hash,
    latest_receipt_hash: Hash,
    borrower_sig: Vec<u8>,
    lender_sig: Vec<u8>,
}

struct RepayMaterial {
    terms_data: Vec<u8>,
    active_data: Vec<u8>,
    closed_data: Vec<u8>,
    lender_payout: Payout,
    lender_payout_data: Vec<u8>,
    borrower_payout_data: Vec<u8>,
    receipt_data: Vec<u8>,
    signed_intent: Vec<u8>,
    signed_intent_hash: Hash,
    latest_receipt_hash: Hash,
    borrower_sig: Vec<u8>,
    lender_sig: Vec<u8>,
    repayment_amount: u64,
}

struct ClaimMaterial {
    terms_data: Vec<u8>,
    active_data: Vec<u8>,
    closed_data: Vec<u8>,
    claim_payout_data: Vec<u8>,
    receipt_data: Vec<u8>,
    signed_intent: Vec<u8>,
    signed_intent_hash: Hash,
    latest_receipt_hash: Hash,
    borrower_sig: Vec<u8>,
    lender_sig: Vec<u8>,
    claim_amount: u64,
}

fn append(target: &mut Vec<u8>, chunks: &[&[u8]]) {
    for chunk in chunks {
        target.extend_from_slice(chunk);
    }
}

fn pack_terms(value: &Terms) -> Vec<u8> {
    let mut out = Vec::new();
    append(
        &mut out,
        &[
            &u16_bytes(VERSION),
            &value.agreement_id,
            &value.terms_hash,
            &value.borrower,
            &value.lender,
            &u8_bytes(value.collateral_kind),
            &value.collateral_hash,
            &u64_bytes(value.collateral_amount),
            &u8_bytes(value.principal_kind),
            &value.principal_hash,
            &u64_bytes(value.principal_amount),
            &u64_bytes(value.fixed_fee),
            &u64_bytes(value.start),
            &u64_bytes(value.expiry),
            &u8_bytes(value.early_close),
        ],
    );
    out
}

fn pack_active(value: &Active) -> Vec<u8> {
    let mut out = Vec::new();
    append(
        &mut out,
        &[
            &u16_bytes(VERSION),
            &value.agreement_id,
            &value.terms_hash,
            &value.borrower,
            &value.lender,
            &u8_bytes(value.collateral_kind),
            &value.collateral_hash,
            &u64_bytes(value.collateral_amount),
            &u8_bytes(value.principal_kind),
            &value.principal_hash,
            &u64_bytes(value.principal_amount),
            &u64_bytes(value.fixed_fee),
            &u64_bytes(value.expiry),
            &u8_bytes(value.status),
            &value.latest_receipt,
            &u64_bytes(value.nonce),
        ],
    );
    out
}

#[allow(clippy::too_many_arguments)]
fn pack_intent(
    action: u64,
    terms: &Terms,
    old_status: u64,
    new_status: u64,
    old_nonce: u64,
    new_nonce: u64,
    terminal_amount: u64,
    payout_hash: &Hash,
) -> Vec<u8> {
    let mut out = Vec::new();
    append(
        &mut out,
        &[
            &u8_bytes(action),
            &terms.agreement_id,
            &terms.terms_hash,
            &terms.borrower,
            &terms.lender,
            &u8_bytes(old_status),
            &u8_bytes(new_status),
            &u64_bytes(old_nonce),
            &u64_bytes(new_nonce),
            &u64_bytes(terminal_amount),
            payout_hash,
            &u64_bytes(terms.expiry),
        ],
    );
    out
}

#[allow(clippy::too_many_arguments)]
fn canonical_hash(
    action: u64,
    terms: &Terms,
    old_state: &Hash,
    new_state: &Hash,
    old_nonce: u64,
    new_nonce: u64,
    authority: &Hash,
    body_hash: &Hash,
    payout_hash: &Hash,
) -> Hash {
    let mut packed = Vec::new();
    append(
        &mut packed,
        &[
            &terms.agreement_id,
            &terms.terms_hash,
            &u8_bytes(action),
            &u8_bytes(action),
            &terms.agreement_id,
            old_state,
            new_state,
            &u64_bytes(old_nonce),
            &u64_bytes(new_nonce),
            &u64_bytes(terms.expiry),
            authority,
            body_hash,
            payout_hash,
        ],
    );
    ckb_hash(&packed)
}

#[allow(clippy::too_many_arguments)]
fn receipt_commitment(
    action: u64,
    terms: &Terms,
    old_status: u64,
    new_status: u64,
    terminal_amount: u64,
    old_nonce: u64,
    new_nonce: u64,
    intent_hash: &Hash,
    payout_hash: &Hash,
) -> Hash {
    let mut packed = Vec::new();
    append(
        &mut packed,
        &[
            &u8_bytes(action),
            &terms.agreement_id,
            &u8_bytes(old_status),
            &u8_bytes(new_status),
            &terms.terms_hash,
            &terms.borrower,
            &terms.lender,
            &u64_bytes(terminal_amount),
            &u64_bytes(old_nonce),
            &u64_bytes(new_nonce),
            intent_hash,
            payout_hash,
        ],
    );
    ckb_hash(&packed)
}

fn pack_payout(value: &Payout) -> Vec<u8> {
    let mut out = Vec::new();
    append(
        &mut out,
        &[
            &u8_bytes(value.action),
            &value.agreement_id,
            &u8_bytes(value.role),
            &value.recipient,
            &u8_bytes(value.asset_kind),
            &value.asset_hash,
            &u64_bytes(value.amount),
            &value.terms_hash,
            &u64_bytes(value.nonce),
        ],
    );
    out
}

#[allow(clippy::too_many_arguments)]
fn pack_receipt(
    action: u64,
    terms: &Terms,
    old_status: u64,
    new_status: u64,
    terminal_amount: u64,
    previous: &Hash,
    latest: &Hash,
    intent_core: &Hash,
    signed_intent: &Hash,
    payout: &Hash,
    nonce: u64,
    timepoint: u64,
) -> Vec<u8> {
    let mut out = Vec::new();
    append(
        &mut out,
        &[
            &u8_bytes(action),
            &terms.agreement_id,
            &u8_bytes(old_status),
            &u8_bytes(new_status),
            &terms.terms_hash,
            &terms.borrower,
            &terms.lender,
            &u64_bytes(terms.collateral_amount),
            &u64_bytes(terms.principal_amount),
            &u64_bytes(terms.fixed_fee),
            &u64_bytes(terminal_amount),
            previous,
            latest,
            intent_core,
            signed_intent,
            payout,
            &u64_bytes(nonce),
            &u64_bytes(timepoint),
        ],
    );
    out
}

fn signature(secret: &[u8; 32], message: &Hash, aux: &[u8; 32], mutate: bool) -> Result<Vec<u8>> {
    let (public, signed) = schnorr_sign(message, secret, aux)?;
    let mut payload = Vec::with_capacity(96);
    payload.extend_from_slice(&public);
    payload.extend_from_slice(&signed);
    if mutate {
        *payload.last_mut().unwrap() ^= 1;
    }
    Ok(payload)
}

fn witness(op: u64, terms: &[u8], active: &[u8], intent: &[u8], borrower: &[u8], lender: &[u8]) -> String {
    let mut payload = b"CSARGv1\0".to_vec();
    payload.extend_from_slice(&u8_bytes(op));
    for value in [terms, active, intent, borrower, lender] {
        payload.extend_from_slice(&u32_bytes(value.len()));
        payload.extend_from_slice(value);
    }
    hex0x(&payload)
}

fn make_terms(now: u64, label: &str, expiry: Option<u64>) -> Result<Terms> {
    Ok(Terms {
        agreement_id: ckb_hash(format!("NovaSeal Agreement live devnet v0 {label}").as_bytes()),
        terms_hash: ckb_hash(format!("NovaSeal Agreement live devnet terms v0 {label}").as_bytes()),
        borrower: xonly_pubkey(&TEST_SECRET_KEY)?,
        lender: xonly_pubkey(&LENDER_SECRET)?,
        collateral_kind: ASSET_KIND_CKB,
        collateral_hash: ZERO_HASH,
        collateral_amount: 50 * SHANNONS,
        principal_kind: ASSET_KIND_CKB,
        principal_hash: ZERO_HASH,
        principal_amount: 20 * SHANNONS,
        fixed_fee: 2 * SHANNONS,
        start: 0,
        expiry: expiry.unwrap_or(now + 1_000_000),
        early_close: EARLY_CLOSE_FIXED_FEE,
    })
}

fn origin_material(terms: &Terms, now: u64, mutate_borrower: bool, mutate_lender: bool) -> Result<OriginMaterial> {
    let payout = Payout {
        action: PATH_ORIGINATE,
        agreement_id: terms.agreement_id,
        role: PAYOUT_BORROWER_PRINCIPAL,
        recipient: terms.borrower,
        asset_kind: terms.principal_kind,
        asset_hash: terms.principal_hash,
        amount: terms.principal_amount,
        terms_hash: terms.terms_hash,
        nonce: 0,
    };
    let payout_data = pack_payout(&payout);
    let payout_hash = ckb_hash(&payout_data);
    let core = pack_intent(PATH_ORIGINATE, terms, STATUS_OFFERED, STATUS_ACTIVE, 0, 0, terms.principal_amount, &payout_hash);
    let core_hash = ckb_hash(&core);
    let latest = receipt_commitment(
        PATH_ORIGINATE,
        terms,
        STATUS_OFFERED,
        STATUS_ACTIVE,
        terms.principal_amount,
        0,
        0,
        &core_hash,
        &payout_hash,
    );
    let canonical = canonical_hash(PATH_ORIGINATE, terms, &ZERO_HASH, &latest, 0, 0, &terms.borrower, &core_hash, &payout_hash);
    let mut signed_intent = core;
    signed_intent.extend_from_slice(&canonical);
    signed_intent.extend_from_slice(&latest);
    let signed_hash = ckb_hash(&signed_intent);
    let active = Active {
        agreement_id: terms.agreement_id,
        terms_hash: terms.terms_hash,
        borrower: terms.borrower,
        lender: terms.lender,
        collateral_kind: terms.collateral_kind,
        collateral_hash: terms.collateral_hash,
        collateral_amount: terms.collateral_amount,
        principal_kind: terms.principal_kind,
        principal_hash: terms.principal_hash,
        principal_amount: terms.principal_amount,
        fixed_fee: terms.fixed_fee,
        expiry: terms.expiry,
        status: STATUS_ACTIVE,
        latest_receipt: latest,
        nonce: 0,
    };
    let active_data = pack_active(&active);
    let receipt_data = pack_receipt(
        PATH_ORIGINATE,
        terms,
        STATUS_OFFERED,
        STATUS_ACTIVE,
        terms.principal_amount,
        &ZERO_HASH,
        &latest,
        &core_hash,
        &signed_hash,
        &payout_hash,
        0,
        now,
    );
    Ok(OriginMaterial {
        terms_data: pack_terms(terms),
        active,
        active_data,
        payout_data,
        receipt_data,
        signed_intent,
        signed_intent_hash: signed_hash,
        latest_receipt_hash: latest,
        borrower_sig: signature(&TEST_SECRET_KEY, &signed_hash, &TEST_AUX_RAND, mutate_borrower)?,
        lender_sig: signature(&LENDER_SECRET, &signed_hash, &LENDER_AUX, mutate_lender)?,
    })
}

fn repay_material(terms: &Terms, active: &Active, previous: &Hash, now: u64, mutate_borrower: bool) -> Result<RepayMaterial> {
    let amount = active.principal_amount + active.fixed_fee;
    let nonce = active.nonce + 1;
    let lender_payout = Payout {
        action: PATH_REPAY,
        agreement_id: active.agreement_id,
        role: PAYOUT_LENDER_REPAYMENT,
        recipient: active.lender,
        asset_kind: active.principal_kind,
        asset_hash: active.principal_hash,
        amount,
        terms_hash: active.terms_hash,
        nonce,
    };
    let borrower_payout = Payout {
        action: PATH_REPAY,
        agreement_id: active.agreement_id,
        role: PAYOUT_BORROWER_COLLATERAL_RETURN,
        recipient: active.borrower,
        asset_kind: active.collateral_kind,
        asset_hash: active.collateral_hash,
        amount: active.collateral_amount,
        terms_hash: active.terms_hash,
        nonce,
    };
    let lender_data = pack_payout(&lender_payout);
    let borrower_data = pack_payout(&borrower_payout);
    let mut payout_commitment = Vec::new();
    payout_commitment.extend_from_slice(&ckb_hash(&lender_data));
    payout_commitment.extend_from_slice(&ckb_hash(&borrower_data));
    let payout_hash = ckb_hash(&payout_commitment);
    terminal_material(
        terms,
        active,
        previous,
        now,
        PATH_REPAY,
        STATUS_REPAID,
        amount,
        payout_hash,
        lender_payout,
        lender_data,
        Some(borrower_data),
        mutate_borrower,
        false,
    )
    .map(|value| RepayMaterial {
        terms_data: value.terms_data,
        active_data: value.active_data,
        closed_data: value.closed_data,
        lender_payout: value.payout,
        lender_payout_data: value.payout_data,
        borrower_payout_data: value.second_payout_data.unwrap(),
        receipt_data: value.receipt_data,
        signed_intent: value.signed_intent,
        signed_intent_hash: value.signed_intent_hash,
        latest_receipt_hash: value.latest_receipt_hash,
        borrower_sig: value.borrower_sig,
        lender_sig: value.lender_sig,
        repayment_amount: amount,
    })
}

fn claim_material(terms: &Terms, active: &Active, previous: &Hash, now: u64, mutate_lender: bool) -> Result<ClaimMaterial> {
    let amount = active.collateral_amount;
    let payout = Payout {
        action: PATH_CLAIM,
        agreement_id: active.agreement_id,
        role: PAYOUT_LENDER_DEFAULT_CLAIM,
        recipient: active.lender,
        asset_kind: active.collateral_kind,
        asset_hash: active.collateral_hash,
        amount,
        terms_hash: active.terms_hash,
        nonce: active.nonce + 1,
    };
    let payout_data = pack_payout(&payout);
    let payout_hash = ckb_hash(&payout_data);
    terminal_material(
        terms,
        active,
        previous,
        now,
        PATH_CLAIM,
        STATUS_DEFAULTED,
        amount,
        payout_hash,
        payout,
        payout_data,
        None,
        false,
        mutate_lender,
    )
    .map(|value| ClaimMaterial {
        terms_data: value.terms_data,
        active_data: value.active_data,
        closed_data: value.closed_data,
        claim_payout_data: value.payout_data,
        receipt_data: value.receipt_data,
        signed_intent: value.signed_intent,
        signed_intent_hash: value.signed_intent_hash,
        latest_receipt_hash: value.latest_receipt_hash,
        borrower_sig: value.borrower_sig,
        lender_sig: value.lender_sig,
        claim_amount: amount,
    })
}

struct TerminalMaterial {
    terms_data: Vec<u8>,
    active_data: Vec<u8>,
    closed_data: Vec<u8>,
    payout: Payout,
    payout_data: Vec<u8>,
    second_payout_data: Option<Vec<u8>>,
    receipt_data: Vec<u8>,
    signed_intent: Vec<u8>,
    signed_intent_hash: Hash,
    latest_receipt_hash: Hash,
    borrower_sig: Vec<u8>,
    lender_sig: Vec<u8>,
}

#[allow(clippy::too_many_arguments)]
fn terminal_material(
    terms: &Terms,
    active: &Active,
    previous: &Hash,
    now: u64,
    action: u64,
    new_status: u64,
    amount: u64,
    payout_hash: Hash,
    payout: Payout,
    payout_data: Vec<u8>,
    second_payout_data: Option<Vec<u8>>,
    mutate_borrower: bool,
    mutate_lender: bool,
) -> Result<TerminalMaterial> {
    let nonce = active.nonce + 1;
    let core = pack_intent(action, terms, STATUS_ACTIVE, new_status, active.nonce, nonce, amount, &payout_hash);
    let core_hash = ckb_hash(&core);
    let latest = receipt_commitment(action, terms, STATUS_ACTIVE, new_status, amount, active.nonce, nonce, &core_hash, &payout_hash);
    let authority = if action == PATH_REPAY { &active.borrower } else { &active.lender };
    let canonical = canonical_hash(action, terms, previous, &latest, active.nonce, nonce, authority, &core_hash, &payout_hash);
    let mut signed_intent = core;
    signed_intent.extend_from_slice(&canonical);
    signed_intent.extend_from_slice(&latest);
    let signed_hash = ckb_hash(&signed_intent);
    let mut closed = active.clone();
    closed.status = new_status;
    closed.latest_receipt = latest;
    closed.nonce = nonce;
    let receipt_data = pack_receipt(
        action,
        terms,
        STATUS_ACTIVE,
        new_status,
        amount,
        previous,
        &latest,
        &core_hash,
        &signed_hash,
        &payout_hash,
        nonce,
        now,
    );
    Ok(TerminalMaterial {
        terms_data: pack_terms(terms),
        active_data: pack_active(active),
        closed_data: pack_active(&closed),
        payout,
        payout_data,
        second_payout_data,
        receipt_data,
        signed_intent,
        signed_intent_hash: signed_hash,
        latest_receipt_hash: latest,
        borrower_sig: signature(&TEST_SECRET_KEY, &signed_hash, &TEST_AUX_RAND, mutate_borrower)?,
        lender_sig: signature(&LENDER_SECRET, &signed_hash, &LENDER_AUX, mutate_lender)?,
    })
}

fn lifecycle_type(data_hash: &str) -> Value {
    json!({"code_hash": data_hash, "hash_type": "data2", "args": "0x"})
}

fn build_origin_tx(
    funding: &Value,
    lifecycle_hash: &str,
    deps: Vec<Value>,
    header: &str,
    terms: &Terms,
    material: &OriginMaterial,
) -> Result<Value> {
    let payout_capacity = PAYOUT_CAPACITY_BASE + terms.principal_amount;
    let total = funding["total_capacity"].as_u64().context("originate funding total is missing")?;
    let change =
        total.checked_sub(STATE_CAPACITY + payout_capacity + RECEIPT_CAPACITY).context("originate funding capacity is too small")?;
    if change == 0 {
        bail!("originate funding capacity is too small");
    }
    let cells = funding_cells(funding);
    let mut witnesses = vec![witness(
        PATH_ORIGINATE,
        &material.terms_data,
        &material.active_data,
        &material.signed_intent,
        &material.borrower_sig,
        &material.lender_sig,
    )];
    witnesses.extend(vec!["0x".into(); cells.len().saturating_sub(1)]);
    Ok(transaction(
        cells,
        vec![
            json!({"capacity": format!("0x{STATE_CAPACITY:x}"), "lock": always_success_lock("0x"), "type": lifecycle_type(lifecycle_hash)}),
            json!({"capacity": format!("0x{payout_capacity:x}"), "lock": always_success_lock(&hex0x(&terms.borrower)), "type": Value::Null}),
            json!({"capacity": format!("0x{RECEIPT_CAPACITY:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
            json!({"capacity": format!("0x{change:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
        ],
        vec![hex0x(&material.active_data), hex0x(&material.payout_data), hex0x(&material.receipt_data), "0x".into()],
        deps,
        witnesses,
        vec![header.into()],
    ))
}

#[allow(clippy::too_many_arguments)]
fn build_repay_tx(
    active_ref: &Value,
    funding: &Value,
    lifecycle_hash: &str,
    deps: Vec<Value>,
    header: &str,
    terms: &Terms,
    material: &RepayMaterial,
    capacity_delta: i64,
    lock_override: Option<&Hash>,
    payout_override: Option<&[u8]>,
) -> Result<Value> {
    let base = PAYOUT_CAPACITY_BASE + material.repayment_amount;
    let repayment_capacity =
        if capacity_delta < 0 { base.checked_sub(capacity_delta.unsigned_abs()) } else { base.checked_add(capacity_delta as u64) }
            .context("repay payout capacity overflow")?;
    let collateral_capacity = PAYOUT_CAPACITY_BASE + terms.collateral_amount;
    let total = funding["total_capacity"].as_u64().context("repay funding total is missing")?;
    let change = total
        .checked_sub(repayment_capacity + collateral_capacity + RECEIPT_CAPACITY)
        .context("repay funding capacity is too small")?;
    if change == 0 {
        bail!("repay funding capacity is too small");
    }
    let mut inputs = vec![active_ref.clone()];
    inputs.extend_from_slice(funding_cells(funding));
    let lock_args = lock_override.unwrap_or(&terms.lender);
    let payout_data = payout_override.unwrap_or(&material.lender_payout_data);
    let mut witnesses = vec![witness(
        PATH_REPAY,
        &material.terms_data,
        &material.active_data,
        &material.signed_intent,
        &material.borrower_sig,
        &material.lender_sig,
    )];
    witnesses.extend(vec!["0x".into(); funding_cells(funding).len()]);
    Ok(transaction(
        &inputs,
        vec![
            json!({"capacity": format!("0x{:x}", active_ref["capacity"].as_u64().unwrap()), "lock": always_success_lock("0x"), "type": lifecycle_type(lifecycle_hash)}),
            json!({"capacity": format!("0x{repayment_capacity:x}"), "lock": always_success_lock(&hex0x(lock_args)), "type": Value::Null}),
            json!({"capacity": format!("0x{collateral_capacity:x}"), "lock": always_success_lock(&hex0x(&terms.borrower)), "type": Value::Null}),
            json!({"capacity": format!("0x{RECEIPT_CAPACITY:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
            json!({"capacity": format!("0x{change:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
        ],
        vec![
            hex0x(&material.closed_data),
            hex0x(payout_data),
            hex0x(&material.borrower_payout_data),
            hex0x(&material.receipt_data),
            "0x".into(),
        ],
        deps,
        witnesses,
        vec![header.into()],
    ))
}

#[allow(clippy::too_many_arguments)]
fn build_claim_tx(
    active_ref: &Value,
    funding: &Value,
    lifecycle_hash: &str,
    deps: Vec<Value>,
    header: &str,
    terms: &Terms,
    material: &ClaimMaterial,
    capacity_delta: i64,
    lock_override: Option<&Hash>,
    payout_override: Option<&[u8]>,
) -> Result<Value> {
    let base = PAYOUT_CAPACITY_BASE + material.claim_amount;
    let claim_capacity =
        if capacity_delta < 0 { base.checked_sub(capacity_delta.unsigned_abs()) } else { base.checked_add(capacity_delta as u64) }
            .context("claim payout capacity overflow")?;
    let total = funding["total_capacity"].as_u64().context("claim funding total is missing")?;
    let change = total.checked_sub(claim_capacity + RECEIPT_CAPACITY).context("claim funding capacity is too small")?;
    if change == 0 {
        bail!("claim funding capacity is too small");
    }
    let mut inputs = vec![active_ref.clone()];
    inputs.extend_from_slice(funding_cells(funding));
    let lock_args = lock_override.unwrap_or(&terms.lender);
    let payout_data = payout_override.unwrap_or(&material.claim_payout_data);
    let mut witnesses = vec![witness(
        PATH_CLAIM,
        &material.terms_data,
        &material.active_data,
        &material.signed_intent,
        &material.borrower_sig,
        &material.lender_sig,
    )];
    witnesses.extend(vec!["0x".into(); funding_cells(funding).len()]);
    Ok(transaction(
        &inputs,
        vec![
            json!({"capacity": format!("0x{:x}", active_ref["capacity"].as_u64().unwrap()), "lock": always_success_lock("0x"), "type": lifecycle_type(lifecycle_hash)}),
            json!({"capacity": format!("0x{claim_capacity:x}"), "lock": always_success_lock(&hex0x(lock_args)), "type": Value::Null}),
            json!({"capacity": format!("0x{RECEIPT_CAPACITY:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
            json!({"capacity": format!("0x{change:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
        ],
        vec![hex0x(&material.closed_data), hex0x(payout_data), hex0x(&material.receipt_data), "0x".into()],
        deps,
        witnesses,
        vec![header.into()],
    ))
}

fn epoch_number(header: &Value) -> Result<u64> {
    let encoded = header["epoch"].as_str().context("tip header has no epoch")?;
    Ok(u64::from_str_radix(encoded.trim_start_matches("0x"), 16)? & ((1 << 24) - 1))
}

fn wait_epoch_after(devnet: &CkbDevnet, expiry: u64) -> Result<Value> {
    let mut last = Value::Null;
    for _ in 0..5_000 {
        last = devnet.rpc("get_tip_header", vec![])?;
        if epoch_number(&last)? > expiry {
            return Ok(last);
        }
        devnet.rpc("generate_block", vec![])?;
    }
    bail!("devnet epoch did not advance past expiry {expiry}; last epoch={}", last["epoch"])
}

struct OriginRun {
    material: OriginMaterial,
    active_ref: Value,
    dry_run: Value,
    commit: Value,
    active_live: Value,
    payout_live: Value,
    receipt_live: Value,
}

fn submit_origin(devnet: &mut CkbDevnet, lifecycle_hash: &str, deps: &[Value], terms: &Terms, label: &str) -> Result<OriginRun> {
    let header = devnet.rpc("get_tip_header", vec![])?;
    let now = epoch_number(&header)?;
    let material = origin_material(terms, now, false, false)?;
    let required = STATE_CAPACITY + RECEIPT_CAPACITY + PAYOUT_CAPACITY_BASE + terms.principal_amount;
    let funding = devnet.collect_spendable(required + 100 * SHANNONS)?;
    let tx = build_origin_tx(
        &funding,
        lifecycle_hash,
        deps.to_vec(),
        header["hash"].as_str().context("tip header has no hash")?,
        terms,
        &material,
    )?;
    let dry_run = devnet.rpc("dry_run_transaction", vec![tx.clone()])?;
    let commit = devnet.submit_and_commit(&tx, label)?;
    let hash = commit["tx_hash"].as_str().context("origin commit has no transaction hash")?;
    let type_script = lifecycle_type(lifecycle_hash);
    let active_live = devnet.assert_live_cell(
        hash,
        0,
        &format!("{label} active"),
        Some(STATE_CAPACITY),
        Some(&always_success_lock("0x")),
        Some(&type_script),
        Some(&material.active_data),
    )?;
    let payout_live = devnet.assert_live_cell(
        hash,
        1,
        &format!("{label} principal payout"),
        Some(PAYOUT_CAPACITY_BASE + terms.principal_amount),
        Some(&always_success_lock(&hex0x(&terms.borrower))),
        Some(&Value::Null),
        Some(&material.payout_data),
    )?;
    let receipt_live = devnet.assert_live_cell(
        hash,
        2,
        &format!("{label} receipt"),
        Some(RECEIPT_CAPACITY),
        Some(&always_success_lock("0x")),
        Some(&Value::Null),
        Some(&material.receipt_data),
    )?;
    Ok(OriginRun {
        active_ref: json!({"tx_hash": hash, "index": 0, "capacity": STATE_CAPACITY}),
        material,
        dry_run,
        commit,
        active_live,
        payout_live,
        receipt_live,
    })
}

fn compile(root: &Path, output: &Path) -> Result<()> {
    let status = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--locked",
            "--bin",
            "cellc",
            "--",
            "proposals/novaseal/agreement-profile-v0/src/nova_agreement_lifecycle_type.cell",
            "--target-profile",
            "ckb",
            "--target",
            "riscv64-elf",
            "--entry-action",
            "nova_agreement_lifecycle",
            "-o",
            output.to_str().context("agreement lifecycle output path is not UTF-8")?,
        ])
        .current_dir(root)
        .status()?;
    if !status.success() {
        bail!("failed to compile NovaSeal Agreement lifecycle");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    root: &Path,
    ckb_repo: Option<&Path>,
    ckb_bin: Option<&Path>,
    output: Option<&Path>,
    run_dir: Option<&Path>,
    pretty: bool,
    keep_node: bool,
) -> Result<i32> {
    let root = fs::canonicalize(root)?;
    let ckb_repo = fs::canonicalize(ckb_repo.map(Path::to_path_buf).unwrap_or_else(|| root.parent().unwrap().join("ckb")))?;
    let ckb_bin = resolve_ckb_bin(&ckb_repo, ckb_bin)?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let run_dir = run_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join(format!("target/novaseal-agreement-devnet-stateful-live/{timestamp}")));
    fs::create_dir_all(&run_dir)?;
    let run_dir = fs::canonicalize(run_dir)?;
    let lifecycle_path = run_dir.join("nova-agreement-lifecycle-type.elf");
    compile(&root, &lifecycle_path)?;
    let verifier_path = root.join("proposals/novaseal/v0-mvp-skeleton/target/novaseal-btc-verifier-riscv-shell-release.elf");
    if !verifier_path.is_file() {
        bail!("missing verifier ELF: {}", verifier_path.display());
    }
    let mut devnet = CkbDevnet::new(ckb_repo.clone(), ckb_bin.clone(), run_dir.clone())?;
    let mut report = json!({
        "schema": "novaseal-agreement-devnet-stateful-live-v0.1",
        "status": "running",
        "scenario": "agreement_profile_originate_repay_and_claim",
        "repo_root": root.display().to_string(),
        "ckb_repo": ckb_repo.display().to_string(),
        "ckb_bin": ckb_bin.display().to_string(),
        "run_dir": run_dir.display().to_string(),
    });
    let mut stage = "initializing";
    let scenario = (|| -> Result<()> {
        stage = "start devnet";
        devnet.start()?;
        stage = "deploy artifacts";
        let genesis = devnet.get_block_by_number(0)?;
        let always = always_success_dep(genesis["transactions"][0]["hash"].as_str().context("genesis cellbase hash is missing")?);
        let verifier = deploy_code(&mut devnet, "cellscript_btc_bip340_verifier_riscv", &fs::read(&verifier_path)?, &always)?;
        let lifecycle = deploy_code(&mut devnet, "nova_agreement_lifecycle_type", &fs::read(&lifecycle_path)?, &always)?;
        let lifecycle_hash = lifecycle["data_hash"].as_str().context("lifecycle data hash is missing")?.to_owned();
        let deps = vec![verifier["cell_dep"].clone(), lifecycle["cell_dep"].clone(), always];
        let source_paths = [
            "proposals/novaseal/agreement-profile-v0/Cell.toml",
            "proposals/novaseal/agreement-profile-v0/src",
            "proposals/novaseal/agreement-profile-v0/schemas",
            "proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier",
            "crates/cellscript-tools/src/novaseal_agreement_live.rs",
            "crates/cellscript-tools/src/ckb_devnet.rs",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
        let artifacts = BTreeMap::from([("verifier".into(), verifier_path.clone()), ("lifecycle".into(), lifecycle_path.clone())]);
        let source_provenance = provenance(&root, &source_paths, &artifacts)?;

        stage = "negative originate wrong lender signature";
        let negative_origin_header = devnet.rpc("get_tip_header", vec![])?;
        let negative_origin_now = epoch_number(&negative_origin_header)?;
        let wrong_lender_terms = make_terms(negative_origin_now, "wrong-lender-signature", None)?;
        let wrong_lender_material = origin_material(&wrong_lender_terms, negative_origin_now, false, true)?;
        let origin_required = STATE_CAPACITY + RECEIPT_CAPACITY + PAYOUT_CAPACITY_BASE + wrong_lender_terms.principal_amount;
        let funding = devnet.collect_spendable(origin_required + 100 * SHANNONS)?;
        let tx = build_origin_tx(
            &funding,
            &lifecycle_hash,
            deps.clone(),
            negative_origin_header["hash"].as_str().context("tip header has no hash")?,
            &wrong_lender_terms,
            &wrong_lender_material,
        )?;
        let wrong_lender_origin_reject = devnet.dry_run_rejects(
            &tx,
            "wrong lender signature originate",
            Some("Outputs[0].Type"),
            Some(&lifecycle_hash),
            Some(56),
        )?;

        stage = "negative originate non-CKB asset kind";
        let mut non_ckb_terms = make_terms(negative_origin_now, "non-ckb-asset-kind", None)?;
        non_ckb_terms.principal_kind = 1;
        let non_ckb_material = origin_material(&non_ckb_terms, negative_origin_now, false, false)?;
        let funding = devnet.collect_spendable(origin_required + 100 * SHANNONS)?;
        let tx = build_origin_tx(
            &funding,
            &lifecycle_hash,
            deps.clone(),
            negative_origin_header["hash"].as_str().context("tip header has no hash")?,
            &non_ckb_terms,
            &non_ckb_material,
        )?;
        let non_ckb_reject =
            devnet.dry_run_rejects(&tx, "non-CKB asset kind originate", Some("Outputs[0].Type"), Some(&lifecycle_hash), Some(5))?;

        stage = "valid repay-path originate";
        let repay_seed = devnet.rpc("get_tip_header", vec![])?;
        let repay_terms = make_terms(epoch_number(&repay_seed)?, "repay", None)?;
        let repay_origin = submit_origin(&mut devnet, &lifecycle_hash, &deps, &repay_terms, "agreement repay-path originate")?;

        stage = "negative repay wrong borrower signature";
        let negative_header = devnet.rpc("get_tip_header", vec![])?;
        let negative_now = epoch_number(&negative_header)?;
        let negative_material = repay_material(
            &repay_terms,
            &repay_origin.material.active,
            &repay_origin.material.latest_receipt_hash,
            negative_now,
            true,
        )?;
        let repay_required = RECEIPT_CAPACITY
            + PAYOUT_CAPACITY_BASE
            + negative_material.repayment_amount
            + PAYOUT_CAPACITY_BASE
            + repay_terms.collateral_amount;
        let funding = devnet.collect_spendable(repay_required + 100 * SHANNONS)?;
        let tx = build_repay_tx(
            &repay_origin.active_ref,
            &funding,
            &lifecycle_hash,
            deps.clone(),
            negative_header["hash"].as_str().context("tip header has no hash")?,
            &repay_terms,
            &negative_material,
            0,
            None,
            None,
        )?;
        let wrong_borrower_reject =
            devnet.dry_run_rejects(&tx, "wrong borrower signature repay", Some("Inputs[0].Type"), Some(&lifecycle_hash), Some(56))?;

        stage = "negative repay payout capacity short";
        let capacity_material = repay_material(
            &repay_terms,
            &repay_origin.material.active,
            &repay_origin.material.latest_receipt_hash,
            negative_now,
            false,
        )?;
        let funding = devnet.collect_spendable(repay_required + 100 * SHANNONS)?;
        let tx = build_repay_tx(
            &repay_origin.active_ref,
            &funding,
            &lifecycle_hash,
            deps.clone(),
            negative_header["hash"].as_str().context("tip header has no hash")?,
            &repay_terms,
            &capacity_material,
            -1,
            None,
            None,
        )?;
        let capacity_reject =
            devnet.dry_run_rejects(&tx, "repay payout capacity short", Some("Inputs[0].Type"), Some(&lifecycle_hash), Some(5))?;

        stage = "negative repay payout lock args mismatch";
        let wrong_lock = ckb_hash(b"wrong lender payout lock args");
        let funding = devnet.collect_spendable(repay_required + 100 * SHANNONS)?;
        let tx = build_repay_tx(
            &repay_origin.active_ref,
            &funding,
            &lifecycle_hash,
            deps.clone(),
            negative_header["hash"].as_str().context("tip header has no hash")?,
            &repay_terms,
            &capacity_material,
            0,
            Some(&wrong_lock),
            None,
        )?;
        let lock_reject =
            devnet.dry_run_rejects(&tx, "repay payout lock args mismatch", Some("Inputs[0].Type"), Some(&lifecycle_hash), Some(5))?;

        stage = "negative repay wrong payout amount";
        let mut wrong_payout = capacity_material.lender_payout.clone();
        wrong_payout.amount += 1;
        let wrong_payout_data = pack_payout(&wrong_payout);
        let funding = devnet.collect_spendable(repay_required + 100 * SHANNONS)?;
        let tx = build_repay_tx(
            &repay_origin.active_ref,
            &funding,
            &lifecycle_hash,
            deps.clone(),
            negative_header["hash"].as_str().context("tip header has no hash")?,
            &repay_terms,
            &capacity_material,
            0,
            None,
            Some(&wrong_payout_data),
        )?;
        let wrong_payout_reject =
            devnet.dry_run_rejects(&tx, "repay wrong payout amount", Some("Inputs[0].Type"), Some(&lifecycle_hash), Some(5))?;
        let agreement_type = lifecycle_type(&lifecycle_hash);
        let active_still_live = devnet.assert_live_cell(
            repay_origin.active_ref["tx_hash"].as_str().unwrap(),
            0,
            "post-negative repay active",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&agreement_type),
            Some(&repay_origin.material.active_data),
        )?;

        stage = "valid repay";
        let repay_header = devnet.rpc("get_tip_header", vec![])?;
        let repay_material = repay_material(
            &repay_terms,
            &repay_origin.material.active,
            &repay_origin.material.latest_receipt_hash,
            epoch_number(&repay_header)?,
            false,
        )?;
        let funding = devnet.collect_spendable(repay_required + 100 * SHANNONS)?;
        let repay_tx = build_repay_tx(
            &repay_origin.active_ref,
            &funding,
            &lifecycle_hash,
            deps.clone(),
            repay_header["hash"].as_str().context("tip header has no hash")?,
            &repay_terms,
            &repay_material,
            0,
            None,
            None,
        )?;
        let repay_dry = devnet.rpc("dry_run_transaction", vec![repay_tx.clone()])?;
        let repay_commit = devnet.submit_and_commit(&repay_tx, "agreement repay before expiry")?;
        let active_dead = devnet.wait_dead_cell(repay_origin.active_ref["tx_hash"].as_str().unwrap(), 0)?;
        let repay_hash = repay_commit["tx_hash"].as_str().unwrap();
        let closed_live = devnet.assert_live_cell(
            repay_hash,
            0,
            "repay closed agreement",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&agreement_type),
            Some(&repay_material.closed_data),
        )?;
        let lender_live = devnet.assert_live_cell(
            repay_hash,
            1,
            "repay lender repayment",
            Some(PAYOUT_CAPACITY_BASE + repay_material.repayment_amount),
            Some(&always_success_lock(&hex0x(&repay_terms.lender))),
            Some(&Value::Null),
            Some(&repay_material.lender_payout_data),
        )?;
        let borrower_live = devnet.assert_live_cell(
            repay_hash,
            2,
            "repay borrower collateral return",
            Some(PAYOUT_CAPACITY_BASE + repay_terms.collateral_amount),
            Some(&always_success_lock(&hex0x(&repay_terms.borrower))),
            Some(&Value::Null),
            Some(&repay_material.borrower_payout_data),
        )?;
        let repay_receipt_live = devnet.assert_live_cell(
            repay_hash,
            3,
            "repay receipt",
            Some(RECEIPT_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&Value::Null),
            Some(&repay_material.receipt_data),
        )?;

        stage = "valid claim-path originate";
        let claim_seed = devnet.rpc("get_tip_header", vec![])?;
        let claim_seed_now = epoch_number(&claim_seed)?;
        let claim_terms = make_terms(claim_seed_now, "claim", Some(claim_seed_now + 1))?;
        let claim_origin = submit_origin(&mut devnet, &lifecycle_hash, &deps, &claim_terms, "agreement claim-path originate")?;

        stage = "negative early claim";
        let early_header = devnet.rpc("get_tip_header", vec![])?;
        let early_material = claim_material(
            &claim_terms,
            &claim_origin.material.active,
            &claim_origin.material.latest_receipt_hash,
            epoch_number(&early_header)?,
            false,
        )?;
        let claim_required = RECEIPT_CAPACITY + PAYOUT_CAPACITY_BASE + early_material.claim_amount;
        let funding = devnet.collect_spendable(claim_required + 100 * SHANNONS)?;
        let tx = build_claim_tx(
            &claim_origin.active_ref,
            &funding,
            &lifecycle_hash,
            deps.clone(),
            early_header["hash"].as_str().context("tip header has no hash")?,
            &claim_terms,
            &early_material,
            0,
            None,
            None,
        )?;
        let early_reject =
            devnet.dry_run_rejects(&tx, "early claim before expiry", Some("Inputs[0].Type"), Some(&lifecycle_hash), Some(5))?;

        stage = "wait claim expiry";
        let claim_header = wait_epoch_after(&devnet, claim_terms.expiry)?;
        let claim_now = epoch_number(&claim_header)?;
        stage = "negative claim wrong lender signature";
        let wrong_claim_material =
            claim_material(&claim_terms, &claim_origin.material.active, &claim_origin.material.latest_receipt_hash, claim_now, true)?;
        let funding = devnet.collect_spendable(claim_required + 100 * SHANNONS)?;
        let tx = build_claim_tx(
            &claim_origin.active_ref,
            &funding,
            &lifecycle_hash,
            deps.clone(),
            claim_header["hash"].as_str().context("tip header has no hash")?,
            &claim_terms,
            &wrong_claim_material,
            0,
            None,
            None,
        )?;
        let wrong_claim_reject =
            devnet.dry_run_rejects(&tx, "wrong lender signature claim", Some("Inputs[0].Type"), Some(&lifecycle_hash), Some(56))?;
        let claim_active_still_live = devnet.assert_live_cell(
            claim_origin.active_ref["tx_hash"].as_str().unwrap(),
            0,
            "post-negative claim active",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&agreement_type),
            Some(&claim_origin.material.active_data),
        )?;

        stage = "valid claim";
        let claim_material =
            claim_material(&claim_terms, &claim_origin.material.active, &claim_origin.material.latest_receipt_hash, claim_now, false)?;
        let funding = devnet.collect_spendable(claim_required + 100 * SHANNONS)?;
        let claim_tx = build_claim_tx(
            &claim_origin.active_ref,
            &funding,
            &lifecycle_hash,
            deps.clone(),
            claim_header["hash"].as_str().context("tip header has no hash")?,
            &claim_terms,
            &claim_material,
            0,
            None,
            None,
        )?;
        let claim_dry = devnet.rpc("dry_run_transaction", vec![claim_tx.clone()])?;
        let claim_commit = devnet.submit_and_commit(&claim_tx, "agreement claim after expiry")?;
        let claim_dead = devnet.wait_dead_cell(claim_origin.active_ref["tx_hash"].as_str().unwrap(), 0)?;
        let claim_hash = claim_commit["tx_hash"].as_str().unwrap();
        let claim_closed_live = devnet.assert_live_cell(
            claim_hash,
            0,
            "claim closed agreement",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&agreement_type),
            Some(&claim_material.closed_data),
        )?;
        let claim_payout_live = devnet.assert_live_cell(
            claim_hash,
            1,
            "claim lender default claim",
            Some(PAYOUT_CAPACITY_BASE + claim_material.claim_amount),
            Some(&always_success_lock(&hex0x(&claim_terms.lender))),
            Some(&Value::Null),
            Some(&claim_material.claim_payout_data),
        )?;
        let claim_receipt_live = devnet.assert_live_cell(
            claim_hash,
            2,
            "claim receipt",
            Some(RECEIPT_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&Value::Null),
            Some(&claim_material.receipt_data),
        )?;

        report.as_object_mut().unwrap().extend(
            json!({
                "status": "passed",
                "live_devnet_rpc_executed": true,
                "stateful_lifecycle_executed": true,
                "ckb_log": devnet.log_path.display().to_string(),
                "rpc_url": devnet.rpc_url,
                "artifacts": {"verifier": verifier, "lifecycle": lifecycle},
                "provenance": source_provenance,
                "repay_terms": terms_json(&repay_terms),
                "claim_terms": terms_json(&claim_terms),
                "originate": {
                    "dry_run_cycles": repay_origin.dry_run["cycles"],
                    "commit": repay_origin.commit,
                    "active_live": repay_origin.active_live["status"] == "live",
                    "principal_payout_live": repay_origin.payout_live["status"] == "live",
                    "receipt_live": repay_origin.receipt_live["status"] == "live",
                    "active_data_hash": hex0x(&ckb_hash(&repay_origin.material.active_data)),
                    "principal_payout_data_hash": ckb_hash_hex(&repay_origin.material.payout_data),
                    "signed_intent_hash": hex0x(&repay_origin.material.signed_intent_hash),
                    "latest_receipt_hash": hex0x(&repay_origin.material.latest_receipt_hash),
                },
                "repay": {
                    "dry_run_cycles": repay_dry["cycles"], "commit": repay_commit,
                    "old_active_not_live": active_dead["status"] != "live", "closed_live": closed_live["status"] == "live",
                    "lender_repayment_live": lender_live["status"] == "live", "borrower_collateral_return_live": borrower_live["status"] == "live",
                    "receipt_live": repay_receipt_live["status"] == "live", "closed_data_hash": hex0x(&ckb_hash(&repay_material.closed_data)),
                    "lender_payout_data_hash": ckb_hash_hex(&repay_material.lender_payout_data),
                    "borrower_payout_data_hash": ckb_hash_hex(&repay_material.borrower_payout_data),
                    "signed_intent_hash": hex0x(&repay_material.signed_intent_hash), "latest_receipt_hash": hex0x(&repay_material.latest_receipt_hash),
                },
                "claim_originate": {
                    "dry_run_cycles": claim_origin.dry_run["cycles"], "commit": claim_origin.commit,
                    "active_live": claim_origin.active_live["status"] == "live", "principal_payout_live": claim_origin.payout_live["status"] == "live",
                    "receipt_live": claim_origin.receipt_live["status"] == "live", "latest_receipt_hash": hex0x(&claim_origin.material.latest_receipt_hash),
                },
                "claim": {
                    "dry_run_cycles": claim_dry["cycles"], "commit": claim_commit, "old_active_not_live": claim_dead["status"] != "live",
                    "closed_live": claim_closed_live["status"] == "live", "lender_default_claim_live": claim_payout_live["status"] == "live",
                    "receipt_live": claim_receipt_live["status"] == "live", "closed_data_hash": hex0x(&ckb_hash(&claim_material.closed_data)),
                    "claim_payout_data_hash": ckb_hash_hex(&claim_material.claim_payout_data),
                    "signed_intent_hash": hex0x(&claim_material.signed_intent_hash), "latest_receipt_hash": hex0x(&claim_material.latest_receipt_hash),
                    "timepoint": claim_now,
                },
                "negative_cases": {
                    "wrong_lender_signature_dry_run": wrong_lender_origin_reject,
                    "non_ckb_asset_kind_dry_run": non_ckb_reject,
                    "wrong_borrower_signature_dry_run": wrong_borrower_reject,
                    "repay_payout_capacity_short_dry_run": capacity_reject,
                    "repay_payout_lock_args_mismatch_dry_run": lock_reject,
                    "repay_wrong_payout_amount_dry_run": wrong_payout_reject,
                    "early_claim_dry_run": early_reject,
                    "wrong_lender_claim_signature_dry_run": wrong_claim_reject,
                    "post_negative_active_still_live": active_still_live["status"] == "live",
                    "post_claim_negative_active_still_live": claim_active_still_live["status"] == "live",
                },
            })
            .as_object()
            .unwrap()
            .clone(),
        );
        Ok(())
    })();
    if let Err(error) = scenario {
        report["status"] = json!("failed");
        report["stage"] = json!(stage);
        report["error"] = json!(error.to_string());
        report["ckb_log"] = json!(devnet.log_path.display().to_string());
        report["rpc_url"] = json!(devnet.rpc_url);
    }
    if !keep_node {
        devnet.stop();
    }
    let output = match output {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => root.join(path),
        None => root.join("target/novaseal-agreement-devnet-stateful-live.json"),
    };
    fs::create_dir_all(output.parent().context("output path has no parent")?)?;
    let text = if pretty { python_json_pretty(&report)? } else { python_json_default(&report)? };
    fs::write(&output, format!("{text}\n"))?;
    println!(
        "wrote {} status={} live_devnet_rpc_executed={}",
        output.display(),
        report["status"].as_str().unwrap_or("failed"),
        report["live_devnet_rpc_executed"].as_bool().unwrap_or(false)
    );
    Ok(if report["status"] == "passed" { 0 } else { 1 })
}

fn terms_json(terms: &Terms) -> Value {
    json!({
        "agreement_id": hex0x(&terms.agreement_id),
        "terms_hash": hex0x(&terms.terms_hash),
        "borrower_authority_hash": hex0x(&terms.borrower),
        "lender_authority_hash": hex0x(&terms.lender),
        "principal_amount": terms.principal_amount,
        "collateral_amount": terms.collateral_amount,
        "fixed_fee_amount": terms.fixed_fee,
        "expiry_timepoint": terms.expiry,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_lender_key_matches_python_contract() {
        assert_eq!(
            hex0x(&xonly_pubkey(&LENDER_SECRET).unwrap()),
            "0x4f355bdcb7cc0af728ef3cceb9615d90684bb5b2ca5f859ab0f0b704075871aa"
        );
    }

    #[test]
    fn origin_material_is_deterministic() {
        let terms = make_terms(42, "parity", None).unwrap();
        let first = origin_material(&terms, 42, false, false).unwrap();
        let second = origin_material(&terms, 42, false, false).unwrap();
        assert_eq!(first.active_data, second.active_data);
        assert_eq!(first.signed_intent_hash, second.signed_intent_hash);
        assert_eq!(hex0x(&ckb_hash(&first.active_data)), "0xba0a5845b3b3915c3852980d89277fd1ee0a98cb0d511a578599cdbd08847359");
        assert_eq!(hex0x(&first.signed_intent_hash), "0x32596edbe701807be5ab8835ee9381d3ad31ed73569800a8834b4fc7686ff201");
        assert_eq!(hex0x(&first.latest_receipt_hash), "0xf13b028a01060cd4af902360a32024e608f189638c98e84769f2e480b42e2241");
        assert_eq!(ckb_hash_hex(&first.payout_data), "0x716280b50ce2b3c50d94be67ca79726e02a83c2bcf671d7061921783dded9c80");
    }
}
