//! Actual shared-payment order allocation, including rebound hostile intents.
#![cfg(all(feature = "cli", not(feature = "wasm")))]

#[path = "support/cost_measurement.rs"]
#[allow(dead_code)]
mod cost_measurement;
#[path = "support/finance_protocol_bundle.rs"]
mod finance_protocol_bundle;
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

const MAKER: u8 = 0x61;
const BUYER: u8 = 0x62;

struct Programs {
    _directory: tempfile::TempDir,
    asset: Program,
    escrow: Program,
    allocation: Program,
    order: Program,
}

fn configuration(root: &Path, asset: &Program, escrow: &Program, allocation: [u8; 32]) {
    let mut source = String::from("module finance_deployment;\n");
    for (name, hash) in [
        ("asset_code_hash", cellscript::ckb_blake2b256(asset.code())),
        ("reserve_code_hash", [0; 32]),
        ("lp_code_hash", [0; 32]),
        ("minimum_code_hash", [0; 32]),
        ("network_domain", [0x11; 32]),
        ("escrow_code_hash", cellscript::ckb_blake2b256(escrow.code())),
        ("allocation_code_hash", allocation),
    ] {
        let bytes = hash.iter().map(|byte| format!("\\x{byte:02x}")).collect::<String>();
        source.push_str(&format!("public fn {name}() -> Hash {{ return Hash::from_bytes(b\"{bytes}\"); }}\n"));
    }
    std::fs::write(root.join("finance/deployment/src/lib.cell"), source).unwrap();
}

impl Programs {
    fn compile() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
        copy_package(&repository.join("examples/finance"), &directory.path().join("finance"));
        copy_package(&repository.join("examples/finance_math"), &directory.path().join("finance_math"));
        let root = directory.path();
        let asset = Program::compile(&root.join("finance/asset"), "asset", true);
        let escrow = Program::compile(&root.join("finance/escrow"), "escrow", false);
        configuration(root, &asset, &escrow, [0; 32]);
        let preliminary = Program::compile(&root.join("finance/allocation"), "allocation", false);
        configuration(root, &asset, &escrow, cellscript::ckb_blake2b256(preliminary.code()));
        let allocation = Program::compile(&root.join("finance/allocation"), "allocation", false);
        assert_eq!(allocation.code(), preliminary.code(), "unused deployment self-hash must not create an ELF hash cycle");
        let order = Program::compile(&root.join("finance/order"), "order", false);
        assert_eq!(
            hex::encode(owner_lock(BUYER).code_hash().as_slice()),
            "709f3fda12f561cfacf92273c57a98fede188a3f1a59b1f888d113f9cce08649"
        );
        Self { _directory: directory, asset, escrow, allocation, order }
    }
    fn all(&self) -> [&Program; 4] {
        [&self.asset, &self.escrow, &self.allocation, &self.order]
    }
}

fn token(program: &Program, script: &packed::Script, owner: u8, amount: u128) -> Cell {
    cell(owner_lock(owner), Some(script.clone()), program.data("AssetData", &[("amount", amount.to_le_bytes().to_vec())]))
}

fn value(ledger: &Ledger, point: &packed::OutPoint) -> u128 {
    u128::from_le_bytes(ledger.live[point].1[..16].try_into().unwrap())
}

fn issue(programs: &Programs, ledger: &mut Ledger, issuer: u8, owner: u8) -> (packed::Script, packed::OutPoint) {
    let script = programs.asset.script(owner_lock(issuer).calc_script_hash().as_slice());
    let funding = ledger.seed_funding(issuer, 1);
    let tx = ledger.build(
        &[funding],
        vec![token(&programs.asset, &script, owner, 10_000), cell(owner_lock(issuer), None, vec![])],
        1,
        vec![programs.asset.witness(&[])],
    );
    (script, ledger.commit(&tx, &format!("allocation-real-asset-issuance-{issuer}"))[0].clone())
}

struct Order {
    points: Vec<packed::OutPoint>,
    identity: packed::Script,
    controller: packed::Script,
    sold: packed::Script,
    requested: packed::Script,
    nonce: u64,
    filled: u128,
    entitlement: u128,
    revision: u64,
}

fn order_data(programs: &Programs, order: &Order, filled: u128, entitlement: u128) -> Vec<u8> {
    let maker = owner_lock(MAKER).calc_script_hash().as_slice().to_vec();
    programs.order.data(
        "OrderState",
        &[
            ("order", order.identity.calc_script_hash().as_slice().to_vec()),
            ("sold_asset", order.sold.calc_script_hash().as_slice().to_vec()),
            ("requested_asset", order.requested.calc_script_hash().as_slice().to_vec()),
            ("maker", maker.clone()),
            ("proceeds", maker.clone()),
            ("remainder", maker),
            ("network", vec![0x11; 32]),
            ("numerator", 7u128.to_le_bytes().to_vec()),
            ("denominator", 3u128.to_le_bytes().to_vec()),
            ("total_sold", 10u128.to_le_bytes().to_vec()),
            ("nonce", order.nonce.to_le_bytes().to_vec()),
            ("filled", filled.to_le_bytes().to_vec()),
            ("entitlement", entitlement.to_le_bytes().to_vec()),
            ("revision", (order.revision + u64::from(filled != order.filled)).to_le_bytes().to_vec()),
        ],
    )
}

