//! A real order and pool settle directly with no pre-funded taker B.

use super::{
    backend::{cell, prefixed, Backend, CELL_CAPACITY, FEE},
    bundle, checked_operation, order,
    protocol::{self, Programs, Wallet},
    quote,
    recovery::{AmountEncoding, AssetAmountBound, FinanceIntent, FinanceIntentParameters},
};
use anyhow::{bail, Context, Result};
use cellscript::EntryWitnessArg as Arg;
use cellscript_ckb_adapter::recovery::RecoverySubject;
use ckb_types::{
    core::{Capacity, TransactionView},
    packed,
    prelude::*,
};
use serde_json::json;

fn bound(asset: &packed::Script, amount: u128) -> AssetAmountBound {
    AssetAmountBound { asset: asset.clone(), encoding: AmountEncoding::U128Le, amount }
}
fn id(label: &str, identity: &packed::Script) -> ckb_types::H256 {
    ckb_hash::blake2b_256([label.as_bytes(), identity.as_slice()].concat()).into()
}

fn submit(
    node: &mut Backend,
    programs: &Programs,
    tx: &TransactionView,
    parameters: FinanceIntentParameters,
    single: bool,
) -> Result<Vec<packed::OutPoint>> {
    let label = parameters.operation.clone();
    let intent = FinanceIntent::new(parameters)?;
    if single {
        let cells = tx
            .inputs()
            .into_iter()
            .map(|input| {
                let (output, data) = &node.live[&input.previous_output()];
                cellscript_ckb_adapter::ProtocolBundleLiveCellObservation {
                    out_point: input.previous_output(),
                    output: output.clone(),
                    data: data.clone(),
                }
            })
            .collect::<Vec<_>>();
        intent.verify_candidate(tx, &cells)?;
        let points = node.commit(&node.sign(tx)?, &label)?;
        node.records.last_mut().context("missing C1 funding")?["application_policy"] = json!({"adapter_path":"ordinary-single-action","custom_groups":1,
            "candidate_constraints":"checked-against-actual-resolved-inputs-and-packed-outputs","protocol_bundle_evidence":"not-applicable","durable_recovery_journal":"not-used-for-this-funding-step"});
        Ok(points)
    } else {
        let prepared = bundle::prepare(&node.run_dir.join("artifacts"), &programs.all(), node, tx)?;
        checked_operation(node, programs, &prepared, intent, &label)
    }
}

