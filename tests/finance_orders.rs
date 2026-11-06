//! Persisted, genuinely escrowed orders. All economic, custody, token and
//! holder groups execute against the same signed transaction and prior outputs.
#![cfg(all(feature = "cli", not(feature = "wasm")))]

#[path = "support/finance_protocol_bundle.rs"]
mod finance_protocol_bundle;
#[path = "support/finance_ledger.rs"]
mod ledger;
// This owner exercises the order part of the shared application quotation API.
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

const MAKER: u8 = 0x61;
const FIRST_TAKER: u8 = 0x62;
const SECOND_TAKER: u8 = 0x63;
const SOLD_ISSUER: u8 = 0x71;
const REQUESTED_ISSUER: u8 = 0x72;

struct Programs {
    _directory: tempfile::TempDir,
    asset: Program,
    escrow: Program,
    order: Program,
}

impl Programs {
    fn compile() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = directory.path();
        copy_package(&repository.join("examples/finance"), &root.join("finance"));
        copy_package(&repository.join("examples/finance_math"), &root.join("finance_math"));
        let asset = Program::compile(&root.join("finance/asset"), "asset", true);
        let escrow = Program::compile(&root.join("finance/escrow"), "escrow", false);
        let mut deployment = String::from("module finance_deployment\n");
        for (name, hash) in [
            ("asset_code_hash", cellscript_artifact_checker::ckb_blake2b256(asset.code())),
            ("reserve_code_hash", [0; 32]),
            ("lp_code_hash", [0; 32]),
            ("minimum_code_hash", [0; 32]),
            ("network_domain", [0x11; 32]),
            ("escrow_code_hash", cellscript_artifact_checker::ckb_blake2b256(escrow.code())),
            ("allocation_code_hash", [0; 32]),
        ] {
            let literal = hash.iter().map(|byte| format!("\\x{byte:02x}")).collect::<String>();
            deployment.push_str(&format!("public fn {name}() -> Hash {{ return Hash::from_bytes(b\"{literal}\") }}\n"));
        }
        std::fs::write(root.join("finance/deployment/src/lib.cell"), deployment).unwrap();
        let order = Program::compile(&root.join("finance/order"), "order", false);
        // Empty entry payloads retain the canonical generated ABI even when a
        // custody Lock and its asset Type share one transaction witness slot.
        assert_eq!(asset.witness(&[]), escrow.witness(&[]));
        Self { _directory: directory, asset, escrow, order }
    }

    fn all(&self) -> [&Program; 3] {
        [&self.asset, &self.escrow, &self.order]
    }
}

fn asset_data(program: &Program, amount: u128) -> Vec<u8> {
    program.data("AssetData", &[("amount", amount.to_le_bytes().to_vec())])
}

fn token(program: &Program, asset: &packed::Script, owner: u8, amount: u128) -> Cell {
    cell(owner_lock(owner), Some(asset.clone()), asset_data(program, amount))
}

fn amount(ledger: &Ledger, point: &packed::OutPoint) -> u128 {
    u128::from_le_bytes(ledger.live[point].1[..16].try_into().unwrap())
}

fn issue(ledger: &mut Ledger, asset: &Program, issuer: u8, owners: &[u8]) -> (packed::Script, Vec<packed::OutPoint>) {
    issue_amount(ledger, asset, issuer, owners, 10_000, 1)
}

fn issue_amount(
    ledger: &mut Ledger,
    asset: &Program,
    issuer: u8,
    owners: &[u8],
    amount: u128,
    ordinal: u32,
) -> (packed::Script, Vec<packed::OutPoint>) {
    let funding = ledger.seed_funding(issuer, ordinal);
    let script = asset.script(owner_lock(issuer).calc_script_hash().as_slice());
    let mut outputs = owners.iter().map(|owner| token(asset, &script, *owner, amount)).collect::<Vec<_>>();
    outputs.push(cell(owner_lock(issuer), None, vec![]));
    let tx = ledger.build(&[funding], outputs, owners.len(), vec![asset.witness(&[])]);
    let name = if ordinal == 1 {
        format!("issue-real-order-asset-{issuer}")
    } else {
        format!("issue-real-order-asset-{issuer}-amount-{amount}")
    };
    let points = ledger.commit(&tx, &name);
    (script, points[..owners.len()].to_vec())
}

