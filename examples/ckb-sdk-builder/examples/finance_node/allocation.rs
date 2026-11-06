//! Actual shared-output allocation and authenticated recovery of a stale plan.

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

struct Credit {
    order: [u8; 32],
    output: u64,
    amount: u128,
}
struct Plan {
    credits: Vec<Credit>,
    change: u64,
}
struct Intent {
    points: Vec<packed::OutPoint>,
    script: packed::Script,
    ordinal: u64,
}
fn bound(asset: &packed::Script, amount: u128) -> AssetAmountBound {
    AssetAmountBound { asset: asset.clone(), encoding: AmountEncoding::U128Le, amount }
}
fn identifier(label: &str, script: &packed::Script, ordinal: u64) -> ckb_types::H256 {
    ckb_hash::blake2b_256([label.as_bytes(), script.as_slice(), &ordinal.to_le_bytes()].concat()).into()
}

fn table(programs: &Programs, node: &Backend, orders: &[order::Order; 2], buyer: u8, plan: &Plan) -> Result<Vec<u8>> {
    let program = programs.allocation.as_ref().context("C2 allocation is not selected")?;
    let order_code = programs.order.as_ref().context("C2 order is not selected")?;
    let mut fields = vec![
        ("authorizer".into(), node.owner(buyer).calc_script_hash().as_slice().to_vec()),
        ("allowed_order_code".into(), packed::CellOutput::calc_data_hash(order_code.code()).as_slice().to_vec()),
        ("network".into(), node.network_domain.to_vec()),
        ("auth_input".into(), 8u64.to_le_bytes().to_vec()),
        ("change_output".into(), plan.change.to_le_bytes().to_vec()),
        ("order_count".into(), 2u64.to_le_bytes().to_vec()),
        ("credit_count".into(), (plan.credits.len() as u64).to_le_bytes().to_vec()),
    ];
    for (index, order) in orders.iter().enumerate() {
        fields.push((format!("order{index}"), order.identity.calc_script_hash().as_slice().to_vec()));
    }
    for row in 0..4 {
        let credit = plan.credits.get(row);
        fields.push((format!("credit{row}_order"), credit.map_or([0; 32], |c| c.order).to_vec()));
        fields.push((format!("credit{row}_output"), credit.map_or(0, |c| c.output).to_le_bytes().to_vec()));
        fields.push((format!("credit{row}_amount"), credit.map_or(0, |c| c.amount).to_le_bytes().to_vec()));
    }
    let fields = fields.iter().map(|(name, value)| (name.as_str(), value.clone())).collect::<Vec<_>>();
    let data = program.data("AllocationData", &fields)?;
    if data.len() != 416 {
        bail!("C2 table is not the fixed 416-byte profile");
    }
    Ok(data)
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
        node.records.last_mut().context("missing C2 funding")?["application_policy"] = json!({"adapter_path":"ordinary-single-action","custom_groups":1,"protocol_bundle_evidence":"not-applicable","durable_recovery_journal":"not-used-for-this-funding-step","candidate_constraints":"actual-resolved-inputs-and-packed-outputs"});
        Ok(points)
    } else {
        let prepared = bundle::prepare(&node.run_dir.join("artifacts"), &programs.all(), node, tx)?;
        checked_operation(node, programs, &prepared, intent, &label)
    }
}

