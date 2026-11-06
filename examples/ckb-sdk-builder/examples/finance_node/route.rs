//! Two distinct pools net the actual intermediate reserves on the real node.

use super::{
    backend::{cell, prefixed, Backend, CELL_CAPACITY, FEE},
    bundle, checked_operation,
    protocol::{self, Pool, Programs, Wallet},
    quote,
    recovery::{AmountEncoding, AssetAmountBound, FinanceIntent, FinanceIntentParameters},
};
use anyhow::{bail, Context, Result};
use cellscript::EntryWitnessArg as Arg;
use cellscript_ckb_adapter::recovery::RecoverySubject;
use ckb_types::{core::TransactionView, packed, prelude::*};
use serde_json::json;

fn bound(asset: &packed::Script, amount: u128) -> AssetAmountBound {
    AssetAmountBound { asset: asset.clone(), encoding: AmountEncoding::U128Le, amount }
}

struct Intent {
    controller: packed::Script,
    data: Vec<u8>,
    points: Vec<packed::OutPoint>,
}

struct Action {
    label: &'static str,
    owner: packed::Script,
    recipient: packed::Script,
    debits: Vec<AssetAmountBound>,
    credits: Vec<AssetAmountBound>,
    subjects: Vec<RecoverySubject>,
    capacity: u64,
    identity: Vec<u8>,
    single_action: bool,
}

fn submit(node: &mut Backend, programs: &Programs, tx: &TransactionView, action: Action) -> Result<Vec<packed::OutPoint>> {
    let id = ckb_hash::blake2b_256([action.label.as_bytes(), action.identity.as_slice()].concat());
    let intent = FinanceIntent::new(FinanceIntentParameters {
        operation_id: id.into(),
        operation: action.label.into(),
        owner: action.owner,
        recipient: action.recipient,
        debits: action.debits,
        credits: action.credits,
        subjects: action.subjects,
        maximum_fee_shannons: FEE,
        maximum_capacity_debit_shannons: action.capacity,
        maximum_occupied_capacity_shannons: 100 * CELL_CAPACITY,
    })?;
    if action.single_action {
        let observed = tx
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
        intent.verify_candidate(tx, &observed)?;
        let points = node.commit(&node.sign(tx)?, action.label)?;
        node.records.last_mut().context("missing single action commitment")?["application_policy"] = json!({
            "candidate_constraints":"checked-against-actual-resolved-inputs-and-packed-outputs",
            "adapter_path":"ordinary-single-action","custom_groups":1,
            "protocol_bundle_evidence":"not-applicable-fewer-than-two-custom-groups","durable_recovery_journal":"not-used-for-this-funding-step"});
        return Ok(points);
    }
    let prepared = bundle::prepare(&node.run_dir.join("artifacts"), &programs.all(), node, tx)?;
    checked_operation(node, programs, &prepared, intent, action.label)
}

fn initialise_pool(
    programs: &Programs,
    node: &mut Backend,
    wallet: &mut Wallet,
    assets: [packed::Script; 2],
    second: bool,
) -> Result<Pool> {
    let (a, b, fee, nonce) = if second { (200_000, 600_000, 50, 102) } else { (100_000, 500_000, 30, 101) };
    let (mut pool, unsigned) = protocol::initialise(programs, node, wallet, assets, fee, nonce, a, b)?;
    let points = submit(
        node,
        programs,
        &unsigned,
        Action {
            label: "initialise-authentic-route-pool",
            owner: node.owner(wallet.key),
            recipient: node.owner(wallet.key),
            debits: vec![bound(&pool.assets[0], a), bound(&pool.assets[1], b)],
            credits: vec![AssetAmountBound {
                asset: pool.lp.clone(),
                amount: pool.state.total_lp - quote::LOCKED_MINIMUM,
                encoding: AmountEncoding::PoolShares {
                    pool: pool.identity.calc_script_hash().unpack(),
                    controller_code: pool.controller.code_hash().unpack(),
                },
            }],
            subjects: vec![RecoverySubject::Lock(pool.controller.clone())],
            capacity: 3 * CELL_CAPACITY + FEE,
            identity: pool.identity.as_slice().to_vec(),
            single_action: false,
        },
    )?;
    pool.points = points[..3].to_vec();
    wallet.a = points[3].clone();
    wallet.b = points[4].clone();
    wallet.lp = Some(points[5].clone());
    Ok(pool)
}