pub fn run(
    programs: &Programs,
    node: &mut Backend,
    provider: &mut Wallet,
    maker: &mut Wallet,
    assets: [packed::Script; 2],
) -> Result<()> {
    eprintln!("[finance-node] actual direct order/pool settlement without taker B inventory");
    let settlement = programs.settlement.as_ref().context("complete lane needs settlement program")?;
    let order_code = programs.order.as_ref().context("complete lane needs order program")?;
    // Deploy this additional immutable verifier separately; every code Cell is
    // occupied-capacity checked and funded by genuine upstream-signed inputs.
    let code_capacity = cell(node.owner(provider.key), None, settlement.code().to_vec())
        .0
        .occupied_capacity(Capacity::bytes(settlement.code().len())?)?
        .as_u64()
        + CELL_CAPACITY;
    let mut code_inputs = vec![];
    let mut code_total = 0u64;
    while code_total < code_capacity + CELL_CAPACITY + FEE {
        let point = provider.funding()?;
        code_total += u64::from(node.live[&point].0.capacity());
        code_inputs.push(point);
    }
    let code = cell(node.owner(provider.key), None, settlement.code().to_vec());
    let unsigned = node.build(
        &code_inputs,
        vec![(code.0.as_builder().capacity(code_capacity).build(), code.1), cell(node.owner(provider.key), None, vec![])],
        1,
        vec![],
    )?;
    let code_point = node.commit(&node.sign(&unsigned)?, "signed-deployment-of-ordinary-settlement-verifier")?[0].clone();
    node.code_points.insert(settlement.name.clone(), code_point.clone());
    node.deps.push(packed::CellDep::new_builder().out_point(code_point).build());

    let (mut pool, unsigned) = protocol::initialise(programs, node, provider, assets.clone(), 30, 201, 20_000, 100_000)?;
    let points = submit(
        node,
        programs,
        &unsigned,
        FinanceIntentParameters {
            operation_id: id("c1-create-pool", &pool.identity),
            operation: "actual-c1-pool-initialisation".into(),
            owner: node.owner(provider.key),
            recipient: node.owner(provider.key),
            debits: vec![bound(&assets[0], 20_000), bound(&assets[1], 100_000)],
            credits: vec![AssetAmountBound {
                asset: pool.lp.clone(),
                amount: pool.state.total_lp - quote::LOCKED_MINIMUM,
                encoding: AmountEncoding::PoolShares {
                    pool: pool.identity.calc_script_hash().unpack(),
                    controller_code: pool.controller.code_hash().unpack(),
                },
            }],
            subjects: vec![RecoverySubject::Lock(pool.controller.clone())],
            maximum_fee_shannons: FEE,
            maximum_capacity_debit_shannons: 3 * CELL_CAPACITY + FEE,
            maximum_occupied_capacity_shannons: 100 * CELL_CAPACITY,
        },
        false,
    )?;
    pool.points = points[..3].to_vec();
    provider.a = points[3].clone();
    provider.b = points[4].clone();
    provider.lp = Some(points[5].clone());

    let actor = node.key_labels[2];
    let mut sources = (0..4).map(|_| provider.funding()).collect::<Result<Vec<_>>>()?;
    sources.push(provider.a.clone());
    let mut outputs = vec![
        protocol::token(programs, node, &assets[0], maker.key, 2000)?,
        protocol::token(programs, node, &assets[0], actor, 200)?,
        protocol::token(
            programs,
            node,
            &assets[0],
            provider.key,
            protocol::value(node, &provider.a).checked_sub(2200).context("provider lacks C1 inventory")?,
        )?,
    ];
    for _ in 0..3 {
        let c = cell(node.owner(actor), None, vec![]);
        outputs.push((c.0.as_builder().capacity(10 * CELL_CAPACITY).build(), c.1));
    }
    outputs.push(cell(node.owner(provider.key), None, vec![]));
    let mut witnesses = vec![packed::WitnessArgs::default(); 4];
    witnesses.push(programs.asset.witness(&[])?);
    let unsigned = node.build(&sources, outputs, 6, witnesses)?;
    let points = node.commit(&node.sign(&unsigned)?, "real-signed-c1-maker-and-a-only-taker-inventory")?;
    maker.a = points[0].clone();
    provider.a = points[2].clone();
    let actor_wallet = points[1].clone();
    let mut funding = points[3..6].to_vec();
    let mut order = order::initialise(programs, node, maker, assets.clone(), 201, 1000)?;
    if node.live.values().any(|(output, _)| output.lock() == node.owner(actor) && output.type_().to_opt().as_ref() == Some(&assets[1]))
    {
        bail!("C1 actor has unexpected B inventory");
    }
    let tails = if node.c4 { super::operations::fund_unrelated(node, maker, "c1")? } else { vec![] };
    let known = [pool.points.clone(), order.points.clone(), vec![actor_wallet.clone()]].concat();
    let clock = super::operations::Clock::start(node, programs, &known)?;
    let data = settlement.data(
        "SettlementIntent",
        &[
            ("pool", pool.identity.calc_script_hash().as_slice().to_vec()),
            ("pool_controller", pool.controller.calc_script_hash().as_slice().to_vec()),
            ("order", order.identity.calc_script_hash().as_slice().to_vec()),
            ("order_controller", order.controller.calc_script_hash().as_slice().to_vec()),
            ("asset_a", assets[0].calc_script_hash().as_slice().to_vec()),
            ("asset_b", assets[1].calc_script_hash().as_slice().to_vec()),
            ("authorizer", node.owner(actor).calc_script_hash().as_slice().to_vec()),
            ("maximum_input_a", 60u128.to_le_bytes().to_vec()),
            ("minimum_profit_a", 40u128.to_le_bytes().to_vec()),
            ("maximum_fee", FEE.to_le_bytes().to_vec()),
            ("maximum_capacity_debit", (2 * CELL_CAPACITY).to_le_bytes().to_vec()),
            ("nonce", 201u64.to_le_bytes().to_vec()),
        ],
    )?;
    let controller = settlement.script(&ckb_hash::blake2b_256(&data));
    let capacity = cell(controller.clone(), None, vec![]);
    let unsigned = node.build(
        &[funding.pop().context("missing C1 funding")?, actor_wallet],
        vec![
            cell(controller.clone(), None, data.clone()),
            (capacity.0.as_builder().capacity(5 * CELL_CAPACITY).build(), capacity.1),
            cell(
                controller.clone(),
                Some(assets[0].clone()),
                programs.asset.data("AssetData", &[("amount", 200u128.to_le_bytes().to_vec())])?,
            ),
            cell(node.owner(actor), None, vec![]),
        ],
        3,
        vec![packed::WitnessArgs::default(), programs.asset.witness(&[])?],
    )?;
    let points = submit(
        node,
        programs,
        &unsigned,
        FinanceIntentParameters {
            operation_id: id("c1-fund-intent", &controller),
            operation: "actual-a-only-settlement-intent-funding".into(),
            owner: node.owner(actor),
            recipient: controller.clone(),
            debits: vec![bound(&assets[0], 200)],
            credits: vec![bound(&assets[0], 200)],
            subjects: vec![RecoverySubject::Type(assets[0].clone())],
            maximum_fee_shannons: FEE,
            maximum_capacity_debit_shannons: 7 * CELL_CAPACITY + FEE,
            maximum_occupied_capacity_shannons: 100 * CELL_CAPACITY,
        },
        true,
    )?;
    let rejection_clock = if node.c4 { Some(super::operations::Clock::start(node, programs, &pool.points)?) } else { None };
    let pool_quote =
        node.measure_quote("settlement-pool-exact-input", || quote::swap_exact_in(pool.state, quote::Direction::AForB, 50, 1))?;
    let order_quote = node.measure_quote("settlement-order-fill", || quote::fill_order(order.quote, 100, pool_quote.amount_out))?;
    let authority = funding.pop().context("missing settlement authority")?;
    let inputs =
        [pool.points.clone(), order.points.clone(), vec![points[1].clone(), points[2].clone(), points[0].clone(), authority.clone()]]
            .concat();
    let unsigned = node.build(
        &inputs,
        vec![
            cell(
                pool.controller.clone(),
                Some(pool.identity.clone()),
                protocol::pool_data(programs, node, &pool, pool_quote.next, pool.revision + 1)?,
            ),
            protocol::reserve(programs, &pool, 0, pool_quote.next.reserve_a)?,
            protocol::reserve(programs, &pool, 1, pool_quote.next.reserve_b)?,
            cell(
                order.controller.clone(),
                Some(order.identity.clone()),
                order::data(programs, node, &order, order_quote.next, order.revision + 1)?,
            ),
            order::escrow(programs, &order, 900)?,
            protocol::token(programs, node, &assets[1], maker.key, pool_quote.amount_out)?,
            cell(controller.clone(), None, vec![]),
            cell(
                controller.clone(),
                Some(assets[0].clone()),
                programs.asset.data("AssetData", &[("amount", 250u128.to_le_bytes().to_vec())])?,
            ),
            cell(controller.clone(), None, data.clone()),
            node.live[&authority].clone(),
        ],
        6,
        vec![
            programs.pool.witness(&[
                Arg::U64(5),
                Arg::U64(0),
                Arg::U64(0),
                Arg::U128(50),
                Arg::U128(0),
                Arg::U128(0),
                Arg::U128(0),
                Arg::U128(pool_quote.amount_out),
                Arg::Hash([0; 32]),
            ])?,
            programs.asset.witness(&[])?,
            programs.asset.witness(&[])?,
            order_code.witness(&[
                Arg::U64(4),
                Arg::U64(3),
                Arg::U64(3),
                Arg::U128(100),
                Arg::U128(pool_quote.amount_out),
                Arg::Hash(controller.calc_script_hash().unpack()),
                Arg::Hash(assets[1].args().raw_data().as_ref().try_into().context("requested issuer width")?),
            ])?,
            programs.escrow.witness(&[])?,
            settlement.witness(&[Arg::U64(0), Arg::U64(7), Arg::U64(8)])?,
        ],
    )?;
    let unsigned = if node.c4 {
        let input_prefix = unsigned.inputs().len();
        let output_prefix = unsigned.outputs().len();
        let extended = super::operations::append_unrelated(node, &unsigned, &tails, 6)?;
        let permuted = super::operations::permutation_pair(node, programs, extended, input_prefix, output_prefix, 6, "c1")?;
        super::operations::reject_tail_fee(node, &permuted, settlement, "actual-c1-c4-rejects-tail-fee-slack")?;
        rejection_clock.context("C1 rejection clock missing")?.record(node, programs, "c1-c4-late-reject-tail-fee")?;
        permuted
    } else {
        unsigned
    };
    let points = submit(
        node,
        programs,
        &unsigned,
        FinanceIntentParameters {
            operation_id: id("c1-execute", &controller),
            operation: "actual-direct-order-pool-settlement-with-zero-taker-b-input".into(),
            owner: controller.clone(),
            recipient: controller.clone(),
            debits: vec![bound(&assets[0], 60)],
            credits: vec![bound(&assets[0], 40)],
            subjects: vec![
                RecoverySubject::Lock(controller.clone()),
                RecoverySubject::Type(pool.identity.clone()),
                RecoverySubject::Type(order.identity.clone()),
            ],
            maximum_fee_shannons: FEE,
            maximum_capacity_debit_shannons: 2 * CELL_CAPACITY,
            maximum_occupied_capacity_shannons: 100 * CELL_CAPACITY,
        },
        false,
    )?;
    node.records.last_mut().context("missing C1 commitment")?["settlement_flows"] = json!({"pool_input_a":"50","order_sold_a":"100","net_taker_profit_a":"50",
        "pool_b_equals_actual_maker_payment":pool_quote.amount_out.to_string(),"taker_b_input":false,"order_entitlement":order_quote.next.entitlement.to_string(),"pool_identity":prefixed(pool.identity.as_slice()),"order_identity":prefixed(order.identity.as_slice())});
    order.points = points[3..5].to_vec();
    order.quote = order_quote.next;
    order.revision += 1;
    let authority = funding.pop().context("missing C1 unwrap authority")?;
    let unsigned = node.build(
        &[points[6].clone(), points[7].clone(), points[8].clone(), authority],
        vec![protocol::token(programs, node, &assets[0], actor, 250)?, cell(node.owner(actor), None, vec![])],
        1,
        vec![settlement.witness(&[Arg::U64(1), Arg::U64(2), Arg::U64(3)])?, programs.asset.witness(&[])?],
    )?;
    submit(
        node,
        programs,
        &unsigned,
        FinanceIntentParameters {
            operation_id: id("c1-unwrap", &controller),
            operation: "actual-c1-authorizer-unwraps-all-residual-a-and-capacity".into(),
            owner: controller.clone(),
            recipient: node.owner(actor),
            debits: vec![bound(&assets[0], 250)],
            credits: vec![bound(&assets[0], 250)],
            subjects: vec![RecoverySubject::Lock(controller)],
            maximum_fee_shannons: FEE,
            maximum_capacity_debit_shannons: 20 * CELL_CAPACITY,
            maximum_occupied_capacity_shannons: 100 * CELL_CAPACITY,
        },
        false,
    )?;
    clock.record(node, programs, "c1-fund-execute-and-unwrap")?;
    order::cancel(programs, node, &mut order, maker)?;
    node.records.push(json!({"case":"actual-direct-order-pool-settlement-accounting","status":"matched","net_taker_a":"250","initial_taker_a":"200","taker_b_prefunding":false,"executed":"actual-node-send-and-commit"}));
    Ok(())
}