fn escrow(programs: &Programs, order: &Order, amount: u128) -> Cell {
    let data = programs.escrow.data(
        "EscrowData",
        &[
            ("amount", amount.to_le_bytes().to_vec()),
            ("order", order.identity.calc_script_hash().as_slice().to_vec()),
            ("controller", order.controller.calc_script_hash().as_slice().to_vec()),
            ("asset", order.sold.calc_script_hash().as_slice().to_vec()),
        ],
    );
    cell(programs.escrow.script(&cellscript::ckb_blake2b256(&data[16..])), Some(order.sold.clone()), data)
}

fn order_witness(programs: &Programs, mode: u64, ib: u64, ob: u64, sold: u128, paid: u128, actor: &packed::Script) -> packed::Bytes {
    programs.order.witness(&[
        Arg::U64(mode),
        Arg::U64(ib),
        Arg::U64(ob),
        Arg::U128(sold),
        Arg::U128(paid),
        Arg::Hash(actor.calc_script_hash().unpack()),
        Arg::Hash(owner_lock(0x72).calc_script_hash().unpack()),
    ])
}

fn checked_commit(programs: &Programs, ledger: &mut Ledger, tx: &TransactionView, name: &str) -> Vec<packed::OutPoint> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/finance-protocol-evidence/allocation-construction")
        .join(hex::encode(tx.hash().as_slice()));
    let mut construction = finance_protocol_bundle::check_and_materialize(&root, &programs.all(), ledger, tx)
        .unwrap_or_else(|error| panic!("{name}: {error}"));
    let points = ledger.commit(tx, name);
    construction["local_complete_transaction_vm"]["status"] = json!("accepted");
    construction["local_complete_transaction_vm"]["cycles"] = ledger.records.last().unwrap()["transaction_cycles"].clone();
    ledger.records.last_mut().unwrap()["construction"] = construction;
    points
}

fn checked_candidate(programs: &Programs, ledger: &mut Ledger, tx: &TransactionView, name: &str) {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/finance-protocol-evidence/allocation-construction")
        .join(hex::encode(tx.hash().as_slice()));
    let mut construction = finance_protocol_bundle::check_and_materialize(&directory, &programs.all(), ledger, tx)
        .unwrap_or_else(|error| panic!("{name} public ProtocolBundle construction: {error}"));
    let cycles = ledger.accept_candidate(tx, name);
    construction["local_complete_transaction_vm"]["status"] = json!("accepted-not-consumed");
    construction["local_complete_transaction_vm"]["cycles"] = json!(cycles);
    ledger.records.last_mut().unwrap()["construction"] = construction;
}

fn initialise(
    programs: &Programs,
    ledger: &mut Ledger,
    wallet: &mut packed::OutPoint,
    sold: &packed::Script,
    requested: &packed::Script,
    ordinal: u32,
) -> Order {
    let funding = ledger.seed_funding(MAKER, ordinal);
    let bootstrap_funding = ledger.seed_funding(MAKER, ordinal + 1);
    let first = packed::CellInput::new_builder().previous_output(funding.clone()).build();
    let anchor = cellscript::ckb_blake2b256(&[first.as_slice(), &0u64.to_le_bytes()].concat());
    let identity = packed::Script::new_builder()
        .code_hash(cellscript::ckb_abi::type_id::CODE_HASH)
        .hash_type(1u8)
        .args(Bytes::copy_from_slice(&anchor).pack())
        .build();
    let mut order = Order {
        points: vec![],
        identity,
        controller: programs.order.script(&[]),
        sold: sold.clone(),
        requested: requested.clone(),
        nonce: ordinal as u64,
        filled: 0,
        entitlement: 0,
        revision: 0,
    };
    let data = order_data(programs, &order, 0, 0);
    order.controller = programs.order.script(&cellscript::ckb_blake2b256(&data[..280]));
    let bootstrap = ledger.build(
        &[bootstrap_funding],
        vec![cell(order.controller.clone(), None, data.clone()), cell(owner_lock(MAKER), None, vec![])],
        1,
        vec![],
    );
    let bootstrap = ledger.commit(&bootstrap, &format!("allocation-order-bootstrap-{ordinal}"))[0].clone();
    let tx = ledger.build(
        &[funding, bootstrap, wallet.clone()],
        vec![
            cell(order.controller.clone(), Some(order.identity.clone()), data),
            escrow(programs, &order, 10),
            token(&programs.asset, sold, MAKER, value(ledger, wallet) - 10),
            cell(owner_lock(MAKER), None, vec![]),
        ],
        3,
        vec![Bytes::new().pack(), order_witness(programs, 0, 1, 0, 0, 0, &owner_lock(MAKER)), programs.asset.witness(&[])],
    );
    let points = checked_commit(programs, ledger, &tx, &format!("allocation-authentic-order-genesis-{ordinal}"));
    order.points = points[..2].to_vec();
    *wallet = points[2].clone();
    order
}

