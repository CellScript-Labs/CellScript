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

const OP_CLOSE: u64 = 0;
const OP_INITIALIZE: u64 = 255;
const STATUS_ACTIVE: u64 = 1;
const STATUS_CLOSED: u64 = 2;
type Hash = [u8; 32];

#[derive(Clone)]
struct Base {
    seal: Hash,
    policy: Hash,
    owner: Hash,
    initial_state: Hash,
    closed_state: Hash,
    txid: Hash,
    vout: u64,
    amount_sats: u64,
    script_pubkey: Hash,
    spend_txid: Hash,
    spend_wtxid: Hash,
    spend_input: u64,
    expiry: u64,
}

#[derive(Clone)]
struct Cell {
    seal: Hash,
    policy: Hash,
    owner: Hash,
    sealed_utxo: Hash,
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
    vout: u64,
    amount_sats: u64,
    script_pubkey: Hash,
    spend_txid: Hash,
    spend_wtxid: Hash,
    spend_input: u64,
    sealed_utxo: Hash,
    closure: Hash,
    receipt_hash: Hash,
}

fn append(out: &mut Vec<u8>, chunks: &[&[u8]]) {
    for chunk in chunks {
        out.extend_from_slice(chunk);
    }
}

fn base(label: &str) -> Result<Base> {
    Ok(Base {
        seal: ckb_hash(format!("NovaSeal BTC UTXO seal {label}").as_bytes()),
        policy: ckb_hash(format!("NovaSeal BTC UTXO policy {label}").as_bytes()),
        owner: xonly_pubkey(&TEST_SECRET_KEY)?,
        initial_state: ckb_hash(format!("NovaSeal BTC UTXO active state {label}").as_bytes()),
        closed_state: ckb_hash(format!("NovaSeal BTC UTXO closed state {label}").as_bytes()),
        txid: ckb_hash(format!("NovaSeal BTC UTXO txid {label}").as_bytes()),
        vout: 1,
        amount_sats: 250_000,
        script_pubkey: ckb_hash(format!("NovaSeal BTC UTXO script pubkey {label}").as_bytes()),
        spend_txid: ckb_hash(format!("NovaSeal BTC UTXO spend txid {label}").as_bytes()),
        spend_wtxid: ckb_hash(format!("NovaSeal BTC UTXO spend wtxid {label}").as_bytes()),
        spend_input: 0,
        expiry: (1_u64 << 63) - 1,
    })
}

fn zero_cell() -> Cell {
    Cell {
        seal: ZERO_HASH,
        policy: ZERO_HASH,
        owner: ZERO_HASH,
        sealed_utxo: ZERO_HASH,
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
            &cell.owner,
            &cell.sealed_utxo,
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
            &cell.owner,
            &cell.sealed_utxo,
            &cell.state,
            &u8_bytes(cell.status),
            &cell.receipt,
            &u64_bytes(cell.nonce),
            &u64_bytes(cell.expiry),
        ],
    );
    out
}

fn utxo_commitment(txid: &Hash, vout: u64, amount: u64, script_pubkey: &Hash) -> Vec<u8> {
    let mut out = Vec::new();
    append(&mut out, &[txid, &u32_bytes(vout as usize), &u64_bytes(amount), script_pubkey]);
    out
}

fn closure_commitment(sealed: &Hash, spend_txid: &Hash, spend_wtxid: &Hash, spend_input: u64, transition: &Hash) -> Vec<u8> {
    let mut out = Vec::new();
    append(&mut out, &[sealed, spend_txid, spend_wtxid, &u32_bytes(spend_input as usize), transition, &ZERO_HASH]);
    out
}

#[allow(clippy::too_many_arguments)]
fn pack_core(
    op: u64,
    base: &Base,
    txid: &Hash,
    spend_txid: &Hash,
    spend_wtxid: &Hash,
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
            &base.owner,
            txid,
            &u32_bytes(base.vout as usize),
            &u64_bytes(base.amount_sats),
            &base.script_pubkey,
            spend_txid,
            spend_wtxid,
            &u32_bytes(base.spend_input as usize),
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
    sealed: &Hash,
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
            &u8_bytes(OP_CLOSE),
            &base.seal,
            &base.policy,
            &base.owner,
            sealed,
            closure,
            old_state,
            new_state,
            &u8_bytes(STATUS_ACTIVE),
            &u8_bytes(STATUS_CLOSED),
            &u64_bytes(old_nonce),
            &u64_bytes(new_nonce),
            core_hash,
        ],
    );
    if let (Some(signed_hash), Some(receipt_hash)) = (signed_hash, receipt_hash) {
        append(&mut out, &[signed_hash, &ZERO_HASH, receipt_hash, &base.owner, &u64_bytes(base.expiry)]);
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
            &base.owner,
            body,
            &ZERO_HASH,
        ],
    );
    ckb_hash(&out)
}

