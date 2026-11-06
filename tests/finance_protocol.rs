//! Authentic application protocol evidence. Every transaction executes its real
//! funding Locks, asset Types, reserve Locks, pool and LP policy together.
#![cfg(all(feature = "cli", not(feature = "wasm")))]

#[path = "support/cost_measurement.rs"]
#[allow(dead_code)]
mod cost_measurement;

#[path = "support/finance_pool_boundaries.rs"]
mod finance_pool_boundaries;
#[path = "support/finance_protocol_bundle.rs"]
mod finance_protocol_bundle;
#[path = "support/finance_route.rs"]
mod finance_route;
#[path = "support/finance_settlement.rs"]
mod finance_settlement;
#[path = "support/finance_ledger.rs"]
mod ledger;
#[path = "../examples/finance_client/quote.rs"]
#[allow(dead_code)]
mod quote;

use cellscript::EntryWitnessArg as Arg;
use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionView, packed, prelude::*};
use ledger::{cell, copy_package, owner_lock, Cell, Ledger, Program};
use serde_json::{json, Value};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

struct Programs {
    _directory: tempfile::TempDir,
    evidence_namespace: &'static str,
    asset: Program,
    reserve: Program,
    lp: Program,
    minimum: Program,
    pool: Program,
}

impl Programs {
    fn compile() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = directory.path();
        copy_package(&repository.join("examples/finance"), &root.join("finance"));
        copy_package(&repository.join("examples/finance_math"), &root.join("finance_math"));
        let asset = Program::compile(&root.join("finance/asset"), "asset", true);
        let reserve = Program::compile(&root.join("finance/reserve"), "reserve", false);
        let lp = Program::compile(&root.join("finance/lp"), "lp", true);
        let minimum = Program::compile(&root.join("finance/minimum"), "minimum", false);
        let mut deployment = String::from("module finance_deployment\n");
        for (name, hash) in [
            ("asset_code_hash", cellscript_artifact_checker::ckb_blake2b256(asset.code())),
            ("reserve_code_hash", cellscript_artifact_checker::ckb_blake2b256(reserve.code())),
            ("lp_code_hash", cellscript_artifact_checker::ckb_blake2b256(lp.code())),
            ("minimum_code_hash", cellscript_artifact_checker::ckb_blake2b256(minimum.code())),
            ("network_domain", [0x11; 32]),
            ("allocation_code_hash", [0; 32]),
        ] {
            let literal = hash.iter().map(|byte| format!("\\x{byte:02x}")).collect::<String>();
            deployment.push_str(&format!("public fn {name}() -> Hash {{ return Hash::from_bytes(b\"{literal}\") }}\n"));
        }
        std::fs::write(root.join("finance/deployment/src/lib.cell"), deployment).unwrap();
        let pool = Program::compile(&root.join("finance/pool"), "pool", false);
        Self { _directory: directory, evidence_namespace: "pool", asset, reserve, lp, minimum, pool }
    }

    fn all(&self) -> [&Program; 5] {
        [&self.asset, &self.reserve, &self.lp, &self.minimum, &self.pool]
    }
}

fn amount(program: &Program, value: u128) -> Vec<u8> {
    program.data("AssetData", &[("amount", value.to_le_bytes().to_vec())])
}

fn capacity_trap(programs: &Programs) -> Program {
    let root = programs._directory.path().join("capacity-trap");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("Cell.toml"),"[package]\ncellscript_version = \">=0.33.0-alpha.2\"\nedition = \"2027\"\nname = \"finance_capacity_trap\"\nversion = \"0.1.0\"\nentry = \"src/main.cell\"\n\n[build]\ntarget = \"riscv64-elf\"\ntarget_profile = \"ckb\"\n").unwrap();
    std::fs::write(root.join("src/main.cell"), include_str!("fixtures/finance_capacity_trap.cell")).unwrap();
    Program::compile(&root, "capacity-trap", true)
}