#[derive(Clone)]
struct Credit {
    order: [u8; 32],
    output: u64,
    amount: u128,
}

#[derive(Clone)]
struct Plan {
    orders: Vec<[u8; 32]>,
    credits: Vec<Credit>,
    auth_input: u64,
    change_output: u64,
}

impl Plan {
    fn data(&self, programs: &Programs) -> Vec<u8> {
        self.data_with_count(programs, self.credits.len() as u64)
    }
    fn data_with_count(&self, programs: &Programs, credit_count: u64) -> Vec<u8> {
        let mut fields = vec![
            ("authorizer".to_owned(), owner_lock(BUYER).calc_script_hash().as_slice().to_vec()),
            ("allowed_order_code".to_owned(), packed::CellOutput::calc_data_hash(programs.order.code()).as_slice().to_vec()),
            ("network".to_owned(), vec![0x11; 32]),
            ("auth_input".to_owned(), self.auth_input.to_le_bytes().to_vec()),
            ("change_output".to_owned(), self.change_output.to_le_bytes().to_vec()),
            ("order_count".to_owned(), (self.orders.len() as u64).to_le_bytes().to_vec()),
            ("credit_count".to_owned(), credit_count.to_le_bytes().to_vec()),
        ];
        for row in 0..2 {
            fields.push((format!("order{row}"), self.orders.get(row).copied().unwrap_or([0; 32]).to_vec()));
        }
        for row in 0..4 {
            let credit = self.credits.get(row);
            fields.push((format!("credit{row}_order"), credit.map_or([0; 32], |credit| credit.order).to_vec()));
            fields.push((format!("credit{row}_output"), credit.map_or(0, |credit| credit.output).to_le_bytes().to_vec()));
            fields.push((format!("credit{row}_amount"), credit.map_or(0, |credit| credit.amount).to_le_bytes().to_vec()));
        }
        let fields = fields.iter().map(|(name, bytes)| (name.as_str(), bytes.clone())).collect::<Vec<_>>();
        let data = programs.allocation.data("AllocationData", &fields);
        assert_eq!(data.len(), 416);
        data
    }
}

struct Intent {
    points: Vec<packed::OutPoint>,
    script: packed::Script,
}

fn fund_intent(
    programs: &Programs,
    ledger: &mut Ledger,
    plan: &Plan,
    wallet: &mut packed::OutPoint,
    requested: &packed::Script,
    ordinal: u32,
) -> Intent {
    fund_intent_data(programs, ledger, plan.data(programs), wallet, requested, ordinal)
}

fn fund_intent_data(
    programs: &Programs,
    ledger: &mut Ledger,
    data: Vec<u8>,
    wallet: &mut packed::OutPoint,
    requested: &packed::Script,
    ordinal: u32,
) -> Intent {
    let script = programs.allocation.script(&cellscript::ckb_blake2b256(&data));
    let funding = ledger.seed_funding(BUYER, ordinal);
    let tx = ledger.build(
        &[wallet.clone(), funding],
        vec![
            cell(script.clone(), None, data),
            cell(script.clone(), None, vec![]),
            cell(script.clone(), None, vec![]),
            cell(
                script.clone(),
                Some(requested.clone()),
                programs.asset.data("AssetData", &[("amount", 50u128.to_le_bytes().to_vec())]),
            ),
            token(&programs.asset, requested, BUYER, value(ledger, wallet) - 50),
            cell(owner_lock(BUYER), None, vec![]),
        ],
        5,
        vec![programs.asset.witness(&[])],
    );
    let points = ledger.commit(&tx, &format!("fund-committed-allocation-intent-{ordinal}"));
    *wallet = points[4].clone();
    Intent { points: points[..4].to_vec(), script }
}

fn settlement(
    programs: &Programs,
    ledger: &mut Ledger,
    intent: &Intent,
    orders: [&Order; 2],
    actual_paid: u128,
    ordinal: u32,
    include_second: bool,
) -> TransactionView {
    settlement_with_payments(programs, ledger, intent, orders, &[actual_paid], ordinal, include_second)
}

