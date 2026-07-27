use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::ckb_devnet::{
    always_success_dep, always_success_lock, ckb_hash, deploy_code, funding_cells, hex0x, provenance, resolve_ckb_bin, schnorr_sign,
    transaction, u16_bytes, u32_bytes, u64_bytes, u8_bytes, xonly_pubkey, CkbDevnet, RECEIPT_CAPACITY, SHANNONS, STATE_CAPACITY,
    TEST_AUX_RAND, TEST_SECRET_KEY, ZERO_HASH,
};
use crate::novaseal_planned_live::{compile_contract, contract_report_header, lifecycle_type, Contract};

const OP_ISSUE: u64 = 0;
const OP_TRANSFER: u64 = 1;
const OP_SETTLE: u64 = 2;
const STATUS_ACTIVE: u64 = 1;
const STATUS_SETTLED: u64 = 2;
const HOLDER_SECRET: [u8; 32] = [0x22; 32];
const HOLDER_AUX: [u8; 32] = [0x42; 32];
const RECEIVER_SECRET: [u8; 32] = [0x33; 32];
const RECEIVER_AUX: [u8; 32] = [0x66; 32];

type Hash = [u8; 32];

#[derive(Clone)]
struct Base {
    asset: Hash,
    xudt: Hash,
    issuer: Hash,
    holder: Hash,
    amount: u64,
    expiry: u64,
}

#[derive(Clone)]
struct Cell {
    asset: Hash,
    xudt: Hash,
    issuer: Hash,
    holder: Hash,
    amount: u64,
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
    receipt_hash: Hash,
    signature: Vec<u8>,
}

fn append(target: &mut Vec<u8>, chunks: &[&[u8]]) {
    for chunk in chunks {
        target.extend_from_slice(chunk);
    }
}

