//! C4 for C1: fixed prefix roles and authentic unrelated trailing Cells.
use super::*;

fn outputs(tx: &TransactionView) -> Vec<Cell> {
    tx.outputs().into_iter().zip(tx.outputs_data()).map(|(output, data)| (output, data.raw_data())).collect()
}

fn candidate(namespace: &str, programs: &[&Program], ledger: &mut Ledger, tx: &TransactionView, name: &str) {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/finance-protocol-evidence/construction")
        .join(namespace)
        .join(hex::encode(tx.hash().as_slice()));
    let mut construction = finance_protocol_bundle::check_and_materialize(&directory, programs, ledger, tx)
        .unwrap_or_else(|error| panic!("{name} public ProtocolBundle construction: {error}"));
    let cycles = ledger.accept_candidate(tx, name);
    construction["local_complete_transaction_vm"] = json!({"status":"accepted", "cycles":cycles,
        "local_bookkeeping":"not-consumed-vm-preflight", "node_acceptance":null});
    ledger.records.last_mut().unwrap()["construction"] = construction;
}

#[allow(clippy::too_many_arguments)]
pub(super) fn cases(
    programs: &Programs,
    order_code: &Program,
    escrow_code: &Program,
    settlement: &Program,
    all: &[&Program],
    ledger: &mut Ledger,
    pool: &Pool,
    order: &Order,
    intent: &Intent,
    authority: &packed::OutPoint,
    provider: &mut Wallet,
    tx: &TransactionView,
) -> TransactionView {
    let sources = tx.inputs().into_iter().map(|input| input.previous_output()).collect::<Vec<_>>();
    let witnesses = || tx.witnesses().into_iter().collect::<Vec<_>>();

    // Balance both genuine asset Types. The extra requested-asset payment
    // cannot satisfy the fixed maker output's cumulative obligation.
    let mut captured = outputs(tx);
    captured[5] = token(&programs.asset, &pool.assets[1], 0x62, 233);
    captured.push(token(&programs.asset, &pool.assets[1], 0x62, 16));
    let hostile = ledger.build(&sources, captured, 6, witnesses());
    ledger.reject(&hostile, &["Inputs[3].Lock"], "c1-trailing-maker-payment-cannot-replace-the-obligation-output");

    let mut reserve_mismatch = outputs(tx);
    reserve_mismatch[1] = reserve(&programs.reserve, pool, 0, 100_051);
    reserve_mismatch[7].1 = Bytes::from(amount(&programs.asset, 249));
    let hostile = ledger.build(&sources, reserve_mismatch, 6, witnesses());
    ledger.reject(&hostile, &["Inputs[0].Lock", "Inputs[1].Lock"], "c1-conserved-assets-do-not-authorize-reserve-state-mismatch");

    let (foreign_asset, foreign) = issue(ledger, &programs.asset, 0x73);
    let mut wrong_asset_inputs = sources.clone();
    wrong_asset_inputs.push(foreign[0].clone());
    let mut wrong_asset = outputs(tx);
    wrong_asset[5] = token(&programs.asset, &foreign_asset, 0x62, 249);
    wrong_asset.push(token(&programs.asset, &pool.assets[1], provider.key, 249));
    wrong_asset.push(token(&programs.asset, &foreign_asset, provider.key, value(ledger, &foreign[0]) - 249));
    let mut foreign_witnesses = witnesses();
    foreign_witnesses.resize(10, packed::WitnessArgs::default().as_bytes().pack());
    foreign_witnesses[9] = programs.asset.witness(&[]);
    let hostile = ledger.build(&wrong_asset_inputs, wrong_asset, 6, foreign_witnesses);
    ledger.reject(&hostile, &["Inputs[3].Lock", "Inputs[5].Lock"], "c1-genuine-foreign-asset-cannot-replace-maker-payment");

    // Preserve the prefix indices with real signed untyped Cells; the absent
    // pool is not replaced by a seeded lookalike state or counterfeit Type ID.
    let mut missing_pool_inputs = sources.clone();
    let mut missing_pool = outputs(tx);
    for index in 0..3 {
        missing_pool_inputs[index] = ledger.seed_funding(provider.key, 9200 + index as u32);
        missing_pool[index] = cell(owner_lock(provider.key), None, vec![]);
    }
    missing_pool_inputs.push(provider.b.clone());
    missing_pool.push(token(&programs.asset, &pool.assets[0], provider.key, 50));
    missing_pool.push(token(&programs.asset, &pool.assets[1], provider.key, value(ledger, &provider.b) - 249));
    let hostile = ledger.build(&missing_pool_inputs, missing_pool, 6, witnesses());
    ledger.reject(&hostile, &["Inputs[5].Lock"], "c1-missing-pool-group-cannot-be-replaced-by-honest-external-funding");

    let mut missing_order_inputs = sources.clone();
    let mut missing_order = outputs(tx);
    for index in 3..5 {
        missing_order_inputs[index] = ledger.seed_funding(provider.key, 9300 + index as u32);
        missing_order[index] = cell(owner_lock(provider.key), None, vec![]);
    }
    missing_order_inputs.push(provider.a.clone());
    missing_order.push(token(&programs.asset, &pool.assets[0], provider.key, value(ledger, &provider.a) - 100));
    let hostile = ledger.build(&missing_order_inputs, missing_order, 6, witnesses());
    ledger.reject(&hostile, &["Inputs[5].Lock"], "c1-missing-order-group-cannot-be-replaced-by-honest-external-funding");

    let trace = model(&[json!({"kind":"swap_a","amount":"50"})]);
    let (other_pool, _) = initialise_configured(programs, ledger, pool.assets.clone(), provider, &trace[0], 30, 2, 9400);
    assert_ne!(other_pool.identity, pool.identity);
    let substituted = settlement_transaction(
        programs,
        order_code,
        escrow_code,
        settlement,
        ledger,
        &other_pool,
        order,
        intent,
        authority,
        (50, 100),
    );
    ledger.reject(&substituted, &["Inputs[5].Lock"], "c1-fully-authentic-other-pool-cannot-satisfy-committed-identity");

    let mut moved_prefix = sources.clone();
    moved_prefix.swap(5, 6);
    let hostile = ledger.build(&moved_prefix, outputs(tx), 6, witnesses());
    // The order independently requires its ib+2 funding role to be untyped;
    // it rejects this substitution before the compositor's own prefix check.
    ledger.reject(&hostile, &["Inputs[3].Lock"], "c1-same-controller-prefix-substitution-is-outside-layout-contract");

    // These are genuine independent standard-Lock inputs. Different output
    // payloads make the permutation observable; fixed role indices stay put.
    let tail = (9500..9523).map(|index| ledger.seed_funding(0x64, index)).collect::<Vec<_>>();
    let mut canonical_sources = sources.clone();
    canonical_sources.extend(tail.iter().cloned());
    let mut canonical_outputs = outputs(tx);
    for (index, point) in tail.iter().take(22).enumerate() {
        let mut refund = ledger.live[point].clone();
        refund.1 = Bytes::from(vec![0xa1, index as u8]);
        if index == 21 {
            refund.0 = refund.0.as_builder().capacity(200 * ledger::CELL_CAPACITY).build();
        }
        canonical_outputs.push(refund);
    }
    assert_eq!(canonical_sources.len(), 32);
    assert_eq!(canonical_outputs.len(), 32);
    let canonical = ledger.build(&canonical_sources, canonical_outputs.clone(), 6, witnesses());
    candidate(programs.evidence_namespace, all, ledger, &canonical, "c1-unrelated-trailing-cells-canonical-preflight");
    assert!(canonical_sources.iter().all(|point| ledger.live.contains_key(point)));
    canonical_sources.swap(9, 31);
    canonical_outputs.swap(10, 31);
    let permuted = ledger.build(&canonical_sources, canonical_outputs, 6, witnesses());
    assert_ne!(canonical.hash(), permuted.hash());

    let mut stale = permuted.witnesses().into_iter().collect::<Vec<_>>();
    stale[5] = settlement.witness(&[Arg::U64(0), Arg::U64(10), Arg::U64(8)]);
    let hostile = ledger.build(&canonical_sources, outputs(&permuted), 6, stale);
    ledger.reject(&hostile, &["Inputs[5].Lock"], "c1-trailing-witness-reference-cannot-substitute-the-authenticated-intent");

    let mut too_many = canonical_sources.clone();
    too_many.push(ledger.seed_funding(0x64, 9550));
    let hostile = ledger.build(&too_many, outputs(&permuted), 6, witnesses());
    ledger.reject(&hostile, &["Inputs[4].Lock"], "c1-thirty-three-inputs-exceed-the-layout-domain");
    let mut too_many = outputs(&permuted);
    too_many.push(cell(owner_lock(0x64), None, vec![0xff]));
    let hostile = ledger.build(&canonical_sources, too_many, 6, witnesses());
    ledger.reject(&hostile, &["Inputs[4].Lock"], "c1-thirty-three-outputs-exceed-the-layout-domain");
    permuted
}