fn prove_capacity_trap(ledger: &mut Ledger, trap: &Program) {
    let funding = ledger.seed_funding(0x63, 9000);
    let tx = ledger.build(
        &[funding],
        vec![cell(owner_lock(0x63), Some(trap.script(&[])), vec![]), cell(owner_lock(0x63), None, vec![])],
        1,
        vec![trap.witness(&[])],
    );
    let created = ledger.commit(&tx, "capacity-trap-type-accepts-real-output-creation");
    let funding = ledger.seed_funding(0x63, 9001);
    let spend = ledger.build(&[created[0].clone(), funding], vec![cell(owner_lock(0x63), None, vec![])], 0, vec![trap.witness(&[])]);
    ledger.reject(&spend, &["Inputs[0].Type"], "capacity-trap-type-rejects-every-real-spend");
}

fn trap_witness(tx: &TransactionView, trap: &Program, output: usize) -> Vec<packed::Bytes> {
    let mut witnesses = tx.witnesses().into_iter().collect::<Vec<_>>();
    witnesses.resize(witnesses.len().max(output + 1), packed::WitnessArgs::default().as_bytes().pack());
    witnesses[output] = trap.witness(&[]);
    witnesses
}

fn token(program: &Program, script: &packed::Script, owner: u8, value: u128) -> Cell {
    cell(owner_lock(owner), Some(script.clone()), amount(program, value))
}

fn value(ledger: &Ledger, point: &packed::OutPoint) -> u128 {
    u128::from_le_bytes(ledger.live[point].1[..16].try_into().unwrap())
}

fn issue(ledger: &mut Ledger, asset: &Program, issuer: u8) -> (packed::Script, [packed::OutPoint; 2]) {
    let funding = ledger.seed_funding(issuer, 1);
    let script = asset.script(owner_lock(issuer).calc_script_hash().as_slice());
    let tx = ledger.build(
        &[funding],
        vec![token(asset, &script, 0x61, 10_000_000), token(asset, &script, 0x62, 10_000_000), cell(owner_lock(issuer), None, vec![])],
        2,
        vec![asset.witness(&[])],
    );
    let points = ledger.commit(&tx, &format!("issue-enforcing-asset-{issuer}"));
    (script, [points[0].clone(), points[1].clone()])
}

struct Wallet {
    key: u8,
    a: packed::OutPoint,
    b: packed::OutPoint,
    lp: Option<packed::OutPoint>,
}

struct Pool {
    points: Vec<packed::OutPoint>,
    identity: packed::Script,
    controller: packed::Script,
    lp: packed::Script,
    minimum: packed::Script,
    assets: [packed::Script; 2],
    fee: u64,
    nonce: u64,
    revision: u64,
}

fn pool_data(program: &Program, pool: &Pool, a: u128, b: u128, supply: u128, revision: u64) -> Vec<u8> {
    let bytes = program.data(
        "PoolState",
        &[
            ("pool", pool.identity.calc_script_hash().as_slice().to_vec()),
            ("asset_a", pool.assets[0].calc_script_hash().as_slice().to_vec()),
            ("asset_b", pool.assets[1].calc_script_hash().as_slice().to_vec()),
            ("lp", pool.lp.calc_script_hash().as_slice().to_vec()),
            ("minimum_lock", pool.minimum.calc_script_hash().as_slice().to_vec()),
            ("network", vec![0x11; 32]),
            ("fee", pool.fee.to_le_bytes().to_vec()),
            ("nonce", pool.nonce.to_le_bytes().to_vec()),
            ("reserve_a", a.to_le_bytes().to_vec()),
            ("reserve_b", b.to_le_bytes().to_vec()),
            ("total_lp", supply.to_le_bytes().to_vec()),
            ("revision", revision.to_le_bytes().to_vec()),
        ],
    );
    assert_eq!(bytes.len(), 264);
    bytes
}

