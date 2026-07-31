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

const OP_MATERIALIZE: u64 = 0;
const OP_CLAIM: u64 = 1;
const OP_SETTLE: u64 = 2;
const STATUS_MATERIALIZED: u64 = 1;
const STATUS_CLAIMED: u64 = 2;
const STATUS_SETTLED: u64 = 3;
const HOLDER_SECRET: [u8; 32] = [0x22; 32];
const HOLDER_AUX: [u8; 32] = [0x42; 32];

type Hash = [u8; 32];

#[derive(Clone)]
struct Base {
    receipt_id: Hash,
    registry: Hash,
    asset: Hash,
    document: Hash,
    issuer: Hash,
    holder: Hash,
    amount: u64,
    expiry: u64,
}

#[derive(Clone)]
struct Cell {
    receipt_id: Hash,
    registry: Hash,
    asset: Hash,
    document: Hash,
    issuer: Hash,
    holder: Hash,
    amount: u64,
    status: u64,
    receipt: Hash,
    nonce: u64,
    expiry: u64,
}

struct Material {
    old_cell: Cell,
    old_cell_data: Vec<u8>,
    new_cell: Cell,
    new_cell_data: Vec<u8>,
    event_data: Vec<u8>,
    signed_intent: Vec<u8>,
    receipt_hash: Hash,
    signer_signature: Vec<u8>,
    cosigner_signature: Vec<u8>,
}

fn append(out: &mut Vec<u8>, chunks: &[&[u8]]) {
    for chunk in chunks {
        out.extend_from_slice(chunk);
    }
}

fn zero_cell() -> Cell {
    Cell {
        receipt_id: ZERO_HASH,
        registry: ZERO_HASH,
        asset: ZERO_HASH,
        document: ZERO_HASH,
        issuer: ZERO_HASH,
        holder: ZERO_HASH,
        amount: 0,
        status: 0,
        receipt: ZERO_HASH,
        nonce: 0,
        expiry: 0,
    }
}

fn base(label: &str) -> Result<Base> {
    Ok(Base {
        receipt_id: ckb_hash(format!("NovaSeal RWA receipt {label}").as_bytes()),
        registry: ckb_hash(format!("NovaSeal RWA registry {label}").as_bytes()),
        asset: ckb_hash(format!("NovaSeal RWA asset {label}").as_bytes()),
        document: ckb_hash(format!("NovaSeal RWA document {label}").as_bytes()),
        issuer: xonly_pubkey(&TEST_SECRET_KEY)?,
        holder: xonly_pubkey(&HOLDER_SECRET)?,
        amount: 10_000,
        expiry: (1_u64 << 63) - 1,
    })
}

fn pack_state(cell: &Cell) -> Vec<u8> {
    let mut out = u16_bytes(0);
    append(
        &mut out,
        &[
            &cell.receipt_id,
            &cell.registry,
            &cell.asset,
            &cell.document,
            &cell.issuer,
            &cell.holder,
            &u64_bytes(cell.amount),
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
            &cell.receipt_id,
            &cell.registry,
            &cell.asset,
            &cell.document,
            &cell.issuer,
            &cell.holder,
            &u64_bytes(cell.amount),
            &u8_bytes(cell.status),
            &cell.receipt,
            &u64_bytes(cell.nonce),
            &u64_bytes(cell.expiry),
        ],
    );
    out
}

#[allow(clippy::too_many_arguments)]
fn pack_core(
    op: u64,
    base: &Base,
    old_status: u64,
    new_status: u64,
    old_amount: u64,
    settlement_amount: u64,
    old_nonce: u64,
    new_nonce: u64,
) -> Vec<u8> {
    let mut out = Vec::new();
    append(
        &mut out,
        &[
            &u8_bytes(op),
            &base.receipt_id,
            &base.registry,
            &base.asset,
            &base.document,
            &base.issuer,
            &base.holder,
            &u8_bytes(old_status),
            &u8_bytes(new_status),
            &u64_bytes(old_amount),
            &u64_bytes(settlement_amount),
            &u64_bytes(old_nonce),
            &u64_bytes(new_nonce),
            &u64_bytes(base.expiry),
            &ZERO_HASH,
        ],
    );
    out
}