fn fund(
    programs: &Programs,
    node: &mut Backend,
    buyer: &mut Wallet,
    orders: &[order::Order; 2],
    plan: &Plan,
    ordinal: u64,
) -> Result<Intent> {
    let program = programs.allocation.as_ref().context("missing allocation")?;
    let requested = &orders[0].assets[1];
    let data = table(programs, node, orders, buyer.key, plan)?;
    let script = program.script(&ckb_hash::blake2b_256(&data));
    let funding = buyer.funding()?;
    let unsigned = node.build(
        &[buyer.b.clone(), funding],
        vec![
            cell(script.clone(), None, data),
            cell(script.clone(), None, vec![]),
            cell(script.clone(), None, vec![]),
            cell(
                script.clone(),
                Some(requested.clone()),
                programs.asset.data("AssetData", &[("amount", 50u128.to_le_bytes().to_vec())])?,
            ),
            protocol::token(
                programs,
                node,
                requested,
                buyer.key,
                protocol::value(node, &buyer.b).checked_sub(50).context("buyer lacks C2 payment asset")?,
            )?,
            cell(node.owner(buyer.key), None, vec![]),
        ],
        5,
        vec![programs.asset.witness(&[])?],
    )?;
    let points = submit(
        node,
        programs,
        &unsigned,
        FinanceIntentParameters {
            operation_id: identifier("c2-fund", &script, ordinal),
            operation: "actual-buyer-funds-authenticated-allocation-plan".into(),
            owner: node.owner(buyer.key),
            recipient: script.clone(),
            debits: vec![bound(requested, 50)],
            credits: vec![bound(requested, 50)],
            subjects: vec![RecoverySubject::Type(requested.clone())],
            maximum_fee_shannons: FEE,
            maximum_capacity_debit_shannons: 5 * CELL_CAPACITY + FEE,
            maximum_occupied_capacity_shannons: 100 * CELL_CAPACITY,
        },
        true,
    )?;
    buyer.b = points[4].clone();
    Ok(Intent { points: points[..4].to_vec(), script, ordinal })
}

struct Candidate {
    tx: TransactionView,
    quotes: [quote::FillQuote; 2],
}
fn candidate(
    programs: &Programs,
    node: &Backend,
    orders: &[order::Order; 2],
    intent: &Intent,
    buyer: u8,
    authority: packed::OutPoint,
    payments: &[u128],
    padding: &[packed::OutPoint],
) -> Result<Candidate> {
    let allocation = programs.allocation.as_ref().context("missing allocation")?;
    let code = programs.order.as_ref().context("missing order")?;
    let quotes = [
        node.measure_quote("allocation-first-order-fill", || quote::fill_order(orders[0].quote, 2, 5))?,
        node.measure_quote("allocation-second-order-fill", || quote::fill_order(orders[1].quote, 3, 7))?,
    ];
    let mut inputs = vec![
        intent.points[0].clone(),
        orders[0].points[0].clone(),
        orders[0].points[1].clone(),
        intent.points[1].clone(),
        orders[1].points[0].clone(),
        orders[1].points[1].clone(),
        intent.points[2].clone(),
        intent.points[3].clone(),
        authority,
    ];
    let mut outputs = vec![];
    for (index, order) in orders.iter().enumerate() {
        outputs.extend([
            cell(
                order.controller.clone(),
                Some(order.identity.clone()),
                order::data(programs, node, order, quotes[index].next, order.revision + 1)?,
            ),
            order::escrow(programs, order, quotes[index].remaining)?,
        ]);
    }
    for payment in payments {
        outputs.push(protocol::token(programs, node, &orders[0].assets[1], orders[0].maker, *payment)?);
    }
    outputs.extend([
        protocol::token(programs, node, &orders[0].assets[0], buyer, 5)?,
        protocol::token(programs, node, &orders[0].assets[1], buyer, 50 - payments.iter().sum::<u128>())?,
    ]);
    let change = outputs.len();
    outputs.push(cell(node.owner(buyer), None, vec![]));
    for point in padding {
        inputs.push(point.clone());
        outputs.push(node.live[point].clone());
    }
    let order_witness = |index: usize| {
        code.witness(&[
            Arg::U64(6),
            Arg::U64(if index == 0 { 1 } else { 4 }),
            Arg::U64((index * 2) as u64),
            Arg::U128(if index == 0 { 2 } else { 3 }),
            Arg::U128(if index == 0 { 5 } else { 7 }),
            Arg::Hash(intent.script.calc_script_hash().unpack()),
            Arg::Hash(orders[index].assets[1].args().raw_data().as_ref().try_into().context("requested issuer width")?),
        ])
    };
    let tx = node.build(
        &inputs,
        outputs,
        change,
        vec![
            allocation.witness(&[Arg::U64(0), Arg::U64(8)])?,
            order_witness(0)?,
            programs.escrow.witness(&[])?,
            packed::WitnessArgs::default(),
            order_witness(1)?,
            programs.escrow.witness(&[])?,
            packed::WitnessArgs::default(),
            programs.asset.witness(&[])?,
            packed::WitnessArgs::default(),
        ],
    )?;
    Ok(Candidate { tx, quotes })
}