#[derive(Clone)]
struct Order {
    points: Vec<packed::OutPoint>,
    identity: packed::Script,
    controller: packed::Script,
    sold_asset: packed::Script,
    requested_asset: packed::Script,
    numerator: u128,
    denominator: u128,
    total: u128,
    filled: u128,
    entitlement: u128,
    revision: u64,
    nonce: u64,
}

impl Order {
    fn quote_state(&self) -> quote::OrderState {
        quote::OrderState {
            total: self.total,
            numerator: self.numerator,
            denominator: self.denominator,
            filled: self.filled,
            entitlement: self.entitlement,
        }
    }
}

fn order_data(program: &Program, order: &Order, filled: u128, entitlement: u128, revision: u64) -> Vec<u8> {
    let maker = owner_lock(MAKER).calc_script_hash().as_slice().to_vec();
    let data = program.data(
        "OrderState",
        &[
            ("order", order.identity.calc_script_hash().as_slice().to_vec()),
            ("sold_asset", order.sold_asset.calc_script_hash().as_slice().to_vec()),
            ("requested_asset", order.requested_asset.calc_script_hash().as_slice().to_vec()),
            ("maker", maker.clone()),
            ("proceeds", maker.clone()),
            ("remainder", maker),
            ("network", vec![0x11; 32]),
            ("numerator", order.numerator.to_le_bytes().to_vec()),
            ("denominator", order.denominator.to_le_bytes().to_vec()),
            ("total_sold", order.total.to_le_bytes().to_vec()),
            ("nonce", order.nonce.to_le_bytes().to_vec()),
            ("filled", filled.to_le_bytes().to_vec()),
            ("entitlement", entitlement.to_le_bytes().to_vec()),
            ("revision", revision.to_le_bytes().to_vec()),
        ],
    );
    assert_eq!(data.len(), 320);
    data
}

fn escrow(program: &Program, order: &Order, amount: u128) -> Cell {
    let data = program.data(
        "EscrowData",
        &[
            ("amount", amount.to_le_bytes().to_vec()),
            ("order", order.identity.calc_script_hash().as_slice().to_vec()),
            ("controller", order.controller.calc_script_hash().as_slice().to_vec()),
            ("asset", order.sold_asset.calc_script_hash().as_slice().to_vec()),
        ],
    );
    assert_eq!(data.len(), 112);
    let lock = program.script(&cellscript_artifact_checker::ckb_blake2b256(&data[16..]));
    cell(lock, Some(order.sold_asset.clone()), data)
}

fn order_witness(program: &Program, mode: u64, input_base: u64, sold: u128, paid: u128, taker: u8) -> packed::Bytes {
    program.witness(&[
        Arg::U64(mode),
        Arg::U64(input_base),
        Arg::U64(0),
        Arg::U128(sold),
        Arg::U128(paid),
        Arg::Hash(owner_lock(taker).calc_script_hash().unpack()),
        Arg::Hash(owner_lock(REQUESTED_ISSUER).calc_script_hash().unpack()),
    ])
}

fn checked_commit(programs: &Programs, ledger: &mut Ledger, tx: &TransactionView, name: &str) -> Vec<packed::OutPoint> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/finance-protocol-evidence/order-construction")
        .join(hex::encode(tx.hash().as_slice()));
    let construction = finance_protocol_bundle::check_and_materialize(&directory, &programs.all(), ledger, tx)
        .unwrap_or_else(|error| panic!("{name} public ProtocolBundle construction: {error}"));
    let points = ledger.commit(tx, name);
    ledger.records.last_mut().unwrap()["construction"] = construction;
    points
}