#[allow(clippy::too_many_arguments)]
fn pack_event(
    op: u64,
    base: &Base,
    old_status: u64,
    new_status: u64,
    old_amount: u64,
    settlement_amount: u64,
    old_nonce: u64,
    new_nonce: u64,
    core_hash: &Hash,
    receipt_hash: Option<&Hash>,
    signer: Option<&Hash>,
) -> Vec<u8> {
    let mut out = Vec::new();
    append(
        &mut out,
        &[
            &u8_bytes(op),
            &base.receipt_id,
            &base.registry,
            &base.asset,
            &base.document,
            &base.issuer,
            &base.holder,
            &u8_bytes(old_status),
            &u8_bytes(new_status),
            &u64_bytes(old_amount),
            &u64_bytes(settlement_amount),
            &u64_bytes(old_nonce),
            &u64_bytes(new_nonce),
            core_hash,
            &ZERO_HASH,
        ],
    );
    if let (Some(receipt_hash), Some(signer)) = (receipt_hash, signer) {
        append(&mut out, &[receipt_hash, signer, &u64_bytes(base.expiry)]);
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn canonical(
    op: u64,
    base: &Base,
    old_state: &Hash,
    new_state: &Hash,
    old_nonce: u64,
    new_nonce: u64,
    authority: &Hash,
    body: &Hash,
) -> Hash {
    let mut out = Vec::new();
    append(
        &mut out,
        &[
            &base.receipt_id,
            &base.registry,
            &u8_bytes(op),
            &u8_bytes(op),
            &base.receipt_id,
            old_state,
            new_state,
            &u64_bytes(old_nonce),
            &u64_bytes(new_nonce),
            &u64_bytes(base.expiry),
            authority,
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

fn material(
    op: u64,
    base: &Base,
    old: Option<&Cell>,
    mutate_issuer: bool,
    mutate_holder: bool,
    amount_override: Option<u64>,
) -> Result<Material> {
    let (old_status, new_status, old_amount, settlement_amount, old_nonce, new_nonce, authority, mut next) = match op {
        OP_MATERIALIZE => (
            0,
            STATUS_MATERIALIZED,
            0,
            base.amount,
            0,
            0,
            base.issuer,
            Cell {
                receipt_id: base.receipt_id,
                registry: base.registry,
                asset: base.asset,
                document: base.document,
                issuer: base.issuer,
                holder: base.holder,
                amount: base.amount,
                status: STATUS_MATERIALIZED,
                receipt: ZERO_HASH,
                nonce: 0,
                expiry: base.expiry,
            },
        ),
        OP_CLAIM => {
            let old = old.context("RWA claim material requires an old cell")?;
            let mut next = old.clone();
            next.status = STATUS_CLAIMED;
            next.receipt = ZERO_HASH;
            next.nonce += 1;
            (
                STATUS_MATERIALIZED,
                STATUS_CLAIMED,
                old.amount,
                amount_override.unwrap_or(old.amount),
                old.nonce,
                old.nonce + 1,
                old.holder,
                next,
            )
        }
        OP_SETTLE => {
            let old = old.context("RWA settle material requires an old cell")?;
            (
                STATUS_CLAIMED,
                STATUS_SETTLED,
                old.amount,
                amount_override.unwrap_or(old.amount),
                old.nonce,
                old.nonce + 1,
                old.issuer,
                zero_cell(),
            )
        }
        _ => bail!("unknown RWA op {op}"),
    };
    let old_value = old.cloned().unwrap_or_else(zero_cell);
    let old_state = old.map(|value| ckb_hash(&pack_state(value))).unwrap_or(ZERO_HASH);
    let new_state = if op == OP_SETTLE { ZERO_HASH } else { ckb_hash(&pack_state(&next)) };
    let core = pack_core(op, base, old_status, new_status, old_amount, settlement_amount, old_nonce, new_nonce);
    let core_hash = ckb_hash(&core);
    let receipt_hash = ckb_hash(&pack_event(
        op,
        base,
        old_status,
        new_status,
        old_amount,
        settlement_amount,
        old_nonce,
        new_nonce,
        &core_hash,
        None,
        None,
    ));
    let canonical = canonical(op, base, &old_state, &new_state, old_nonce, new_nonce, &authority, &core_hash);
    if op != OP_SETTLE {
        next.receipt = receipt_hash;
    }
    let new_cell_data = pack_cell(&next);
    let event_data = pack_event(
        op,
        base,
        old_status,
        new_status,
        old_amount,
        settlement_amount,
        old_nonce,
        new_nonce,
        &core_hash,
        Some(&receipt_hash),
        Some(&authority),
    );
    let mut signed_intent = core;
    append(
        &mut signed_intent,
        &[&canonical, &receipt_hash, &if op == OP_SETTLE { ZERO_HASH } else { ckb_hash(&new_cell_data) }, &ckb_hash(&event_data)],
    );
    let signed_hash = ckb_hash(&signed_intent);
    let issuer_signature = signature(&TEST_SECRET_KEY, &TEST_AUX_RAND, &signed_hash, mutate_issuer)?;
    let holder_signature = signature(&HOLDER_SECRET, &HOLDER_AUX, &signed_hash, mutate_holder)?;
    let signer_signature = if op == OP_CLAIM { holder_signature.clone() } else { issuer_signature.clone() };
    let cosigner_signature = if op == OP_SETTLE { holder_signature } else { issuer_signature };
    Ok(Material {
        old_cell: old_value.clone(),
        old_cell_data: pack_cell(&old_value),
        new_cell: next,
        new_cell_data,
        event_data,
        signed_intent,
        receipt_hash,
        signer_signature,
        cosigner_signature,
    })
}

fn witness(op: u64, material: &Material) -> String {
    let mut out = b"CSARGv1\0".to_vec();
    out.extend_from_slice(&u8_bytes(op));
    for value in [
        material.old_cell_data.as_slice(),
        material.signed_intent.as_slice(),
        material.signer_signature.as_slice(),
        material.cosigner_signature.as_slice(),
    ] {
        out.extend_from_slice(&u32_bytes(value.len()));
        out.extend_from_slice(value);
    }
    entry_witness_input_type_hex(&out)
}

fn build_state_event(
    op: u64,
    old_ref: Option<&Value>,
    funding: &Value,
    lifecycle_hash: &str,
    deps: Vec<Value>,
    header: &str,
    material: &Material,
) -> Result<Value> {
    let funding_total = funding["total_capacity"].as_u64().context("RWA funding total is missing")?;
    let (inputs, change, state_capacity, extra_witnesses) = if op == OP_MATERIALIZE {
        (
            funding_cells(funding).to_vec(),
            funding_total.checked_sub(STATE_CAPACITY + RECEIPT_CAPACITY),
            STATE_CAPACITY,
            funding_cells(funding).len().saturating_sub(1),
        )
    } else {
        let old_ref = old_ref.context("RWA state/event tx requires an old ref")?;
        let mut inputs = vec![old_ref.clone()];
        inputs.extend_from_slice(funding_cells(funding));
        (
            inputs,
            funding_total.checked_sub(RECEIPT_CAPACITY),
            old_ref["capacity"].as_u64().context("RWA old ref capacity is missing")?,
            funding_cells(funding).len(),
        )
    };
    let change = change.context("RWA state/event funding capacity is too small")?;
    if change == 0 {
        bail!("RWA state/event funding capacity is too small");
    }
    let mut witnesses = vec![witness(op, material)];
    witnesses.extend(vec!["0x".into(); extra_witnesses]);
    Ok(transaction(
        &inputs,
        vec![
            json!({"capacity": format!("0x{state_capacity:x}"), "lock": always_success_lock("0x"), "type": lifecycle_type(lifecycle_hash)}),
            json!({"capacity": format!("0x{RECEIPT_CAPACITY:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
            json!({"capacity": format!("0x{change:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
        ],
        vec![hex0x(&material.new_cell_data), hex0x(&material.event_data), "0x".into()],
        deps,
        witnesses,
        vec![header.into()],
    ))
}

fn build_settle(old_ref: &Value, funding: &Value, deps: Vec<Value>, header: &str, material: &Material) -> Result<Value> {
    let total = old_ref["capacity"].as_u64().context("RWA old ref capacity is missing")?
        + funding["total_capacity"].as_u64().context("RWA funding total is missing")?;
    let change = total.checked_sub(RECEIPT_CAPACITY).context("RWA settle funding capacity is too small")?;
    if change == 0 {
        bail!("RWA settle funding capacity is too small");
    }
    let mut inputs = vec![old_ref.clone()];
    inputs.extend_from_slice(funding_cells(funding));
    let mut witnesses = vec![witness(OP_SETTLE, material)];
    witnesses.extend(vec!["0x".into(); funding_cells(funding).len()]);
    Ok(transaction(
        &inputs,
        vec![
            json!({"capacity": format!("0x{RECEIPT_CAPACITY:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
            json!({"capacity": format!("0x{change:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
        ],
        vec![hex0x(&material.event_data), "0x".into()],
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
        .unwrap_or_else(|| root.join(format!("target/novaseal-rwa-receipt-devnet-stateful-live/{timestamp}")));
    fs::create_dir_all(&run_dir)?;
    let run_dir = fs::canonicalize(run_dir)?;
    let lifecycle_path = run_dir.join("nova-rwa-receipt-lifecycle-type.elf");
    compile_contract(&root, contract, &lifecycle_path)?;
    let verifier_path = root.join("proposals/novaseal/v0-mvp-skeleton/target/novaseal-btc-verifier-riscv-shell-release.elf");
    if !verifier_path.is_file() {
        bail!("missing verifier ELF: {}", verifier_path.display());
    }
    let mut devnet = CkbDevnet::new(ckb_repo.clone(), ckb_bin.clone(), run_dir.clone())?;
    let mut report = contract_report_header(contract, "rwa_receipt_materialize_claim_settle", &root, &ckb_repo, &ckb_bin, &run_dir);
    let mut stage = "initializing";
    let scenario = (|| -> Result<()> {
        stage = "start devnet";
        devnet.start()?;
        stage = "deploy artifacts";
        let genesis = devnet.get_block_by_number(0)?;
        let always = always_success_dep(genesis["transactions"][0]["hash"].as_str().context("genesis hash is missing")?);
        let verifier = deploy_code(&mut devnet, "cellscript_btc_bip340_verifier_riscv", &fs::read(&verifier_path)?, &always)?;
        let lifecycle = deploy_code(&mut devnet, "nova_rwa_receipt_lifecycle_type", &fs::read(&lifecycle_path)?, &always)?;
        let lifecycle_hash = lifecycle["data_hash"].as_str().context("lifecycle hash is missing")?.to_owned();
        let deps = vec![verifier["cell_dep"].clone(), lifecycle["cell_dep"].clone(), always];
        let source_paths = [
            "proposals/novaseal/rwa-receipt-profile-v0/Cell.toml",
            "proposals/novaseal/rwa-receipt-profile-v0/src",
            "proposals/novaseal/rwa-receipt-profile-v0/schemas",
            "proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier",
            "crates/cellscript-tools/src/novaseal_planned_rwa.rs",
            "crates/cellscript-tools/src/ckb_devnet.rs",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
        let artifacts = BTreeMap::from([("verifier".into(), verifier_path.clone()), ("lifecycle".into(), lifecycle_path.clone())]);
        let source_provenance = provenance(&root, &source_paths, &artifacts)?;
        let base = base("live")?;
        let type_script = lifecycle_type(&lifecycle_hash);

        stage = "valid materialize";
        let materialize = material(OP_MATERIALIZE, &base, None, false, false, None)?;
        let header = devnet.rpc("get_tip_header", vec![])?;
        let funding = devnet.collect_spendable(STATE_CAPACITY + RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_state_event(
            OP_MATERIALIZE,
            None,
            &funding,
            &lifecycle_hash,
            deps.clone(),
            header["hash"].as_str().unwrap(),
            &materialize,
        )?;
        let materialize_dry = devnet.rpc("dry_run_transaction", vec![tx.clone()])?;
        let materialize_commit = devnet.submit_and_commit(&tx, "RWA receipt materialize")?;
        let materialize_hash = materialize_commit["tx_hash"].as_str().unwrap();
        let materialized_live = devnet.assert_live_cell(
            materialize_hash,
            0,
            "RWA materialized receipt",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&materialize.new_cell_data),
        )?;
        let materialized_event = devnet.assert_live_cell(
            materialize_hash,
            1,
            "RWA materialized audit event",
            Some(RECEIPT_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&Value::Null),
            Some(&materialize.event_data),
        )?;
        let materialized_ref = json!({"tx_hash": materialize_hash, "index": 0, "capacity": STATE_CAPACITY});

        stage = "negative claim wrong holder signature";
        let header = devnet.rpc("get_tip_header", vec![])?;
        let wrong_claim = material(OP_CLAIM, &base, Some(&materialize.new_cell), false, true, None)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_state_event(
            OP_CLAIM,
            Some(&materialized_ref),
            &funding,
            &lifecycle_hash,
            deps.clone(),
            header["hash"].as_str().unwrap(),
            &wrong_claim,
        )?;
        let wrong_claim_reject =
            devnet.dry_run_rejects(&tx, "RWA wrong holder claim", Some("Inputs[0].Type"), Some(&lifecycle_hash), Some(56))?;
        let _post_claim_negative = devnet.assert_live_cell(
            materialize_hash,
            0,
            "post-negative RWA materialized receipt",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&materialize.new_cell_data),
        )?;

        stage = "valid claim";
        let header = devnet.rpc("get_tip_header", vec![])?;
        let claim = material(OP_CLAIM, &base, Some(&materialize.new_cell), false, false, None)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_state_event(
            OP_CLAIM,
            Some(&materialized_ref),
            &funding,
            &lifecycle_hash,
            deps.clone(),
            header["hash"].as_str().unwrap(),
            &claim,
        )?;
        let claim_dry = devnet.rpc("dry_run_transaction", vec![tx.clone()])?;
        let claim_commit = devnet.submit_and_commit(&tx, "RWA receipt claim")?;
        let old_dead = devnet.wait_dead_cell(materialize_hash, 0)?;
        let claim_hash = claim_commit["tx_hash"].as_str().unwrap();
        let claimed_live = devnet.assert_live_cell(
            claim_hash,
            0,
            "RWA claimed receipt",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&claim.new_cell_data),
        )?;
        let claim_event = devnet.assert_live_cell(
            claim_hash,
            1,
            "RWA claim event",
            Some(RECEIPT_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&Value::Null),
            Some(&claim.event_data),
        )?;
        let claimed_ref = json!({"tx_hash": claim_hash, "index": 0, "capacity": STATE_CAPACITY});

        stage = "negative settlement wrong issuer signature";
        let header = devnet.rpc("get_tip_header", vec![])?;
        let wrong_settle = material(OP_SETTLE, &base, Some(&claim.new_cell), true, false, None)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_settle(&claimed_ref, &funding, deps.clone(), header["hash"].as_str().unwrap(), &wrong_settle)?;
        let wrong_settle_reject =
            devnet.dry_run_rejects(&tx, "RWA wrong issuer settlement", Some("Inputs[0].Type"), Some(&lifecycle_hash), Some(56))?;

        stage = "negative settlement amount mutation";
        let amount_mutation = material(OP_SETTLE, &base, Some(&claim.new_cell), false, false, Some(claim.new_cell.amount - 1))?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_settle(&claimed_ref, &funding, deps.clone(), header["hash"].as_str().unwrap(), &amount_mutation)?;
        let amount_reject =
            devnet.dry_run_rejects(&tx, "RWA settlement amount mutation", Some("Inputs[0].Type"), Some(&lifecycle_hash), Some(5))?;
        let post_negative = devnet.assert_live_cell(
            claim_hash,
            0,
            "post-negative RWA claimed receipt",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&claim.new_cell_data),
        )?;

        stage = "valid settle";
        let header = devnet.rpc("get_tip_header", vec![])?;
        let settle = material(OP_SETTLE, &base, Some(&claim.new_cell), false, false, None)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_settle(&claimed_ref, &funding, deps, header["hash"].as_str().unwrap(), &settle)?;
        let settle_dry = devnet.rpc("dry_run_transaction", vec![tx.clone()])?;
        let settle_commit = devnet.submit_and_commit(&tx, "RWA receipt settle")?;
        let claim_dead = devnet.wait_dead_cell(claim_hash, 0)?;
        let settle_event = devnet.assert_live_cell(
            settle_commit["tx_hash"].as_str().unwrap(),
            0,
            "RWA settlement event",
            Some(RECEIPT_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&Value::Null),
            Some(&settle.event_data),
        )?;

        report.as_object_mut().unwrap().extend(
            json!({
                "status": "passed", "live_devnet_rpc_executed": true, "stateful_lifecycle_executed": true,
                "ckb_log": devnet.log_path.display().to_string(), "rpc_url": devnet.rpc_url,
                "artifacts": {"verifier": verifier, "lifecycle": lifecycle}, "provenance": source_provenance,
                "materialize": {"dry_run_cycles": materialize_dry["cycles"], "commit": materialize_commit,
                    "receipt_live": materialized_live["status"] == "live", "audit_event_live": materialized_event["status"] == "live",
                    "event_hash": hex0x(&materialize.receipt_hash)},
                "claim": {"dry_run_cycles": claim_dry["cycles"], "commit": claim_commit,
                    "old_receipt_not_live": old_dead["status"] != "live", "claimed_receipt_live": claimed_live["status"] == "live",
                    "claim_event_live": claim_event["status"] == "live", "event_hash": hex0x(&claim.receipt_hash)},
                "settle": {"dry_run_cycles": settle_dry["cycles"], "commit": settle_commit,
                    "old_claim_not_live": claim_dead["status"] != "live", "settlement_receipt_live": settle_event["status"] == "live",
                    "settlement_event_live": settle_event["status"] == "live", "amount_conserved": settle.old_cell.amount == claim.new_cell.amount,
                    "event_hash": hex0x(&settle.receipt_hash)},
                "negative_cases": {"wrong_holder_claim_dry_run": wrong_claim_reject,
                    "wrong_issuer_settlement_dry_run": wrong_settle_reject, "amount_mutation_dry_run": amount_reject,
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
    fn materialize_material_matches_legacy_vectors() {
        let base = base("parity").unwrap();
        let value = material(OP_MATERIALIZE, &base, None, false, false, None).unwrap();
        assert_eq!(hex0x(&ckb_hash(&value.new_cell_data)), "0xa6022d9b654a0e062d2eefaea34e008ee12ac020f6f74c54bfedc7dcddfc1a3e");
        assert_eq!(hex0x(&ckb_hash(&value.signed_intent)), "0x265e8ffa7c5adaeeb7942713e8507bd53269953c5be222174b1b2804192a275f");
        assert_eq!(hex0x(&value.receipt_hash), "0xf85aeee6b63d3b9fc7eda2c9969cb31844cd1aa14eccf03c9f484dd1f7cc4790");
        assert_eq!(hex0x(&ckb_hash(&value.event_data)), "0xbadc9d1806c37c8223e7583455454e2aa754b4a517f9e077aeb8f91e165d0380");
    }
}
