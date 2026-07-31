use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::ckb_devnet::{
    always_success_dep, always_success_lock, ckb_hash, deploy_code, entry_witness_input_type_hex, funding_cells, hex0x, provenance,
    resolve_ckb_bin, schnorr_sign, transaction, u16_bytes, u32_bytes, u64_bytes, u8_bytes, xonly_pubkey, CkbDevnet, RECEIPT_CAPACITY,
    SHANNONS, STATE_CAPACITY, TEST_AUX_RAND, TEST_SECRET_KEY, ZERO_HASH,
};
use crate::novaseal_planned_live::{compile_contract, contract_report_header, lifecycle_type, Contract};

const OP_FINALIZE: u64 = 0;
const OP_INITIALIZE: u64 = 255;
const STATUS_ACTIVE: u64 = 1;
const STATUS_FINALIZED: u64 = 2;
const CKB_SECRET: [u8; 32] = [0x22; 32];
const CKB_AUX: [u8; 32] = [0x42; 32];
type Hash = [u8; 32];

#[derive(Clone)]
struct Base {
    seal: Hash,
    policy: Hash,
    btc_owner: Hash,
    ckb_authority: Hash,
    sealed_txid: Hash,
    sealed_vout: u64,
    sealed_amount: u64,
    script_pubkey: Hash,
    sealed_utxo: Hash,
    initial_state: Hash,
    final_state: Hash,
    btc_closure: Hash,
    btc_txid: Hash,
    btc_wtxid: Hash,
    spend_input: u64,
    maturity: u64,
    expiry: u64,
}

#[derive(Clone)]
struct Cell {
    seal: Hash,
    policy: Hash,
    btc_owner: Hash,
    ckb_authority: Hash,
    sealed_utxo: Hash,
    state: Hash,
    status: u64,
    receipt: Hash,
    nonce: u64,
    maturity: u64,
    expiry: u64,
}

struct Material {
    old_cell: Cell,
    old_cell_data: Vec<u8>,
    new_cell: Cell,
    new_cell_data: Vec<u8>,
    receipt_data: Vec<u8>,
    signed_intent: Vec<u8>,
    signed_hash: Hash,
    btc_signature: Vec<u8>,
    ckb_signature: Vec<u8>,
    finality: Hash,
    btc_closure: Hash,
    receipt_hash: Hash,
}

fn append(out: &mut Vec<u8>, chunks: &[&[u8]]) {
    for chunk in chunks {
        out.extend_from_slice(chunk);
    }
}

fn utxo_commitment(txid: &Hash, vout: u64, amount: u64, script_pubkey: &Hash) -> Vec<u8> {
    let mut out = Vec::new();
    append(&mut out, &[txid, &u32_bytes(vout as usize), &u64_bytes(amount), script_pubkey]);
    out
}

fn base(label: &str) -> Result<Base> {
    let sealed_txid = ckb_hash(format!("NovaSeal dual sealed BTC txid {label}").as_bytes());
    let sealed_vout = 1;
    let sealed_amount = 350_000;
    let script_pubkey = ckb_hash(format!("NovaSeal dual sealed BTC script pubkey {label}").as_bytes());
    let sealed_utxo = ckb_hash(&utxo_commitment(&sealed_txid, sealed_vout, sealed_amount, &script_pubkey));
    Ok(Base {
        seal: ckb_hash(format!("NovaSeal dual seal {label}").as_bytes()),
        policy: ckb_hash(format!("NovaSeal dual policy {label}").as_bytes()),
        btc_owner: xonly_pubkey(&TEST_SECRET_KEY)?,
        ckb_authority: xonly_pubkey(&CKB_SECRET)?,
        sealed_txid,
        sealed_vout,
        sealed_amount,
        script_pubkey,
        sealed_utxo,
        initial_state: ckb_hash(format!("NovaSeal dual active CKB state {label}").as_bytes()),
        final_state: ckb_hash(format!("NovaSeal dual finalized CKB state {label}").as_bytes()),
        btc_closure: ckb_hash(format!("NovaSeal dual BTC closure {label}").as_bytes()),
        btc_txid: ckb_hash(format!("NovaSeal dual BTC closure txid {label}").as_bytes()),
        btc_wtxid: ckb_hash(format!("NovaSeal dual BTC closure wtxid {label}").as_bytes()),
        spend_input: 0,
        maturity: 0,
        expiry: (1_u64 << 63) - 1,
    })
}

