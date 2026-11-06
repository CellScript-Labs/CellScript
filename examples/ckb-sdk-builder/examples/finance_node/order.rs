//! Ordinary escrowed orders, signed by actual makers and takers on the node.

use super::{
    backend::{cell, prefixed, Backend, Cell, CELL_CAPACITY, FEE},
    bundle, checked_operation, protocol,
    protocol::{Programs, Wallet},
    quote,
    recovery::{AmountEncoding, AssetAmountBound, FinanceIntent, FinanceIntentParameters},
};
use anyhow::{bail, Context, Result};
use cellscript::EntryWitnessArg as Arg;
use cellscript_ckb_adapter::recovery::RecoverySubject;
use ckb_types::{bytes::Bytes, core::TransactionView, packed, prelude::*};
use serde_json::json;

pub(super) struct Order {
    pub(super) identity: packed::Script,
    pub(super) controller: packed::Script,
    pub(super) assets: [packed::Script; 2],
    pub(super) maker: u8,
    nonce: u64,
    pub(super) quote: quote::OrderState,
    pub(super) revision: u64,
    pub(super) points: Vec<packed::OutPoint>,
}

pub(super) fn data(programs: &Programs, node: &Backend, order: &Order, state: quote::OrderState, revision: u64) -> Result<Vec<u8>> {
    let maker = node.owner(order.maker).calc_script_hash().as_slice().to_vec();
    programs.order.as_ref().context("order lane is not enabled")?.data(
        "OrderState",
        &[
            ("order", order.identity.calc_script_hash().as_slice().to_vec()),
            ("sold_asset", order.assets[0].calc_script_hash().as_slice().to_vec()),
            ("requested_asset", order.assets[1].calc_script_hash().as_slice().to_vec()),
            ("maker", maker.clone()),
            ("proceeds", maker.clone()),
            ("remainder", maker),
            ("network", node.network_domain.to_vec()),
            ("numerator", state.numerator.to_le_bytes().to_vec()),
            ("denominator", state.denominator.to_le_bytes().to_vec()),
            ("total_sold", state.total.to_le_bytes().to_vec()),
            ("nonce", order.nonce.to_le_bytes().to_vec()),
            ("filled", state.filled.to_le_bytes().to_vec()),
            ("entitlement", state.entitlement.to_le_bytes().to_vec()),
            ("revision", revision.to_le_bytes().to_vec()),
        ],
    )
}

pub(super) fn escrow(programs: &Programs, order: &Order, amount: u128) -> Result<Cell> {
    let data = programs.escrow.data(
        "EscrowData",
        &[
            ("amount", amount.to_le_bytes().to_vec()),
            ("order", order.identity.calc_script_hash().as_slice().to_vec()),
            ("controller", order.controller.calc_script_hash().as_slice().to_vec()),
            ("asset", order.assets[0].calc_script_hash().as_slice().to_vec()),
        ],
    )?;
    Ok(cell(programs.escrow.script(&ckb_hash::blake2b_256(&data[16..])), Some(order.assets[0].clone()), data))
}

fn witness(
    programs: &Programs,
    node: &Backend,
    order: &Order,
    mode: u64,
    sold: u128,
    paid: u128,
    actor: u8,
) -> Result<packed::WitnessArgs> {
    programs.order.as_ref().context("order lane is not enabled")?.witness(&[
        Arg::U64(mode),
        Arg::U64(if mode == 0 { 1 } else { 0 }),
        Arg::U64(0),
        Arg::U128(sold),
        Arg::U128(paid),
        Arg::Hash(node.owner(actor).calc_script_hash().unpack()),
        Arg::Hash(order.assets[1].args().raw_data().as_ref().try_into().context("requested asset issuer width")?),
    ])
}

fn bound(asset: &packed::Script, amount: u128) -> AssetAmountBound {
    AssetAmountBound { asset: asset.clone(), encoding: AmountEncoding::U128Le, amount }
}

struct Limits {
    owner: u8,
    debits: Vec<AssetAmountBound>,
    credits: Vec<AssetAmountBound>,
}