fn settlement_with_payments(
    programs: &Programs,
    ledger: &mut Ledger,
    intent: &Intent,
    orders: [&Order; 2],
    payments: &[u128],
    ordinal: u32,
    include_second: bool,
) -> TransactionView {
    let first = quote::fill_order(
        quote::OrderState { total: 10, numerator: 7, denominator: 3, filled: orders[0].filled, entitlement: orders[0].entitlement },
        2,
        5,
    )
    .unwrap();
    let second = quote::fill_order(
        quote::OrderState { total: 10, numerator: 7, denominator: 3, filled: orders[1].filled, entitlement: orders[1].entitlement },
        3,
        7,
    )
    .unwrap();
    let auth = ledger.seed_funding(BUYER, ordinal);
    let mut inputs =
        vec![intent.points[0].clone(), orders[0].points[0].clone(), orders[0].points[1].clone(), intent.points[1].clone()];
    let mut outputs = vec![
        cell(
            orders[0].controller.clone(),
            Some(orders[0].identity.clone()),
            order_data(programs, orders[0], first.next.filled, first.next.entitlement),
        ),
        escrow(programs, orders[0], first.remaining),
    ];
    let mut witnesses = vec![
        Bytes::new().pack(),
        order_witness(programs, 6, 1, 0, 2, 5, &intent.script),
        programs.asset.witness(&[]),
        Bytes::new().pack(),
    ];
    if include_second {
        inputs.extend([orders[1].points[0].clone(), orders[1].points[1].clone(), intent.points[2].clone()]);
        outputs.extend([
            cell(
                orders[1].controller.clone(),
                Some(orders[1].identity.clone()),
                order_data(programs, orders[1], second.next.filled, second.next.entitlement),
            ),
            escrow(programs, orders[1], second.remaining),
        ]);
        witnesses.extend([order_witness(programs, 6, 4, 2, 3, 7, &intent.script), programs.asset.witness(&[]), Bytes::new().pack()]);
    }
    inputs.extend([intent.points[3].clone(), auth]);
    witnesses.extend([programs.asset.witness(&[]), Bytes::new().pack()]);
    let auth_index = inputs.len() as u64 - 1;
    witnesses[0] = programs.allocation.witness(&[Arg::U64(0), Arg::U64(auth_index)]);
    let actual_paid = payments.iter().sum::<u128>();
    outputs.extend(payments.iter().map(|paid| token(&programs.asset, &orders[0].requested, MAKER, *paid)));
    outputs.extend([
        token(&programs.asset, &orders[0].sold, BUYER, if include_second { 5 } else { 2 }),
        token(&programs.asset, &orders[0].requested, BUYER, value(ledger, &intent.points[3]) - actual_paid),
    ]);
    let change = outputs.len();
    outputs.push(cell(owner_lock(BUYER), None, vec![]));
    ledger.build(&inputs, outputs, change, witnesses)
}

fn allocator_refund_with_underpaid_orders(
    programs: &Programs,
    ledger: &mut Ledger,
    intent: &Intent,
    orders: [&Order; 2],
    wallet: &packed::OutPoint,
) -> TransactionView {
    let auth = ledger.seed_funding(BUYER, 381);
    let inputs = vec![
        intent.points[0].clone(),
        orders[0].points[0].clone(),
        orders[0].points[1].clone(),
        intent.points[1].clone(),
        orders[1].points[0].clone(),
        orders[1].points[1].clone(),
        intent.points[2].clone(),
        intent.points[3].clone(),
        auth,
        wallet.clone(),
    ];
    let mut outputs = intent
        .points
        .iter()
        .map(|point| {
            let (cell, data) = &ledger.live[point];
            (cell.clone().as_builder().lock(owner_lock(BUYER)).build(), data.clone())
        })
        .collect::<Vec<_>>();
    outputs.extend([
        cell(orders[0].controller.clone(), Some(orders[0].identity.clone()), order_data(programs, orders[0], 2, 5)),
        escrow(programs, orders[0], 8),
        cell(orders[1].controller.clone(), Some(orders[1].identity.clone()), order_data(programs, orders[1], 3, 7)),
        escrow(programs, orders[1], 7),
        token(&programs.asset, &orders[0].requested, MAKER, 7),
        token(&programs.asset, &orders[0].sold, BUYER, 5),
        token(&programs.asset, &orders[0].requested, BUYER, value(ledger, wallet) - 7),
        cell(owner_lock(BUYER), None, vec![]),
    ]);
    ledger.build(
        &inputs,
        outputs,
        11,
        vec![
            programs.allocation.witness(&[Arg::U64(1), Arg::U64(8)]),
            order_witness(programs, 6, 1, 4, 2, 5, &intent.script),
            programs.asset.witness(&[]),
            Bytes::new().pack(),
            order_witness(programs, 6, 4, 6, 3, 7, &intent.script),
            programs.asset.witness(&[]),
            Bytes::new().pack(),
            programs.asset.witness(&[]),
            Bytes::new().pack(),
            programs.asset.witness(&[]),
        ],
    )
}

fn cancel_intent(programs: &Programs, ledger: &mut Ledger, intent: &Intent, key: u8, ordinal: u32) -> TransactionView {
    let auth = ledger.seed_funding(key, ordinal);
    let mut inputs = intent.points.clone();
    inputs.push(auth);
    let mut outputs = intent
        .points
        .iter()
        .map(|point| {
            let (output, data) = &ledger.live[point];
            (output.clone().as_builder().lock(owner_lock(BUYER)).build(), data.clone())
        })
        .collect::<Vec<_>>();
    outputs.push(cell(owner_lock(key), None, vec![]));
    ledger.build(
        &inputs,
        outputs,
        4,
        vec![
            programs.allocation.witness(&[Arg::U64(1), Arg::U64(4)]),
            Bytes::new().pack(),
            Bytes::new().pack(),
            programs.asset.witness(&[]),
            Bytes::new().pack(),
        ],
    )
}

