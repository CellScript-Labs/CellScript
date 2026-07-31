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

const OP_COMMIT: u64 = 0;
const OP_INITIALIZE: u64 = 255;
const STATUS_ACTIVE: u64 = 1;
const STATUS_COMMITTED: u64 = 2;
type Hash = [u8; 32];

#[derive(Clone)]
struct Base {
    seal: Hash,
    policy: Hash,
    committer: Hash,
    initial_state: Hash,
    committed_state: Hash,
    txid: Hash,
    wtxid: Hash,
    output_index: u64,
    amount_sats: u64,
    expiry: u64,
}

#[derive(Clone)]
struct Cell {
    seal: Hash,
    policy: Hash,
    committer: Hash,
    btc_commitment: Hash,
    state: Hash,
    status: u64,
    receipt: Hash,
    nonce: u64,
    expiry: u64,
}

struct Material {
    old_cell_data: Vec<u8>,
    new_cell: Cell,
    new_cell_data: Vec<u8>,
    receipt_data: Vec<u8>,
    signed_intent: Vec<u8>,
    signed_hash: Hash,
    signature: Vec<u8>,
    txid: Hash,
    wtxid: Hash,
    output_index: u64,
    amount_sats: u64,
    btc_commitment: Hash,
    transition_commitment: Hash,
    receipt_hash: Hash,
}

fn append(out: &mut Vec<u8>, chunks: &[&[u8]]) {
    for chunk in chunks {
        out.extend_from_slice(chunk);
    }
}

fn base(label: &str) -> Result<Base> {
    Ok(Base {
        seal: ckb_hash(format!("NovaSeal BTC transaction seal {label}").as_bytes()),
        policy: ckb_hash(format!("NovaSeal BTC transaction policy {label}").as_bytes()),
        committer: xonly_pubkey(&TEST_SECRET_KEY)?,
        initial_state: ckb_hash(format!("NovaSeal BTC transaction active state {label}").as_bytes()),
        committed_state: ckb_hash(format!("NovaSeal BTC transaction committed state {label}").as_bytes()),
        txid: ckb_hash(format!("NovaSeal BTC txid {label}").as_bytes()),
        wtxid: ckb_hash(format!("NovaSeal BTC wtxid {label}").as_bytes()),
        output_index: 2,
        amount_sats: 125_000,
        expiry: (1_u64 << 63) - 1,
    })
}

fn zero_cell() -> Cell {
    Cell {
        seal: ZERO_HASH,
        policy: ZERO_HASH,
        committer: ZERO_HASH,
        btc_commitment: ZERO_HASH,
        state: ZERO_HASH,
        status: 0,
        receipt: ZERO_HASH,
        nonce: 0,
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
            &cell.committer,
            &cell.btc_commitment,
            &cell.state,
            &u8_bytes(cell.status),
            &u64_bytes(cell.nonce),
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
            &cell.committer,
            &cell.btc_commitment,
            &cell.state,
            &u8_bytes(cell.status),
            &cell.receipt,
            &u64_bytes(cell.nonce),
            &u64_bytes(cell.expiry),
        ],
    );
    out
}

fn public_commitment(txid: &Hash, wtxid: &Hash, output_index: u64, amount: u64, transition: &Hash) -> Vec<u8> {
    let mut out = Vec::new();
    append(&mut out, &[txid, wtxid, &u32_bytes(output_index as usize), &u64_bytes(amount), transition]);
    out
}

#[allow(clippy::too_many_arguments)]
fn pack_core(
    op: u64,
    base: &Base,
    txid: &Hash,
    wtxid: &Hash,
    output_index: u64,
    amount: u64,
    old_state: &Hash,
    new_state: &Hash,
    transition: &Hash,
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
            &base.committer,
            txid,
            wtxid,
            &u32_bytes(output_index as usize),
            &u64_bytes(amount),
            old_state,
            new_state,
            transition,
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
    btc_commitment: &Hash,
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
            &u8_bytes(OP_COMMIT),
            &base.seal,
            &base.policy,
            &base.committer,
            btc_commitment,
            old_state,
            new_state,
            &u8_bytes(STATUS_ACTIVE),
            &u8_bytes(STATUS_COMMITTED),
            &u64_bytes(old_nonce),
            &u64_bytes(new_nonce),
            core_hash,
        ],
    );
    if let (Some(signed_hash), Some(receipt_hash)) = (signed_hash, receipt_hash) {
        append(&mut out, &[signed_hash, &ZERO_HASH, receipt_hash, &base.committer, &u64_bytes(base.expiry)]);
    } else {
        out.extend_from_slice(&ZERO_HASH);
    }
    out
}