fn intent(node: &Backend, order: &Order, label: &str, limits: Limits) -> Result<FinanceIntent> {
    let identity = order.identity.calc_script_hash();
    let id = ckb_hash::blake2b_256([label.as_bytes(), identity.as_slice(), &order.revision.to_le_bytes()].concat());
    let mut subjects = vec![RecoverySubject::Lock(order.controller.clone())];
    if !order.points.is_empty() {
        subjects.push(RecoverySubject::Type(order.identity.clone()));
    }
    FinanceIntent::new(FinanceIntentParameters {
        operation_id: id.into(),
        operation: label.into(),
        owner: node.owner(limits.owner),
        recipient: node.owner(limits.owner),
        debits: limits.debits,
        credits: limits.credits,
        subjects,
        maximum_fee_shannons: FEE,
        maximum_capacity_debit_shannons: 2 * CELL_CAPACITY + FEE,
        maximum_occupied_capacity_shannons: 100 * CELL_CAPACITY,
    })
}

fn submit(
    node: &mut Backend,
    programs: &Programs,
    tx: &TransactionView,
    intent: FinanceIntent,
    label: &str,
) -> Result<Vec<packed::OutPoint>> {
    let prepared = bundle::prepare(&node.run_dir.join("artifacts"), &programs.all(), node, tx)?;
    checked_operation(node, programs, &prepared, intent, label)
}

pub(super) fn initialise(
    programs: &Programs,
    node: &mut Backend,
    maker: &mut Wallet,
    assets: [packed::Script; 2],
    nonce: u64,
    total: u128,
) -> Result<Order> {
    let clock = super::operations::Clock::start(node, programs, std::slice::from_ref(&maker.a))?;
    let funding = maker.funding()?;
    let first = packed::CellInput::new_builder().previous_output(funding.clone()).build();
    let anchor = ckb_hash::blake2b_256([first.as_slice(), &0u64.to_le_bytes()].concat());
    let identity = packed::Script::new_builder()
        .code_hash(cellscript::ckb_abi::type_id::CODE_HASH)
        .hash_type(1u8)
        .args(Bytes::copy_from_slice(&anchor).pack())
        .build();
    let mut order = Order {
        identity,
        controller: programs.order.as_ref().context("order lane is not enabled")?.script(&[]),
        assets,
        maker: maker.key,
        nonce,
        quote: quote::OrderState { total, numerator: 7, denominator: 3, filled: 0, entitlement: 0 },
        revision: 0,
        points: vec![],
    };
    let bytes = data(programs, node, &order, order.quote, 0)?;
    order.controller = programs.order.as_ref().context("order lane is not enabled")?.script(&ckb_hash::blake2b_256(&bytes[..280]));
    let bootstrap_funding = maker.funding()?;
    let unsigned = node.build(
        &[bootstrap_funding],
        vec![cell(order.controller.clone(), None, bytes.clone()), cell(node.owner(maker.key), None, vec![])],
        1,
        vec![],
    )?;
    let bootstrap = node.commit(&node.sign(&unsigned)?, "signed-maker-creates-authenticated-order-bootstrap")?[0].clone();
    let before = protocol::value(node, &maker.a);
    let unsigned = node.build(
        &[funding, bootstrap, maker.a.clone()],
        vec![
            cell(order.controller.clone(), Some(order.identity.clone()), bytes),
            escrow(programs, &order, total)?,
            protocol::token(
                programs,
                node,
                &order.assets[0],
                maker.key,
                before.checked_sub(total).context("maker lacks sold asset")?,
            )?,
            cell(node.owner(maker.key), None, vec![]),
        ],
        3,
        vec![packed::WitnessArgs::default(), witness(programs, node, &order, 0, 0, 0, maker.key)?, programs.asset.witness(&[])?],
    )?;
    let limits = Limits { owner: maker.key, debits: vec![bound(&order.assets[0], total)], credits: vec![] };
    let authorization = intent(node, &order, "initialise-authentic-escrow-order", limits)?;
    let points = submit(node, programs, &unsigned, authorization, "actual-maker-initialises-enforcing-escrow-order")?;
    order.points = points[..2].to_vec();
    maker.a = points[2].clone();
    node.records.last_mut().context("missing order init")?["order_identity"] = json!(prefixed(order.identity.as_slice()));
    node.records.last_mut().context("missing order init")?["maker_lock"] = json!(prefixed(node.owner(maker.key).as_slice()));
    clock.record(node, programs, "order-initialise")?;
    Ok(order)
}