fn zero_cell() -> Cell {
    Cell {
        asset: ZERO_HASH,
        xudt: ZERO_HASH,
        issuer: ZERO_HASH,
        holder: ZERO_HASH,
        amount: 0,
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
            &cell.asset,
            &cell.xudt,
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
            &cell.asset,
            &cell.xudt,
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
    old_holder: &Hash,
    new_holder: &Hash,
    old_status: u64,
    new_status: u64,
    old_amount: u64,
    transfer_amount: u64,
    new_amount: u64,
    old_nonce: u64,
    new_nonce: u64,
) -> Vec<u8> {
    let mut out = Vec::new();
    append(
        &mut out,
        &[
            &u8_bytes(op),
            &base.asset,
            &base.xudt,
            &base.issuer,
            old_holder,
            new_holder,
            &u8_bytes(old_status),
            &u8_bytes(new_status),
            &u64_bytes(old_amount),
            &u64_bytes(transfer_amount),
            &u64_bytes(new_amount),
            &u64_bytes(old_nonce),
            &u64_bytes(new_nonce),
            &u64_bytes(base.expiry),
            &ZERO_HASH,
        ],
    );
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
    let mut packed = Vec::new();
    append(
        &mut packed,
        &[
            &base.asset,
            &base.xudt,
            &u8_bytes(op),
            &u8_bytes(op),
            &base.asset,
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
    ckb_hash(&packed)
}

#[allow(clippy::too_many_arguments)]
fn receipt_commitment(
    op: u64,
    base: &Base,
    old_holder: &Hash,
    new_holder: &Hash,
    old_status: u64,
    new_status: u64,
    old_amount: u64,
    transfer_amount: u64,
    new_amount: u64,
    old_nonce: u64,
    new_nonce: u64,
    core_hash: &Hash,
) -> Vec<u8> {
    let mut out = Vec::new();
    append(
        &mut out,
        &[
            &u8_bytes(op),
            &base.asset,
            &base.xudt,
            old_holder,
            new_holder,
            &u8_bytes(old_status),
            &u8_bytes(new_status),
            &u64_bytes(old_amount),
            &u64_bytes(transfer_amount),
            &u64_bytes(new_amount),
            &u64_bytes(old_nonce),
            &u64_bytes(new_nonce),
            core_hash,
            &ZERO_HASH,
        ],
    );
    out
}

#[allow(clippy::too_many_arguments)]
fn receipt(
    op: u64,
    base: &Base,
    old_holder: &Hash,
    new_holder: &Hash,
    old_status: u64,
    new_status: u64,
    old_amount: u64,
    transfer_amount: u64,
    new_amount: u64,
    old_nonce: u64,
    new_nonce: u64,
    core_hash: &Hash,
    signed_hash: &Hash,
    receipt_hash: &Hash,
    authority: &Hash,
) -> Vec<u8> {
    let mut out = Vec::new();
    append(
        &mut out,
        &[
            &u8_bytes(op),
            &base.asset,
            &base.xudt,
            old_holder,
            new_holder,
            &u8_bytes(old_status),
            &u8_bytes(new_status),
            &u64_bytes(old_amount),
            &u64_bytes(transfer_amount),
            &u64_bytes(new_amount),
            &u64_bytes(old_nonce),
            &u64_bytes(new_nonce),
            core_hash,
            signed_hash,
            &ZERO_HASH,
            receipt_hash,
            authority,
            &u64_bytes(base.expiry),
        ],
    );
    out
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

fn base(label: &str) -> Result<Base> {
    Ok(Base {
        asset: ckb_hash(format!("NovaSeal fungible xUDT asset {label}").as_bytes()),
        xudt: ckb_hash(format!("NovaSeal fungible xUDT type {label}").as_bytes()),
        issuer: xonly_pubkey(&TEST_SECRET_KEY)?,
        holder: xonly_pubkey(&HOLDER_SECRET)?,
        amount: 1_000,
        expiry: (1_u64 << 63) - 1,
    })
}

fn material(op: u64, base: &Base, old: Option<&Cell>, mutate: bool, amount_override: Option<u64>) -> Result<Material> {
    let (
        old_holder,
        new_holder,
        old_status,
        new_status,
        old_amount,
        transfer_amount,
        new_amount,
        old_nonce,
        new_nonce,
        authority,
        secret,
        aux,
        mut next,
    ) = match op {
        OP_ISSUE => {
            let next = Cell {
                asset: base.asset,
                xudt: base.xudt,
                issuer: base.issuer,
                holder: base.holder,
                amount: base.amount,
                status: STATUS_ACTIVE,
                receipt: ZERO_HASH,
                nonce: 0,
                expiry: base.expiry,
            };
            (
                ZERO_HASH,
                base.holder,
                0,
                STATUS_ACTIVE,
                0,
                base.amount,
                base.amount,
                0,
                0,
                base.issuer,
                &TEST_SECRET_KEY,
                &TEST_AUX_RAND,
                next,
            )
        }
        OP_TRANSFER => {
            let old = old.context("xUDT transfer material requires an old cell")?;
            let receiver = xonly_pubkey(&RECEIVER_SECRET)?;
            let mut next = old.clone();
            next.holder = receiver;
            next.receipt = ZERO_HASH;
            next.nonce += 1;
            (
                old.holder,
                receiver,
                STATUS_ACTIVE,
                STATUS_ACTIVE,
                old.amount,
                amount_override.unwrap_or(old.amount),
                old.amount,
                old.nonce,
                old.nonce + 1,
                old.holder,
                &HOLDER_SECRET,
                &HOLDER_AUX,
                next,
            )
        }
        OP_SETTLE => {
            let old = old.context("xUDT settle material requires an old cell")?;
            (
                old.holder,
                old.holder,
                STATUS_ACTIVE,
                STATUS_SETTLED,
                old.amount,
                old.amount,
                0,
                old.nonce,
                old.nonce + 1,
                old.holder,
                &RECEIVER_SECRET,
                &RECEIVER_AUX,
                zero_cell(),
            )
        }
        _ => bail!("unknown xUDT op {op}"),
    };
    let old_state = old.map(|cell| ckb_hash(&pack_state(cell))).unwrap_or(ZERO_HASH);
    let new_state = if op == OP_SETTLE { ZERO_HASH } else { ckb_hash(&pack_state(&next)) };
    let core = pack_core(
        op,
        base,
        &old_holder,
        &new_holder,
        old_status,
        new_status,
        old_amount,
        transfer_amount,
        new_amount,
        old_nonce,
        new_nonce,
    );
    let core_hash = ckb_hash(&core);
    let receipt_hash = ckb_hash(&receipt_commitment(
        op,
        base,
        &old_holder,
        &new_holder,
        old_status,
        new_status,
        old_amount,
        transfer_amount,
        new_amount,
        old_nonce,
        new_nonce,
        &core_hash,
    ));
    let canonical = canonical(op, base, &old_state, &new_state, old_nonce, new_nonce, &authority, &core_hash);
    let mut signed_intent = core;
    signed_intent.extend_from_slice(&canonical);
    signed_intent.extend_from_slice(&receipt_hash);
    let signed_hash = ckb_hash(&signed_intent);
    let receipt_data = receipt(
        op,
        base,
        &old_holder,
        &new_holder,
        old_status,
        new_status,
        old_amount,
        transfer_amount,
        new_amount,
        old_nonce,
        new_nonce,
        &core_hash,
        &signed_hash,
        &receipt_hash,
        &authority,
    );
    if op != OP_SETTLE {
        next.receipt = receipt_hash;
    }
    Ok(Material {
        old_cell_data: pack_cell(old.unwrap_or(&zero_cell())),
        new_cell_data: pack_cell(&next),
        new_cell: next,
        receipt_data,
        signed_intent,
        receipt_hash,
        signature: signature(secret, aux, &signed_hash, mutate)?,
    })
}

fn witness(op: u64, material: &Material) -> String {
    let mut out = b"CSARGv1\0".to_vec();
    out.extend_from_slice(&u8_bytes(op));
    for value in [
        material.old_cell_data.as_slice(),
        material.new_cell_data.as_slice(),
        material.signed_intent.as_slice(),
        material.signature.as_slice(),
    ] {
        out.extend_from_slice(&u32_bytes(value.len()));
        out.extend_from_slice(value);
    }
    hex0x(&out)
}

fn build_issue(funding: &Value, lifecycle_hash: &str, deps: Vec<Value>, header: &str, material: &Material) -> Result<Value> {
    let total = funding["total_capacity"].as_u64().context("xUDT issue funding total is missing")?;
    let change = total.checked_sub(STATE_CAPACITY + RECEIPT_CAPACITY).context("xUDT issue funding capacity is too small")?;
    if change == 0 {
        bail!("xUDT issue funding capacity is too small");
    }
    let cells = funding_cells(funding);
    let mut witnesses = vec![witness(OP_ISSUE, material)];
    witnesses.extend(vec!["0x".into(); cells.len().saturating_sub(1)]);
    Ok(transaction(
        cells,
        vec![
            json!({"capacity": format!("0x{STATE_CAPACITY:x}"), "lock": always_success_lock("0x"), "type": lifecycle_type(lifecycle_hash)}),
            json!({"capacity": format!("0x{RECEIPT_CAPACITY:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
            json!({"capacity": format!("0x{change:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
        ],
        vec![hex0x(&material.new_cell_data), hex0x(&material.receipt_data), "0x".into()],
        deps,
        witnesses,
        vec![header.into()],
    ))
}

fn build_transfer(
    old_ref: &Value,
    funding: &Value,
    lifecycle_hash: &str,
    deps: Vec<Value>,
    header: &str,
    material: &Material,
) -> Result<Value> {
    let total = funding["total_capacity"].as_u64().context("xUDT transfer funding total is missing")?;
    let change = total.checked_sub(RECEIPT_CAPACITY).context("xUDT transfer funding capacity is too small")?;
    if change == 0 {
        bail!("xUDT transfer funding capacity is too small");
    }
    let mut inputs = vec![old_ref.clone()];
    inputs.extend_from_slice(funding_cells(funding));
    let mut witnesses = vec![witness(OP_TRANSFER, material)];
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

fn build_settle(old_ref: &Value, funding: &Value, deps: Vec<Value>, header: &str, material: &Material) -> Result<Value> {
    let total =
        old_ref["capacity"].as_u64().unwrap() + funding["total_capacity"].as_u64().context("xUDT settle funding total is missing")?;
    let change = total.checked_sub(RECEIPT_CAPACITY).context("xUDT settle funding capacity is too small")?;
    if change == 0 {
        bail!("xUDT settle funding capacity is too small");
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
        .unwrap_or_else(|| root.join(format!("target/novaseal-fungible-xudt-devnet-stateful-live/{timestamp}")));
    fs::create_dir_all(&run_dir)?;
    let run_dir = fs::canonicalize(run_dir)?;
    let lifecycle_path = run_dir.join("nova-fungible-xudt-lifecycle-type.elf");
    compile_contract(&root, contract, &lifecycle_path)?;
    let verifier_path = root.join("proposals/novaseal/v0-mvp-skeleton/target/novaseal-btc-verifier-riscv-shell-release.elf");
    if !verifier_path.is_file() {
        bail!("missing verifier ELF: {}", verifier_path.display());
    }
    let mut devnet = CkbDevnet::new(ckb_repo.clone(), ckb_bin.clone(), run_dir.clone())?;
    let mut report = contract_report_header(contract, "fungible_xudt_issue_transfer_settle", &root, &ckb_repo, &ckb_bin, &run_dir);
    let mut stage = "initializing";
    let scenario = (|| -> Result<()> {
        stage = "start devnet";
        devnet.start()?;
        stage = "deploy artifacts";
        let genesis = devnet.get_block_by_number(0)?;
        let always = always_success_dep(genesis["transactions"][0]["hash"].as_str().context("genesis hash is missing")?);
        let verifier = deploy_code(&mut devnet, "cellscript_btc_bip340_verifier_riscv", &fs::read(&verifier_path)?, &always)?;
        let lifecycle = deploy_code(&mut devnet, "nova_fungible_xudt_lifecycle_type", &fs::read(&lifecycle_path)?, &always)?;
        let lifecycle_hash = lifecycle["data_hash"].as_str().context("lifecycle hash is missing")?.to_owned();
        let deps = vec![verifier["cell_dep"].clone(), lifecycle["cell_dep"].clone(), always];
        let source_paths = [
            "proposals/novaseal/fungible-xudt-profile-v0/Cell.toml",
            "proposals/novaseal/fungible-xudt-profile-v0/src",
            "proposals/novaseal/fungible-xudt-profile-v0/schemas",
            "proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier",
            "crates/cellscript-tools/src/novaseal_planned_fungible.rs",
            "crates/cellscript-tools/src/ckb_devnet.rs",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
        let artifacts = BTreeMap::from([("verifier".into(), verifier_path.clone()), ("lifecycle".into(), lifecycle_path.clone())]);
        let source_provenance = provenance(&root, &source_paths, &artifacts)?;
        let base = base("live")?;

        stage = "valid issue";
        let issue_material = material(OP_ISSUE, &base, None, false, None)?;
        let header = devnet.rpc("get_tip_header", vec![])?;
        let funding = devnet.collect_spendable(STATE_CAPACITY + RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_issue(&funding, &lifecycle_hash, deps.clone(), header["hash"].as_str().unwrap(), &issue_material)?;
        let issue_dry = devnet.rpc("dry_run_transaction", vec![tx.clone()])?;
        let issue_commit = devnet.submit_and_commit(&tx, "fungible xUDT issue")?;
        let issue_hash = issue_commit["tx_hash"].as_str().unwrap();
        let type_script = lifecycle_type(&lifecycle_hash);
        let issue_balance_live = devnet.assert_live_cell(
            issue_hash,
            0,
            "xUDT issued balance",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&issue_material.new_cell_data),
        )?;
        let issue_receipt_live = devnet.assert_live_cell(
            issue_hash,
            1,
            "xUDT issue receipt",
            Some(RECEIPT_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&Value::Null),
            Some(&issue_material.receipt_data),
        )?;
        let issued_ref = json!({"tx_hash": issue_hash, "index": 0, "capacity": STATE_CAPACITY});

        stage = "negative transfer wrong holder signature";
        let negative_header = devnet.rpc("get_tip_header", vec![])?;
        let wrong = material(OP_TRANSFER, &base, Some(&issue_material.new_cell), true, None)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx =
            build_transfer(&issued_ref, &funding, &lifecycle_hash, deps.clone(), negative_header["hash"].as_str().unwrap(), &wrong)?;
        let wrong_signature = devnet.dry_run_rejects(
            &tx,
            "xUDT wrong holder signature transfer",
            Some("Inputs[0].Type"),
            Some(&lifecycle_hash),
            Some(56),
        )?;

        stage = "negative transfer amount mismatch";
        let mismatch = material(OP_TRANSFER, &base, Some(&issue_material.new_cell), false, Some(issue_material.new_cell.amount - 1))?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_transfer(
            &issued_ref,
            &funding,
            &lifecycle_hash,
            deps.clone(),
            negative_header["hash"].as_str().unwrap(),
            &mismatch,
        )?;
        let amount_mismatch =
            devnet.dry_run_rejects(&tx, "xUDT transfer amount mismatch", Some("Inputs[0].Type"), Some(&lifecycle_hash), Some(5))?;
        let post_transfer_negative = devnet.assert_live_cell(
            issue_hash,
            0,
            "post-negative xUDT issued balance",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&issue_material.new_cell_data),
        )?;

        stage = "valid transfer";
        let transfer_header = devnet.rpc("get_tip_header", vec![])?;
        let transfer_material = material(OP_TRANSFER, &base, Some(&issue_material.new_cell), false, None)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_transfer(
            &issued_ref,
            &funding,
            &lifecycle_hash,
            deps.clone(),
            transfer_header["hash"].as_str().unwrap(),
            &transfer_material,
        )?;
        let transfer_dry = devnet.rpc("dry_run_transaction", vec![tx.clone()])?;
        let transfer_commit = devnet.submit_and_commit(&tx, "fungible xUDT transfer")?;
        let old_dead = devnet.wait_dead_cell(issue_hash, 0)?;
        let transfer_hash = transfer_commit["tx_hash"].as_str().unwrap();
        let receiver_live = devnet.assert_live_cell(
            transfer_hash,
            0,
            "xUDT receiver balance",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&transfer_material.new_cell_data),
        )?;
        let transfer_receipt_live = devnet.assert_live_cell(
            transfer_hash,
            1,
            "xUDT transfer receipt",
            Some(RECEIPT_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&Value::Null),
            Some(&transfer_material.receipt_data),
        )?;
        let receiver_ref = json!({"tx_hash": transfer_hash, "index": 0, "capacity": STATE_CAPACITY});

        stage = "negative settle wrong holder signature";
        let settle_negative_header = devnet.rpc("get_tip_header", vec![])?;
        let wrong_settle = material(OP_SETTLE, &base, Some(&transfer_material.new_cell), true, None)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_settle(&receiver_ref, &funding, deps.clone(), settle_negative_header["hash"].as_str().unwrap(), &wrong_settle)?;
        let settle_wrong_signature = devnet.dry_run_rejects(
            &tx,
            "xUDT wrong holder signature settle",
            Some("Inputs[0].Type"),
            Some(&lifecycle_hash),
            Some(56),
        )?;
        let post_negative = devnet.assert_live_cell(
            transfer_hash,
            0,
            "post-negative xUDT receiver balance",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&transfer_material.new_cell_data),
        )?;

        stage = "valid settle";
        let settle_header = devnet.rpc("get_tip_header", vec![])?;
        let settle_material = material(OP_SETTLE, &base, Some(&transfer_material.new_cell), false, None)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_settle(&receiver_ref, &funding, deps, settle_header["hash"].as_str().unwrap(), &settle_material)?;
        let settle_dry = devnet.rpc("dry_run_transaction", vec![tx.clone()])?;
        let settle_commit = devnet.submit_and_commit(&tx, "fungible xUDT settle")?;
        let receiver_dead = devnet.wait_dead_cell(transfer_hash, 0)?;
        let settle_live = devnet.assert_live_cell(
            settle_commit["tx_hash"].as_str().unwrap(),
            0,
            "xUDT settlement receipt",
            Some(RECEIPT_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&Value::Null),
            Some(&settle_material.receipt_data),
        )?;

        report.as_object_mut().unwrap().extend(
            json!({
                "status": "passed", "live_devnet_rpc_executed": true, "stateful_lifecycle_executed": true,
                "ckb_log": devnet.log_path.display().to_string(), "rpc_url": devnet.rpc_url,
                "artifacts": {"verifier": verifier, "lifecycle": lifecycle}, "provenance": source_provenance,
                "issue": {"dry_run_cycles": issue_dry["cycles"], "commit": issue_commit,
                    "balance_live": issue_balance_live["status"] == "live", "receipt_live": issue_receipt_live["status"] == "live",
                    "balance_data_hash": hex0x(&ckb_hash(&issue_material.new_cell_data)), "receipt_hash": hex0x(&issue_material.receipt_hash)},
                "transfer": {"dry_run_cycles": transfer_dry["cycles"], "commit": transfer_commit,
                    "old_balance_not_live": old_dead["status"] != "live", "sender_balance_live": post_transfer_negative["status"] == "live",
                    "receiver_balance_live": receiver_live["status"] == "live", "receipt_live": transfer_receipt_live["status"] == "live",
                    "amount_conserved": transfer_material.new_cell.amount == issue_material.new_cell.amount,
                    "receipt_hash": hex0x(&transfer_material.receipt_hash)},
                "settle": {"dry_run_cycles": settle_dry["cycles"], "commit": settle_commit,
                    "old_balance_not_live": receiver_dead["status"] != "live", "settlement_receipt_live": settle_live["status"] == "live",
                    "receipt_hash": hex0x(&settle_material.receipt_hash)},
                "negative_cases": {"wrong_holder_signature_dry_run": wrong_signature,
                    "transfer_amount_mismatch_dry_run": amount_mismatch, "settle_wrong_holder_signature_dry_run": settle_wrong_signature,
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
    fn issue_material_is_stable() {
        let base = base("parity").unwrap();
        let value = material(OP_ISSUE, &base, None, false, None).unwrap();
        assert_eq!(value.new_cell.amount, 1_000);
        assert_eq!(hex0x(&ckb_hash(&value.new_cell_data)), "0x93a3f78c8cde6463adb34d4fbb112577bad13dcc9d13fbdede8ced2c69c707dd");
        assert_eq!(hex0x(&ckb_hash(&value.signed_intent)), "0x9935e84f62134cd4760cd08c5b47256d179b0fdf9388a8820cacffe111b01e5e");
        assert_eq!(hex0x(&value.receipt_hash), "0xedbd62d6f61220475c7284cb1e624d26b6147fb6792abc1554b95888cda0a990");
        assert_eq!(hex0x(&ckb_hash(&value.receipt_data)), "0xc456ae4d35cf68a160eb8c15a0b4abe2204f74ba482485466fcf451b552eef48");
    }
}