fn initialise(
    programs: &Programs,
    ledger: &mut Ledger,
    maker_wallet: &mut packed::OutPoint,
    assets: [&packed::Script; 2],
    total: u128,
    numerator: u128,
    denominator: u128,
    ordinal: u32,
) -> Order {
    let funding = ledger.seed_funding(MAKER, ordinal);
    let bootstrap_funding = ledger.seed_funding(MAKER, ordinal + 1);
    let first = packed::CellInput::new_builder().previous_output(funding.clone()).build();
    let anchor = cellscript_artifact_checker::ckb_blake2b256(&[first.as_slice(), &0u64.to_le_bytes()].concat());
    let identity = packed::Script::new_builder()
        .code_hash(cellscript::ckb_abi::type_id::CODE_HASH)
        .hash_type(1u8)
        .args(Bytes::copy_from_slice(&anchor).pack())
        .build();
    let mut order = Order {
        points: vec![],
        identity,
        controller: programs.order.script(&[]),
        sold_asset: assets[0].clone(),
        requested_asset: assets[1].clone(),
        numerator,
        denominator,
        total,
        filled: 0,
        entitlement: 0,
        revision: 0,
        nonce: ordinal as u64,
    };
    let state = order_data(&programs.order, &order, 0, 0, 0);
    order.controller = programs.order.script(&cellscript_artifact_checker::ckb_blake2b256(&state[..280]));
    let bootstrap = ledger.build(
        &[bootstrap_funding],
        vec![cell(order.controller.clone(), None, state.clone()), cell(owner_lock(MAKER), None, vec![])],
        1,
        vec![],
    );
    let bootstrap = ledger.commit(&bootstrap, &format!("create-order-bootstrap-{ordinal}"))[0].clone();
    let tx = ledger.build(
        &[funding, bootstrap, maker_wallet.clone()],
        vec![
            cell(order.controller.clone(), Some(order.identity.clone()), state),
            escrow(&programs.escrow, &order, total),
            token(&programs.asset, &order.sold_asset, MAKER, amount(ledger, maker_wallet) - total),
            cell(owner_lock(MAKER), None, vec![]),
        ],
        3,
        vec![Bytes::new().pack(), order_witness(&programs.order, 0, 1, 0, 0, MAKER), programs.asset.witness(&[])],
    );
    let points = checked_commit(programs, ledger, &tx, &format!("initialise-authentic-order-{ordinal}"));
    order.points = points[..2].to_vec();
    *maker_wallet = points[2].clone();
    assert_eq!(amount(ledger, &order.points[1]), total);
    order
}

#[derive(Clone, Copy)]
struct Fill {
    sold: u128,
    paid: u128,
    entitlement: u128,
    terminal: bool,
}

fn fill_transaction(
    programs: &Programs,
    ledger: &mut Ledger,
    order: &Order,
    wallet: &packed::OutPoint,
    taker: u8,
    fill: Fill,
    ordinal: u32,
) -> TransactionView {
    let funding = ledger.seed_funding(taker, ordinal);
    let mut inputs = order.points.clone();
    inputs.extend([funding, wallet.clone()]);
    let available = amount(ledger, wallet);
    let filled = order.filled + fill.sold;
    let mut outputs = Vec::new();
    if !fill.terminal {
        outputs.extend([
            cell(
                order.controller.clone(),
                Some(order.identity.clone()),
                order_data(&programs.order, order, filled, fill.entitlement, order.revision + 1),
            ),
            escrow(&programs.escrow, order, order.total - filled),
        ]);
    }
    outputs.extend([
        token(&programs.asset, &order.sold_asset, taker, fill.sold),
        token(&programs.asset, &order.requested_asset, MAKER, fill.paid),
        token(&programs.asset, &order.requested_asset, taker, available - fill.paid),
    ]);
    if fill.terminal {
        let capacity = order.points.iter().map(|point| u64::from(ledger.live[point].0.capacity())).sum::<u64>();
        let (refund, data) = cell(owner_lock(MAKER), None, vec![]);
        outputs.push((refund.as_builder().capacity(capacity).build(), data));
    }
    let change = outputs.len();
    outputs.push(cell(owner_lock(taker), None, vec![]));
    ledger.build(
        &inputs,
        outputs,
        change,
        vec![
            order_witness(&programs.order, if fill.terminal { 2 } else { 1 }, 0, fill.sold, fill.paid, taker),
            programs.asset.witness(&[]),
            Bytes::new().pack(),
            programs.asset.witness(&[]),
        ],
    )
}

