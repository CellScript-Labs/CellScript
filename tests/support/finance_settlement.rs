//! One authentic order and pool settle directly, with no taker B inventory.
use super::*;

#[path = "finance_settlement_layout.rs"]
mod layout;

struct Order {
    identity: packed::Script,
    controller: packed::Script,
    assets: [packed::Script; 2],
    points: Vec<packed::OutPoint>,
}

fn order_data(program: &Program, order: &Order, filled: u128, entitlement: u128, revision: u64) -> Vec<u8> {
    let maker = owner_lock(0x62).calc_script_hash().as_slice().to_vec();
    program.data(
        "OrderState",
        &[
            ("order", order.identity.calc_script_hash().as_slice().to_vec()),
            ("sold_asset", order.assets[0].calc_script_hash().as_slice().to_vec()),
            ("requested_asset", order.assets[1].calc_script_hash().as_slice().to_vec()),
            ("maker", maker.clone()),
            ("proceeds", maker.clone()),
            ("remainder", maker),
            ("network", vec![0x11; 32]),
            ("numerator", 7u128.to_le_bytes().to_vec()),
            ("denominator", 3u128.to_le_bytes().to_vec()),
            ("total_sold", 1000u128.to_le_bytes().to_vec()),
            ("nonce", 1u64.to_le_bytes().to_vec()),
            ("filled", filled.to_le_bytes().to_vec()),
            ("entitlement", entitlement.to_le_bytes().to_vec()),
            ("revision", revision.to_le_bytes().to_vec()),
        ],
    )
}

fn escrow(program: &Program, order: &Order, amount: u128) -> Cell {
    let data = program.data(
        "EscrowData",
        &[
            ("amount", amount.to_le_bytes().to_vec()),
            ("order", order.identity.calc_script_hash().as_slice().to_vec()),
            ("controller", order.controller.calc_script_hash().as_slice().to_vec()),
            ("asset", order.assets[0].calc_script_hash().as_slice().to_vec()),
        ],
    );
    cell(program.script(&cellscript_artifact_checker::ckb_blake2b256(&data[16..])), Some(order.assets[0].clone()), data)
}

fn initialise_order(
    programs: &Programs,
    order_code: &Program,
    escrow_code: &Program,
    all: &[&Program],
    ledger: &mut Ledger,
    maker: &mut Wallet,
    assets: [packed::Script; 2],
) -> Order {
    let funding = ledger.seed_funding(maker.key, 3000);
    let bootstrap_funding = ledger.seed_funding(maker.key, 3001);
    let input = packed::CellInput::new_builder().previous_output(funding.clone()).build();
    let anchor = cellscript_artifact_checker::ckb_blake2b256(&[input.as_slice(), &0u64.to_le_bytes()].concat());
    let identity = packed::Script::new_builder()
        .code_hash(cellscript::ckb_abi::type_id::CODE_HASH)
        .hash_type(1u8)
        .args(Bytes::copy_from_slice(&anchor).pack())
        .build();
    let mut order = Order { identity, controller: order_code.script(&[]), assets, points: vec![] };
    let state = order_data(order_code, &order, 0, 0, 0);
    order.controller = order_code.script(&cellscript_artifact_checker::ckb_blake2b256(&state[..280]));
    let bootstrap = ledger.build(
        &[bootstrap_funding],
        vec![cell(order.controller.clone(), None, state.clone()), cell(owner_lock(maker.key), None, vec![])],
        1,
        vec![],
    );
    let bootstrap = ledger.commit(&bootstrap, "create-c1-order-bootstrap")[0].clone();
    let tx = ledger.build(
        &[funding, bootstrap, maker.a.clone()],
        vec![
            cell(order.controller.clone(), Some(order.identity.clone()), state),
            escrow(escrow_code, &order, 1000),
            token(&programs.asset, &order.assets[0], maker.key, value(ledger, &maker.a) - 1000),
            cell(owner_lock(maker.key), None, vec![]),
        ],
        3,
        vec![
            Bytes::new().pack(),
            order_code.witness(&[
                Arg::U64(0),
                Arg::U64(1),
                Arg::U64(0),
                Arg::U128(0),
                Arg::U128(0),
                Arg::Hash(owner_lock(maker.key).calc_script_hash().unpack()),
                Arg::Hash(owner_lock(0x72).calc_script_hash().unpack()),
            ]),
            programs.asset.witness(&[]),
        ],
    );
    let points = checked_commit_with(programs.evidence_namespace, all, ledger, &tx, "create-authentic-c1-order");
    order.points = points[..2].to_vec();
    maker.a = points[2].clone();
    order
}