fn execute(
    programs: &Programs,
    node: &mut Backend,
    orders: &mut [order::Order; 2],
    intent: &Intent,
    buyer: &Wallet,
    candidate: Candidate,
    maximum: bool,
) -> Result<()> {
    let points = submit(
        node,
        programs,
        &candidate.tx,
        FinanceIntentParameters {
            operation_id: identifier("c2-execute", &intent.script, intent.ordinal),
            operation: if maximum {
                "actual-four-row-allocation-two-shared-payments-at-32-cells"
            } else {
                "actual-two-orders-share-one-aggregate-maker-payment"
            }
            .into(),
            owner: intent.script.clone(),
            recipient: node.owner(buyer.key),
            debits: vec![bound(&orders[0].assets[1], 50)],
            credits: vec![bound(&orders[0].assets[0], 5), bound(&orders[0].assets[1], 38)],
            subjects: vec![
                RecoverySubject::Lock(intent.script.clone()),
                RecoverySubject::Type(orders[0].identity.clone()),
                RecoverySubject::Type(orders[1].identity.clone()),
            ],
            maximum_fee_shannons: FEE,
            maximum_capacity_debit_shannons: 20 * CELL_CAPACITY,
            maximum_occupied_capacity_shannons: 100 * CELL_CAPACITY,
        },
        false,
    )?;
    for (index, order) in orders.iter_mut().enumerate() {
        order.points = points[index * 2..index * 2 + 2].to_vec();
        order.quote = candidate.quotes[index].next;
        order.revision += 1;
    }
    node.records.last_mut().context("missing C2 commit")?["allocation_flows"] = json!({"orders":2,"rows":if maximum{4}else{2},"actual_shared_payment_amounts":if maximum{vec![6,6]}else{vec![12]},
        "per_order_credits":["5","7"],"inputs":candidate.tx.inputs().len(),"outputs":candidate.tx.outputs().len(),"aggregate_credit_not_reused":true});
    Ok(())
}

fn cancel(programs: &Programs, node: &mut Backend, intent: &Intent, buyer: &mut Wallet) -> Result<()> {
    let allocation = programs.allocation.as_ref().context("missing allocation")?;
    let before = intent.points.iter().map(|point| node.live[point].clone()).collect::<Vec<_>>();
    let mut inputs = intent.points.clone();
    inputs.push(buyer.funding()?);
    let mut outputs = before
        .iter()
        .map(|(output, data)| (output.clone().as_builder().lock(node.owner(buyer.key)).build(), data.clone()))
        .collect::<Vec<_>>();
    outputs.push(cell(node.owner(buyer.key), None, vec![]));
    let unsigned = node.build(
        &inputs,
        outputs,
        4,
        vec![
            allocation.witness(&[Arg::U64(1), Arg::U64(4)])?,
            packed::WitnessArgs::default(),
            packed::WitnessArgs::default(),
            programs.asset.witness(&[])?,
        ],
    )?;
    let requested = before[3].0.type_().to_opt().context("allocation has no requested asset")?;
    let points = submit(
        node,
        programs,
        &unsigned,
        FinanceIntentParameters {
            operation_id: identifier("c2-cancel-stale", &intent.script, intent.ordinal),
            operation: "actual-stale-allocation-authorizer-refunds-every-original-cell".into(),
            owner: intent.script.clone(),
            recipient: node.owner(buyer.key),
            debits: vec![bound(&requested, 50)],
            credits: vec![bound(&requested, 50)],
            subjects: vec![RecoverySubject::Lock(intent.script.clone())],
            maximum_fee_shannons: FEE,
            maximum_capacity_debit_shannons: 20 * CELL_CAPACITY,
            maximum_occupied_capacity_shannons: 100 * CELL_CAPACITY,
        },
        false,
    )?;
    for (index, (output, data)) in before.iter().enumerate() {
        let (actual, bytes) = &node.live[&points[index]];
        if actual.capacity() != output.capacity()
            || actual.type_() != output.type_()
            || bytes != data
            || actual.lock() != node.owner(buyer.key)
        {
            bail!("stale allocation refund changed original type/data/capacity");
        }
    }
    node.records.last_mut().context("missing C2 refund")?["whole_cell_refund"] =
        json!({"count":4,"type_data_capacity_preserved":true,"recipient":prefixed(node.owner(buyer.key).as_slice())});
    Ok(())
}