fn zero_cell() -> Cell {
    Cell {
        seal: ZERO_HASH,
        policy: ZERO_HASH,
        btc_owner: ZERO_HASH,
        ckb_authority: ZERO_HASH,
        sealed_utxo: ZERO_HASH,
        state: ZERO_HASH,
        status: 0,
        receipt: ZERO_HASH,
        nonce: 0,
        maturity: 0,
        expiry: 0,
    }
}

fn pack_state(cell: &Cell) -> Vec<u8> {
    let mut out = u16_bytes(0);
    append(
        &mut out,
        &[
            &cell.seal,
            &cell.policy,
            &cell.btc_owner,
            &cell.ckb_authority,
            &cell.sealed_utxo,
            &cell.state,
            &u8_bytes(cell.status),
            &u64_bytes(cell.nonce),
            &u64_bytes(cell.maturity),
            &u64_bytes(cell.expiry),
        ],
    );
    out
}

fn pack_cell(cell: &Cell) -> Vec<u8> {
    let mut out = u16_bytes(0);
    append(
        &mut out,
        &[
            &cell.seal,
            &cell.policy,
            &cell.btc_owner,
            &cell.ckb_authority,
            &cell.sealed_utxo,
            &cell.state,
            &u8_bytes(cell.status),
            &cell.receipt,
            &u64_bytes(cell.nonce),
            &u64_bytes(cell.maturity),
            &u64_bytes(cell.expiry),
        ],
    );
    out
}

fn finality(sealed: &Hash, closure: &Hash, old_state: &Hash, new_state: &Hash, maturity: u64) -> Vec<u8> {
    let mut out = Vec::new();
    append(&mut out, &[sealed, closure, old_state, new_state, &u64_bytes(maturity), &ZERO_HASH]);
    out
}

#[allow(clippy::too_many_arguments)]
fn pack_core(
    op: u64,
    base: &Base,
    closure: &Hash,
    old_state: &Hash,
    new_state: &Hash,
    old_status: u64,
    new_status: u64,
    old_nonce: u64,
    new_nonce: u64,
) -> Vec<u8> {
    let mut out = Vec::new();
    append(
        &mut out,
        &[
            &u8_bytes(op),
            &base.seal,
            &base.policy,
            &base.btc_owner,
            &base.ckb_authority,
            &base.sealed_utxo,
            closure,
            old_state,
            new_state,
            &u64_bytes(base.maturity),
            &u8_bytes(old_status),
            &u8_bytes(new_status),
            &u64_bytes(old_nonce),
            &u64_bytes(new_nonce),
            &u64_bytes(base.expiry),
            &ZERO_HASH,
        ],
    );
    out
}

#[allow(clippy::too_many_arguments)]
fn pack_receipt(
    base: &Base,
    closure: &Hash,
    old_state: &Hash,
    new_state: &Hash,
    old_nonce: u64,
    new_nonce: u64,
    core_hash: &Hash,
    signed_hash: Option<&Hash>,
    receipt_hash: Option<&Hash>,
) -> Vec<u8> {
    let mut out = Vec::new();
    append(
        &mut out,
        &[
            &u8_bytes(OP_FINALIZE),
            &base.seal,
            &base.policy,
            &base.btc_owner,
            &base.ckb_authority,
            &base.sealed_utxo,
            closure,
            old_state,
            new_state,
            &u8_bytes(STATUS_ACTIVE),
            &u8_bytes(STATUS_FINALIZED),
            &u64_bytes(old_nonce),
            &u64_bytes(new_nonce),
            core_hash,
        ],
    );
    if let (Some(signed_hash), Some(receipt_hash)) = (signed_hash, receipt_hash) {
        append(
            &mut out,
            &[signed_hash, &ZERO_HASH, receipt_hash, &base.ckb_authority, &u64_bytes(base.maturity), &u64_bytes(base.expiry)],
        );
    } else {
        out.extend_from_slice(&ZERO_HASH);
    }
    out
}