struct Intent {
    controller: packed::Script,
    data: Vec<u8>,
    funding: packed::OutPoint,
    wallet: packed::OutPoint,
    intent: packed::OutPoint,
}

struct Actor {
    key: u8,
    a: packed::OutPoint,
}

fn prepare_intent(
    programs: &Programs,
    settlement: &Program,
    ledger: &mut Ledger,
    pool: &Pool,
    order: &Order,
    actor: &mut Actor,
) -> Intent {
    let data = settlement.data(
        "SettlementIntent",
        &[
            ("pool", pool.identity.calc_script_hash().as_slice().to_vec()),
            ("pool_controller", pool.controller.calc_script_hash().as_slice().to_vec()),
            ("order", order.identity.calc_script_hash().as_slice().to_vec()),
            ("order_controller", order.controller.calc_script_hash().as_slice().to_vec()),
            ("asset_a", pool.assets[0].calc_script_hash().as_slice().to_vec()),
            ("asset_b", pool.assets[1].calc_script_hash().as_slice().to_vec()),
            ("authorizer", owner_lock(actor.key).calc_script_hash().as_slice().to_vec()),
            ("maximum_input_a", 60u128.to_le_bytes().to_vec()),
            ("minimum_profit_a", 40u128.to_le_bytes().to_vec()),
            ("maximum_fee", (2 * ledger::FEE).to_le_bytes().to_vec()),
            ("maximum_capacity_debit", (2 * ledger::CELL_CAPACITY).to_le_bytes().to_vec()),
            ("nonce", 1u64.to_le_bytes().to_vec()),
        ],
    );
    assert_eq!(data.len(), 280);
    let controller = settlement.script(&cellscript_artifact_checker::ckb_blake2b256(&data));
    let funding = ledger.seed_funding(actor.key, 3100);
    let mut reserved = cell(controller.clone(), None, vec![]);
    reserved.0 = reserved.0.as_builder().capacity(10 * ledger::CELL_CAPACITY).build();
    let tx = ledger.build(
        &[funding, actor.a.clone()],
        vec![
            cell(controller.clone(), None, data.clone()),
            reserved,
            cell(controller.clone(), Some(pool.assets[0].clone()), amount(&programs.asset, 200)),
            token(&programs.asset, &pool.assets[0], actor.key, value(ledger, &actor.a) - 200),
            cell(owner_lock(actor.key), None, vec![]),
        ],
        4,
        vec![Bytes::new().pack(), programs.asset.witness(&[])],
    );
    let points = ledger.commit(&tx, "fund-authenticated-c1-intent-with-real-a-only");
    actor.a = points[3].clone();
    Intent { controller, data, funding: points[1].clone(), wallet: points[2].clone(), intent: points[0].clone() }
}

