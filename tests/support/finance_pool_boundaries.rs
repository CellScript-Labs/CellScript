//! Full-transaction financial boundaries, beyond host-only quote assertions.
//! This is registered only by the finance_protocol integration-test owner.
use super::*;

#[path = "finance_pool_admission.rs"]
mod admission;

#[path = "finance_pool_math_comparison.rs"]
mod math_comparison;

const MAXIMUM: u128 = 1_000_000_000_000;

fn reference(a: u128, b: u128, fee: u64, operations: Vec<Value>) -> Value {
    let mut child = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/finance_model.mjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let request = json!({"a":a.to_string(),"b":b.to_string(),"fee":fee.to_string(),"operations":operations});
    child.stdin.take().unwrap().write_all(&serde_json::to_vec(&request).unwrap()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "boundary BigInt model: {}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

fn issue_maximum(ledger: &mut Ledger, asset: &Program, issuer: u8) -> (packed::Script, [packed::OutPoint; 2]) {
    let script = asset.script(owner_lock(issuer).calc_script_hash().as_slice());
    let invalid_funding = ledger.seed_funding(issuer, 500);
    let invalid = ledger.build(
        &[invalid_funding],
        vec![token(asset, &script, 0x61, MAXIMUM + 1), cell(owner_lock(issuer), None, vec![])],
        1,
        vec![asset.witness(&[])],
    );
    ledger.reject(&invalid, &["Outputs[0].Type"], "enforcing-asset-rejects-above-financial-domain");
    let funding = ledger.seed_funding(issuer, 501);
    let tx = ledger.build(
        &[funding],
        vec![token(asset, &script, 0x61, MAXIMUM), token(asset, &script, 0x62, MAXIMUM), cell(owner_lock(issuer), None, vec![])],
        2,
        vec![asset.witness(&[])],
    );
    let points = ledger.commit(&tx, &format!("issue-maximum-domain-asset-{issuer}"));
    (script, [points[0].clone(), points[1].clone()])
}

fn cells(tx: &TransactionView) -> Vec<Cell> {
    tx.outputs().into_iter().zip(tx.outputs_data()).map(|(output, data)| (output, data.raw_data())).collect()
}

fn inputs(tx: &TransactionView) -> Vec<packed::OutPoint> {
    tx.inputs().into_iter().map(|input| input.previous_output()).collect()
}

fn all_circulating_withdrawal(
    programs: &Programs,
    ledger: &mut Ledger,
    pool: &Pool,
    wallet: &Wallet,
    step: &Value,
    ordinal: u32,
) -> TransactionView {
    let quotation = quote::withdraw(quoted_state(ledger, pool), number(step, "burned"), 0, 0).unwrap();
    assert_model_state(quotation.next, step);
    assert_eq!((quotation.paid_a, quotation.paid_b), (number(step, "paidA"), number(step, "paidB")));
    assert_eq!(value(ledger, wallet.lp.as_ref().unwrap()), quotation.burned);
    let funding = ledger.seed_funding(wallet.key, ordinal);
    let mut selected = pool.points.clone();
    selected.extend([funding, wallet.lp.clone().unwrap()]);
    let mut outputs = state_outputs(programs, pool, step);
    outputs.extend([
        token(&programs.asset, &pool.assets[0], wallet.key, quotation.paid_a),
        token(&programs.asset, &pool.assets[1], wallet.key, quotation.paid_b),
        cell(owner_lock(wallet.key), None, vec![]),
    ]);
    ledger.build(
        &selected,
        outputs,
        5,
        vec![
            witness(&programs.pool, 4, 0, 0, 0, quotation.burned, quotation.paid_a, quotation.paid_b, wallet.key),
            programs.asset.witness(&[]),
            programs.asset.witness(&[]),
            Bytes::new().pack(),
            programs.lp.witness(&[]),
        ],
    )
}

fn changed_fee(pool: &Pool) -> Pool {
    Pool {
        points: pool.points.clone(),
        identity: pool.identity.clone(),
        controller: pool.controller.clone(),
        lp: pool.lp.clone(),
        minimum: pool.minimum.clone(),
        assets: pool.assets.clone(),
        fee: if pool.fee == 0 { 1 } else { pool.fee - 1 },
        nonce: pool.nonce,
        revision: pool.revision,
    }
}

fn hostile_swap_boundaries(
    programs: &Programs,
    ledger: &mut Ledger,
    pool: &Pool,
    wallet: &Wallet,
    tx: &TransactionView,
    step: &Value,
) {
    let received = number(step, "received");
    let offered = number(step, "input");
    let mut witnesses = tx.witnesses().into_iter().collect::<Vec<_>>();
    witnesses[0] = witness(&programs.pool, 1, 0, offered, 0, 0, 0, received + 1, wallet.key);
    ledger.reject(
        &tx.as_advanced_builder().set_witnesses(witnesses).build(),
        &["Inputs[0].Lock"],
        "on-chain-slippage-one-above-real-quote",
    );

    let mut outputs = tx.outputs().into_iter().collect::<Vec<_>>();
    outputs[3] = outputs[3].clone().as_builder().lock(owner_lock(0x63)).build();
    ledger.reject(
        &tx.as_advanced_builder().set_outputs(outputs).build(),
        &["Inputs[0].Lock"],
        "wallet-refund-cannot-go-to-another-owner",
    );

    let mut short = cells(tx);
    let refund = value(ledger, &wallet.a) - offered;
    assert!(refund > 0);
    short[3].1 = Bytes::from(amount(&programs.asset, refund - 1));
    short.push(token(&programs.asset, &pool.assets[0], 0x63, 1));
    let short = ledger.build(&inputs(tx), short, 5, tx.witnesses().into_iter().collect());
    ledger.reject(&short, &["Inputs[0].Lock"], "asset-conserving-extra-output-cannot-replace-owner-refund");

    let mut data = tx.outputs_data().into_iter().collect::<Vec<_>>();
    data[0] = Bytes::from(pool_data(
        &programs.pool,
        &changed_fee(pool),
        number(step, "a"),
        number(step, "b"),
        number(step, "supply"),
        pool.revision + 1,
    ))
    .pack();
    ledger.reject(
        &tx.as_advanced_builder().set_outputs_data(data).build(),
        &["Inputs[0].Lock"],
        "fee-is-immutable-even-within-admitted-rate-domain",
    );

    for protected in [0, 1, 2] {
        let mut outputs = tx.outputs().into_iter().collect::<Vec<_>>();
        let protected_capacity = u64::from(outputs[protected].capacity());
        let change_capacity = u64::from(outputs[5].capacity());
        outputs[protected] = outputs[protected].clone().as_builder().capacity(protected_capacity - 1).build();
        outputs[5] = outputs[5].clone().as_builder().capacity(change_capacity + 1).build();
        ledger.reject(
            &tx.as_advanced_builder().set_outputs(outputs).build(),
            &["Inputs[0].Lock"],
            &format!("protected-role-{protected}-capacity-cannot-fund-change"),
        );
    }

    let mut witnesses = tx.witnesses().into_iter().collect::<Vec<_>>();
    witnesses[0] = witness(&programs.pool, 3, 0, 0, 0, 0, 0, 0, wallet.key);
    ledger.reject(
        &tx.as_advanced_builder().set_witnesses(witnesses).build(),
        &["Inputs[0].Lock"],
        "zero-share-deposit-rejects-on-chain",
    );

    let mut excessive = inputs(tx);
    excessive.extend((0..27).map(|index| ledger.seed_funding(wallet.key, 1100 + index)));
    let excessive = ledger.build(&excessive, cells(tx), 5, tx.witnesses().into_iter().collect());
    assert_eq!(excessive.inputs().len(), 33);
    ledger.reject(
        &excessive,
        &["Inputs[0].Lock", "Inputs[1].Lock", "Inputs[2].Lock", "Inputs[1].Type", "Inputs[2].Type"],
        "thirty-three-inputs-exceed-frozen-scan-domain",
    );
    assert!(ledger.records.last().unwrap()["error"].as_str().unwrap().contains("error code 5"));
}

#[test]
fn real_pool_transactions_cover_wide_math_fee_and_capacity_boundaries() {
    let mut programs = Programs::compile();
    programs.evidence_namespace = "pool-boundaries";
    let mut ledger = Ledger::new(&programs.all(), &[0x61, 0x62, 0x71, 0x72]);
    let (a, wa) = issue_maximum(&mut ledger, &programs.asset, 0x71);
    let (b, wb) = issue_maximum(&mut ledger, &programs.asset, 0x72);
    let mut first = Wallet { key: 0x61, a: wa[0].clone(), b: wb[0].clone(), lp: None };
    let mut second = Wallet { key: 0x62, a: wa[1].clone(), b: wb[1].clone(), lp: None };
    assert!(MAXIMUM * MAXIMUM > u128::from(u64::MAX));
    let maximum_model = reference(MAXIMUM, MAXIMUM, 0, vec![json!({"kind":"withdraw","shares":"circulating"})]);
    let (mut maximum_pool, locked) =
        initialise_configured(&programs, &mut ledger, [a.clone(), b.clone()], &mut first, &maximum_model[0], 0, 10, 600);
    assert_eq!(quoted_state(&ledger, &maximum_pool).total_lp, MAXIMUM);
    let full = all_circulating_withdrawal(&programs, &mut ledger, &maximum_pool, &first, &maximum_model[1], 1000);
    // Preserve total capacity and all token/share equations, but divert part
    // of the required LP-capacity refund to a separate output. Only D14 fails.
    let old_lp_capacity = u64::from(ledger.live[first.lp.as_ref().unwrap()].0.capacity());
    let mut diverted = cells(&full);
    let original_change = u64::from(diverted[5].0.capacity());
    let (extra, extra_data) = cell(owner_lock(first.key), None, vec![]);
    diverted.push((extra.as_builder().capacity(original_change - old_lp_capacity + 1).build(), extra_data));
    let diverted = ledger.build(&inputs(&full), diverted, 5, full.witnesses().into_iter().collect());
    assert_eq!(u64::from(diverted.outputs().get(5).unwrap().capacity()), old_lp_capacity - 1);
    ledger.reject(&diverted, &["Inputs[0].Lock"], "full-redemption-must-refund-old-lp-capacity-to-designated-owner-change");
    let redeemed = checked_commit(&programs, &mut ledger, &full, "maximum-domain-init-and-80-bit-product-redemption");
    maximum_pool.points = redeemed[..3].to_vec();
    maximum_pool.revision += 1;
    first.a = redeemed[3].clone();
    first.b = redeemed[4].clone();
    first.lp = None;
    assert_eq!(quoted_state(&ledger, &maximum_pool).total_lp, 1000);
    assert_eq!(value(&ledger, &locked), 1000);

    let large = MAXIMUM - 1_000_000;
    let wide_model = reference(large, large, 1000, vec![json!({"kind":"swap_a","amount":"123456"})]);
    let (mut wide_pool, _) =
        initialise_configured(&programs, &mut ledger, [a.clone(), b.clone()], &mut second, &wide_model[0], 1000, 11, 700);
    assert_ne!(maximum_pool.identity, wide_pool.identity);
    assert_ne!(maximum_pool.lp, wide_pool.lp, "same-pair pools have different authentic LP identities");
    let wide = swap_transaction(&programs, &mut ledger, &wide_pool, &second, &wide_model[1], 1001);
    assert!(large * 123456 * 9000 > u128::from(u64::MAX));
    let points = checked_commit(&programs, &mut ledger, &wide, "actual-swap-evaluates-more-than-64-bit-numerator");
    wide_pool.points = points[..3].to_vec();
    wide_pool.revision += 1;
    second.a = points[3].clone();
    second.b = points[4].clone();

    let fractional_model = reference(1000, 100000, 1000, vec![json!({"kind":"swap_a","amount":"1"})]);
    let (mut fractional_pool, _) =
        initialise_configured(&programs, &mut ledger, [a.clone(), b.clone()], &mut first, &fractional_model[0], 1000, 12, 800);
    assert_ne!(fractional_pool.identity, wide_pool.identity);
    assert_ne!(fractional_pool.lp, wide_pool.lp);
    let fractional = swap_transaction(&programs, &mut ledger, &fractional_pool, &first, &fractional_model[1], 1002);
    assert_eq!(number(&fractional_model[1], "received"), 89);
    hostile_swap_boundaries(&programs, &mut ledger, &fractional_pool, &first, &fractional, &fractional_model[1]);

    let prior = quoted_state(&ledger, &fractional_pool);
    let incorrect = prior.reserve_b / (prior.reserve_a + 1);
    assert_eq!(incorrect, 99, "pre-rounding the fee incorrectly prices this input");
    let mut rounded = cells(&fractional);
    rounded[0].1 = Bytes::from(pool_data(
        &programs.pool,
        &fractional_pool,
        prior.reserve_a + 1,
        prior.reserve_b - incorrect,
        prior.total_lp,
        fractional_pool.revision + 1,
    ));
    rounded[2] = reserve(&programs.reserve, &fractional_pool, 1, prior.reserve_b - incorrect);
    rounded[4] = token(&programs.asset, &fractional_pool.assets[1], first.key, value(&ledger, &first.b) + incorrect);
    let rounded = ledger.build(&inputs(&fractional), rounded, 5, fractional.witnesses().into_iter().collect());
    ledger.reject(&rounded, &["Inputs[0].Lock"], "rounded-fee-first-quote-rejects-with-conserved-real-assets");
    let points = checked_commit(&programs, &mut ledger, &fractional, "maximum-fee-one-unit-input-pays-exact-scaled-quote");
    fractional_pool.points = points[..3].to_vec();
    fractional_pool.revision += 1;
    first.a = points[3].clone();
    first.b = points[4].clone();

    let zero_fee_model = reference(1000, 100000, 0, vec![json!({"kind":"swap_a","amount":"1"})]);
    let (zero_fee_pool, _) = initialise_configured(&programs, &mut ledger, [a, b], &mut second, &zero_fee_model[0], 0, 13, 900);
    let zero_fee = swap_transaction(&programs, &mut ledger, &zero_fee_pool, &second, &zero_fee_model[1], 1003);
    assert_eq!(number(&zero_fee_model[1], "received"), 99);
    checked_commit(&programs, &mut ledger, &zero_fee, "zero-fee-endpoint-with-one-unit-input");
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/finance-protocol-evidence/pool-boundaries");
    ledger.archive(&directory, &programs.all());
    std::fs::write(
        directory.join("independent-model.json"),
        serde_json::to_vec_pretty(&json!({
            "maximum":maximum_model,"wide_numerator":wide_model,"fractional_fee":fractional_model,"zero_fee":zero_fee_model,
        }))
        .unwrap(),
    )
    .unwrap();
    println!("{} retained maximum-domain/fee/capacity pool cases", ledger.records.len());
}