fn model() -> Value {
    let mut child = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/finance_math_model.mjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            &serde_json::to_vec(&json!({"schema":"cellscript-finance-math-model-v1","cases":[
        {"id":"first-order","kind":"quotient","a":"2","b":"7","d":"3"},
        {"id":"second-order","kind":"quotient","a":"3","b":"7","d":"3"},
        {"id":"first-second-partial","kind":"quotient","a":"4","b":"7","d":"3"},
        {"id":"second-second-partial","kind":"quotient","a":"6","b":"7","d":"3"}]}))
            .unwrap(),
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["results"][0]["ceil"], "5");
    assert_eq!(result["results"][1]["ceil"], "7");
    result
}

#[test]
fn shared_orders_authenticate_complete_allocation_and_stale_intent_refunds() {
    let programs = Programs::compile();
    let mut ledger = Ledger::new(&programs.all(), &[MAKER, BUYER, 0x63, 0x71, 0x72]);
    let (sold, mut maker_wallet) = issue(&programs, &mut ledger, 0x71, MAKER);
    let (requested, mut buyer_wallet) = issue(&programs, &mut ledger, 0x72, BUYER);
    let mut first = initialise(&programs, &mut ledger, &mut maker_wallet, &sold, &requested, 100);
    let mut second = initialise(&programs, &mut ledger, &mut maker_wallet, &sold, &requested, 200);
    let ids = [first.identity.calc_script_hash().unpack(), second.identity.calc_script_hash().unpack()];
    let plan = Plan {
        orders: ids.to_vec(),
        credits: vec![Credit { order: ids[0], output: 4, amount: 5 }, Credit { order: ids[1], output: 4, amount: 7 }],
        auth_input: 8,
        change_output: 7,
    };
    let reference = model();
    for sold in [2, 3] {
        assert!(quote::fill_order(
            quote::OrderState { total: 10, numerator: 7, denominator: 3, filled: 0, entitlement: 0 },
            sold,
            if sold == 2 { 5 } else { 7 }
        )
        .is_ok());
    }
    let underfunded = fund_intent(&programs, &mut ledger, &plan, &mut buyer_wallet, &requested, 300);
    let invalid = settlement(&programs, &mut ledger, &underfunded, [&first, &second], 7, 301, true);
    ledger.reject(
        &invalid,
        &["Inputs[0].Lock", "Inputs[1].Lock", "Inputs[4].Lock"],
        "individually-sufficient-payment-cannot-be-credited-twice",
    );

    let mut duplicate = plan.clone();
    duplicate.orders[1] = ids[0];
    let intent = fund_intent(&programs, &mut ledger, &duplicate, &mut buyer_wallet, &requested, 310);
    let invalid = settlement(&programs, &mut ledger, &intent, [&first, &second], 12, 311, true);
    ledger.reject(&invalid, &["Inputs[0].Lock", "Inputs[1].Lock", "Inputs[4].Lock"], "rebound-table-duplicates-an-order-identity");

    let mut omitted = plan.clone();
    omitted.orders.truncate(1);
    omitted.credits.truncate(1);
    let intent = fund_intent(&programs, &mut ledger, &omitted, &mut buyer_wallet, &requested, 320);
    let invalid = settlement(&programs, &mut ledger, &intent, [&first, &second], 12, 321, true);
    ledger.reject(&invalid, &["Inputs[0].Lock", "Inputs[1].Lock", "Inputs[4].Lock"], "actual-order-set-cannot-be-omitted-from-table");

    let mut missing = plan.clone();
    missing.auth_input = 5;
    missing.change_output = 5;
    for credit in &mut missing.credits {
        credit.output = 2;
    }
    let intent = fund_intent(&programs, &mut ledger, &missing, &mut buyer_wallet, &requested, 330);
    let invalid = settlement(&programs, &mut ledger, &intent, [&first, &second], 12, 331, false);
    ledger.reject(&invalid, &["Inputs[0].Lock", "Inputs[1].Lock"], "listed-order-must-actually-execute-in-transaction");

    let mut duplicate_row = plan.clone();
    duplicate_row.credits.insert(1, Credit { order: ids[0], output: 4, amount: 1 });
    let intent = fund_intent(&programs, &mut ledger, &duplicate_row, &mut buyer_wallet, &requested, 340);
    let invalid = settlement(&programs, &mut ledger, &intent, [&first, &second], 13, 341, true);
    ledger.reject(&invalid, &["Inputs[0].Lock", "Inputs[1].Lock", "Inputs[4].Lock"], "rebound-table-repeats-order-output-credit");

    let mut padded = plan.clone();
    padded.credits.push(Credit { order: ids[1], output: 5, amount: 1 });
    let padded_data = padded.data_with_count(&programs, 2);
    let intent = fund_intent_data(&programs, &mut ledger, padded_data, &mut buyer_wallet, &requested, 345);
    let invalid = settlement(&programs, &mut ledger, &intent, [&first, &second], 12, 346, true);
    ledger.reject(&invalid, &["Inputs[0].Lock", "Inputs[1].Lock", "Inputs[4].Lock"], "inactive-credit-padding-must-be-canonical-zero");

    let actor = fund_intent(&programs, &mut ledger, &plan, &mut buyer_wallet, &requested, 350);
    let valid = settlement(&programs, &mut ledger, &actor, [&first, &second], 12, 351, true);
    let mut outputs = valid.outputs().into_iter().collect::<Vec<_>>();
    outputs[4] = outputs[4].clone().as_builder().lock(owner_lock(0x63)).build();
    ledger.reject(
        &valid.as_advanced_builder().set_outputs(outputs).build(),
        &["Inputs[0].Lock", "Inputs[1].Lock", "Inputs[4].Lock"],
        "shared-payment-recipient-is-derived-from-every-authentic-order",
    );
    let mut foreign_inputs = valid.inputs().into_iter().map(|input| input.previous_output()).collect::<Vec<_>>();
    foreign_inputs.push(maker_wallet.clone());
    let mut foreign_outputs =
        valid.outputs().into_iter().zip(valid.outputs_data()).map(|(output, data)| (output, data.raw_data())).collect::<Vec<_>>();
    foreign_outputs[4] = token(&programs.asset, &sold, MAKER, 12);
    foreign_outputs[6] = token(&programs.asset, &requested, BUYER, 50);
    foreign_outputs.push(token(&programs.asset, &sold, MAKER, value(&ledger, &maker_wallet) - 12));
    let foreign = ledger.build(&foreign_inputs, foreign_outputs, 7, valid.witnesses().into_iter().collect());
    ledger.reject(
        &foreign,
        &["Inputs[0].Lock", "Inputs[1].Lock", "Inputs[4].Lock"],
        "genuine-foreign-asset-cannot-satisfy-requested-credit",
    );
    let mut witnesses = valid.witnesses().into_iter().collect::<Vec<_>>();
    witnesses[0] = programs.allocation.witness(&[Arg::U64(1), Arg::U64(8)]);
    ledger.reject(
        &valid.as_advanced_builder().set_witnesses(witnesses).build(),
        &["Inputs[0].Lock", "Inputs[1].Lock", "Inputs[4].Lock"],
        "allocator-cancel-mode-cannot-bypass-orders-shared-payment-checks",
    );

    let mut isolated_plan = plan.clone();
    isolated_plan.change_output = 11;
    for credit in &mut isolated_plan.credits {
        credit.output = 8;
    }
    let isolated_intent = fund_intent(&programs, &mut ledger, &isolated_plan, &mut buyer_wallet, &requested, 380);
    let isolated = allocator_refund_with_underpaid_orders(&programs, &mut ledger, &isolated_intent, [&first, &second], &buyer_wallet);
    let probe = cost_measurement::measure_group(
        &ledger.context,
        &isolated,
        &isolated_intent.script,
        ckb_testtool::ckb_script::ScriptGroupType::Lock,
        100_000_000,
    );
    assert!(
        matches!(probe.observation, cost_measurement::CycleObservation::Measured { exit_code: 0, .. }),
        "the actual allocator refund group must accept this same underpaid transaction: {probe:?}"
    );
    ledger.reject(
        &isolated,
        &["Inputs[1].Lock", "Inputs[4].Lock"],
        "orders-independently-reject-shared-underpayment-after-valid-allocator-refund",
    );
    ledger.records.last_mut().unwrap()["separate_allocator_group_replay"] = serde_json::to_value(&probe).unwrap();

    let stale = fund_intent(&programs, &mut ledger, &plan, &mut buyer_wallet, &requested, 360);
    let stale_transaction = settlement(&programs, &mut ledger, &stale, [&first, &second], 12, 361, true);
    let points = checked_commit(&programs, &mut ledger, &valid, "two-authentic-orders-share-one-aggregate-maker-payment");
    assert_eq!(value(&ledger, &points[4]), 12);
    assert!(!ledger.live.contains_key(&first.points[0]) && !ledger.live.contains_key(&second.points[0]));
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/finance-protocol-evidence/allocation-stale-replay");
    let error = finance_protocol_bundle::check_and_materialize(&root, &programs.all(), &ledger, &stale_transaction).unwrap_err();
    assert!(error.contains("not locally live"), "{error}");
    ledger.records.push(json!({"case":"old-order-outpoints-are-locally-stale-after-competing-partial-fill","outcome":"rejected",
        "rejection_stage":"local-live-set","error":error,"transaction":hex::encode(stale_transaction.data().as_slice()),
        "node_acceptance":null,"transaction_cycles":null}));
    let wrong_authorizer = cancel_intent(&programs, &mut ledger, &stale, 0x63, 370);
    ledger.reject(&wrong_authorizer, &["Inputs[0].Lock"], "stale-allocation-cancellation-requires-committed-real-authorizer");
    let cancel = cancel_intent(&programs, &mut ledger, &stale, BUYER, 371);
    for (field, index) in [("capacity", 1usize), ("data", 3usize), ("type", 3usize)] {
        let mut outputs = cancel.outputs().into_iter().collect::<Vec<_>>();
        let mut data = cancel.outputs_data().into_iter().collect::<Vec<_>>();
        if field == "capacity" {
            outputs[index] = outputs[index].clone().as_builder().capacity(u64::from(outputs[index].capacity()) - 1).build();
            outputs[4] = outputs[4].clone().as_builder().capacity(u64::from(outputs[4].capacity()) + 1).build();
        } else if field == "data" {
            data[index] = Bytes::from(programs.asset.data("AssetData", &[("amount", 49u128.to_le_bytes().to_vec())])).pack();
            let extra = token(&programs.asset, &requested, BUYER, 1);
            outputs.push(extra.0);
            data.push(extra.1.pack());
            outputs[4] = outputs[4].clone().as_builder().capacity(u64::from(outputs[4].capacity()) - ledger::CELL_CAPACITY).build();
        } else {
            outputs[index] = outputs[index].clone().as_builder().type_(None::<packed::Script>.pack()).build();
            let extra = token(&programs.asset, &requested, BUYER, 50);
            outputs.push(extra.0);
            data.push(extra.1.pack());
            outputs[4] = outputs[4].clone().as_builder().capacity(u64::from(outputs[4].capacity()) - ledger::CELL_CAPACITY).build();
        }
        ledger.reject(
            &cancel.as_advanced_builder().set_outputs(outputs).set_outputs_data(data).build(),
            &["Inputs[0].Lock"],
            &format!("stale-intent-refund-must-preserve-original-{field}"),
        );
    }
    let refunded = checked_commit(&programs, &mut ledger, &cancel, "stale-funded-intent-returns-all-original-cells-to-authorizer");
    for point in refunded.iter().take(4) {
        assert_eq!(ledger.live[point].0.lock(), owner_lock(BUYER));
    }

    first.points = points[..2].to_vec();
    first.filled = 2;
    first.entitlement = 5;
    first.revision = 1;
    second.points = points[2..4].to_vec();
    second.filled = 3;
    second.entitlement = 7;
    second.revision = 1;
    let four = Plan {
        orders: ids.to_vec(),
        credits: vec![
            Credit { order: ids[0], output: 4, amount: 2 },
            Credit { order: ids[0], output: 5, amount: 3 },
            Credit { order: ids[1], output: 4, amount: 4 },
            Credit { order: ids[1], output: 5, amount: 3 },
        ],
        auth_input: 8,
        change_output: 8,
    };
    let mut fifth = four.clone();
    fifth.credits.push(Credit { order: ids[1], output: 6, amount: 1 });
    let intent = fund_intent(&programs, &mut ledger, &fifth, &mut buyer_wallet, &requested, 400);
    let invalid = settlement_with_payments(&programs, &mut ledger, &intent, [&first, &second], &[6, 6], 401, true);
    ledger.reject(
        &invalid,
        &["Inputs[0].Lock", "Inputs[1].Lock", "Inputs[4].Lock"],
        "fifth-credit-exceeds-authenticated-four-row-domain",
    );
    let maximum = fund_intent(&programs, &mut ledger, &four, &mut buyer_wallet, &requested, 410);
    let maximum_tx = settlement_with_payments(&programs, &mut ledger, &maximum, [&first, &second], &[6, 6], 411, true);
    let mut padded_inputs = maximum_tx.inputs().into_iter().map(|input| input.previous_output()).collect::<Vec<_>>();
    let mut padded_outputs = maximum_tx
        .outputs()
        .into_iter()
        .zip(maximum_tx.outputs_data())
        .map(|(output, data)| (output, data.raw_data()))
        .collect::<Vec<_>>();
    for index in 0..23 {
        let point = ledger.seed_funding(0x63, 1200 + index);
        let (output, _) = ledger.live[&point].clone();
        // Genuine separately owned Cells with distinct data make the output
        // permutation observable, rather than swapping identical padding.
        padded_outputs.push((output, Bytes::from(vec![index as u8])));
        padded_inputs.push(point);
    }
    let maximum_tx = ledger.build(&padded_inputs, padded_outputs.clone(), 8, maximum_tx.witnesses().into_iter().collect());
    assert_eq!((maximum_tx.inputs().len(), maximum_tx.outputs().len()), (32, 32));
    checked_candidate(&programs, &mut ledger, &maximum_tx, "c2-canonical-maximum-unrelated-cell-layout-preflight");

    let mut stale_credit_outputs = padded_outputs.clone();
    stale_credit_outputs.swap(4, 9);
    let stale_credit = ledger.build(&padded_inputs, stale_credit_outputs, 8, maximum_tx.witnesses().into_iter().collect());
    ledger.reject(
        &stale_credit,
        &["Inputs[0].Lock", "Inputs[1].Lock", "Inputs[4].Lock"],
        "c2-payment-output-permutation-cannot-retain-stale-allocation-indices",
    );

    // Moving complete successor blocks preserves all actual assets and order
    // identities. The old output_base witnesses must still reject; rebuilding
    // both ordinary witnesses admits the equivalent reconstructed layout.
    let mut moved_successors = padded_outputs.clone();
    moved_successors.swap(0, 2);
    moved_successors.swap(1, 3);
    let stale_successor = ledger.build(&padded_inputs, moved_successors.clone(), 8, maximum_tx.witnesses().into_iter().collect());
    ledger.reject(
        &stale_successor,
        &["Inputs[1].Lock", "Inputs[4].Lock"],
        "c2-successor-block-permutation-cannot-reuse-old-order-output-witnesses",
    );
    let mut rebound_witnesses = maximum_tx.witnesses().into_iter().collect::<Vec<_>>();
    rebound_witnesses[1] = order_witness(&programs, 6, 1, 2, 2, 5, &maximum.script);
    rebound_witnesses[4] = order_witness(&programs, 6, 4, 0, 3, 7, &maximum.script);
    let rebuilt = ledger.build(&padded_inputs, moved_successors, 8, rebound_witnesses);
    checked_candidate(&programs, &mut ledger, &rebuilt, "c2-rebuilt-order-witnesses-admit-equivalent-successor-layout");

    let mut crossed_witnesses = maximum_tx.witnesses().into_iter().collect::<Vec<_>>();
    crossed_witnesses.swap(1, 4);
    let crossed = ledger.build(&padded_inputs, padded_outputs.clone(), 8, crossed_witnesses);
    ledger.reject(&crossed, &["Inputs[1].Lock", "Inputs[4].Lock"], "c2-one-order-cannot-reuse-another-participants-witness");
    let mut wrong_authorizer = maximum_tx.witnesses().into_iter().collect::<Vec<_>>();
    wrong_authorizer[0] = programs.allocation.witness(&[Arg::U64(0), Arg::U64(9)]);
    let wrong_authorizer = ledger.build(&padded_inputs, padded_outputs.clone(), 8, wrong_authorizer);
    ledger.reject(&wrong_authorizer, &["Inputs[0].Lock"], "c2-trailing-real-signer-cannot-rebind-committed-authorizer-index");

    let mut equivalent_inputs = padded_inputs.clone();
    let mut equivalent_outputs = padded_outputs.clone();
    equivalent_inputs.swap(9, 10);
    equivalent_outputs.swap(9, 10);
    let equivalent = ledger.build(&equivalent_inputs, equivalent_outputs, 8, maximum_tx.witnesses().into_iter().collect());
    assert_ne!(maximum_tx.hash(), equivalent.hash());
    assert_ne!(maximum_tx.outputs_data().get(9), equivalent.outputs_data().get(9));
    let mut excessive_inputs = padded_inputs;
    excessive_inputs.push(ledger.seed_funding(BUYER, 1223));
    let excessive = ledger.build(&excessive_inputs, padded_outputs, 8, maximum_tx.witnesses().into_iter().collect());
    ledger.reject(
        &excessive,
        &[
            "Inputs[0].Lock",
            "Inputs[1].Lock",
            "Inputs[2].Lock",
            "Inputs[4].Lock",
            "Inputs[5].Lock",
            "Inputs[2].Type",
            "Inputs[7].Type",
        ],
        "c2-thirty-three-inputs-exceed-the-frozen-domain",
    );
    assert!(ledger.records.last().unwrap()["error"].as_str().unwrap().contains("error code 5"));
    // Keep the maximum valid participant/payment graph and change only the
    // last order's successor revision, after its authentic cumulative fill.
    let mut late_data = maximum_tx.outputs_data().into_iter().collect::<Vec<_>>();
    let mut last_order = late_data[2].raw_data().to_vec();
    assert_eq!(u64::from_le_bytes(last_order[312..320].try_into().unwrap()), second.revision + 1);
    last_order[312..320].copy_from_slice(&(second.revision + 2).to_le_bytes());
    late_data[2] = Bytes::from(last_order).pack();
    let late_invalid = maximum_tx.as_advanced_builder().set_outputs_data(late_data).build();
    assert_eq!((late_invalid.inputs().len(), late_invalid.outputs().len()), (32, 32));
    ledger.reject(&late_invalid, &["Inputs[0].Lock", "Inputs[4].Lock"], "c2-maximum-domain-last-order-revision-mismatch");
    assert!(ledger.records.last().unwrap()["error"].as_str().unwrap().contains("error code 5"));
    let maximum_points =
        checked_commit(&programs, &mut ledger, &equivalent, "four-credits-two-shared-outputs-at-thirty-two-cell-domain");
    ledger.records.last_mut().unwrap()["layout"] = json!({"canonical_preflight_raw_hash":hex::encode(maximum_tx.hash().as_slice()),
        "equivalent_trailing_input_swap":[9,10],"equivalent_trailing_output_swap":[9,10],"required_role_and_credit_indices_unchanged":true});
    assert_eq!((value(&ledger, &maximum_points[4]), value(&ledger, &maximum_points[5])), (6, 6));
    assert_eq!(ledger.live[&maximum_points[0]].1.as_ref(), order_data(&programs, &first, 4, 10));
    assert_eq!(ledger.live[&maximum_points[2]].1.as_ref(), order_data(&programs, &second, 6, 14));
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/finance-protocol-evidence/c2");
    ledger.archive(&directory, &programs.all());
    std::fs::write(
        directory.join("independent-model.json"),
        serde_json::to_vec_pretty(&json!({
        "model":reference,"first_credit":"5","second_credit":"7","actual_shared_payment":"12",
        "table_schema":"fixed-flat-416-bytes","orders":2,"maximum_orders":2,"maximum_credits":4,
        "maximum_valid_inputs":32,"maximum_valid_outputs":32,"maximum_rows_executed":4,
        "node_acceptance":null,"external_reproduction":null}))
        .unwrap(),
    )
    .unwrap();
}