fn reserve(program: &Program, pool: &Pool, asset: usize, value: u128) -> Cell {
    let bytes = program.data(
        "ReserveData",
        &[
            ("amount", value.to_le_bytes().to_vec()),
            ("pool", pool.identity.calc_script_hash().as_slice().to_vec()),
            ("controller", pool.controller.calc_script_hash().as_slice().to_vec()),
            ("asset", pool.assets[asset].calc_script_hash().as_slice().to_vec()),
        ],
    );
    assert_eq!(bytes.len(), 112);
    let lock = program.script(&cellscript_artifact_checker::ckb_blake2b256(&bytes[16..]));
    cell(lock, Some(pool.assets[asset].clone()), bytes)
}

fn shares(program: &Program, pool: &Pool, lock: packed::Script, value: u128) -> Cell {
    let bytes = program.data(
        "ShareData",
        &[
            ("amount", value.to_le_bytes().to_vec()),
            ("pool", pool.identity.calc_script_hash().as_slice().to_vec()),
            ("pool_code", pool.controller.code_hash().as_slice().to_vec()),
        ],
    );
    assert_eq!(bytes.len(), 80);
    cell(lock, Some(pool.lp.clone()), bytes)
}

fn number(step: &Value, field: &str) -> u128 {
    step[field].as_str().unwrap_or_else(|| panic!("missing model field {field}: {step}")).parse().unwrap()
}

fn quoted_state(ledger: &Ledger, pool: &Pool) -> quote::PoolState {
    let data = &ledger.live[&pool.points[0]].1;
    quote::PoolState {
        reserve_a: u128::from_le_bytes(data[208..224].try_into().unwrap()),
        reserve_b: u128::from_le_bytes(data[224..240].try_into().unwrap()),
        total_lp: u128::from_le_bytes(data[240..256].try_into().unwrap()),
        fee_bps: pool.fee,
    }
}

fn assert_model_state(state: quote::PoolState, model: &Value) {
    assert_eq!(state.reserve_a, number(model, "a"));
    assert_eq!(state.reserve_b, number(model, "b"));
    assert_eq!(state.total_lp, number(model, "supply"));
}