fn prepare(programs: &Programs, node: &mut Backend, pools: &[Pool; 2], user: &mut Wallet, mode: u64) -> Result<Intent> {
    let (maximum, desired) = if mode == 0 { (1000u128, 10000u128) } else { (1200, 12000) };
    let data = programs.route.data(
        "RouteIntent",
        &[
            ("pool_a", pools[0].identity.calc_script_hash().as_slice().to_vec()),
            ("controller_a", pools[0].controller.calc_script_hash().as_slice().to_vec()),
            ("pool_b", pools[1].identity.calc_script_hash().as_slice().to_vec()),
            ("controller_b", pools[1].controller.calc_script_hash().as_slice().to_vec()),
            ("asset_x", pools[0].assets[0].calc_script_hash().as_slice().to_vec()),
            ("asset_y", pools[0].assets[1].calc_script_hash().as_slice().to_vec()),
            ("asset_z", pools[1].assets[1].calc_script_hash().as_slice().to_vec()),
            ("authorizer", node.owner(user.key).calc_script_hash().as_slice().to_vec()),
            ("maximum_input_x", maximum.to_le_bytes().to_vec()),
            ("minimum_output_z", desired.to_le_bytes().to_vec()),
            ("exact_output_z", (if mode == 0 { 0u128 } else { desired }).to_le_bytes().to_vec()),
            ("maximum_fee", FEE.to_le_bytes().to_vec()),
            ("maximum_capacity_debit", (2 * CELL_CAPACITY).to_le_bytes().to_vec()),
            ("nonce", mode.to_le_bytes().to_vec()),
        ],
    )?;
    let controller = programs.route.script(&ckb_hash::blake2b_256(&data));
    let funding = user.funding()?;
    let held = protocol::value(node, &user.a);
    let capacity = cell(controller.clone(), None, vec![]);
    let unsigned = node.build(
        &[user.a.clone(), funding],
        vec![
            cell(
                controller.clone(),
                Some(pools[0].assets[0].clone()),
                programs.asset.data("AssetData", &[("amount", held.to_le_bytes().to_vec())])?,
            ),
            (capacity.0.as_builder().capacity(5 * CELL_CAPACITY).build(), capacity.1),
            cell(controller.clone(), None, data.clone()),
            cell(node.owner(user.key), None, vec![]),
        ],
        3,
        vec![programs.asset.witness(&[])?],
    )?;
    let points = submit(
        node,
        programs,
        &unsigned,
        Action {
            label: "fund-authenticated-x-only-route-intent",
            owner: node.owner(user.key),
            recipient: controller.clone(),
            debits: vec![bound(&pools[0].assets[0], held)],
            credits: vec![bound(&pools[0].assets[0], held)],
            subjects: vec![RecoverySubject::Type(pools[0].assets[0].clone())],
            capacity: 7 * CELL_CAPACITY + FEE,
            identity: controller.as_slice().to_vec(),
            single_action: true,
        },
    )?;
    Ok(Intent { controller, data, points: points[..3].to_vec() })
}