fn change_payment(programs: &Programs, tx: &TransactionView, fill: Fill, taker: u8, available: u128, paid: u128) -> TransactionView {
    let payment_index = if fill.terminal { 1 } else { 3 };
    let refund_index = payment_index + 1;
    let mut data = tx.outputs_data().into_iter().collect::<Vec<_>>();
    data[payment_index] = Bytes::from(asset_data(&programs.asset, paid)).pack();
    data[refund_index] = Bytes::from(asset_data(&programs.asset, available - paid)).pack();
    let mut witnesses = tx.witnesses().into_iter().collect::<Vec<_>>();
    witnesses[0] = order_witness(&programs.order, if fill.terminal { 2 } else { 1 }, 0, fill.sold, paid, taker);
    tx.as_advanced_builder().set_outputs_data(data).set_witnesses(witnesses).build()
}

fn accept_fill(
    programs: &Programs,
    ledger: &mut Ledger,
    order: &mut Order,
    wallet: &mut packed::OutPoint,
    tx: &TransactionView,
    fill: Fill,
    name: &str,
) {
    let points = checked_commit(programs, ledger, tx, name);
    order.filled += fill.sold;
    order.entitlement = fill.entitlement;
    order.revision += 1;
    *wallet = points[if fill.terminal { 2 } else { 4 }].clone();
    if fill.terminal {
        order.points.clear();
        assert_eq!(order.filled, order.total);
    } else {
        order.points = points[..2].to_vec();
        assert_eq!(
            ledger.live[&points[0]].1.as_ref(),
            order_data(&programs.order, order, order.filled, order.entitlement, order.revision)
        );
        assert_eq!(amount(ledger, &points[1]), order.total - order.filled);
    }
}

fn unsolicited_escrow(
    programs: &Programs,
    ledger: &mut Ledger,
    order: &Order,
    maker_wallet: &mut packed::OutPoint,
) -> packed::OutPoint {
    let funding = ledger.seed_funding(MAKER, 150);
    let remaining = order.total - order.filled;
    let tx = ledger.build(
        &[maker_wallet.clone(), funding],
        vec![
            escrow(&programs.escrow, order, remaining),
            token(&programs.asset, &order.sold_asset, MAKER, amount(ledger, maker_wallet) - remaining),
            cell(owner_lock(MAKER), None, vec![]),
        ],
        2,
        vec![programs.asset.witness(&[])],
    );
    let points = ledger.commit(&tx, "unsolicited-matching-escrow-is-not-authentic-inventory");
    *maker_wallet = points[1].clone();
    points[0].clone()
}

fn cancel_transaction(programs: &Programs, ledger: &mut Ledger, order: &Order, funding_owner: u8, ordinal: u32) -> TransactionView {
    let funding = ledger.seed_funding(funding_owner, ordinal);
    let mut inputs = order.points.clone();
    inputs.push(funding);
    ledger.build(
        &inputs,
        vec![token(&programs.asset, &order.sold_asset, MAKER, order.total - order.filled), cell(owner_lock(MAKER), None, vec![])],
        1,
        vec![order_witness(&programs.order, 3, 0, 0, 0, MAKER), programs.asset.witness(&[]), Bytes::new().pack()],
    )
}

fn independent_entitlements(numerator: u128, denominator: u128, sold: &[u128]) -> (Vec<u128>, Value) {
    let cases = sold
        .iter()
        .enumerate()
        .map(|(index, value)| {
            json!({
                "id":format!("entitlement-{index}"),"kind":"quotient", "a":value.to_string(),
                "b":numerator.to_string(),"d":denominator.to_string(),
            })
        })
        .collect::<Vec<_>>();
    let request = json!({"schema":"cellscript-finance-math-model-v1","cases":cases});
    let mut child = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/finance_math_model.mjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&serde_json::to_vec(&request).unwrap()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "independent BigInt model: {}", String::from_utf8_lossy(&output.stderr));
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = response["results"].as_array().unwrap();
    assert_eq!(results.len(), sold.len());
    let values = results.iter().map(|row| row["ceil"].as_str().unwrap().parse().unwrap()).collect();
    (values, json!({"request":request,"response":response}))
}