fn material(op: u64, base: &Base, old: Option<&Cell>, mutate: bool, mismatch: bool, zero_spend: bool) -> Result<Material> {
    let txid = if mismatch { ckb_hash(b"NovaSeal mismatched UTXO txid") } else { base.txid };
    let sealed = ckb_hash(&utxo_commitment(&txid, base.vout, base.amount_sats, &base.script_pubkey));
    let (
        old_status,
        new_status,
        old_nonce,
        new_nonce,
        old_state,
        new_state,
        spend_txid,
        spend_wtxid,
        transition,
        closure,
        mut next,
        new_commitment,
    ) = match op {
        OP_INITIALIZE => {
            let next = Cell {
                seal: base.seal,
                policy: base.policy,
                owner: base.owner,
                sealed_utxo: sealed,
                state: base.initial_state,
                status: STATUS_ACTIVE,
                receipt: ZERO_HASH,
                nonce: 0,
                expiry: base.expiry,
            };
            let new_commitment = ckb_hash(&pack_state(&next));
            (0, STATUS_ACTIVE, 0, 0, ZERO_HASH, base.initial_state, ZERO_HASH, ZERO_HASH, ZERO_HASH, ZERO_HASH, next, new_commitment)
        }
        OP_CLOSE => {
            let old = old.context("BTC UTXO close material requires an old cell")?;
            let spend_txid = if zero_spend { ZERO_HASH } else { base.spend_txid };
            let transition = ckb_hash(&base.closed_state);
            let closure = ckb_hash(&closure_commitment(&sealed, &spend_txid, &base.spend_wtxid, base.spend_input, &transition));
            (
                STATUS_ACTIVE,
                STATUS_CLOSED,
                old.nonce,
                old.nonce + 1,
                old.state,
                base.closed_state,
                spend_txid,
                base.spend_wtxid,
                transition,
                closure,
                Cell {
                    seal: old.seal,
                    policy: old.policy,
                    owner: old.owner,
                    sealed_utxo: sealed,
                    state: base.closed_state,
                    status: STATUS_CLOSED,
                    receipt: ZERO_HASH,
                    nonce: old.nonce + 1,
                    expiry: old.expiry,
                },
                closure,
            )
        }
        _ => bail!("unknown BTC UTXO op {op}"),
    };
    let old_commitment = old.map(|value| ckb_hash(&pack_state(value))).unwrap_or(ZERO_HASH);
    let core = pack_core(
        op,
        base,
        &txid,
        &spend_txid,
        &spend_wtxid,
        &old_state,
        &new_state,
        &transition,
        old_status,
        new_status,
        old_nonce,
        new_nonce,
    );
    let core_hash = ckb_hash(&core);
    let receipt_hash = if op == OP_CLOSE {
        ckb_hash(&pack_receipt(base, &sealed, &closure, &old_state, &new_state, old_nonce, new_nonce, &core_hash, None, None))
    } else {
        ZERO_HASH
    };
    if op == OP_CLOSE {
        next.receipt = receipt_hash;
    }
    let canonical = canonical(op, base, &old_commitment, &new_commitment, old_nonce, new_nonce, &core_hash);
    let mut signed_intent = core;
    append(&mut signed_intent, &[&canonical, &receipt_hash]);
    let mut signing_digest = Vec::new();
    append(&mut signing_digest, &[&core_hash, &canonical, &receipt_hash]);
    let signed_hash = ckb_hash(&signing_digest);
    let receipt_data = if op == OP_CLOSE {
        pack_receipt(
            base,
            &sealed,
            &closure,
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
        vout: base.vout,
        amount_sats: base.amount_sats,
        script_pubkey: base.script_pubkey,
        spend_txid,
        spend_wtxid,
        spend_input: base.spend_input,
        sealed_utxo: sealed,
        closure,
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
    let total = funding["total_capacity"].as_u64().context("BTC UTXO initialize funding total is missing")?;
    let change = total.checked_sub(STATE_CAPACITY).context("BTC UTXO initialize funding capacity is too small")?;
    if change == 0 {
        bail!("BTC UTXO initialize funding capacity is too small");
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

fn build_close(
    old_ref: &Value,
    funding: &Value,
    lifecycle_hash: &str,
    deps: Vec<Value>,
    header: &str,
    material: &Material,
) -> Result<Value> {
    let total = funding["total_capacity"].as_u64().context("BTC UTXO close funding total is missing")?;
    let change = total.checked_sub(RECEIPT_CAPACITY).context("BTC UTXO close funding capacity is too small")?;
    if change == 0 {
        bail!("BTC UTXO close funding capacity is too small");
    }
    let mut inputs = vec![old_ref.clone()];
    inputs.extend_from_slice(funding_cells(funding));
    let mut witnesses = vec![witness(OP_CLOSE, material)];
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
        .unwrap_or_else(|| root.join(format!("target/novaseal-btc-utxo-seal-devnet-stateful-live/{timestamp}")));
    fs::create_dir_all(&run_dir)?;
    let run_dir = fs::canonicalize(run_dir)?;
    let lifecycle_path = run_dir.join("nova-btc-utxo-seal-lifecycle-type.elf");
    compile_contract(&root, contract, &lifecycle_path)?;
    let verifier_path = root.join("proposals/novaseal/v0-mvp-skeleton/target/novaseal-btc-verifier-riscv-shell-release.elf");
    if !verifier_path.is_file() {
        bail!("missing verifier ELF: {}", verifier_path.display());
    }
    let mut devnet = CkbDevnet::new(ckb_repo.clone(), ckb_bin.clone(), run_dir.clone())?;
    let mut report = contract_report_header(contract, "btc_utxo_seal_initialize_then_close", &root, &ckb_repo, &ckb_bin, &run_dir);
    report["btc_public_verification_scope"] = json!(
        "live CKB closure executes the BIP340 runtime verifier and binds a declared BTC UTXO/spend tuple; SPV/indexer spend-finality evidence remains separate production evidence"
    );
    let mut stage = "initializing";
    let scenario = (|| -> Result<()> {
        stage = "start devnet";
        devnet.start()?;
        stage = "deploy artifacts";
        let genesis = devnet.get_block_by_number(0)?;
        let always = always_success_dep(genesis["transactions"][0]["hash"].as_str().context("genesis hash is missing")?);
        let verifier = deploy_code(&mut devnet, "cellscript_btc_bip340_verifier_riscv", &fs::read(&verifier_path)?, &always)?;
        let lifecycle = deploy_code(&mut devnet, "nova_btc_utxo_seal_lifecycle_type", &fs::read(&lifecycle_path)?, &always)?;
        let lifecycle_hash = lifecycle["data_hash"].as_str().context("lifecycle hash is missing")?.to_owned();
        let deps = vec![verifier["cell_dep"].clone(), lifecycle["cell_dep"].clone(), always];
        let source_paths = [
            "proposals/novaseal/btc-utxo-seal-profile-v0/Cell.toml",
            "proposals/novaseal/btc-utxo-seal-profile-v0/src",
            "proposals/novaseal/btc-utxo-seal-profile-v0/schemas",
            "proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier",
            "crates/cellscript-tools/src/novaseal_planned_btc_utxo.rs",
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
        let initialize_commit = devnet.submit_and_commit(&tx, "BTC UTXO seal initialize")?;
        let initialize_hash = initialize_commit["tx_hash"].as_str().unwrap();
        let initial_live = devnet.assert_live_cell(
            initialize_hash,
            0,
            "BTC UTXO active seal",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&initialize.new_cell_data),
        )?;
        let initial_ref = json!({"tx_hash": initialize_hash, "index": 0, "capacity": STATE_CAPACITY});

        stage = "negative wrong owner signature";
        let negative_header = devnet.rpc("get_tip_header", vec![])?;
        let wrong = material(OP_CLOSE, &base, Some(&initialize.new_cell), true, false, false)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx =
            build_close(&initial_ref, &funding, &lifecycle_hash, deps.clone(), negative_header["hash"].as_str().unwrap(), &wrong)?;
        let wrong_reject =
            devnet.dry_run_rejects(&tx, "BTC UTXO wrong owner signature", Some("Inputs[0].Type"), Some(&lifecycle_hash), Some(56))?;

        stage = "negative UTXO commitment mismatch";
        let mismatch = material(OP_CLOSE, &base, Some(&initialize.new_cell), false, true, false)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx =
            build_close(&initial_ref, &funding, &lifecycle_hash, deps.clone(), negative_header["hash"].as_str().unwrap(), &mismatch)?;
        let mismatch_reject =
            devnet.dry_run_rejects(&tx, "BTC UTXO commitment mismatch", Some("Inputs[0].Type"), Some(&lifecycle_hash), Some(5))?;

        stage = "negative zero spend txid";
        let zero = material(OP_CLOSE, &base, Some(&initialize.new_cell), false, false, true)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_close(&initial_ref, &funding, &lifecycle_hash, deps.clone(), negative_header["hash"].as_str().unwrap(), &zero)?;
        let zero_reject =
            devnet.dry_run_rejects(&tx, "BTC UTXO zero spend txid", Some("Inputs[0].Type"), Some(&lifecycle_hash), Some(5))?;
        let post_negative = devnet.assert_live_cell(
            initialize_hash,
            0,
            "post-negative BTC UTXO active seal",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&initialize.new_cell_data),
        )?;

        stage = "valid close UTXO seal";
        let header = devnet.rpc("get_tip_header", vec![])?;
        let close_material = material(OP_CLOSE, &base, Some(&initialize.new_cell), false, false, false)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_close(&initial_ref, &funding, &lifecycle_hash, deps, header["hash"].as_str().unwrap(), &close_material)?;
        let close_dry = devnet.rpc("dry_run_transaction", vec![tx.clone()])?;
        let close = devnet.submit_and_commit(&tx, "BTC UTXO seal closure")?;
        let old_dead = devnet.wait_dead_cell(initialize_hash, 0)?;
        let close_hash = close["tx_hash"].as_str().unwrap();
        let closed_live = devnet.assert_live_cell(
            close_hash,
            0,
            "BTC UTXO closed seal",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&close_material.new_cell_data),
        )?;
        let receipt_live = devnet.assert_live_cell(
            close_hash,
            1,
            "BTC UTXO closure receipt",
            Some(RECEIPT_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&Value::Null),
            Some(&close_material.receipt_data),
        )?;
        report.as_object_mut().unwrap().extend(
            json!({
                "status": "passed", "live_devnet_rpc_executed": true, "stateful_lifecycle_executed": true,
                "ckb_log": devnet.log_path.display().to_string(), "rpc_url": devnet.rpc_url,
                "artifacts": {"verifier": verifier, "lifecycle": lifecycle}, "provenance": source_provenance,
                "initialize": {"dry_run_cycles": initialize_dry["cycles"], "commit": initialize_commit,
                    "state_live": initial_live["status"] == "live", "state_data_hash": hex0x(&ckb_hash(&initialize.new_cell_data))},
                "close_utxo_seal": {"dry_run_cycles": close_dry["cycles"], "commit": close,
                    "old_state_not_live": old_dead["status"] != "live", "new_state_live": closed_live["status"] == "live",
                    "receipt_live": receipt_live["status"] == "live", "sealed_utxo_tuple_bound": initialize.new_cell.sealed_utxo == close_material.sealed_utxo,
                    "spend_tuple_bound": close_material.closure != ZERO_HASH, "public_btc_spend_verification_executed": true,
                    "public_btc_verification_scope": "BIP340 runtime verifier execution over the signed BTC UTXO closure intent",
                    "sealed_utxo_commitment_hash": hex0x(&close_material.sealed_utxo), "closure_commitment_hash": hex0x(&close_material.closure),
                    "public_btc_anchor": {"kind": "btc_utxo_spend", "anchor_source": "local_deterministic_fixture",
                        "sealed_btc_txid": hex0x(&close_material.txid), "sealed_btc_vout_index": close_material.vout,
                        "sealed_btc_amount_sats": close_material.amount_sats, "script_pubkey_hash": hex0x(&close_material.script_pubkey),
                        "btc_txid": hex0x(&close_material.spend_txid), "btc_wtxid": hex0x(&close_material.spend_wtxid),
                        "spend_input_index": close_material.spend_input, "ckb_btc_commitment_hash": hex0x(&close_material.closure),
                        "sealed_utxo_commitment_hash": hex0x(&close_material.sealed_utxo)},
                    "signed_intent_hash": hex0x(&close_material.signed_hash), "receipt_hash": hex0x(&close_material.receipt_hash)},
                "negative_cases": {"wrong_owner_signature_dry_run": wrong_reject,
                    "utxo_commitment_mismatch_dry_run": mismatch_reject, "zero_spend_txid_dry_run": zero_reject,
                    "post_negative_state_still_live": post_negative["status"] == "live"},
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
    fn close_material_is_deterministic() {
        let base = base("parity").unwrap();
        let initial = material(OP_INITIALIZE, &base, None, false, false, false).unwrap();
        let closed = material(OP_CLOSE, &base, Some(&initial.new_cell), false, false, false).unwrap();
        assert_eq!(hex0x(&ckb_hash(&initial.new_cell_data)), "0xdd07b127b77136877a21d67d7f2fdae74b72dcef2f98d31ba33a0c7257881a31");
        assert_eq!(hex0x(&ckb_hash(&closed.new_cell_data)), "0xfcbd780069f1541b9c5619d41a4a6a159f31726c4a5599ace5451ecfe1d9862d");
        assert_eq!(hex0x(&closed.signed_hash), "0xcc66217dabfe2b031c9899dbe314ee7d5a39a7c1e59611120e6296a56f38aa46");
        assert_eq!(hex0x(&closed.receipt_hash), "0xa4e3127e6e0a3ae4c92207acbd4b91bb8969b4efed3af44449955b55a40eee11");
        assert_eq!(hex0x(&ckb_hash(&closed.receipt_data)), "0xddcafd05403466f543ef27a9b22f45af0c6df7f6ed1211e2bb449436700cd2d2");
    }
}