pub(super) fn build_fill(
    programs: &Programs,
    node: &Backend,
    order: &Order,
    taker: &mut Wallet,
    sold: u128,
    paid: u128,
) -> Result<(TransactionView, FinanceIntent, quote::FillQuote)> {
    let quoted = node.measure_quote("order-fill", || quote::fill_order(order.quote, sold, paid))?;
    let funding = taker.funding()?;
    let available = protocol::value(node, &taker.b);
    let remainder = available.checked_sub(paid).context("taker lacks requested asset")?;
    let mut outputs = if quoted.complete {
        vec![]
    } else {
        vec![
            cell(
                order.controller.clone(),
                Some(order.identity.clone()),
                data(programs, node, order, quoted.next, order.revision + 1)?,
            ),
            escrow(programs, order, quoted.remaining)?,
        ]
    };
    outputs.extend([
        protocol::token(programs, node, &order.assets[0], taker.key, sold)?,
        protocol::token(programs, node, &order.assets[1], order.maker, paid)?,
        protocol::token(programs, node, &order.assets[1], taker.key, remainder)?,
    ]);
    if quoted.complete {
        let refund = cell(node.owner(order.maker), None, vec![]);
        outputs.push((refund.0.as_builder().capacity(2 * CELL_CAPACITY).build(), refund.1));
    }
    outputs.push(cell(node.owner(taker.key), None, vec![]));
    let change = outputs.len() - 1;
    let unsigned = node.build(
        &[order.points[0].clone(), order.points[1].clone(), funding, taker.b.clone()],
        outputs,
        change,
        vec![
            witness(programs, node, order, if quoted.complete { 2 } else { 1 }, sold, paid, taker.key)?,
            programs.escrow.witness(&[])?,
            packed::WitnessArgs::default(),
            programs.asset.witness(&[])?,
        ],
    )?;
    let authorization = intent(
        node,
        order,
        "fill-authentic-escrow-order",
        Limits { owner: taker.key, debits: vec![bound(&order.assets[1], paid)], credits: vec![bound(&order.assets[0], sold)] },
    )?;
    Ok((unsigned, authorization, quoted))
}

pub(super) fn fill(
    programs: &Programs,
    node: &mut Backend,
    order: &mut Order,
    taker: &mut Wallet,
    sold: u128,
    paid: u128,
) -> Result<()> {
    let clock = super::operations::Clock::start(node, programs, &order.points)?;
    let (unsigned, authorization, quoted) = build_fill(programs, node, order, taker, sold, paid)?;
    let points = submit(
        node,
        programs,
        &unsigned,
        authorization,
        if quoted.complete {
            "actual-order-final-fill-and-exact-maker-capacity-refund"
        } else {
            "actual-order-partial-fill-with-cumulative-entitlement"
        },
    )?;
    let base = if quoted.complete { 0 } else { 2 };
    taker.b = points[base + 2].clone();
    if !quoted.complete {
        order.points = points[..2].to_vec();
    } else {
        order.points.clear();
    }
    order.quote = quoted.next;
    order.revision += 1;
    let record = node.records.last_mut().context("missing order fill")?;
    record["cumulative_fill"] = json!({"sold":sold.to_string(),"paid":paid.to_string(),"payment_due":quoted.payment_due.to_string(),"donation":quoted.donation.to_string(),
        "filled":quoted.next.filled.to_string(),"entitlement":quoted.next.entitlement.to_string(),"remaining":quoted.remaining.to_string(),"complete":quoted.complete});
    if quoted.complete {
        record["maker_capacity_refund_shannons"] = json!(2 * CELL_CAPACITY);
    }
    clock.record(node, programs, if quoted.complete { "order-final-fill" } else { "order-partial-fill" })?;
    Ok(())
}