fn execute_route(
    programs: &Programs,
    node: &mut Backend,
    pools: &mut [Pool; 2],
    user: &mut Wallet,
    mode: u64,
    tails: &[packed::OutPoint],
) -> Result<()> {
    let known = [pools[0].points.clone(), pools[1].points.clone(), vec![user.a.clone()]].concat();
    let clock = super::operations::Clock::start(node, programs, &known)?;
    let intent = prepare(programs, node, pools, user, mode)?;
    let rejection_clock = super::operations::Clock::start(node, programs, &pools[0].points)?;
    let legs = pools.each_ref().map(|pool| quote::RouteLeg {
        pool_id: pool.identity.calc_script_hash().unpack(),
        asset_a: pool.assets[0].calc_script_hash().unpack(),
        asset_b: pool.assets[1].calc_script_hash().unpack(),
        state: pool.state,
        direction: quote::Direction::AForB,
    });
    let quoted = if mode == 0 {
        node.measure_quote("two-pool-route-exact-input", || quote::route_exact_in(legs, 1000, 10000))?
    } else {
        node.measure_quote("two-pool-route-exact-output", || quote::route_exact_out(legs, 12000, 1200))?
    };
    let quotations = quoted.legs;
    if quotations[0].amount_out != quotations[1].amount_in {
        bail!("intermediate reserve flows are not equal");
    }
    let spent = quotations[0].amount_in;
    let received = quotations[1].amount_out;
    let authority = user.funding()?;
    let mut outputs = vec![];
    for (index, pool) in pools.iter().enumerate() {
        let next = quotations[index].next;
        outputs.extend([
            cell(
                pool.controller.clone(),
                Some(pool.identity.clone()),
                protocol::pool_data(programs, node, pool, next, pool.revision + 1)?,
            ),
            protocol::reserve(programs, pool, 0, next.reserve_a)?,
            protocol::reserve(programs, pool, 1, next.reserve_b)?,
        ]);
    }
    let amount_data = |amount: u128| programs.asset.data("AssetData", &[("amount", amount.to_le_bytes().to_vec())]);
    outputs.extend([
        cell(
            intent.controller.clone(),
            Some(pools[0].assets[0].clone()),
            amount_data(protocol::value(node, &intent.points[0]) - spent)?,
        ),
        cell(intent.controller.clone(), Some(pools[1].assets[1].clone()), amount_data(received)?),
        cell(intent.controller.clone(), None, vec![]),
        cell(intent.controller.clone(), None, intent.data.clone()),
        node.live[&authority].clone(),
    ]);
    let args = |index: usize| {
        let q = quotations[index];
        programs.pool.witness(&[
            Arg::U64(if mode == 0 { 5 } else { 7 }),
            Arg::U64((index * 3) as u64),
            Arg::U64((index * 3) as u64),
            Arg::U128(q.amount_in),
            Arg::U128(if mode == 0 { 0 } else { q.amount_out }),
            Arg::U128(0),
            Arg::U128(0),
            Arg::U128(if mode == 0 { q.amount_out } else { 0 }),
            Arg::Hash([0; 32]),
        ])
    };
    let inputs = [pools[0].points.clone(), pools[1].points.clone(), intent.points.clone(), vec![authority]].concat();
    let unsigned = node.build(
        &inputs,
        outputs,
        8,
        vec![
            args(0)?,
            programs.asset.witness(&[])?,
            programs.asset.witness(&[])?,
            args(1)?,
            packed::WitnessArgs::default(),
            programs.asset.witness(&[])?,
            programs.route.witness(&[Arg::U64(mode), Arg::U64(8), Arg::U64(9)])?,
        ],
    )?;
    let unsigned = if node.c4 {
        let input_prefix = unsigned.inputs().len();
        let output_prefix = unsigned.outputs().len();
        let extended = super::operations::append_unrelated(node, &unsigned, tails, 8)?;
        super::operations::permutation_pair(
            node,
            programs,
            extended,
            input_prefix,
            output_prefix,
            8,
            if mode == 0 { "c3-exact-input" } else { "c3-exact-output" },
        )?
    } else {
        unsigned
    };
    let mut redirected = unsigned.outputs().into_iter().collect::<Vec<_>>();
    redirected[7] = redirected[7].clone().as_builder().lock(node.owner(node.key_labels[1])).build();
    node.reject(
        &unsigned.as_advanced_builder().set_outputs(redirected).build(),
        &programs.route,
        "actual-route-rejects-redirected-final-z-credit",
        cellscript::runtime_errors::CellScriptRuntimeError::ScriptRoleMismatch.code() as i64,
    )?;
    if node.c4 {
        super::operations::reject_tail_fee(
            node,
            &unsigned,
            &programs.route,
            if mode == 0 {
                "actual-c3-c4-exact-input-rejects-tail-fee-slack"
            } else {
                "actual-c3-c4-exact-output-rejects-tail-fee-slack"
            },
        )?;
    }
    rejection_clock.record(
        node,
        programs,
        if mode == 0 { "c3-exact-input-late-reject-recipient" } else { "c3-exact-output-late-reject-recipient" },
    )?;
    let points = submit(
        node,
        programs,
        &unsigned,
        Action {
            label: if mode == 0 {
                "actual-two-distinct-pool-exact-input-route"
            } else {
                "actual-two-distinct-pool-exact-output-route"
            },
            owner: intent.controller.clone(),
            recipient: intent.controller.clone(),
            debits: vec![bound(&pools[0].assets[0], if mode == 0 { 1000 } else { 1200 })],
            credits: vec![bound(&pools[1].assets[1], if mode == 0 { 10000 } else { 12000 })],
            subjects: vec![
                RecoverySubject::Lock(intent.controller.clone()),
                RecoverySubject::Type(pools[0].identity.clone()),
                RecoverySubject::Type(pools[1].identity.clone()),
            ],
            capacity: 2 * CELL_CAPACITY,
            identity: intent.controller.as_slice().to_vec(),
            single_action: false,
        },
    )?;
    let record = node.records.last_mut().context("missing routed commitment")?;
    record["route_flows"] = json!({"amount_mode":if mode==0{"exact-input"}else{"exact-output"},"spent_x":spent.to_string(),"intermediate_y":quotations[0].amount_out.to_string(),
        "received_z":received.to_string(),"taker_y_input":false,"new_output_used_as_input":false,"pool_identities":[prefixed(pools[0].identity.as_slice()),prefixed(pools[1].identity.as_slice())]});
    for (index, pool) in pools.iter_mut().enumerate() {
        pool.points = points[index * 3..index * 3 + 3].to_vec();
        pool.state = quotations[index].next;
        pool.revision += 1;
    }
    let authority = user.funding()?;
    let x = protocol::value(node, &points[6]);
    let z = protocol::value(node, &points[7]);
    let unsigned = node.build(
        &[points[8].clone(), points[6].clone(), points[7].clone(), points[9].clone(), authority],
        vec![
            protocol::token(programs, node, &pools[0].assets[0], user.key, x)?,
            protocol::token(programs, node, &pools[1].assets[1], user.key, z)?,
            cell(node.owner(user.key), None, vec![]),
        ],
        2,
        vec![
            programs.route.witness(&[Arg::U64(2), Arg::U64(3), Arg::U64(4)])?,
            programs.asset.witness(&[])?,
            programs.asset.witness(&[])?,
        ],
    )?;
    let points = submit(
        node,
        programs,
        &unsigned,
        Action {
            label: "actual-route-authorizer-unwraps-residual-assets-and-capacity",
            owner: intent.controller.clone(),
            recipient: node.owner(user.key),
            debits: vec![bound(&pools[0].assets[0], x), bound(&pools[1].assets[1], z)],
            credits: vec![bound(&pools[0].assets[0], x), bound(&pools[1].assets[1], z)],
            subjects: vec![RecoverySubject::Lock(intent.controller.clone())],
            capacity: 20 * CELL_CAPACITY,
            identity: intent.controller.as_slice().to_vec(),
            single_action: false,
        },
    )?;
    user.a = points[0].clone();
    clock.record(
        node,
        programs,
        if mode == 0 { "c3-exact-input-funded-route-and-unwrap" } else { "c3-exact-output-funded-route-and-unwrap" },
    )?;
    Ok(())
}

