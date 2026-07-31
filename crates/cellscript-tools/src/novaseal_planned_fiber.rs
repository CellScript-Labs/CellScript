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

const OP_SETTLE: u64 = 0;
const OP_INITIALIZE: u64 = 255;
const STATUS_ACTIVE: u64 = 1;
const STATUS_SETTLED: u64 = 2;
type Hash = [u8; 32];

#[derive(Clone)]
struct Base {
    candidate: Hash,
    policy: Hash,
    operator: Hash,
    channel: Hash,
    initial_balance: Hash,
    settled_balance: Hash,
    route: Hash,
    payment: Hash,
    amount: u64,
    expiry: u64,
}

#[derive(Clone)]
struct Cell {
    candidate: Hash,
    policy: Hash,
    operator: Hash,
    channel: Hash,
    balance: Hash,
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
    settlement: Hash,
    receipt_hash: Hash,
}

fn append(out: &mut Vec<u8>, chunks: &[&[u8]]) {
    for chunk in chunks {
        out.extend_from_slice(chunk);
    }
}

fn base(label: &str) -> Result<Base> {
    Ok(Base {
        candidate: ckb_hash(format!("NovaSeal Fiber candidate {label}").as_bytes()),
        policy: ckb_hash(format!("NovaSeal Fiber policy {label}").as_bytes()),
        operator: xonly_pubkey(&TEST_SECRET_KEY)?,
        channel: ckb_hash(format!("NovaSeal Fiber channel {label}").as_bytes()),
        initial_balance: ckb_hash(format!("NovaSeal Fiber initial balance {label}").as_bytes()),
        settled_balance: ckb_hash(format!("NovaSeal Fiber settled balance {label}").as_bytes()),
        route: ckb_hash(format!("NovaSeal Fiber route {label}").as_bytes()),
        payment: ckb_hash(format!("NovaSeal Fiber payment {label}").as_bytes()),
        amount: 42_000,
        expiry: (1_u64 << 63) - 1,
    })
}

fn zero_cell() -> Cell {
    Cell {
        candidate: ZERO_HASH,
        policy: ZERO_HASH,
        operator: ZERO_HASH,
        channel: ZERO_HASH,
        balance: ZERO_HASH,
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
            &cell.candidate,
            &cell.policy,
            &cell.operator,
            &cell.channel,
            &cell.balance,
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
            &cell.candidate,
            &cell.policy,
            &cell.operator,
            &cell.channel,
            &cell.balance,
            &u8_bytes(cell.status),
            &cell.receipt,
            &u64_bytes(cell.nonce),
            &u64_bytes(cell.expiry),
        ],
    );
    out
}

fn settlement(base: &Base, old_balance: &Hash, new_balance: &Hash) -> Vec<u8> {
    let mut out = Vec::new();
    append(&mut out, &[&base.channel, &base.route, &base.payment, old_balance, new_balance, &u64_bytes(base.amount), &ZERO_HASH]);
    out
}

#[allow(clippy::too_many_arguments)]
fn pack_core(
    op: u64,
    base: &Base,
    route: &Hash,
    payment: &Hash,
    old_balance: &Hash,
    new_balance: &Hash,
    amount: u64,
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
            &base.candidate,
            &base.policy,
            &base.operator,
            &base.channel,
            route,
            payment,
            old_balance,
            new_balance,
            &u64_bytes(amount),
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
    old_balance: &Hash,
    new_balance: &Hash,
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
            &u8_bytes(OP_SETTLE),
            &base.candidate,
            &base.policy,
            &base.operator,
            &base.channel,
            &base.route,
            &base.payment,
            old_balance,
            new_balance,
            &u64_bytes(base.amount),
            &u8_bytes(STATUS_ACTIVE),
            &u8_bytes(STATUS_SETTLED),
            &u64_bytes(old_nonce),
            &u64_bytes(new_nonce),
            core_hash,
        ],
    );
    if let (Some(signed_hash), Some(receipt_hash)) = (signed_hash, receipt_hash) {
        append(&mut out, &[signed_hash, &ZERO_HASH, receipt_hash, &base.operator, &u64_bytes(base.expiry)]);
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
            &base.candidate,
            &base.policy,
            &u8_bytes(op),
            &u8_bytes(op),
            &base.candidate,
            old_state,
            new_state,
            &u64_bytes(old_nonce),
            &u64_bytes(new_nonce),
            &u64_bytes(base.expiry),
            &base.operator,
            body,
            &ZERO_HASH,
        ],
    );
    ckb_hash(&out)
}