pub fn run(
    programs: &Programs,
    node: &mut Backend,
    buyer: &mut Wallet,
    maker: &mut Wallet,
    assets: [packed::Script; 2],
) -> Result<()> {
    eprintln!("[finance-node] authentic shared-output allocation for two orders");
    let program = programs.allocation.as_ref().context("complete lane needs allocation")?;
    let code = cell(node.owner(buyer.key), None, program.code().to_vec());
    let needed = code.0.occupied_capacity(Capacity::bytes(code.1.len())?)?.as_u64() + CELL_CAPACITY;
    let mut inputs = vec![];
    let mut total = 0u64;
    while total < needed + CELL_CAPACITY + FEE {
        let point = buyer.funding()?;
        total += u64::from(node.live[&point].0.capacity());
        inputs.push(point);
    }
    let unsigned = node.build(
        &inputs,
        vec![(code.0.as_builder().capacity(needed).build(), code.1), cell(node.owner(buyer.key), None, vec![])],
        1,
        vec![],
    )?;
    let code = node.commit(&node.sign(&unsigned)?, "signed-deployment-of-ordinary-allocation-verifier")?[0].clone();
    node.code_points.insert(program.name.clone(), code.clone());
    node.deps.push(packed::CellDep::new_builder().out_point(code).build());
    let first = order::initialise(programs, node, maker, assets.clone(), 301, 10)?;
    let second = order::initialise(programs, node, maker, assets, 302, 10)?;
    let mut orders = [first, second];
    let ids = orders.each_ref().map(|o| o.identity.calc_script_hash().unpack());
    let plan = Plan {
        credits: vec![Credit { order: ids[0], output: 4, amount: 5 }, Credit { order: ids[1], output: 4, amount: 7 }],
        change: 7,
    };
    let current = fund(programs, node, buyer, &orders, &plan, 1)?;
    let stale = fund(programs, node, buyer, &orders, &plan, 2)?;
    let authority = buyer.funding()?;
    let old = candidate(programs, node, &orders, &stale, buyer.key, authority, &[12], &[])?;
    let signed_stale = node.sign(&old.tx)?;
    let authority = buyer.funding()?;
    let rejection_clock = super::operations::Clock::start(node, programs, &orders[0].points)?;
    let underpaid = candidate(programs, node, &orders, &current, buyer.key, authority.clone(), &[7], &[])?;
    node.reject_one_of(
        &underpaid.tx,
        &[program, programs.order.as_ref().context("missing order verifier")?],
        "actual-allocation-rejects-double-credit-of-underfunded-shared-payment",
        cellscript::runtime_errors::CellScriptRuntimeError::AssertionFailed.code() as i64,
    )?;
    rejection_clock.record(node, programs, "c2-late-reject-shared-underpayment")?;
    let contention_start = node.elapsed_ms();
    let preflight = node.node.rpc(
        "test_tx_pool_accept",
        vec![serde_json::to_value(cellscript_ckb_adapter::to_rpc_transaction(&signed_stale))?, json!("passthrough")],
    )?;
    let valid_clock = super::operations::Clock::start(node, programs, &orders[0].points)?;
    let valid = candidate(programs, node, &orders, &current, buyer.key, authority, &[12], &[])?;
    execute(programs, node, &mut orders, &current, buyer, valid, false)?;
    valid_clock.record(node, programs, "c2-two-rows-shared-payment-execute")?;
    let stale_error = node
        .node
        .rpc(
            "test_tx_pool_accept",
            vec![serde_json::to_value(cellscript_ckb_adapter::to_rpc_transaction(&signed_stale))?, json!("passthrough")],
        )
        .expect_err("consumed order predecessors cannot execute again");
    let stale_rpc_error = Backend::require_stale_input(&stale_error)?;
    let stale_submission = node.submit_stale_original(&signed_stale)?;
    let contention_window = node.contention_window(contention_start, "serialized-allocation-plans-share-order-predecessors");
    node.records.push(json!({"case":"actual-competing-allocation-plan-is-stale-after-shared-fill","status":"conflict-observed","presend_acceptance":preflight,
        "after_competitor_error":stale_error.to_string(),"classification":"stale-input-not-financial-script-rejection","new_economic_retry_executed":false,
        "stale_rpc_error":stale_rpc_error,"stale_submission":stale_submission,"contention_window":contention_window}));
    cancel(programs, node, &stale, buyer)?;
    let four = Plan {
        credits: vec![
            Credit { order: ids[0], output: 4, amount: 2 },
            Credit { order: ids[0], output: 5, amount: 3 },
            Credit { order: ids[1], output: 4, amount: 4 },
            Credit { order: ids[1], output: 5, amount: 3 },
        ],
        change: 8,
    };
    let maximum = fund(programs, node, buyer, &orders, &four, 3)?;
    let padding_sources = (0..3).map(|_| buyer.funding()).collect::<Result<Vec<_>>>()?;
    let mut padding_outputs = (0..23)
        .map(|index| {
            cell(
                node.owner(if node.c4 { node.key_labels[3] } else { buyer.key }),
                None,
                if node.c4 { format!("c4-c2-third-party-tail-{index}").into_bytes() } else { vec![] },
            )
        })
        .collect::<Vec<_>>();
    padding_outputs.push(cell(node.owner(buyer.key), None, vec![]));
    let unsigned = node.build(&padding_sources, padding_outputs, 23, vec![])?;
    let padding =
        node.commit(&node.sign(&unsigned)?, "signed-unrelated-capacity-cells-for-32-cell-allocation-boundary")?[..23].to_vec();
    let authority = buyer.funding()?;
    let maximum_clock = super::operations::Clock::start(node, programs, &orders[0].points)?;
    let mut valid = candidate(programs, node, &orders, &maximum, buyer.key, authority, &[6, 6], &padding)?;
    if node.c4 {
        valid.tx = super::operations::permutation_pair(node, programs, valid.tx, 9, 9, 8, "c2-maximum-32")?;
    }
    if valid.tx.inputs().len() != 32 || valid.tx.outputs().len() != 32 {
        bail!("C2 maximum candidate is not 32 inputs and outputs");
    }
    execute(programs, node, &mut orders, &maximum, buyer, valid, true)?;
    maximum_clock.record(node, programs, "c2-c4-four-rows-32-input-output-execute")?;
    for order in &mut orders {
        order::cancel(programs, node, order, maker)?;
    }
    node.records.push(json!({"case":"actual-shared-output-allocation-accounting","status":"matched","first_cumulative_entitlement":"10","second_cumulative_entitlement":"14",
        "two_rows_one_shared_payment":12,"four_rows_two_shared_payments":[6,6],"maximum_inputs":32,"maximum_outputs":32,"stale_plan_whole_cell_refund":true}));
    Ok(())
}