fn settlement_transaction(
    programs: &Programs,
    order_code: &Program,
    escrow_code: &Program,
    settlement: &Program,
    ledger: &mut Ledger,
    pool: &Pool,
    order: &Order,
    intent: &Intent,
    authority: &packed::OutPoint,
    fill: (u128, u128),
) -> TransactionView {
    let (input, sold) = fill;
    let pool_quote = quote::swap_exact_in(quoted_state(ledger, pool), quote::Direction::AForB, input, 0).unwrap();
    let order_quote = quote::fill_order(
        quote::OrderState { total: 1000, numerator: 7, denominator: 3, filled: 0, entitlement: 0 },
        sold,
        pool_quote.amount_out,
    )
    .unwrap();
    let inputs = [
        pool.points.clone(),
        order.points.clone(),
        vec![intent.funding.clone(), intent.wallet.clone(), intent.intent.clone(), authority.clone()],
    ]
    .concat();
    assert_eq!(inputs.len(), 9);
    let paid = pool_quote.amount_out;
    let outputs = vec![
        cell(
            pool.controller.clone(),
            Some(pool.identity.clone()),
            pool_data(
                &programs.pool,
                pool,
                pool_quote.next.reserve_a,
                pool_quote.next.reserve_b,
                pool_quote.next.total_lp,
                pool.revision + 1,
            ),
        ),
        reserve(&programs.reserve, pool, 0, pool_quote.next.reserve_a),
        reserve(&programs.reserve, pool, 1, pool_quote.next.reserve_b),
        cell(
            order.controller.clone(),
            Some(order.identity.clone()),
            order_data(order_code, order, sold, order_quote.next.entitlement, 1),
        ),
        escrow(escrow_code, order, 1000 - sold),
        token(&programs.asset, &pool.assets[1], 0x62, paid),
        cell(intent.controller.clone(), None, vec![]),
        cell(intent.controller.clone(), Some(pool.assets[0].clone()), amount(&programs.asset, 200 - input + sold)),
        cell(intent.controller.clone(), None, intent.data.clone()),
        ledger.live[authority].clone(),
    ];
    ledger.build(
        &inputs,
        outputs,
        6,
        vec![
            programs.pool.witness(&[
                Arg::U64(5),
                Arg::U64(0),
                Arg::U64(0),
                Arg::U128(input),
                Arg::U128(0),
                Arg::U128(0),
                Arg::U128(0),
                Arg::U128(paid),
                Arg::Hash([0; 32]),
            ]),
            programs.asset.witness(&[]),
            programs.asset.witness(&[]),
            order_code.witness(&[
                Arg::U64(4),
                Arg::U64(3),
                Arg::U64(3),
                Arg::U128(sold),
                Arg::U128(paid),
                Arg::Hash(intent.controller.calc_script_hash().unpack()),
                Arg::Hash(owner_lock(0x72).calc_script_hash().unpack()),
            ]),
            escrow_code.witness(&[]),
            settlement.witness(&[Arg::U64(0), Arg::U64(7), Arg::U64(8)]),
        ],
    )
}

#[test]
fn direct_order_pool_settlement_enforces_real_user_intent_and_zero_bridge_inventory() {
    exercise_settlement("c1", false);
}

#[test]
fn direct_settlement_admits_unrelated_permutations_and_binds_every_participant() {
    exercise_settlement("c1-layout", true);
}