#[allow(clippy::too_many_arguments)]
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
            &base.committer,
            body,
            &ZERO_HASH,
        ],
    );
    ckb_hash(&out)
}

fn material(op: u64, base: &Base, old: Option<&Cell>, mutate: bool, zero_txid: bool, mismatch: bool) -> Result<Material> {
    let (
        old_status,
        new_status,
        old_nonce,
        new_nonce,
        old_state,
        new_state,
        txid,
        wtxid,
        output_index,
        amount,
        transition,
        btc_commitment,
        mut next,
    ) = match op {
        OP_INITIALIZE => (
            0,
            STATUS_ACTIVE,
            0,
            0,
            ZERO_HASH,
            base.initial_state,
            ZERO_HASH,
            ZERO_HASH,
            0,
            0,
            ZERO_HASH,
            ZERO_HASH,
            Cell {
                seal: base.seal,
                policy: base.policy,
                committer: base.committer,
                btc_commitment: ZERO_HASH,
                state: base.initial_state,
                status: STATUS_ACTIVE,
                receipt: ZERO_HASH,
                nonce: 0,
                expiry: base.expiry,
            },
        ),
        OP_COMMIT => {
            let old = old.context("BTC transaction commit material requires an old cell")?;
            let txid = if zero_txid { ZERO_HASH } else { base.txid };
            let transition =
                if mismatch { ckb_hash(b"NovaSeal BTC transaction mismatched transition") } else { ckb_hash(&base.committed_state) };
            let commitment = ckb_hash(&public_commitment(&txid, &base.wtxid, base.output_index, base.amount_sats, &transition));
            (
                STATUS_ACTIVE,
                STATUS_COMMITTED,
                old.nonce,
                old.nonce + 1,
                old.state,
                base.committed_state,
                txid,
                base.wtxid,
                base.output_index,
                base.amount_sats,
                transition,
                commitment,
                Cell {
                    seal: old.seal,
                    policy: old.policy,
                    committer: old.committer,
                    btc_commitment: commitment,
                    state: base.committed_state,
                    status: STATUS_COMMITTED,
                    receipt: ZERO_HASH,
                    nonce: old.nonce + 1,
                    expiry: old.expiry,
                },
            )
        }
        _ => bail!("unknown BTC transaction op {op}"),
    };
    let old_commitment = old.map(|value| ckb_hash(&pack_state(value))).unwrap_or(ZERO_HASH);
    let new_commitment = ckb_hash(&pack_state(&next));
    let core = pack_core(
        op,
        base,
        &txid,
        &wtxid,
        output_index,
        amount,
        &old_state,
        &new_state,
        &transition,
        old_status,
        new_status,
        old_nonce,
        new_nonce,
    );
    let core_hash = ckb_hash(&core);
    let receipt_hash = if op == OP_COMMIT {
        ckb_hash(&pack_receipt(base, &btc_commitment, &old_state, &new_state, old_nonce, new_nonce, &core_hash, None, None))
    } else {
        ZERO_HASH
    };
    if op == OP_COMMIT {
        next.receipt = receipt_hash;
    }
    let canonical = canonical(op, base, &old_commitment, &new_commitment, old_nonce, new_nonce, &core_hash);
    let mut signed_intent = core;
    append(&mut signed_intent, &[&canonical, &receipt_hash]);
    let signed_hash = ckb_hash(&signed_intent);
    let receipt_data = if op == OP_COMMIT {
        pack_receipt(
            base,
            &btc_commitment,
            &old_state,
            &new_state,
            old_nonce,
            new_nonce,
            &core_hash,
            Some(&signed_hash),
            Some(&receipt_hash),
        )
    } else {
        Vec::new()
    };
    let (public, signed) = schnorr_sign(&signed_hash, &TEST_SECRET_KEY, &TEST_AUX_RAND)?;
    let mut signature = Vec::with_capacity(96);
    signature.extend_from_slice(&public);
    signature.extend_from_slice(&signed);
    if mutate {
        *signature.last_mut().unwrap() ^= 1;
    }
    Ok(Material {
        old_cell_data: pack_cell(old.unwrap_or(&zero_cell())),
        new_cell_data: pack_cell(&next),
        new_cell: next,
        receipt_data,
        signed_intent,
        signed_hash,
        signature,
        txid,
        wtxid,
        output_index,
        amount_sats: amount,
        btc_commitment,
        transition_commitment: transition,
        receipt_hash,
    })
}