#[test]
fn escrowed_orders_preserve_cumulative_entitlement_and_authority() {
    let programs = Programs::compile();
    let mut ledger = Ledger::new(&programs.all(), &[MAKER, FIRST_TAKER, SECOND_TAKER, SOLD_ISSUER, REQUESTED_ISSUER]);
    let (sold_asset, maker_wallets) = issue(&mut ledger, &programs.asset, SOLD_ISSUER, &[MAKER]);
    let (requested_asset, requested_wallets) = issue(&mut ledger, &programs.asset, REQUESTED_ISSUER, &[FIRST_TAKER, SECOND_TAKER]);
    let mut maker_wallet = maker_wallets[0].clone();
    let mut first_wallet = requested_wallets[0].clone();
    let mut second_wallet = requested_wallets[1].clone();
    let (entitlements, primary_model) = independent_entitlements(7, 3, &[0, 2, 5, 10]);
    assert_eq!(entitlements, vec![0, 5, 12, 24]);
    let mut order = initialise(&programs, &mut ledger, &mut maker_wallet, [&sold_asset, &requested_asset], 10, 7, 3, 100);
    let first = Fill { sold: 2, paid: 7, entitlement: entitlements[1], terminal: false };
    let quoted = quote::fill_order(order.quote_state(), first.sold, first.paid).unwrap();
    assert_eq!((quoted.payment_due, quoted.donation), (5, 2));
    let tx = fill_transaction(&programs, &mut ledger, &order, &first_wallet, FIRST_TAKER, first, 300);
    ledger.accept_candidate(&tx, "authentic-partial-fill-before-single-field-hostile-variants");

    let underpaid = change_payment(&programs, &tx, first, FIRST_TAKER, amount(&ledger, &first_wallet), quoted.payment_due - 1);
    ledger.reject(&underpaid, &["Inputs[0].Lock"], "maker-underpayment-with-valid-taker-authorisation");

    let zero_payment = change_payment(&programs, &tx, first, FIRST_TAKER, amount(&ledger, &first_wallet), 0);
    ledger.reject(&zero_payment, &["Inputs[0].Lock"], "positive-sold-fill-cannot-pay-zero");
    assert!(ledger.records.last().unwrap()["error"].as_str().unwrap().contains("error code 5"));
    let mut zero_sold = tx.witnesses().into_iter().collect::<Vec<_>>();
    zero_sold[0] = order_witness(&programs.order, 1, 0, 0, first.paid, FIRST_TAKER);
    ledger.reject(
        &tx.as_advanced_builder().set_witnesses(zero_sold).build(),
        &["Inputs[0].Lock"],
        "positive-payment-cannot-enable-zero-sold-fill",
    );
    assert!(ledger.records.last().unwrap()["error"].as_str().unwrap().contains("error code 5"));

    for (field, filled, entitlement, revision) in [
        ("cumulative-entitlement", first.sold, first.entitlement - 1, 1),
        ("filled-progress", 0, first.entitlement, 1),
        ("successor-revision", first.sold, first.entitlement, 2),
    ] {
        let mut data = tx.outputs_data().into_iter().collect::<Vec<_>>();
        data[0] = Bytes::from(order_data(&programs.order, &order, filled, entitlement, revision)).pack();
        ledger.reject(
            &tx.as_advanced_builder().set_outputs_data(data).build(),
            &["Inputs[0].Lock"],
            &format!("partial-fill-cannot-forge-{field}"),
        );
        assert!(ledger.records.last().unwrap()["error"].as_str().unwrap().contains("error code 5"));
    }

    // Both Types and holder signatures remain authentic: return all B to the
    // taker and substitute separately funded genuine A for the maker payment.
    let mut foreign_inputs = tx.inputs().into_iter().map(|input| input.previous_output()).collect::<Vec<_>>();
    foreign_inputs.push(maker_wallet.clone());
    let mut foreign_outputs =
        tx.outputs().into_iter().zip(tx.outputs_data()).map(|(output, data)| (output, data.raw_data())).collect::<Vec<_>>();
    foreign_outputs[3] = token(&programs.asset, &sold_asset, MAKER, first.paid);
    foreign_outputs.push(token(&programs.asset, &sold_asset, MAKER, amount(&ledger, &maker_wallet) - first.paid));
    foreign_outputs.push(token(&programs.asset, &requested_asset, SECOND_TAKER, first.paid));
    let foreign = ledger.build(&foreign_inputs, foreign_outputs, 5, tx.witnesses().into_iter().collect());
    ledger.reject(&foreign, &["Inputs[0].Lock"], "genuine-sold-asset-cannot-replace-requested-maker-payment");
    assert!(ledger.records.last().unwrap()["error"].as_str().unwrap().contains("error code 17 "));

    let mut outputs = tx.outputs().into_iter().collect::<Vec<_>>();
    outputs[3] = outputs[3].clone().as_builder().lock(owner_lock(SECOND_TAKER)).build();
    ledger.reject(&tx.as_advanced_builder().set_outputs(outputs).build(), &["Inputs[0].Lock"], "wrong-maker-proceeds-recipient");

    let mut changed = order.clone();
    changed.numerator += 1;
    let mut data = tx.outputs_data().into_iter().collect::<Vec<_>>();
    data[0] = Bytes::from(order_data(&programs.order, &changed, first.sold, first.entitlement, 1)).pack();
    ledger.reject(&tx.as_advanced_builder().set_outputs_data(data).build(), &["Inputs[0].Lock"], "changed-immutable-order-price");

    let mut witnesses = tx.witnesses().into_iter().collect::<Vec<_>>();
    witnesses[0] = order_witness(&programs.order, 1, 0, order.total + 1, first.paid, FIRST_TAKER);
    ledger.reject(&tx.as_advanced_builder().set_witnesses(witnesses).build(), &["Inputs[0].Lock"], "fill-exceeds-authentic-escrow");

    let forged = unsolicited_escrow(&programs, &mut ledger, &order, &mut maker_wallet);
    assert_eq!(ledger.live[&forged].1, ledger.live[&order.points[1]].1);
    let mut inputs = tx.inputs().into_iter().collect::<Vec<_>>();
    inputs[1] = packed::CellInput::new_builder().previous_output(forged).build();
    ledger.reject(
        &tx.as_advanced_builder().set_inputs(inputs).build(),
        &["Inputs[0].Lock"],
        "same-amount-unsolicited-escrow-substitution",
    );

    accept_fill(&programs, &mut ledger, &mut order, &mut first_wallet, &tx, first, "first-partial-fill-with-donation");
    let stale_directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/finance-protocol-evidence/order-stale-replay");
    let stale = finance_protocol_bundle::check_and_materialize(&stale_directory, &programs.all(), &ledger, &tx).unwrap_err();
    assert!(stale.contains("not locally live"), "{stale}");
    ledger.records.push(json!({"case":"replayed-consumed-order-predecessor","outcome":"rejected",
        "rejection_stage":"local-live-set","error":stale,"transaction":hex::encode(tx.data().as_slice()),
        "transaction_cycles":null,"node_acceptance":null}));

    let second = Fill { sold: 3, paid: 7, entitlement: entitlements[2], terminal: false };
    let quoted_second = quote::fill_order(order.quote_state(), second.sold, second.paid).unwrap();
    assert_eq!(quoted_second.payment_due, 7, "the prior donation is never future credit");
    let tx = fill_transaction(&programs, &mut ledger, &order, &second_wallet, SECOND_TAKER, second, 301);
    accept_fill(&programs, &mut ledger, &mut order, &mut second_wallet, &tx, second, "second-taker-partial-fill");
    let final_fill = Fill { sold: 5, paid: 12, entitlement: entitlements[3], terminal: true };
    let quoted_final = quote::fill_order(order.quote_state(), final_fill.sold, final_fill.paid).unwrap();
    assert_eq!((quoted_final.payment_due, quoted_final.next.entitlement, quoted_final.complete), (12, 24, true));
    let tx = fill_transaction(&programs, &mut ledger, &order, &first_wallet, FIRST_TAKER, final_fill, 302);
    let mut redirected_refund = tx.outputs().into_iter().collect::<Vec<_>>();
    redirected_refund[3] = redirected_refund[3].clone().as_builder().lock(owner_lock(SECOND_TAKER)).build();
    ledger.reject(
        &tx.as_advanced_builder().set_outputs(redirected_refund).build(),
        &["Inputs[0].Lock"],
        "final-fill-cannot-redirect-maker-state-and-escrow-capacity",
    );
    assert!(ledger.records.last().unwrap()["error"].as_str().unwrap().contains("error code 47 "));
    let mut reduced_refund = tx.outputs().into_iter().collect::<Vec<_>>();
    reduced_refund[3] = reduced_refund[3].clone().as_builder().capacity(u64::from(reduced_refund[3].capacity()) - 1).build();
    reduced_refund[4] = reduced_refund[4].clone().as_builder().capacity(u64::from(reduced_refund[4].capacity()) + 1).build();
    ledger.reject(
        &tx.as_advanced_builder().set_outputs(reduced_refund).build(),
        &["Inputs[0].Lock"],
        "final-fill-cannot-short-maker-capacity-refund-by-one-shannon",
    );
    accept_fill(
        &programs,
        &mut ledger,
        &mut order,
        &mut first_wallet,
        &tx,
        final_fill,
        "final-fill-refunds-maker-state-and-escrow-capacity",
    );
    assert_eq!(first.paid + second.paid + final_fill.paid, order.entitlement + 2);

    let (dust_entitlements, dust_model) = independent_entitlements(1, 3, &[0, 1, 2]);
    assert_eq!(dust_entitlements, vec![0, 1, 1]);
    let mut dust = initialise(&programs, &mut ledger, &mut maker_wallet, [&sold_asset, &requested_asset], 2, 1, 3, 200);
    let partial = Fill { sold: 1, paid: 1, entitlement: dust_entitlements[1], terminal: false };
    let quoted_partial = quote::fill_order(dust.quote_state(), partial.sold, partial.paid).unwrap();
    assert_eq!(quoted_partial.payment_due, 1);
    let tx = fill_transaction(&programs, &mut ledger, &dust, &second_wallet, SECOND_TAKER, partial, 303);
    accept_fill(&programs, &mut ledger, &mut dust, &mut second_wallet, &tx, partial, "dust-order-first-partial-fill");
    assert_eq!(quote::fill_order(dust.quote_state(), 1, 1), Err(quote::QuoteError::ZeroEntitlementIncrement));
    let zero_increment = Fill { sold: 1, paid: 1, entitlement: dust_entitlements[2], terminal: true };
    let tx = fill_transaction(&programs, &mut ledger, &dust, &second_wallet, SECOND_TAKER, zero_increment, 304);
    ledger.reject(&tx, &["Inputs[0].Lock"], "positive-donation-cannot-enable-zero-increment-final-fill");
    let unauthorized = cancel_transaction(&programs, &mut ledger, &dust, FIRST_TAKER, 305);
    ledger.reject(&unauthorized, &["Inputs[0].Lock"], "cancellation-requires-actual-maker-authority");
    let cancel = cancel_transaction(&programs, &mut ledger, &dust, MAKER, 306);
    let mut redirected_remainder = cancel.outputs().into_iter().collect::<Vec<_>>();
    redirected_remainder[0] = redirected_remainder[0].clone().as_builder().lock(owner_lock(SECOND_TAKER)).build();
    ledger.reject(
        &cancel.as_advanced_builder().set_outputs(redirected_remainder).build(),
        &["Inputs[0].Lock"],
        "maker-cancel-cannot-redirect-authentic-remainder",
    );
    assert!(ledger.records.last().unwrap()["error"].as_str().unwrap().contains("error code 47 "));
    let mut reduced_escrow_capacity = cancel.outputs().into_iter().collect::<Vec<_>>();
    reduced_escrow_capacity[0] =
        reduced_escrow_capacity[0].clone().as_builder().capacity(u64::from(reduced_escrow_capacity[0].capacity()) - 1).build();
    reduced_escrow_capacity[1] =
        reduced_escrow_capacity[1].clone().as_builder().capacity(u64::from(reduced_escrow_capacity[1].capacity()) + 1).build();
    ledger.reject(
        &cancel.as_advanced_builder().set_outputs(reduced_escrow_capacity).build(),
        &["Inputs[0].Lock"],
        "maker-cancel-cannot-move-escrow-capacity-out-of-remainder-cell",
    );
    let closed = checked_commit(&programs, &mut ledger, &cancel, "maker-cancels-dust-and-recovers-actual-escrow");
    assert_eq!(amount(&ledger, &closed[0]), 1);
    assert_eq!(ledger.live[&closed[0]].0.lock(), owner_lock(MAKER));
    assert!(!ledger.live.contains_key(&dust.points[0]) && !ledger.live.contains_key(&dust.points[1]));

    let mut minimum_order = initialise(&programs, &mut ledger, &mut maker_wallet, [&sold_asset, &requested_asset], 1, 1, 1, 700);
    let (minimum_entitlements, minimum_model) = independent_entitlements(1, 1, &[0, 1]);
    let minimum_fill = Fill { sold: 1, paid: 1, entitlement: minimum_entitlements[1], terminal: true };
    let tx = fill_transaction(&programs, &mut ledger, &minimum_order, &second_wallet, SECOND_TAKER, minimum_fill, 710);
    accept_fill(
        &programs,
        &mut ledger,
        &mut minimum_order,
        &mut second_wallet,
        &tx,
        minimum_fill,
        "minimum-one-unit-order-completes-with-exact-payment-and-refund",
    );

    // The protocol boundary is exercised by real issued inventory and an
    // authentic order; a wide arithmetic microbenchmark cannot establish it.
    let maximum = quote::MAX_AMOUNT;
    let (_, sold_maximum) = issue_amount(&mut ledger, &programs.asset, SOLD_ISSUER, &[MAKER], maximum, 2);
    let (_, requested_maximum) = issue_amount(&mut ledger, &programs.asset, REQUESTED_ISSUER, &[FIRST_TAKER], maximum, 2);
    let mut maximum_maker = sold_maximum[0].clone();
    let mut maximum_taker = requested_maximum[0].clone();
    let (maximum_entitlements, maximum_model) = independent_entitlements(maximum, maximum, &[0, maximum - 1, maximum]);
    let mut maximum_order =
        initialise(&programs, &mut ledger, &mut maximum_maker, [&sold_asset, &requested_asset], maximum, maximum, maximum, 600);
    let partial = Fill { sold: maximum - 1, paid: maximum - 1, entitlement: maximum_entitlements[1], terminal: false };
    let tx = fill_transaction(&programs, &mut ledger, &maximum_order, &maximum_taker, FIRST_TAKER, partial, 610);
    for (name, sold, paid) in [("sold", maximum + 1, partial.paid), ("payment", partial.sold, maximum + 1)] {
        let mut witnesses = tx.witnesses().into_iter().collect::<Vec<_>>();
        witnesses[0] = order_witness(&programs.order, 1, 0, sold, paid, FIRST_TAKER);
        ledger.reject(
            &tx.as_advanced_builder().set_witnesses(witnesses).build(),
            &["Inputs[0].Lock"],
            &format!("order-{name}-above-one-trillion-domain-rejects"),
        );
        assert!(ledger.records.last().unwrap()["error"].as_str().unwrap().contains("error code 5"));
    }
    accept_fill(
        &programs,
        &mut ledger,
        &mut maximum_order,
        &mut maximum_taker,
        &tx,
        partial,
        "maximum-domain-price-product-and-partial-fill",
    );
    let terminal = Fill { sold: 1, paid: 1, entitlement: maximum_entitlements[2], terminal: true };
    let tx = fill_transaction(&programs, &mut ledger, &maximum_order, &maximum_taker, FIRST_TAKER, terminal, 611);
    accept_fill(
        &programs,
        &mut ledger,
        &mut maximum_order,
        &mut maximum_taker,
        &tx,
        terminal,
        "minimum-one-unit-terminal-fill-after-maximum-domain-partial",
    );

    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/finance-protocol-evidence/orders");
    ledger.archive(&directory, &programs.all());
    let reference = json!({"schema":"cellscript-finance-order-model-evidence-v1",
        "model_hash":hex::encode(cellscript::ckb_blake2b256(include_bytes!("support/finance_math_model.mjs"))),
        "primary":primary_model,"dust":dust_model,"minimum":minimum_model,"maximum":maximum_model,
        "maker_total_entitlement":"24","maker_actual_paid":"26","donation_not_future_credit":"2",
        "primary_individual_takers":[{"key":FIRST_TAKER,"sold_received":"7","paid":"19"},
            {"key":SECOND_TAKER,"sold_received":"3","paid":"7"}],
        "dust_individual_taker":{"key":SECOND_TAKER,"sold_received":"1","paid":"1"},
        "node_acceptance":null,"outside_reproduction":null});
    std::fs::write(directory.join("independent-model.json"), serde_json::to_vec_pretty(&reference).unwrap()).unwrap();
    println!("{} retained complete-transaction/local-replay order cases", ledger.records.len());
}