fn exercise_settlement(namespace: &'static str, extended: bool) {
    let mut programs = Programs::compile();
    programs.evidence_namespace = namespace;
    let root = programs._directory.path().join("finance");
    let escrow_code = Program::compile(&root.join("escrow"), "escrow", false);
    let mut configuration = std::fs::read_to_string(root.join("deployment/src/lib.cell")).unwrap();
    let literal = cellscript_artifact_checker::ckb_blake2b256(escrow_code.code())
        .iter()
        .map(|byte| format!("\\x{byte:02x}"))
        .collect::<String>();
    configuration.push_str(&format!("public fn escrow_code_hash() -> Hash {{return Hash::from_bytes(b\"{literal}\")}}\n"));
    std::fs::write(root.join("deployment/src/lib.cell"), configuration).unwrap();
    programs.pool = Program::compile(&root.join("pool"), "pool", false);
    let order_code = Program::compile(&root.join("order"), "order", false);
    let settlement = Program::compile(&root.join("settlement"), "settlement", false);
    let trap = capacity_trap(&programs);
    let all = [programs.all().to_vec(), vec![&escrow_code, &order_code, &settlement, &trap]].concat();
    let mut ledger = Ledger::new(&all, &[0x61, 0x62, 0x63, 0x64, 0x71, 0x72, 0x73]);
    prove_capacity_trap(&mut ledger, &trap);
    let (a, wa) = issue(&mut ledger, &programs.asset, 0x71);
    let (b, wb) = issue(&mut ledger, &programs.asset, 0x72);
    let assets = [a, b];
    let mut provider = Wallet { key: 0x61, a: wa[0].clone(), b: wb[0].clone(), lp: None };
    let mut maker = Wallet { key: 0x62, a: wa[1].clone(), b: wb[1].clone(), lp: None };
    let trace = model(&[json!({"kind":"swap_a","amount":"50"})]);
    let (pool, _) = initialise(&programs, &mut ledger, assets.clone(), &mut provider, &trace[0]);
    let order = initialise_order(&programs, &order_code, &escrow_code, &all, &mut ledger, &mut maker, assets);
    let gift_funding = ledger.seed_funding(provider.key, 3050);
    let gift = ledger.build(
        &[provider.a.clone(), gift_funding],
        vec![
            token(&programs.asset, &pool.assets[0], 0x63, 200),
            token(&programs.asset, &pool.assets[0], provider.key, value(&ledger, &provider.a) - 200),
            cell(owner_lock(provider.key), None, vec![]),
        ],
        2,
        vec![programs.asset.witness(&[])],
    );
    let gift = ledger.commit(&gift, "separate-c1-taker-receives-only-a");
    provider.a = gift[1].clone();
    let mut actor = Actor { key: 0x63, a: gift[0].clone() };
    assert!(!ledger
        .live
        .values()
        .any(|(output, _)| output.lock() == owner_lock(actor.key) && output.type_().to_opt() == Some(pool.assets[1].clone())));
    let intent = prepare_intent(&programs, &settlement, &mut ledger, &pool, &order, &mut actor);
    let authority = ledger.seed_funding(actor.key, 3200);
    if !extended {
        let excessive = settlement_transaction(
            &programs,
            &order_code,
            &escrow_code,
            &settlement,
            &mut ledger,
            &pool,
            &order,
            &intent,
            &authority,
            (100, 150),
        );
        ledger.reject(&excessive, &["Inputs[5].Lock"], "c1-actual-pool-input-cannot-exceed-committed-user-limit");
        let low_profit = settlement_transaction(
            &programs,
            &order_code,
            &escrow_code,
            &settlement,
            &mut ledger,
            &pool,
            &order,
            &intent,
            &authority,
            (50, 80),
        );
        ledger.reject(&low_profit, &["Inputs[5].Lock"], "c1-actual-net-profit-cannot-fall-below-user-limit");
    }
    let tx = settlement_transaction(
        &programs,
        &order_code,
        &escrow_code,
        &settlement,
        &mut ledger,
        &pool,
        &order,
        &intent,
        &authority,
        (50, 100),
    );
    if !extended {
        let sources = tx.inputs().into_iter().map(|input| input.previous_output()).collect::<Vec<_>>();
        let outputs =
            || tx.outputs().into_iter().zip(tx.outputs_data()).map(|(output, data)| (output, data.raw_data())).collect::<Vec<_>>();
        let mut redirected = outputs();
        redirected[5].0 = redirected[5].0.clone().as_builder().lock(owner_lock(0x61)).build();
        let hostile = ledger.build(&sources, redirected, 6, tx.witnesses().into_iter().collect());
        ledger.reject(&hostile, &["Inputs[3].Lock", "Inputs[5].Lock"], "c1-pool-payment-cannot-substitute-maker-recipient");
        // Preserve total B and the genuine pool transition while assigning less
        // than E(100)=234 to the maker. The unused B goes to another real owner.
        let mut underpaid = outputs();
        underpaid[5] = token(&programs.asset, &pool.assets[1], 0x62, 233);
        underpaid.push(token(&programs.asset, &pool.assets[1], 0x61, number(&trace[1], "received") - 233));
        let mut witnesses = tx.witnesses().into_iter().collect::<Vec<_>>();
        witnesses[3] = order_code.witness(&[
            Arg::U64(4),
            Arg::U64(3),
            Arg::U64(3),
            Arg::U128(100),
            Arg::U128(233),
            Arg::Hash(intent.controller.calc_script_hash().unpack()),
            Arg::Hash(owner_lock(0x72).calc_script_hash().unpack()),
        ]);
        let hostile = ledger.build(&sources, underpaid, 6, witnesses);
        ledger.reject(&hostile, &["Inputs[3].Lock", "Inputs[5].Lock"], "c1-real-underpayment-is-not-a-donation-credit");
        let mut wrong_remainder = outputs();
        wrong_remainder[4] = escrow(&escrow_code, &order, 901);
        wrong_remainder[7].1 = Bytes::from(amount(&programs.asset, 249));
        let hostile = ledger.build(&sources, wrong_remainder, 6, tx.witnesses().into_iter().collect());
        ledger.reject(&hostile, &["Inputs[3].Lock", "Inputs[4].Lock"], "c1-escrow-remainder-must-match-authentic-order-transition");
        let mut capacity_theft = outputs();
        let refund = u64::from(capacity_theft[9].0.capacity()) + 2 * ledger::CELL_CAPACITY;
        capacity_theft[9].0 = capacity_theft[9].0.clone().as_builder().capacity(refund).build();
        let hostile = ledger.build(&sources, capacity_theft, 6, tx.witnesses().into_iter().collect());
        ledger.reject(&hostile, &["Inputs[5].Lock"], "c1-user-capacity-debit-cannot-exceed-committed-maximum");
        let mut replaced_intent = tx.witnesses().into_iter().collect::<Vec<_>>();
        replaced_intent[5] = settlement.witness(&[Arg::U64(0), Arg::U64(6), Arg::U64(8)]);
        let hostile = ledger.build(&sources, outputs(), 6, replaced_intent);
        ledger.reject(&hostile, &["Inputs[5].Lock"], "c1-witness-cannot-redirect-the-committed-intent-source");
        let mut frozen_refund = outputs();
        frozen_refund[9].0 =
            frozen_refund[9].0.clone().as_builder().capacity(u64::from(frozen_refund[9].0.capacity()) - ledger::CELL_CAPACITY).build();
        frozen_refund.push(cell(owner_lock(actor.key), Some(trap.script(&[])), vec![]));
        let hostile = ledger.build(&sources, frozen_refund, 6, trap_witness(&tx, &trap, 10));
        ledger.reject(&hostile, &["Inputs[5].Lock"], "c1-authorizer-capacity-cannot-be-credited-through-a-spend-rejecting-type");
    }
    let tx = if extended {
        layout::cases(
            &programs,
            &order_code,
            &escrow_code,
            &settlement,
            &all,
            &mut ledger,
            &pool,
            &order,
            &intent,
            &authority,
            &mut provider,
            &tx,
        )
    } else {
        tx
    };
    assert!(!tx.inputs().into_iter().any(|input| {
        let (output, _) = &ledger.live[&input.previous_output()];
        output.lock() == intent.controller && output.type_().to_opt() == Some(pool.assets[1].clone())
    }));
    assert_eq!(
        u128::from_le_bytes(tx.outputs_data().get(5).unwrap().raw_data()[..16].try_into().unwrap()),
        number(&trace[1], "received")
    );
    let points = checked_commit_with(
        namespace,
        &all,
        &mut ledger,
        &tx,
        if extended { "c1-equivalent-trailing-permutation-commits-once" } else { "direct-order-pool-settlement-with-no-user-b-input" },
    );
    assert_eq!(value(&ledger, &points[7]), 250);
    let authority = ledger.seed_funding(actor.key, 3201);
    let unwrap = ledger.build(
        &[points[6].clone(), points[7].clone(), points[8].clone(), authority],
        vec![token(&programs.asset, &pool.assets[0], actor.key, 250), cell(owner_lock(actor.key), None, vec![])],
        1,
        vec![settlement.witness(&[Arg::U64(1), Arg::U64(2), Arg::U64(3)]), programs.asset.witness(&[])],
    );
    if !extended {
        let unwrap_sources = unwrap.inputs().into_iter().map(|input| input.previous_output()).collect::<Vec<_>>();
        let unwrap_outputs = || {
            unwrap.outputs().into_iter().zip(unwrap.outputs_data()).map(|(output, data)| (output, data.raw_data())).collect::<Vec<_>>()
        };
        let mut frozen = unwrap_outputs();
        frozen.push(cell(owner_lock(actor.key), Some(trap.script(&[])), vec![]));
        let hostile = ledger.build(&unwrap_sources, frozen, 1, trap_witness(&unwrap, &trap, 2));
        ledger.reject(&hostile, &["Inputs[0].Lock"], "c1-unwrap-capacity-cannot-be-credited-through-a-spend-rejecting-type");
        let unrelated = ledger.seed_funding(0x61, 9100);
        let mut leak_sources = unwrap_sources;
        leak_sources.push(unrelated.clone());
        let mut leaked = unwrap_outputs();
        let mut refund = ledger.live[&unrelated].clone();
        refund.0 = refund.0.clone().as_builder().capacity(u64::from(refund.0.capacity()) + 1).build();
        leaked.push(refund);
        let hostile = ledger.build(&leak_sources, leaked, 1, unwrap.witnesses().into_iter().collect());
        ledger.reject(&hostile, &["Inputs[0].Lock"], "c1-unused-fee-allowance-cannot-fund-a-third-party-refund");
    }
    checked_commit_with(namespace, &all, &mut ledger, &unwrap, "authorizer-unwraps-all-residual-intent-funds");
    ledger.archive(&Path::new(env!("CARGO_MANIFEST_DIR")).join("target/finance-protocol-evidence").join(namespace), &all);
}