pub(super) fn build_cancel(
    programs: &Programs,
    node: &Backend,
    order: &Order,
    maker: &mut Wallet,
) -> Result<(TransactionView, FinanceIntent)> {
    let funding = maker.funding()?;
    let remaining = order.quote.total - order.quote.filled;
    let unsigned = node.build(
        &[order.points[0].clone(), order.points[1].clone(), funding],
        vec![protocol::token(programs, node, &order.assets[0], maker.key, remaining)?, cell(node.owner(maker.key), None, vec![])],
        1,
        vec![witness(programs, node, order, 3, 0, 0, maker.key)?, programs.escrow.witness(&[])?],
    )?;
    let authorization = intent(
        node,
        order,
        "maker-cancels-authentic-escrow-order",
        Limits { owner: maker.key, debits: vec![], credits: vec![bound(&order.assets[0], remaining)] },
    )?;
    Ok((unsigned, authorization))
}

pub(super) fn cancel(programs: &Programs, node: &mut Backend, order: &mut Order, maker: &mut Wallet) -> Result<()> {
    let clock = super::operations::Clock::start(node, programs, &order.points)?;
    let remaining = order.quote.total - order.quote.filled;
    let (unsigned, authorization) = build_cancel(programs, node, order, maker)?;
    submit(node, programs, &unsigned, authorization, "second-maker-cancels-partially-filled-order-and-recovers-capacity")?;
    order.points.clear();
    let record = node.records.last_mut().context("missing cancellation")?;
    record["remaining_sold_asset_returned"] = json!(remaining.to_string());
    record["maker_capacity_refund_at_least_shannons"] = json!(CELL_CAPACITY);
    clock.record(node, programs, "order-cancel")?;
    Ok(())
}

pub fn run(
    programs: &Programs,
    node: &mut Backend,
    primary: &mut Wallet,
    secondary: &mut Wallet,
    assets: [packed::Script; 2],
) -> Result<()> {
    eprintln!("[finance-node] transferring genuine A/B inventory to a distinct signed taker/maker");
    let funding = primary.funding()?;
    let unsigned = node.build(
        &[funding, primary.a.clone(), primary.b.clone()],
        vec![
            protocol::token(programs, node, &assets[0], secondary.key, 100)?,
            protocol::token(programs, node, &assets[1], secondary.key, 1000)?,
            protocol::token(programs, node, &assets[0], primary.key, protocol::value(node, &primary.a) - 100)?,
            protocol::token(programs, node, &assets[1], primary.key, protocol::value(node, &primary.b) - 1000)?,
            cell(node.owner(primary.key), None, vec![]),
        ],
        4,
        vec![packed::WitnessArgs::default(), programs.asset.witness(&[])?, programs.asset.witness(&[])?],
    )?;
    let points = node.commit(&node.sign(&unsigned)?, "upstream-signed-asset-inventory-transfer-to-distinct-order-maker")?;
    secondary.a = points[0].clone();
    secondary.b = points[1].clone();
    primary.a = points[2].clone();
    primary.b = points[3].clone();
    eprintln!("[finance-node] first maker: partial, cumulative-rounding partial, final fill");
    let mut first = initialise(programs, node, primary, assets.clone(), 1, 10)?;
    for (sold, paid) in [(1, 4), (1, 2), (8, 19)] {
        fill(programs, node, &mut first, secondary, sold, paid)?;
    }
    if first.quote.entitlement != 24 || first.quote.filled != 10 || !first.points.is_empty() {
        bail!("cumulative order settlement mismatch");
    }
    eprintln!("[finance-node] different maker: partial fill then authenticated cancellation");
    let mut second = initialise(programs, node, secondary, assets, 2, 11)?;
    fill(programs, node, &mut second, primary, 3, 7)?;
    cancel(programs, node, &mut second, secondary)?;
    node.records.push(json!({"case":"two-distinct-real-makers-order-lifecycle","status":"matched","first_maker":prefixed(node.owner(primary.key).as_slice()),
        "second_maker":prefixed(node.owner(secondary.key).as_slice()),"first_order_cumulative_entitlement":"24","first_order_total_payment":"25","overpayment_not_credited_again":true,
        "second_order_filled":"3","second_order_remaining_refunded":"8","closed_order_identities":[prefixed(first.identity.as_slice()),prefixed(second.identity.as_slice())]}));
    Ok(())
}