fn canonical(op: u64, base: &Base, old_state: &Hash, new_state: &Hash, old_nonce: u64, new_nonce: u64, body: &Hash) -> Hash {
    let mut out = Vec::new();
    append(
        &mut out,
        &[
            &base.seal,
            &base.policy,
            &u8_bytes(op),
            &u8_bytes(op),
            &base.seal,
            old_state,
            new_state,
            &u64_bytes(old_nonce),
            &u64_bytes(new_nonce),
            &u64_bytes(base.expiry),
            &base.ckb_authority,
            body,
            &ZERO_HASH,
        ],
    );
    ckb_hash(&out)
}

fn signature(secret: &[u8; 32], aux: &[u8; 32], hash: &Hash, mutate: bool) -> Result<Vec<u8>> {
    let (public, signed) = schnorr_sign(hash, secret, aux)?;
    let mut out = Vec::with_capacity(96);
    out.extend_from_slice(&public);
    out.extend_from_slice(&signed);
    if mutate {
        *out.last_mut().unwrap() ^= 1;
    }
    Ok(out)
}

fn material(op: u64, base: &Base, old: Option<&Cell>, mutate_btc: bool, mutate_ckb: bool, zero_closure: bool) -> Result<Material> {
    let (old_status, new_status, old_nonce, new_nonce, old_state, new_state, closure, new_cell, new_commitment, old_commitment) =
        match op {
            OP_INITIALIZE => {
                let next = Cell {
                    seal: base.seal,
                    policy: base.policy,
                    btc_owner: base.btc_owner,
                    ckb_authority: base.ckb_authority,
                    sealed_utxo: base.sealed_utxo,
                    state: base.initial_state,
                    status: STATUS_ACTIVE,
                    receipt: ZERO_HASH,
                    nonce: 0,
                    maturity: base.maturity,
                    expiry: base.expiry,
                };
                let new_commitment = ckb_hash(&pack_state(&next));
                (0, STATUS_ACTIVE, 0, 0, ZERO_HASH, base.initial_state, ZERO_HASH, next, new_commitment, ZERO_HASH)
            }
            OP_FINALIZE => {
                let old = old.context("dual-seal finalization material requires an old cell")?;
                let closure = if zero_closure { ZERO_HASH } else { base.btc_closure };
                let finality = ckb_hash(&finality(&old.sealed_utxo, &closure, &old.state, &base.final_state, old.maturity));
                (
                    STATUS_ACTIVE,
                    STATUS_FINALIZED,
                    old.nonce,
                    old.nonce + 1,
                    old.state,
                    base.final_state,
                    closure,
                    zero_cell(),
                    finality,
                    ckb_hash(&pack_state(old)),
                )
            }
            _ => bail!("unknown dual-seal op {op}"),
        };
    let core = pack_core(op, base, &closure, &old_state, &new_state, old_status, new_status, old_nonce, new_nonce);
    let core_hash = ckb_hash(&core);
    let receipt_hash = if op == OP_FINALIZE {
        ckb_hash(&pack_receipt(base, &closure, &old_state, &new_state, old_nonce, new_nonce, &core_hash, None, None))
    } else {
        ZERO_HASH
    };
    let canonical = canonical(op, base, &old_commitment, &new_commitment, old_nonce, new_nonce, &core_hash);
    let mut signed_intent = core;
    append(&mut signed_intent, &[&canonical, &receipt_hash]);
    let signed_hash = ckb_hash(&signed_intent);
    let receipt_data = if op == OP_FINALIZE {
        pack_receipt(base, &closure, &old_state, &new_state, old_nonce, new_nonce, &core_hash, Some(&signed_hash), Some(&receipt_hash))
    } else {
        Vec::new()
    };
    let old_value = old.cloned().unwrap_or_else(zero_cell);
    Ok(Material {
        old_cell_data: pack_cell(&old_value),
        old_cell: old_value,
        new_cell: new_cell.clone(),
        new_cell_data: pack_cell(&new_cell),
        receipt_data,
        signed_intent,
        signed_hash,
        btc_signature: signature(&TEST_SECRET_KEY, &TEST_AUX_RAND, &signed_hash, mutate_btc)?,
        ckb_signature: signature(&CKB_SECRET, &CKB_AUX, &signed_hash, mutate_ckb)?,
        finality: new_commitment,
        btc_closure: closure,
        receipt_hash,
    })
}