fn model(operations: &[Value]) -> Value {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/finance_model.mjs");
    let mut child =
        Command::new("node").arg(script).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let request = json!({"a":"100000","b":"500000","fee":"30","operations":operations});
    child.stdin.take().unwrap().write_all(&serde_json::to_vec(&request).unwrap()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

fn witness(program: &Program, mode: u64, ib: u64, a: u128, b: u128, shares: u128, min_a: u128, min_b: u128, key: u8) -> packed::Bytes {
    program.witness(&[
        Arg::U64(mode),
        Arg::U64(ib),
        Arg::U64(0),
        Arg::U128(a),
        Arg::U128(b),
        Arg::U128(shares),
        Arg::U128(min_a),
        Arg::U128(min_b),
        Arg::Hash(owner_lock(key).calc_script_hash().unpack()),
    ])
}

fn checked_commit(programs: &Programs, ledger: &mut Ledger, tx: &TransactionView, name: &str) -> Vec<packed::OutPoint> {
    checked_commit_with(programs.evidence_namespace, &programs.all(), ledger, tx, name)
}

fn checked_commit_with(
    namespace: &str,
    programs: &[&Program],
    ledger: &mut Ledger,
    tx: &TransactionView,
    name: &str,
) -> Vec<packed::OutPoint> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/finance-protocol-evidence/construction")
        .join(namespace)
        .join(hex::encode(tx.hash().as_slice()));
    let mut construction = finance_protocol_bundle::check_and_materialize(&directory, programs, ledger, tx)
        .unwrap_or_else(|error| panic!("{name} public ProtocolBundle construction: {error}"));
    let points = ledger.commit(tx, name);
    construction["local_complete_transaction_vm"]["status"] = json!("accepted");
    construction["local_complete_transaction_vm"]["cycles"] = ledger.records.last().unwrap()["transaction_cycles"].clone();
    ledger.records.last_mut().unwrap()["construction"] = construction;
    points
}

fn initialise(
    programs: &Programs,
    ledger: &mut Ledger,
    assets: [packed::Script; 2],
    wallet: &mut Wallet,
    step: &Value,
) -> (Pool, packed::OutPoint) {
    initialise_configured(programs, ledger, assets, wallet, step, 30, 1, 100)
}

fn initialise_configured(
    programs: &Programs,
    ledger: &mut Ledger,
    assets: [packed::Script; 2],
    wallet: &mut Wallet,
    step: &Value,
    fee: u64,
    nonce: u64,
    ordinal: u32,
) -> (Pool, packed::OutPoint) {
    let funding = ledger.seed_funding(wallet.key, ordinal);
    let bootstrap_funding = ledger.seed_funding(wallet.key, ordinal + 1);
    let first = packed::CellInput::new_builder().previous_output(funding.clone()).build();
    let anchor = cellscript_artifact_checker::ckb_blake2b256(&[first.as_slice(), &0u64.to_le_bytes()].concat());
    let identity = packed::Script::new_builder()
        .code_hash(cellscript::ckb_abi::type_id::CODE_HASH)
        .hash_type(1u8)
        .args(Bytes::copy_from_slice(&anchor).pack())
        .build();
    let lp_configuration =
        [identity.calc_script_hash().as_slice(), packed::CellOutput::calc_data_hash(programs.pool.code()).as_slice()].concat();
    let mut pool = Pool {
        points: vec![],
        identity,
        controller: programs.pool.script(&[]),
        lp: programs.lp.script(&cellscript_artifact_checker::ckb_blake2b256(&lp_configuration)),
        minimum: programs.minimum.script(&[]),
        assets,
        fee,
        nonce,
        revision: 0,
    };
    let a = number(step, "a");
    let b = number(step, "b");
    let quotation = quote::initialise(a, b, pool.fee, 1).unwrap();
    assert_model_state(quotation.next, step);
    let supply = quotation.next.total_lp;
    let state = pool_data(&programs.pool, &pool, a, b, supply, 0);
    pool.controller = programs.pool.script(&cellscript_artifact_checker::ckb_blake2b256(&state[..208]));
    let bootstrap = ledger.build(
        &[bootstrap_funding],
        vec![cell(pool.controller.clone(), None, state.clone()), cell(owner_lock(wallet.key), None, vec![])],
        1,
        vec![],
    );
    let bootstrap = ledger.commit(&bootstrap, "create-pool-bootstrap")[0].clone();
    let tx = ledger.build(
        &[funding, bootstrap, wallet.a.clone(), wallet.b.clone()],
        vec![
            cell(pool.controller.clone(), Some(pool.identity.clone()), state),
            reserve(&programs.reserve, &pool, 0, a),
            reserve(&programs.reserve, &pool, 1, b),
            token(&programs.asset, &pool.assets[0], wallet.key, value(ledger, &wallet.a) - a),
            token(&programs.asset, &pool.assets[1], wallet.key, value(ledger, &wallet.b) - b),
            shares(&programs.lp, &pool, owner_lock(wallet.key), supply - 1000),
            shares(&programs.lp, &pool, pool.minimum.clone(), 1000),
            cell(owner_lock(wallet.key), None, vec![]),
        ],
        7,
        vec![
            Bytes::new().pack(),
            witness(&programs.pool, 0, 1, a, b, supply - 1000, 0, 0, wallet.key),
            programs.asset.witness(&[]),
            programs.asset.witness(&[]),
            Bytes::new().pack(),
            programs.lp.witness(&[]),
        ],
    );
    let wrong_funding = ledger.seed_funding(wallet.key, ordinal + 50);
    let mut wrong_inputs = tx.inputs().into_iter().map(|input| input.previous_output()).collect::<Vec<_>>();
    wrong_inputs[0] = wrong_funding;
    let wrong_anchor = ledger.build(
        &wrong_inputs,
        tx.outputs().into_iter().zip(tx.outputs_data()).map(|(output, data)| (output, data.raw_data())).collect(),
        7,
        tx.witnesses().into_iter().collect(),
    );
    ledger.reject(&wrong_anchor, &["Outputs[0].Type"], "pool-creation-rejects-a-different-real-funding-anchor");
    let points = checked_commit(programs, ledger, &tx, "initialise-authentic-pool-and-lp");
    pool.points = points[..3].to_vec();
    wallet.a = points[3].clone();
    wallet.b = points[4].clone();
    wallet.lp = Some(points[5].clone());
    (pool, points[6].clone())
}

fn state_outputs(programs: &Programs, pool: &Pool, step: &Value) -> Vec<Cell> {
    vec![
        cell(
            pool.controller.clone(),
            Some(pool.identity.clone()),
            pool_data(&programs.pool, pool, number(step, "a"), number(step, "b"), number(step, "supply"), pool.revision + 1),
        ),
        reserve(&programs.reserve, pool, 0, number(step, "a")),
        reserve(&programs.reserve, pool, 1, number(step, "b")),
    ]
}

fn transfer_split_merge(programs: &Programs, ledger: &mut Ledger, pool: &Pool, holder: &mut Wallet) {
    let original = holder.lp.clone().unwrap();
    let total = value(ledger, &original);
    let funding = ledger.seed_funding(holder.key, 180);
    let split = ledger.build(
        &[original, funding],
        vec![
            shares(&programs.lp, pool, owner_lock(holder.key), total - 10_000),
            shares(&programs.lp, pool, owner_lock(0x62), 10_000),
            cell(owner_lock(holder.key), None, vec![]),
        ],
        2,
        vec![programs.lp.witness(&[])],
    );
    let split = ledger.commit(&split, "lp-transfer-and-split-without-pool-spend");
    let funding = ledger.seed_funding(holder.key, 181);
    let merge = ledger.build(
        &[split[0].clone(), split[1].clone(), funding],
        vec![
            shares(&programs.lp, pool, owner_lock(holder.key), total),
            cell(owner_lock(0x62), None, vec![]),
            cell(owner_lock(holder.key), None, vec![]),
        ],
        2,
        vec![programs.lp.witness(&[])],
    );
    holder.lp = Some(ledger.commit(&merge, "lp-merge-with-both-real-holder-signatures")[0].clone());
    assert_eq!(value(ledger, holder.lp.as_ref().unwrap()), total);
}

fn deposit(programs: &Programs, ledger: &mut Ledger, pool: &mut Pool, wallet: &mut Wallet, peer_lp: &packed::OutPoint, step: &Value) {
    let quotation = quote::deposit(quoted_state(ledger, pool), 10_000, 70_000, 1).unwrap();
    assert_model_state(quotation.next, step);
    assert_eq!(quotation.minted, number(step, "minted"));
    assert_eq!(quotation.used_a, number(step, "usedA"));
    assert_eq!(quotation.used_b, number(step, "usedB"));
    let funding = ledger.seed_funding(wallet.key, 102);
    let mut inputs = pool.points.clone();
    inputs.extend([funding, wallet.a.clone(), wallet.b.clone()]);
    let mut outputs = state_outputs(programs, pool, step);
    outputs.extend([
        token(&programs.asset, &pool.assets[0], wallet.key, value(ledger, &wallet.a) - quotation.used_a),
        token(&programs.asset, &pool.assets[1], wallet.key, value(ledger, &wallet.b) - quotation.used_b),
        shares(&programs.lp, pool, owner_lock(wallet.key), quotation.minted),
        cell(owner_lock(wallet.key), None, vec![]),
    ]);
    let tx = ledger.build(
        &inputs,
        outputs,
        6,
        vec![
            witness(&programs.pool, 3, 0, 10000, 70000, number(step, "minted"), 0, 0, wallet.key),
            programs.asset.witness(&[]),
            programs.asset.witness(&[]),
        ],
    );
    // A valid holder supplies existing shares while an extra output cancels
    // the mint in group totals. The pool still mints, so LP must check its
    // supply delta even when the complete LP group happens to conserve.
    let mut bad_inputs = inputs;
    bad_inputs.push(peer_lp.clone());
    let mut bad_outputs =
        tx.outputs().into_iter().zip(tx.outputs_data()).map(|(output, data)| (output, data.raw_data())).collect::<Vec<_>>();
    bad_outputs.push(shares(&programs.lp, pool, ledger.live[peer_lp].0.lock(), value(ledger, peer_lp) - quotation.minted));
    let mut bad_witnesses = tx.witnesses().into_iter().collect::<Vec<_>>();
    bad_witnesses[6] = programs.lp.witness(&[]);
    let cancelled_mint = ledger.build(&bad_inputs, bad_outputs, 6, bad_witnesses);
    ledger.reject(&cancelled_mint, &["Inputs[6].Type"], "existing-lp-input-cannot-hide-deposit-supply-increase");
    let points = checked_commit(programs, ledger, &tx, "second-provider-proportional-deposit-and-refunds");
    pool.points = points[..3].to_vec();
    pool.revision += 1;
    wallet.a = points[3].clone();
    wallet.b = points[4].clone();
    wallet.lp = Some(points[5].clone());
}

fn swap_transaction(
    programs: &Programs,
    ledger: &mut Ledger,
    pool: &Pool,
    wallet: &Wallet,
    step: &Value,
    ordinal: u32,
) -> TransactionView {
    let funding = ledger.seed_funding(wallet.key, ordinal);
    let mut inputs = pool.points.clone();
    inputs.extend([funding, wallet.a.clone(), wallet.b.clone()]);
    let mut outputs = state_outputs(programs, pool, step);
    let forward = step["operation"] == "swap_a";
    let input = number(step, "input");
    let quotation = quote::swap_exact_in(
        quoted_state(ledger, pool),
        if forward { quote::Direction::AForB } else { quote::Direction::BForA },
        input,
        number(step, "received"),
    )
    .unwrap();
    assert_model_state(quotation.next, step);
    assert_eq!(quotation.amount_out, number(step, "received"));
    let received = quotation.amount_out;
    outputs.extend([
        token(
            &programs.asset,
            &pool.assets[0],
            wallet.key,
            if forward { value(ledger, &wallet.a) - input } else { value(ledger, &wallet.a) + received },
        ),
        token(
            &programs.asset,
            &pool.assets[1],
            wallet.key,
            if forward { value(ledger, &wallet.b) + received } else { value(ledger, &wallet.b) - input },
        ),
        cell(owner_lock(wallet.key), None, vec![]),
    ]);
    ledger.build(
        &inputs,
        outputs,
        5,
        vec![
            witness(
                &programs.pool,
                if forward { 1 } else { 2 },
                0,
                if forward { input } else { 0 },
                if forward { 0 } else { input },
                0,
                if forward { 0 } else { received },
                if forward { received } else { 0 },
                wallet.key,
            ),
            programs.asset.witness(&[]),
            programs.asset.witness(&[]),
        ],
    )
}

fn unsolicited_reserve(programs: &Programs, ledger: &mut Ledger, pool: &Pool, donor: &mut Wallet) -> packed::OutPoint {
    let funding = ledger.seed_funding(donor.key, 205);
    let recognised = value(ledger, &pool.points[1]);
    let tx = ledger.build(
        &[funding, donor.a.clone()],
        vec![
            ledger.live[&pool.points[1]].clone(),
            token(&programs.asset, &pool.assets[0], donor.key, value(ledger, &donor.a) - recognised),
            cell(owner_lock(donor.key), None, vec![]),
        ],
        2,
        vec![Bytes::new().pack(), programs.asset.witness(&[])],
    );
    let points = ledger.commit(&tx, "genuine-asset-donation-creates-equal-looking-unrecognised-reserve");
    donor.a = points[1].clone();
    assert_eq!(ledger.live[&points[0]], ledger.live[&pool.points[1]]);
    assert_ne!(points[0], pool.points[1]);
    points[0].clone()
}

fn withdraw(programs: &Programs, ledger: &mut Ledger, pool: &mut Pool, wallet: &mut Wallet, step: &Value, ordinal: u32) {
    let funding = ledger.seed_funding(wallet.key, ordinal);
    let lp = wallet.lp.clone().unwrap();
    let burned = number(step, "burned");
    let quotation = quote::withdraw(quoted_state(ledger, pool), burned, number(step, "paidA"), number(step, "paidB")).unwrap();
    assert_model_state(quotation.next, step);
    assert_eq!(quotation.paid_a, number(step, "paidA"));
    assert_eq!(quotation.paid_b, number(step, "paidB"));
    let remaining = value(ledger, &lp) - burned;
    let mut inputs = pool.points.clone();
    inputs.extend([funding, lp]);
    let mut outputs = state_outputs(programs, pool, step);
    outputs.extend([
        token(&programs.asset, &pool.assets[0], wallet.key, quotation.paid_a),
        token(&programs.asset, &pool.assets[1], wallet.key, quotation.paid_b),
    ]);
    if remaining > 0 {
        outputs.push(shares(&programs.lp, pool, owner_lock(wallet.key), remaining));
    }
    let change = outputs.len();
    outputs.push(cell(owner_lock(wallet.key), None, vec![]));
    let tx = ledger.build(
        &inputs,
        outputs,
        change,
        vec![
            witness(&programs.pool, 4, 0, 0, 0, burned, number(step, "paidA"), number(step, "paidB"), wallet.key),
            programs.asset.witness(&[]),
            programs.asset.witness(&[]),
            Bytes::new().pack(),
            programs.lp.witness(&[]),
        ],
    );
    let mut bad_outputs =
        tx.outputs().into_iter().zip(tx.outputs_data()).map(|(output, data)| (output, data.raw_data())).collect::<Vec<_>>();
    bad_outputs.push(shares(&programs.lp, pool, owner_lock(wallet.key), burned));
    let cancelled_burn = ledger.build(&inputs, bad_outputs, change, tx.witnesses().into_iter().collect());
    ledger.reject(&cancelled_burn, &["Inputs[4].Type"], "extra-lp-output-cannot-hide-withdrawal-supply-decrease");
    let points = checked_commit(programs, ledger, &tx, &format!("withdraw-{ordinal}"));
    pool.points = points[..3].to_vec();
    pool.revision += 1;
    wallet.lp = if remaining > 0 { Some(points[5].clone()) } else { None };
}

#[test]
fn formal_bundle_requires_canonical_witnesses_before_signing() {
    assert!(finance_protocol_bundle::witness(3, &[]).unwrap_err().contains("normalize empty WitnessArgs before signing"));
    assert_eq!(finance_protocol_bundle::witness(3, packed::WitnessArgs::default().as_slice()).unwrap(), json!({}));
}

#[test]
fn authentic_pool_lifecycle_and_hostile_raw_transactions() {
    let programs = Programs::compile();
    let mut ledger = Ledger::new(&programs.all(), &[0x61, 0x62, 0x71, 0x72]);
    let (a, wa) = issue(&mut ledger, &programs.asset, 0x71);
    let (b, wb) = issue(&mut ledger, &programs.asset, 0x72);
    assert_ne!(a, b);
    let mut first = Wallet { key: 0x61, a: wa[0].clone(), b: wb[0].clone(), lp: None };
    let mut second = Wallet { key: 0x62, a: wa[1].clone(), b: wb[1].clone(), lp: None };
    let mut operations = vec![
        json!({"kind":"deposit","a":"10000","b":"70000"}),
        json!({"kind":"swap_a","amount":"1234"}),
        json!({"kind":"swap_b","amount":"4321"}),
    ];
    let preliminary = model(&operations);
    let minted = number(&preliminary[1], "minted");
    operations.extend([
        json!({"kind":"withdraw","shares":"10000"}),
        json!({"kind":"withdraw","shares":(minted-10000).to_string()}),
        json!({"kind":"withdraw","shares":"circulating"}),
    ]);
    let trace = model(&operations);
    let (mut pool, locked) = initialise(&programs, &mut ledger, [a, b], &mut first, &trace[0]);
    transfer_split_merge(&programs, &mut ledger, &pool, &mut first);

    let funding = ledger.seed_funding(first.key, 199);
    let locked_spend = ledger.build(
        &[locked.clone(), funding],
        vec![shares(&programs.lp, &pool, owner_lock(first.key), 1000), cell(owner_lock(first.key), None, vec![])],
        1,
        vec![programs.lp.witness(&[])],
    );
    ledger.reject(&locked_spend, &["Inputs[0].Lock"], "locked-minimum-cannot-be-transferred-or-redeemed");

    // Correctly authorised holder destruction must reach the LP Type policy.
    let funding = ledger.seed_funding(first.key, 200);
    let standalone = ledger.build(
        &[first.lp.clone().unwrap(), funding],
        vec![cell(owner_lock(first.key), None, vec![])],
        0,
        vec![programs.lp.witness(&[])],
    );
    ledger.reject(&standalone, &["Inputs[0].Type"], "standalone-lp-destruction-with-valid-holder-signature");

    let funding = ledger.seed_funding(first.key, 201);
    let reserve_only = ledger.build(
        &[pool.points[1].clone(), funding],
        vec![
            token(&programs.asset, &pool.assets[0], first.key, value(&ledger, &pool.points[1])),
            cell(owner_lock(first.key), None, vec![]),
        ],
        1,
        vec![programs.asset.witness(&[])],
    );
    ledger.reject(&reserve_only, &["Inputs[0].Lock"], "reserve-spend-without-pool-state");

    let funding = ledger.seed_funding(first.key, 202);
    let forged = ledger.build(
        &[funding],
        vec![shares(&programs.lp, &pool, owner_lock(first.key), 1), cell(owner_lock(first.key), None, vec![])],
        1,
        vec![programs.lp.witness(&[])],
    );
    ledger.reject(&forged, &["Outputs[0].Type"], "forged-lp-without-authentic-pool-transition");

    deposit(&programs, &mut ledger, &mut pool, &mut second, first.lp.as_ref().unwrap(), &trace[1]);
    let unsolicited = unsolicited_reserve(&programs, &mut ledger, &pool, &mut first);
    for (index, wallet) in [(2, &mut first), (3, &mut second)] {
        let tx = swap_transaction(&programs, &mut ledger, &pool, wallet, &trace[index], 210 + index as u32);
        if index == 2 {
            let mut substituted = tx.inputs().into_iter().collect::<Vec<_>>();
            substituted[1] = substituted[1].clone().as_builder().previous_output(unsolicited.clone()).build();
            let bad = tx.as_advanced_builder().set_inputs(substituted).build();
            ledger.reject(&bad, &["Inputs[0].Lock"], "equal-looking-reserve-cannot-replace-authentic-predecessor-outpoint");
        }
        let points = checked_commit(&programs, &mut ledger, &tx, if index == 2 { "swap-a-for-b" } else { "swap-b-for-a" });
        pool.points = points[..3].to_vec();
        pool.revision += 1;
        wallet.a = points[3].clone();
        wallet.b = points[4].clone();
    }
    withdraw(&programs, &mut ledger, &mut pool, &mut second, &trace[4], 220);
    withdraw(&programs, &mut ledger, &mut pool, &mut second, &trace[5], 221);
    withdraw(&programs, &mut ledger, &mut pool, &mut first, &trace[6], 222);
    assert!(first.lp.is_none() && second.lp.is_none());
    assert_eq!(value(&ledger, &locked), 1000);
    assert_eq!(u128::from_le_bytes(ledger.live[&pool.points[0]].1[240..256].try_into().unwrap()), 1000);
    assert_eq!(value(&ledger, &pool.points[1]), number(&trace[6], "a"));
    assert_eq!(value(&ledger, &pool.points[2]), number(&trace[6], "b"));
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/finance-protocol-evidence/pool");
    ledger.archive(&directory, &programs.all());
    std::fs::write(directory.join("independent-model.json"), serde_json::to_vec_pretty(&trace).unwrap()).unwrap();
    println!("{} complete-transaction VM cases; node and external reproduction remain separate", ledger.records.len());
}