fn material(op: u64, base: &Base, old: Option<&Cell>, mutate: bool, replay: bool) -> Result<Material> {
    let (old_balance, new_balance, route, payment, amount, old_status, new_status, old_nonce, new_nonce, mut next) = match op {
        OP_INITIALIZE => (
            ZERO_HASH,
            base.initial_balance,
            ZERO_HASH,
            ZERO_HASH,
            0,
            0,
            STATUS_ACTIVE,
            0,
            0,
            Cell {
                candidate: base.candidate,
                policy: base.policy,
                operator: base.operator,
                channel: base.channel,
                balance: base.initial_balance,
                status: STATUS_ACTIVE,
                receipt: ZERO_HASH,
                nonce: 0,
                expiry: base.expiry,
            },
        ),
        OP_SETTLE => {
            let old = old.context("Fiber settle material requires an old cell")?;
            let balance = if replay { old.balance } else { base.settled_balance };
            (
                old.balance,
                balance,
                base.route,
                base.payment,
                base.amount,
                STATUS_ACTIVE,
                STATUS_SETTLED,
                old.nonce,
                old.nonce + 1,
                Cell {
                    candidate: old.candidate,
                    policy: old.policy,
                    operator: old.operator,
                    channel: old.channel,
                    balance,
                    status: STATUS_SETTLED,
                    receipt: ZERO_HASH,
                    nonce: old.nonce + 1,
                    expiry: old.expiry,
                },
            )
        }
        _ => bail!("unknown Fiber op {op}"),
    };
    let old_commitment = old.map(|value| ckb_hash(&pack_state(value))).unwrap_or(ZERO_HASH);
    let new_commitment = ckb_hash(&pack_state(&next));
    let core = pack_core(op, base, &route, &payment, &old_balance, &new_balance, amount, old_status, new_status, old_nonce, new_nonce);
    let core_hash = ckb_hash(&core);
    let receipt_hash = if op == OP_SETTLE {
        ckb_hash(&pack_receipt(base, &old_balance, &new_balance, old_nonce, new_nonce, &core_hash, None, None))
    } else {
        ZERO_HASH
    };
    if op == OP_SETTLE {
        next.receipt = receipt_hash;
    }
    let canonical = canonical(op, base, &old_commitment, &new_commitment, old_nonce, new_nonce, &core_hash);
    let mut signed_intent = core;
    append(&mut signed_intent, &[&canonical, &receipt_hash]);
    let signed_hash = ckb_hash(&signed_intent);
    let receipt_data = if op == OP_SETTLE {
        pack_receipt(base, &old_balance, &new_balance, old_nonce, new_nonce, &core_hash, Some(&signed_hash), Some(&receipt_hash))
    } else {
        Vec::new()
    };
    let settlement = if op == OP_SETTLE { ckb_hash(&settlement(base, &old_balance, &new_balance)) } else { ZERO_HASH };
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
        settlement,
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
    let total = funding["total_capacity"].as_u64().context("Fiber initialize funding total is missing")?;
    let change = total.checked_sub(STATE_CAPACITY).context("Fiber initialize funding capacity is too small")?;
    if change == 0 {
        bail!("Fiber initialize funding capacity is too small");
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

fn build_settle(
    old_ref: &Value,
    funding: &Value,
    lifecycle_hash: &str,
    deps: Vec<Value>,
    header: &str,
    material: &Material,
) -> Result<Value> {
    let total = funding["total_capacity"].as_u64().context("Fiber settle funding total is missing")?;
    let change = total.checked_sub(RECEIPT_CAPACITY).context("Fiber settle funding capacity is too small")?;
    if change == 0 {
        bail!("Fiber settle funding capacity is too small");
    }
    let mut inputs = vec![old_ref.clone()];
    inputs.extend_from_slice(funding_cells(funding));
    let mut witnesses = vec![witness(OP_SETTLE, material)];
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
        .unwrap_or_else(|| root.join(format!("target/novaseal-fiber-candidate-devnet-stateful-live/{timestamp}")));
    fs::create_dir_all(&run_dir)?;
    let run_dir = fs::canonicalize(run_dir)?;
    let lifecycle_path = run_dir.join("nova-fiber-candidate-lifecycle-type.elf");
    compile_contract(&root, contract, &lifecycle_path)?;
    let verifier_path = root.join("proposals/novaseal/v0-mvp-skeleton/target/novaseal-btc-verifier-riscv-shell-release.elf");
    if !verifier_path.is_file() {
        bail!("missing verifier ELF: {}", verifier_path.display());
    }
    let mut devnet = CkbDevnet::new(ckb_repo.clone(), ckb_bin.clone(), run_dir.clone())?;
    let mut report = contract_report_header(contract, "fiber_candidate_initialize_then_settle", &root, &ckb_repo, &ckb_bin, &run_dir);
    report["fiber_execution_scope"] =
        json!("live CKB stateful settlement path; real Fiber node/channel execution remains a later external experiment");
    let mut stage = "initializing";
    let scenario = (|| -> Result<()> {
        stage = "start devnet";
        devnet.start()?;
        stage = "deploy artifacts";
        let genesis = devnet.get_block_by_number(0)?;
        let always = always_success_dep(genesis["transactions"][0]["hash"].as_str().context("genesis hash is missing")?);
        let verifier = deploy_code(&mut devnet, "cellscript_btc_bip340_verifier_riscv", &fs::read(&verifier_path)?, &always)?;
        let lifecycle = deploy_code(&mut devnet, "nova_fiber_candidate_lifecycle_type", &fs::read(&lifecycle_path)?, &always)?;
        let lifecycle_hash = lifecycle["data_hash"].as_str().context("lifecycle hash is missing")?.to_owned();
        let deps = vec![verifier["cell_dep"].clone(), lifecycle["cell_dep"].clone(), always];
        let source_paths = [
            "proposals/novaseal/fiber-candidate-profile-v0/Cell.toml",
            "proposals/novaseal/fiber-candidate-profile-v0/src",
            "proposals/novaseal/fiber-candidate-profile-v0/schemas",
            "proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier",
            "crates/cellscript-tools/src/novaseal_planned_fiber.rs",
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
        let initialize = material(OP_INITIALIZE, &base, None, false, false)?;
        let header = devnet.rpc("get_tip_header", vec![])?;
        let funding = devnet.collect_spendable(STATE_CAPACITY + 100 * SHANNONS)?;
        let tx = build_initialize(&funding, &lifecycle_hash, deps.clone(), header["hash"].as_str().unwrap(), &initialize)?;
        let initialize_dry = devnet.rpc("dry_run_transaction", vec![tx.clone()])?;
        let initialize_commit = devnet.submit_and_commit(&tx, "Fiber candidate initialize")?;
        let initialize_hash = initialize_commit["tx_hash"].as_str().unwrap();
        let initial_live = devnet.assert_live_cell(
            initialize_hash,
            0,
            "Fiber active candidate",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&initialize.new_cell_data),
        )?;
        let initial_ref = json!({"tx_hash": initialize_hash, "index": 0, "capacity": STATE_CAPACITY});

        stage = "negative wrong operator signature";
        let negative_header = devnet.rpc("get_tip_header", vec![])?;
        let wrong = material(OP_SETTLE, &base, Some(&initialize.new_cell), true, false)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx =
            build_settle(&initial_ref, &funding, &lifecycle_hash, deps.clone(), negative_header["hash"].as_str().unwrap(), &wrong)?;
        let wrong_reject =
            devnet.dry_run_rejects(&tx, "Fiber wrong operator signature", Some("Inputs[0].Type"), Some(&lifecycle_hash), Some(56))?;

        stage = "negative balance replay";
        let replay = material(OP_SETTLE, &base, Some(&initialize.new_cell), false, true)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx =
            build_settle(&initial_ref, &funding, &lifecycle_hash, deps.clone(), negative_header["hash"].as_str().unwrap(), &replay)?;
        let replay_reject =
            devnet.dry_run_rejects(&tx, "Fiber balance commitment replay", Some("Inputs[0].Type"), Some(&lifecycle_hash), Some(5))?;
        let post_negative = devnet.assert_live_cell(
            initialize_hash,
            0,
            "post-negative Fiber active candidate",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&initialize.new_cell_data),
        )?;

        stage = "valid settle";
        let header = devnet.rpc("get_tip_header", vec![])?;
        let settle = material(OP_SETTLE, &base, Some(&initialize.new_cell), false, false)?;
        let funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)?;
        let tx = build_settle(&initial_ref, &funding, &lifecycle_hash, deps, header["hash"].as_str().unwrap(), &settle)?;
        let settle_dry = devnet.rpc("dry_run_transaction", vec![tx.clone()])?;
        let commit = devnet.submit_and_commit(&tx, "Fiber candidate settlement")?;
        let old_dead = devnet.wait_dead_cell(initialize_hash, 0)?;
        let commit_hash = commit["tx_hash"].as_str().unwrap();
        let settled_live = devnet.assert_live_cell(
            commit_hash,
            0,
            "Fiber settled candidate",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&settle.new_cell_data),
        )?;
        let receipt_live = devnet.assert_live_cell(
            commit_hash,
            1,
            "Fiber settlement receipt",
            Some(RECEIPT_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&Value::Null),
            Some(&settle.receipt_data),
        )?;
        report.as_object_mut().unwrap().extend(
            json!({
                "status": "passed", "live_devnet_rpc_executed": true, "stateful_lifecycle_executed": true,
                "ckb_log": devnet.log_path.display().to_string(), "rpc_url": devnet.rpc_url,
                "artifacts": {"verifier": verifier, "lifecycle": lifecycle}, "provenance": source_provenance,
                "initialize": {"dry_run_cycles": initialize_dry["cycles"], "commit": initialize_commit,
                    "candidate_live": initial_live["status"] == "live", "candidate_data_hash": hex0x(&ckb_hash(&initialize.new_cell_data))},
                "settle_fiber_candidate": {"dry_run_cycles": settle_dry["cycles"], "commit": commit,
                    "old_candidate_not_live": old_dead["status"] != "live", "new_candidate_live": settled_live["status"] == "live",
                    "receipt_live": receipt_live["status"] == "live", "balance_commitment_progressed": settle.new_cell.balance != initialize.new_cell.balance,
                    "fiber_execution_executed": true,
                    "fiber_execution_scope": "profile-level live CKB settlement path; external Fiber node experiment is still separate",
                    "settlement_commitment_hash": hex0x(&settle.settlement), "signed_intent_hash": hex0x(&settle.signed_hash),
                    "receipt_hash": hex0x(&settle.receipt_hash)},
                "negative_cases": {"wrong_operator_signature_dry_run": wrong_reject,
                    "balance_commitment_replay_dry_run": replay_reject, "post_negative_state_still_live": post_negative["status"] == "live"},
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
    fn settlement_material_is_deterministic() {
        let base = base("parity").unwrap();
        let initial = material(OP_INITIALIZE, &base, None, false, false).unwrap();
        let settled = material(OP_SETTLE, &base, Some(&initial.new_cell), false, false).unwrap();
        assert_eq!(hex0x(&ckb_hash(&initial.new_cell_data)), "0xc5c1cb82e0d3ab0f573925695adf1306bf8cfcd94cfec0fbd9f71a826342b039");
        assert_eq!(hex0x(&ckb_hash(&settled.new_cell_data)), "0xc7171e970e8243289832031bd4318c61b55da960f66bdb7a6eceaa026834ec44");
        assert_eq!(hex0x(&settled.signed_hash), "0x0a988c16445df31a8b389cbd9f3a81f7c0d8e50ef0a23c39da85f72b8970aa35");
        assert_eq!(hex0x(&settled.receipt_hash), "0xd0b2f179086571d61c3fca3a048b76faf46c97feeb88c58069d5d6068ac1bf88");
        assert_eq!(hex0x(&ckb_hash(&settled.receipt_data)), "0xdac0f576c6d79fb355828819cce8b06af90e1173e38b0e2772bb2a75083555f9");
    }
}