pub fn run(programs: &Programs, node: &mut Backend, provider: &mut Wallet, assets: [packed::Script; 2]) -> Result<()> {
    eprintln!("[finance-node] genuine third issuer and separate X-only route authorizer");
    let key = node.key_labels[2];
    let inputs = (0..9).map(|_| provider.funding()).collect::<Result<Vec<_>>>()?;
    let mut outputs = (0..8)
        .map(|_| {
            let cell = cell(node.owner(key), None, vec![]);
            (cell.0.as_builder().capacity(10 * CELL_CAPACITY).build(), cell.1)
        })
        .collect::<Vec<_>>();
    outputs.push(cell(node.owner(provider.key), None, vec![]));
    let unsigned = node.build(&inputs, outputs, 8, vec![])?;
    let points = node.commit(&node.sign(&unsigned)?, "real-signed-route-authorizer-capacity-funding")?;
    let mut user =
        Wallet { key, funding: points[..8].to_vec(), a: packed::OutPoint::default(), b: packed::OutPoint::default(), lp: None };
    let z = programs.asset.script(node.owner(key).calc_script_hash().as_slice());
    let funding = user.funding()?;
    let unsigned = node.build(
        &[funding],
        vec![protocol::token(programs, node, &z, provider.key, 1_000_000)?, cell(node.owner(key), None, vec![])],
        1,
        vec![programs.asset.witness(&[])?],
    )?;
    let z_wallet = node.commit(&node.sign(&unsigned)?, "genuine-third-asset-issued-with-real-distinct-issuer-signature")?[0].clone();
    let first = initialise_pool(programs, node, provider, assets.clone(), false)?;
    let mut second_provider =
        Wallet { key: provider.key, funding: std::mem::take(&mut provider.funding), a: provider.b.clone(), b: z_wallet, lp: None };
    let second = initialise_pool(programs, node, &mut second_provider, [assets[1].clone(), z.clone()], true)?;
    provider.funding = second_provider.funding;
    provider.b = second_provider.a;
    let funding = provider.funding()?;
    let unsigned = node.build(
        &[provider.a.clone(), funding],
        vec![
            protocol::token(programs, node, &assets[0], key, 5000)?,
            protocol::token(programs, node, &assets[0], provider.key, protocol::value(node, &provider.a) - 5000)?,
            cell(node.owner(provider.key), None, vec![]),
        ],
        2,
        vec![programs.asset.witness(&[])?],
    )?;
    let points = node.commit(&node.sign(&unsigned)?, "distinct-route-user-receives-only-x-and-no-y-or-z")?;
    user.a = points[0].clone();
    provider.a = points[1].clone();
    if node
        .live
        .values()
        .any(|(output, _)| output.lock() == node.owner(key) && output.type_().to_opt().is_some_and(|s| s == assets[1] || s == z))
    {
        bail!("route authorizer unexpectedly owns Y or Z before routing");
    }
    let mut pools = [first, second];
    for mode in 0..=1 {
        eprintln!("[finance-node] real two-pool route amount mode {mode}");
        let tails = if node.c4 {
            super::operations::fund_unrelated(node, provider, if mode == 0 { "c3-exact-input" } else { "c3-exact-output" })?
        } else {
            vec![]
        };
        execute_route(programs, node, &mut pools, &mut user, mode, &tails)?;
    }
    let total = node
        .live
        .values()
        .filter(|(output, _)| output.type_().to_opt().as_ref() == Some(&z))
        .map(|(_, data)| u128::from_le_bytes(data[..16].try_into().unwrap()))
        .sum::<u128>();
    if total != 1_000_000 {
        bail!("third asset supply changed");
    }
    node.records.push(json!({"case":"actual-pure-two-pool-route-accounting","status":"matched","third_asset_total":total.to_string(),"distinct_full_asset_identities":[prefixed(assets[0].as_slice()),prefixed(assets[1].as_slice()),prefixed(z.as_slice())],"route_modes":["exact-input","exact-output"],"financial_execution":"actual-node-send-and-commit"}));
    Ok(())
}