fn witness(op: u64, material: &Material) -> String {
    let mut out = b"CSARGv1\0".to_vec();
    out.extend_from_slice(&u8_bytes(op));
    for value in [
        material.old_cell_data.as_slice(),
        material.signed_intent.as_slice(),
        material.btc_signature.as_slice(),
        material.ckb_signature.as_slice(),
    ] {
        out.extend_from_slice(&u32_bytes(value.len()));
        out.extend_from_slice(value);
    }
    entry_witness_input_type_hex(&out)
}

fn build_initialize(funding: &Value, lifecycle_hash: &str, deps: Vec<Value>, header: &str, material: &Material) -> Result<Value> {
    let total = funding["total_capacity"].as_u64().context("dual-seal initialize funding total is missing")?;
    let change = total.checked_sub(STATE_CAPACITY).context("dual-seal initialize funding capacity is too small")?;
    if change == 0 {
        bail!("dual-seal initialize funding capacity is too small");
    }
    let cells = funding_cells(funding);
    let mut witnesses = vec![witness(OP_INITIALIZE, material)];
    witnesses.extend(vec!["0x".into(); cells.len().saturating_sub(1)]);
    Ok(transaction(
        cells,
        vec![
            json!({"capacity": format!("0x{STATE_CAPACITY:x}"), "lock": always_success_lock("0x"), "type": lifecycle_type(lifecycle_hash)}),
            json!({"capacity": format!("0x{change:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
        ],
        vec![hex0x(&material.new_cell_data), "0x".into()],
        deps,
        witnesses,
        vec![header.into()],
    ))
}

fn build_finalize(old_ref: &Value, funding: &Value, deps: Vec<Value>, header: &str, material: &Material) -> Result<Value> {
    let total = old_ref["capacity"].as_u64().context("dual-seal old ref capacity is missing")?
        + funding["total_capacity"].as_u64().context("dual-seal funding total is missing")?;
    let change = total.checked_sub(RECEIPT_CAPACITY).context("dual-seal finalize funding capacity is too small")?;
    if change == 0 {
        bail!("dual-seal finalize funding capacity is too small");
    }
    let mut inputs = vec![old_ref.clone()];
    inputs.extend_from_slice(funding_cells(funding));
    let mut witnesses = vec![witness(OP_FINALIZE, material)];
    witnesses.extend(vec!["0x".into(); funding_cells(funding).len()]);
    Ok(transaction(
        &inputs,
        vec![
            json!({"capacity": format!("0x{RECEIPT_CAPACITY:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
            json!({"capacity": format!("0x{change:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
        ],
        vec![hex0x(&material.receipt_data), "0x".into()],
        deps,
        witnesses,
        vec![header.into()],
    ))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    root: &Path,
    ckb_repo: Option<&Path>,
    ckb_bin: Option<&Path>,
    run_dir: Option<&Path>,
    contract: Contract,
    keep_node: bool,
) -> Result<Value> {
    let root = fs::canonicalize(root)?;
    let ckb_repo = fs::canonicalize(ckb_repo.map(Path::to_path_buf).unwrap_or_else(|| root.parent().unwrap().join("ckb")))?;
    let ckb_bin = resolve_ckb_bin(&ckb_repo, ckb_bin)?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let run_dir = run_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join(format!("target/novaseal-dual-seal-devnet-stateful-live/{timestamp}")));
    fs::create_dir_all(&run_dir)?;
    let run_dir = fs::canonicalize(run_dir)?;
    let lifecycle_path = run_dir.join("nova-dual-seal-lifecycle-type.elf");
    compile_contract(&root, contract, &lifecycle_path)?;
    let verifier_path = root.join("proposals/novaseal/v0-mvp-skeleton/target/novaseal-btc-verifier-riscv-shell-release.elf");
    if !verifier_path.is_file() {
        bail!("missing verifier ELF: {}", verifier_path.display());
    }
    let mut devnet = CkbDevnet::new(ckb_repo.clone(), ckb_bin.clone(), run_dir.clone())?;
    let mut report = contract_report_header(contract, "dual_seal_initialize_then_finalize", &root, &ckb_repo, &ckb_bin, &run_dir);
    report["finality_scope"] = json!(
        "live CKB finalisation executes the maturity guard and both BIP340 authorities over a declared BTC closure commitment; public BTC SPV/indexer closure evidence remains separate production evidence"
    );
    let mut stage = "initializing";
    let scenario = (|| -> Result<()> {
        stage = "start devnet";
        devnet.start()?;
        stage = "deploy artifacts";
        let genesis = devnet.get_block_by_number(0)?;
        let always = always_success_dep(genesis["transactions"][0]["hash"].as_str().context("genesis hash is missing")?);
        let verifier = deploy_code(&mut devnet, "cellscript_btc_bip340_verifier_riscv", &fs::read(&verifier_path)?, &always)?;
        let lifecycle = deploy_code(&mut devnet, "nova_dual_seal_lifecycle_type", &fs::read(&lifecycle_path)?, &always)?;
        let lifecycle_hash = lifecycle["data_hash"].as_str().context("lifecycle hash is missing")?.to_owned();
        let deps = vec![verifier["cell_dep"].clone(), lifecycle["cell_dep"].clone(), always];
        let source_paths = [
            "proposals/novaseal/dual-seal-profile-v0/Cell.toml",
            "proposals/novaseal/dual-seal-profile-v0/src",
            "proposals/novaseal/dual-seal-profile-v0/schemas",
            "proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier",
            "crates/cellscript-tools/src/novaseal_planned_dual.rs",
            "crates/cellscript-tools/src/ckb_devnet.rs",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
        let artifacts = BTreeMap::from([("verifier".into(), verifier_path.clone()), ("lifecycle".into(), lifecycle_path.clone())]);
        let source_provenance = provenance(&root, &source_paths, &artifacts)?;
        let base = base("live")?;
        let type_script = lifecycle_type(&lifecycle_hash);

        stage = "valid initialize";
        let initialize = material(OP_INITIALIZE, &base, None, false, false, false)?;
        let header = devnet.rpc("get_tip_header", vec![])?;
        let funding = devnet.collect_spendable(STATE_CAPACITY + 100 * SHANNONS)?;
        let tx = build_initialize(&funding, &lifecycle_hash, deps.clone(), header["hash"].as_str().unwrap(), &initialize)?;
        let initialize_dry = devnet.rpc("dry_run_transaction", vec![tx.clone()])?;
        let initialize_commit = devnet.submit_and_commit(&tx, "dual-seal initialize")?;
        let initialize_hash = initialize_commit["tx_hash"].as_str().unwrap();
        let initial_live = devnet.assert_live_cell(
            initialize_hash,
            0,
            "dual-seal active state",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&initialize.new_cell_data),
        )?;
        let initial_ref = json!({"tx_hash": initialize_hash, "index": 0, "capacity": STATE_CAPACITY});

        stage = "negative wrong BTC owner signature";
        let negative_header = devnet.rpc("get_tip_header", vec![])?;
        let wrong_btc = material(OP_FINALIZE, &base, Some(&initialize.new_cell), true, false, false)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_finalize(&initial_ref, &funding, deps.clone(), negative_header["hash"].as_str().unwrap(), &wrong_btc)?;
        let wrong_btc_reject = devnet.dry_run_rejects(
            &tx,
            "dual-seal wrong BTC owner signature",
            Some("Inputs[0].Type"),
            Some(&lifecycle_hash),
            Some(56),
        )?;

        stage = "negative wrong CKB authority signature";
        let wrong_ckb = material(OP_FINALIZE, &base, Some(&initialize.new_cell), false, true, false)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_finalize(&initial_ref, &funding, deps.clone(), negative_header["hash"].as_str().unwrap(), &wrong_ckb)?;
        let wrong_ckb_reject = devnet.dry_run_rejects(
            &tx,
            "dual-seal wrong CKB authority signature",
            Some("Inputs[0].Type"),
            Some(&lifecycle_hash),
            Some(56),
        )?;

        stage = "negative missing BTC closure";
        let missing = material(OP_FINALIZE, &base, Some(&initialize.new_cell), false, false, true)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_finalize(&initial_ref, &funding, deps.clone(), negative_header["hash"].as_str().unwrap(), &missing)?;
        let missing_reject = devnet.dry_run_rejects(
            &tx,
            "dual-seal missing BTC closure commitment",
            Some("Inputs[0].Type"),
            Some(&lifecycle_hash),
            Some(5),
        )?;
        let post_negative = devnet.assert_live_cell(
            initialize_hash,
            0,
            "post-negative dual-seal active state",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&initialize.new_cell_data),
        )?;

        stage = "valid finalize";
        let header = devnet.rpc("get_tip_header", vec![])?;
        let finalize = material(OP_FINALIZE, &base, Some(&initialize.new_cell), false, false, false)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_finalize(&initial_ref, &funding, deps, header["hash"].as_str().unwrap(), &finalize)?;
        let finalize_dry = devnet.rpc("dry_run_transaction", vec![tx.clone()])?;
        let commit = devnet.submit_and_commit(&tx, "dual-seal finalization")?;
        let old_dead = devnet.wait_dead_cell(initialize_hash, 0)?;
        let receipt_live = devnet.assert_live_cell(
            commit["tx_hash"].as_str().unwrap(),
            0,
            "dual-seal final receipt",
            Some(RECEIPT_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&Value::Null),
            Some(&finalize.receipt_data),
        )?;
        report.as_object_mut().unwrap().extend(
            json!({
                "status": "passed", "live_devnet_rpc_executed": true, "stateful_lifecycle_executed": true,
                "ckb_log": devnet.log_path.display().to_string(), "rpc_url": devnet.rpc_url,
                "artifacts": {"verifier": verifier, "lifecycle": lifecycle}, "provenance": source_provenance,
                "initialize": {"dry_run_cycles": initialize_dry["cycles"], "commit": initialize_commit,
                    "state_live": initial_live["status"] == "live", "state_data_hash": hex0x(&ckb_hash(&initialize.new_cell_data))},
                "finalize_dual_seal": {"dry_run_cycles": finalize_dry["cycles"], "commit": commit,
                    "old_state_not_live": old_dead["status"] != "live", "receipt_live": receipt_live["status"] == "live",
                    "btc_closure_bound": finalize.btc_closure != ZERO_HASH, "ckb_maturity_executed": base.maturity == 0,
                    "dual_authority_executed": true, "finality_commitment_hash": hex0x(&finalize.finality),
                    "btc_closure_commitment_hash": hex0x(&finalize.btc_closure),
                    "public_btc_anchor": {"kind": "dual_seal_btc_closure", "anchor_source": "local_deterministic_fixture",
                        "sealed_btc_txid": hex0x(&base.sealed_txid), "sealed_btc_vout_index": base.sealed_vout,
                        "sealed_btc_amount_sats": base.sealed_amount, "script_pubkey_hash": hex0x(&base.script_pubkey),
                        "btc_txid": hex0x(&base.btc_txid), "btc_wtxid": hex0x(&base.btc_wtxid),
                        "spend_input_index": base.spend_input, "ckb_btc_commitment_hash": hex0x(&finalize.btc_closure),
                        "sealed_utxo_commitment_hash": hex0x(&finalize.old_cell.sealed_utxo)},
                    "signed_intent_hash": hex0x(&finalize.signed_hash), "receipt_hash": hex0x(&finalize.receipt_hash)},
                "negative_cases": {"wrong_btc_owner_signature_dry_run": wrong_btc_reject,
                    "wrong_ckb_authority_signature_dry_run": wrong_ckb_reject,
                    "btc_closure_commitment_missing_dry_run": missing_reject, "post_negative_state_still_live": post_negative["status"] == "live"},
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
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finalization_material_is_deterministic() {
        let base = base("parity").unwrap();
        let initial = material(OP_INITIALIZE, &base, None, false, false, false).unwrap();
        let finalized = material(OP_FINALIZE, &base, Some(&initial.new_cell), false, false, false).unwrap();
        assert_eq!(hex0x(&ckb_hash(&initial.new_cell_data)), "0xc1598e096376a0a4c7e4ed7bd627823729191b22a48b6e520868fbfd58c0ddb9");
        assert_eq!(hex0x(&finalized.signed_hash), "0x6654d1cc26fb7ad081c1f78fd9c76c0c83113993c3bee9d562fcc7234a45f5c7");
        assert_eq!(hex0x(&finalized.receipt_hash), "0x6bfbe13c9fa540ee1695536077a92a60ff693845300662ea85f3f8b27588c5f3");
        assert_eq!(hex0x(&ckb_hash(&finalized.receipt_data)), "0x4ff529a399edc077ff0fc108e198d5d186892e21b00f7d80de7b27ca5ad66c3a");
    }
}