fn witness(op: u64, material: &Material) -> String {
    let mut out = b"CSARGv1\0".to_vec();
    out.extend_from_slice(&u8_bytes(op));
    for value in [material.old_cell_data.as_slice(), material.signed_intent.as_slice(), material.signature.as_slice()] {
        out.extend_from_slice(&u32_bytes(value.len()));
        out.extend_from_slice(value);
    }
    entry_witness_input_type_hex(&out)
}

fn build_initialize(funding: &Value, lifecycle_hash: &str, deps: Vec<Value>, header: &str, material: &Material) -> Result<Value> {
    let total = funding["total_capacity"].as_u64().context("BTC transaction initialize funding total is missing")?;
    let change = total.checked_sub(STATE_CAPACITY).context("BTC transaction initialize funding capacity is too small")?;
    if change == 0 {
        bail!("BTC transaction initialize funding capacity is too small");
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

fn build_commit(
    old_ref: &Value,
    funding: &Value,
    lifecycle_hash: &str,
    deps: Vec<Value>,
    header: &str,
    material: &Material,
) -> Result<Value> {
    let total = funding["total_capacity"].as_u64().context("BTC transaction commit funding total is missing")?;
    let change = total.checked_sub(RECEIPT_CAPACITY).context("BTC transaction commit funding capacity is too small")?;
    if change == 0 {
        bail!("BTC transaction commit funding capacity is too small");
    }
    let mut inputs = vec![old_ref.clone()];
    inputs.extend_from_slice(funding_cells(funding));
    let mut witnesses = vec![witness(OP_COMMIT, material)];
    witnesses.extend(vec!["0x".into(); funding_cells(funding).len()]);
    Ok(transaction(
        &inputs,
        vec![
            json!({"capacity": format!("0x{:x}", old_ref["capacity"].as_u64().unwrap()), "lock": always_success_lock("0x"), "type": lifecycle_type(lifecycle_hash)}),
            json!({"capacity": format!("0x{RECEIPT_CAPACITY:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
            json!({"capacity": format!("0x{change:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
        ],
        vec![hex0x(&material.new_cell_data), hex0x(&material.receipt_data), "0x".into()],
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
        .unwrap_or_else(|| root.join(format!("target/novaseal-btc-transaction-commitment-devnet-stateful-live/{timestamp}")));
    fs::create_dir_all(&run_dir)?;
    let run_dir = fs::canonicalize(run_dir)?;
    let lifecycle_path = run_dir.join("nova-btc-transaction-commitment-lifecycle-type.elf");
    compile_contract(&root, contract, &lifecycle_path)?;
    let verifier_path = root.join("proposals/novaseal/v0-mvp-skeleton/target/novaseal-btc-verifier-riscv-shell-release.elf");
    if !verifier_path.is_file() {
        bail!("missing verifier ELF: {}", verifier_path.display());
    }
    let mut devnet = CkbDevnet::new(ckb_repo.clone(), ckb_bin.clone(), run_dir.clone())?;
    let mut report =
        contract_report_header(contract, "btc_transaction_commitment_initialize_then_commit", &root, &ckb_repo, &ckb_bin, &run_dir);
    report["btc_public_verification_scope"] = json!(
        "live CKB transition executes the BIP340 runtime verifier and binds a declared BTC txid/wtxid/output tuple; SPV/indexer finality remains separate production evidence"
    );
    let mut stage = "initializing";
    let scenario = (|| -> Result<()> {
        stage = "start devnet";
        devnet.start()?;
        stage = "deploy artifacts";
        let genesis = devnet.get_block_by_number(0)?;
        let always = always_success_dep(genesis["transactions"][0]["hash"].as_str().context("genesis hash is missing")?);
        let verifier = deploy_code(&mut devnet, "cellscript_btc_bip340_verifier_riscv", &fs::read(&verifier_path)?, &always)?;
        let lifecycle =
            deploy_code(&mut devnet, "nova_btc_transaction_commitment_lifecycle_type", &fs::read(&lifecycle_path)?, &always)?;
        let lifecycle_hash = lifecycle["data_hash"].as_str().context("lifecycle hash is missing")?.to_owned();
        let deps = vec![verifier["cell_dep"].clone(), lifecycle["cell_dep"].clone(), always];
        let source_paths = [
            "proposals/novaseal/btc-transaction-commitment-profile-v0/Cell.toml",
            "proposals/novaseal/btc-transaction-commitment-profile-v0/src",
            "proposals/novaseal/btc-transaction-commitment-profile-v0/schemas",
            "proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier",
            "crates/cellscript-tools/src/novaseal_planned_btc_tx.rs",
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
        let initialize_commit = devnet.submit_and_commit(&tx, "BTC transaction commitment initialize")?;
        let initialize_hash = initialize_commit["tx_hash"].as_str().unwrap();
        let initial_live = devnet.assert_live_cell(
            initialize_hash,
            0,
            "BTC transaction active state",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&initialize.new_cell_data),
        )?;
        let initial_ref = json!({"tx_hash": initialize_hash, "index": 0, "capacity": STATE_CAPACITY});

        stage = "negative wrong committer signature";
        let negative_header = devnet.rpc("get_tip_header", vec![])?;
        let wrong = material(OP_COMMIT, &base, Some(&initialize.new_cell), true, false, false)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx =
            build_commit(&initial_ref, &funding, &lifecycle_hash, deps.clone(), negative_header["hash"].as_str().unwrap(), &wrong)?;
        let wrong_reject = devnet.dry_run_rejects(
            &tx,
            "BTC transaction wrong committer signature",
            Some("Inputs[0].Type"),
            Some(&lifecycle_hash),
            Some(56),
        )?;

        stage = "negative zero BTC txid";
        let zero = material(OP_COMMIT, &base, Some(&initialize.new_cell), false, true, false)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx =
            build_commit(&initial_ref, &funding, &lifecycle_hash, deps.clone(), negative_header["hash"].as_str().unwrap(), &zero)?;
        let zero_reject =
            devnet.dry_run_rejects(&tx, "BTC transaction zero txid", Some("Inputs[0].Type"), Some(&lifecycle_hash), Some(5))?;

        stage = "negative transition hash mismatch";
        let mismatch = material(OP_COMMIT, &base, Some(&initialize.new_cell), false, false, true)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx =
            build_commit(&initial_ref, &funding, &lifecycle_hash, deps.clone(), negative_header["hash"].as_str().unwrap(), &mismatch)?;
        let mismatch_reject = devnet.dry_run_rejects(
            &tx,
            "BTC transaction transition hash mismatch",
            Some("Inputs[0].Type"),
            Some(&lifecycle_hash),
            Some(5),
        )?;
        let post_negative = devnet.assert_live_cell(
            initialize_hash,
            0,
            "post-negative BTC transaction active state",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&initialize.new_cell_data),
        )?;

        stage = "valid commit transaction";
        let header = devnet.rpc("get_tip_header", vec![])?;
        let commit_material = material(OP_COMMIT, &base, Some(&initialize.new_cell), false, false, false)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_commit(&initial_ref, &funding, &lifecycle_hash, deps, header["hash"].as_str().unwrap(), &commit_material)?;
        let commit_dry = devnet.rpc("dry_run_transaction", vec![tx.clone()])?;
        let commit = devnet.submit_and_commit(&tx, "BTC transaction commitment transition")?;
        let old_dead = devnet.wait_dead_cell(initialize_hash, 0)?;
        let commit_hash = commit["tx_hash"].as_str().unwrap();
        let committed_live = devnet.assert_live_cell(
            commit_hash,
            0,
            "BTC transaction committed state",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&commit_material.new_cell_data),
        )?;
        let receipt_live = devnet.assert_live_cell(
            commit_hash,
            1,
            "BTC transaction commitment receipt",
            Some(RECEIPT_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&Value::Null),
            Some(&commit_material.receipt_data),
        )?;
        report.as_object_mut().unwrap().extend(
            json!({
                "status": "passed", "live_devnet_rpc_executed": true, "stateful_lifecycle_executed": true,
                "ckb_log": devnet.log_path.display().to_string(), "rpc_url": devnet.rpc_url,
                "artifacts": {"verifier": verifier, "lifecycle": lifecycle}, "provenance": source_provenance,
                "initialize": {"dry_run_cycles": initialize_dry["cycles"], "commit": initialize_commit,
                    "state_live": initial_live["status"] == "live", "state_data_hash": hex0x(&ckb_hash(&initialize.new_cell_data))},
                "commit_transaction": {"dry_run_cycles": commit_dry["cycles"], "commit": commit,
                    "old_state_not_live": old_dead["status"] != "live", "new_state_live": committed_live["status"] == "live",
                    "receipt_live": receipt_live["status"] == "live",
                    "btc_tx_tuple_bound": commit_material.new_cell.btc_commitment == commit_material.btc_commitment && commit_material.btc_commitment != ZERO_HASH,
                    "transition_commitment_bound": commit_material.transition_commitment == ckb_hash(&base.committed_state),
                    "public_btc_verification_executed": true,
                    "public_btc_verification_scope": "BIP340 runtime verifier execution over the signed BTC commitment intent",
                    "btc_tx_commitment_hash": hex0x(&commit_material.btc_commitment),
                    "public_btc_anchor": {"kind": "btc_transaction_commitment", "anchor_source": "local_deterministic_fixture",
                        "btc_txid": hex0x(&commit_material.txid), "btc_wtxid": hex0x(&commit_material.wtxid),
                        "btc_output_index": commit_material.output_index, "btc_amount_sats": commit_material.amount_sats,
                        "ckb_btc_commitment_hash": hex0x(&commit_material.btc_commitment)},
                    "signed_intent_hash": hex0x(&commit_material.signed_hash), "receipt_hash": hex0x(&commit_material.receipt_hash)},
                "negative_cases": {"wrong_committer_signature_dry_run": wrong_reject, "zero_btc_txid_dry_run": zero_reject,
                    "transition_hash_mismatch_dry_run": mismatch_reject, "post_negative_state_still_live": post_negative["status"] == "live"},
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
    fn initialization_material_is_deterministic() {
        let base = base("parity").unwrap();
        let initial = material(OP_INITIALIZE, &base, None, false, false, false).unwrap();
        let committed = material(OP_COMMIT, &base, Some(&initial.new_cell), false, false, false).unwrap();
        assert_eq!(hex0x(&ckb_hash(&initial.new_cell_data)), "0x52b95b87ee55d01594d590d042c5f10dcae64e31182a0c8bb6e2388693a4dbc7");
        assert_eq!(hex0x(&ckb_hash(&committed.new_cell_data)), "0xa67add7f0f8033d4b772ed4eeb973a9a27e7f0ab277659ff2e1ea6928f7adc20");
        assert_eq!(hex0x(&committed.signed_hash), "0xff54214ef0cf24022aaa693b833741c7c57270f4b77951fe3587811175b70a2b");
        assert_eq!(hex0x(&committed.receipt_hash), "0xb92df287af5040fe4684125cc6db43e7d2fa65604315dd1c0b51ce338fde0d2b");
        assert_eq!(hex0x(&ckb_hash(&committed.receipt_data)), "0x5e0868fd60f32d5e613e69a4ebd418bbb075c5e6019240b348c6483771abe7ec");
    }
}
