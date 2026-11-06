//! Pure two-pool routes use netted real reserves; the user owns no Y input.
use super::*;
#[path = "finance_route_modes.rs"]
mod amount_modes;

struct Intent {
    controller: packed::Script,
    data: Vec<u8>,
    limits: (u128, u128),
    points: Vec<packed::OutPoint>, // X wallet, funding, descriptor
}

fn reference(ledger: &Ledger, pools: &[Pool; 2], mode: u64, amount: u128) -> Value {
    let states = pools
        .iter()
        .map(|p| {
            let state = quoted_state(ledger, p);
            json!({"a":state.reserve_a.to_string(),"b":state.reserve_b.to_string(),"fee":p.fee.to_string()})
        })
        .collect::<Vec<_>>();
    let mut child = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/finance_route_model.mjs"))
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
            &serde_json::to_vec(&json!({
                "pools":states,"mode":if mode==0 {"exact-input"}else{"exact-output"},"amount":amount.to_string()
            }))
            .unwrap(),
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "route model: {}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

fn prepare(
    programs: &Programs,
    route: &Program,
    ledger: &mut Ledger,
    pools: &[Pool; 2],
    wallet: &packed::OutPoint,
    mode: u64,
    limits: (u128, u128),
    ordinal: u32,
) -> Intent {
    let (maximum, desired) = limits;
    let data = route.data(
        "RouteIntent",
        &[
            ("pool_a", pools[0].identity.calc_script_hash().as_slice().to_vec()),
            ("controller_a", pools[0].controller.calc_script_hash().as_slice().to_vec()),
            ("pool_b", pools[1].identity.calc_script_hash().as_slice().to_vec()),
            ("controller_b", pools[1].controller.calc_script_hash().as_slice().to_vec()),
            ("asset_x", pools[0].assets[0].calc_script_hash().as_slice().to_vec()),
            ("asset_y", pools[0].assets[1].calc_script_hash().as_slice().to_vec()),
            ("asset_z", pools[1].assets[1].calc_script_hash().as_slice().to_vec()),
            ("authorizer", owner_lock(0x63).calc_script_hash().as_slice().to_vec()),
            ("maximum_input_x", maximum.to_le_bytes().to_vec()),
            ("minimum_output_z", desired.to_le_bytes().to_vec()),
            ("exact_output_z", if mode == 0 { 0u128 } else { desired }.to_le_bytes().to_vec()),
            ("maximum_fee", (2 * ledger::FEE).to_le_bytes().to_vec()),
            ("maximum_capacity_debit", (2 * ledger::CELL_CAPACITY).to_le_bytes().to_vec()),
            ("nonce", (ordinal as u64).to_le_bytes().to_vec()),
        ],
    );
    assert_eq!(data.len(), 328);
    let controller = route.script(&cellscript_artifact_checker::ckb_blake2b256(&data));
    let funding = ledger.seed_funding(0x63, ordinal);
    let mut capacity = cell(controller.clone(), None, vec![]);
    capacity.0 = capacity.0.as_builder().capacity(10 * ledger::CELL_CAPACITY).build();
    let tx = ledger.build(
        &[wallet.clone(), funding],
        vec![
            cell(controller.clone(), Some(pools[0].assets[0].clone()), amount(&programs.asset, value(ledger, wallet))),
            capacity,
            cell(controller.clone(), None, data.clone()),
            cell(owner_lock(0x63), None, vec![]),
        ],
        3,
        vec![programs.asset.witness(&[])],
    );
    let points = ledger.commit(&tx, "fund-distinct-pool-route-intent-x-only");
    Intent { controller, data, limits, points: points[..3].to_vec() }
}

fn candidate(
    programs: &Programs,
    route: &Program,
    ledger: &mut Ledger,
    pools: &[Pool; 2],
    intent: &Intent,
    authority: &packed::OutPoint,
    mode: u64,
    operation_amount: u128,
) -> (TransactionView, Value) {
    let reference = reference(ledger, pools, mode, operation_amount);
    let spent = number(&reference, "spent");
    let middle = number(&reference, "middle");
    let received = number(&reference, "received");
    let legs = std::array::from_fn(|i| quote::RouteLeg {
        pool_id: pools[i].identity.calc_script_hash().unpack(),
        asset_a: pools[i].assets[0].calc_script_hash().unpack(),
        asset_b: pools[i].assets[1].calc_script_hash().unpack(),
        state: quoted_state(ledger, &pools[i]),
        direction: quote::Direction::AForB,
    });
    let route_quote = if mode == 0 {
        quote::route_exact_in(legs, operation_amount, intent.limits.1).unwrap()
    } else {
        quote::route_exact_out(legs, operation_amount, intent.limits.0).unwrap()
    };
    let quotations = route_quote.legs;
    assert_eq!(
        (quotations[0].amount_in, quotations[0].amount_out, quotations[1].amount_in, quotations[1].amount_out),
        (spent, middle, middle, received)
    );
    let mut outputs = vec![];
    for (i, pool) in pools.iter().enumerate() {
        let next = quotations[i].next;
        assert_eq!(next.reserve_a, number(&reference["next"][i], "a"));
        assert_eq!(next.reserve_b, number(&reference["next"][i], "b"));
        outputs.extend([
            cell(
                pool.controller.clone(),
                Some(pool.identity.clone()),
                pool_data(&programs.pool, pool, next.reserve_a, next.reserve_b, next.total_lp, pool.revision + 1),
            ),
            reserve(&programs.reserve, pool, 0, next.reserve_a),
            reserve(&programs.reserve, pool, 1, next.reserve_b),
        ]);
    }
    outputs.extend([
        cell(
            intent.controller.clone(),
            Some(pools[0].assets[0].clone()),
            amount(&programs.asset, value(ledger, &intent.points[0]) - spent),
        ),
        cell(intent.controller.clone(), Some(pools[1].assets[1].clone()), amount(&programs.asset, received)),
        cell(intent.controller.clone(), None, vec![]),
        cell(intent.controller.clone(), None, intent.data.clone()),
        ledger.live[authority].clone(),
    ]);
    let args = |index: u64| {
        let q = quotations[index as usize];
        programs.pool.witness(&[
            Arg::U64(if mode == 0 { 5 } else { 7 }),
            Arg::U64(index * 3),
            Arg::U64(index * 3),
            Arg::U128(q.amount_in),
            Arg::U128(if mode == 0 { 0 } else { q.amount_out }),
            Arg::U128(0),
            Arg::U128(0),
            Arg::U128(if mode == 0 { q.amount_out } else { 0 }),
            Arg::Hash([0; 32]),
        ])
    };
    let inputs = [pools[0].points.clone(), pools[1].points.clone(), intent.points.clone(), vec![authority.clone()]].concat();
    let tx = ledger.build(
        &inputs,
        outputs,
        8,
        vec![
            args(0),
            programs.asset.witness(&[]),
            programs.asset.witness(&[]),
            args(1),
            Bytes::new().pack(),
            programs.asset.witness(&[]),
            route.witness(&[Arg::U64(mode), Arg::U64(8), Arg::U64(9)]),
        ],
    );
    (tx, reference)
}

fn outputs(tx: &TransactionView) -> Vec<Cell> {
    tx.outputs().into_iter().zip(tx.outputs_data()).map(|(output, data)| (output, data.raw_data())).collect()
}
fn inputs(tx: &TransactionView) -> Vec<packed::OutPoint> {
    tx.inputs().into_iter().map(|i| i.previous_output()).collect()
}

#[test]
fn pure_distinct_pool_routes_enforce_both_amount_modes_and_real_intermediate_conservation() {
    let mut programs = Programs::compile();
    programs.evidence_namespace = "c3";
    let route = Program::compile(&programs._directory.path().join("finance/route"), "route", false);
    let trap = capacity_trap(&programs);
    let all = [programs.all().to_vec(), vec![&route, &trap]].concat();
    let mut ledger = Ledger::new(&all, &[0x61, 0x62, 0x63, 0x64, 0x71, 0x72, 0x73]);
    prove_capacity_trap(&mut ledger, &trap);
    let (x, wx) = issue(&mut ledger, &programs.asset, 0x71);
    let (y, wy) = issue(&mut ledger, &programs.asset, 0x72);
    let (z, wz) = issue(&mut ledger, &programs.asset, 0x73);
    let mut first_provider = Wallet { key: 0x61, a: wx[0].clone(), b: wy[0].clone(), lp: None };
    let first_trace = model(&[]);
    let (first, _) =
        initialise_configured(&programs, &mut ledger, [x.clone(), y.clone()], &mut first_provider, &first_trace[0], 30, 1, 4100);
    let mut second_provider = Wallet { key: 0x61, a: first_provider.b.clone(), b: wz[0].clone(), lp: None };
    let supply = quote::initialise(200000, 600000, 50, 1).unwrap().next.total_lp;
    let second_step = json!({"a":"200000","b":"600000","supply":supply.to_string()});
    let (second, _) =
        initialise_configured(&programs, &mut ledger, [y.clone(), z.clone()], &mut second_provider, &second_step, 50, 2, 4200);
    let mut pools = [first, second];
    assert_ne!(pools[0].identity, pools[1].identity);
    let funding = ledger.seed_funding(0x61, 4300);
    let gift = ledger.build(
        &[first_provider.a.clone(), funding],
        vec![
            token(&programs.asset, &x, 0x63, 5000),
            token(&programs.asset, &x, 0x61, value(&ledger, &first_provider.a) - 5000),
            cell(owner_lock(0x61), None, vec![]),
        ],
        2,
        vec![programs.asset.witness(&[])],
    );
    let mut wallet = ledger.commit(&gift, "route-user-receives-only-x")[0].clone();
    assert!(!ledger
        .live
        .values()
        .any(|(output, _)| output.lock() == owner_lock(0x63) && output.type_().to_opt().is_some_and(|s| s == y || s == z)));
    let mut models = vec![];
    for mode in 0..=1 {
        let (operation_amount, limits) = if mode == 0 { (1000, (1000, 10000)) } else { (12000, (1200, 12000)) };
        let intent = prepare(&programs, &route, &mut ledger, &pools, &wallet, mode, limits, 4400 + mode as u32);
        let authority = ledger.seed_funding(0x63, 4500 + mode as u32);
        let (tx, reference) = candidate(&programs, &route, &mut ledger, &pools, &intent, &authority, mode, operation_amount);
        // Other protocol groups remain genuine while a hostile builder tries
        // to reinterpret exact-output as an input amount, or vice versa.
        let mut rewritten = tx.witnesses().into_iter().collect::<Vec<_>>();
        rewritten[6] = route.witness(&[Arg::U64(1 - mode), Arg::U64(8), Arg::U64(9)]);
        let bad = ledger.build(&inputs(&tx), outputs(&tx), 8, rewritten);
        ledger.reject(&bad, &["Inputs[6].Lock"], "route-amount-mode-rewrite-cannot-change-committed-intent");
        let mut redirected = outputs(&tx);
        redirected[7].0 = redirected[7].0.clone().as_builder().lock(owner_lock(0x64)).build();
        let bad = ledger.build(&inputs(&tx), redirected, 8, tx.witnesses().into_iter().collect());
        ledger.reject(&bad, &["Inputs[6].Lock"], "route-final-credit-cannot-be-redirected");
        let mut swapped = outputs(&tx);
        swapped.swap(6, 7);
        let bad = ledger.build(&inputs(&tx), swapped, 8, tx.witnesses().into_iter().collect());
        ledger.reject(&bad, &["Inputs[6].Lock"], "route-invalid-output-permutation-is-not-a-new-layout");
        let mut wrong_source = tx.witnesses().into_iter().collect::<Vec<_>>();
        wrong_source[6] = route.witness(&[Arg::U64(mode), Arg::U64(7), Arg::U64(9)]);
        let bad = ledger.build(&inputs(&tx), outputs(&tx), 8, wrong_source);
        ledger.reject(&bad, &["Inputs[6].Lock"], "route-witness-cannot-substitute-intent-source");
        let mut repeated = tx.witnesses().into_iter().collect::<Vec<_>>();
        repeated[3] = programs.pool.witness(&[
            Arg::U64(5),
            Arg::U64(0),
            Arg::U64(0),
            Arg::U128(number(&reference, "spent")),
            Arg::U128(0),
            Arg::U128(0),
            Arg::U128(0),
            Arg::U128(number(&reference, "middle")),
            Arg::Hash([0; 32]),
        ]);
        let bad = ledger.build(&inputs(&tx), outputs(&tx), 8, repeated);
        ledger.reject(&bad, &["Inputs[3].Lock"], "route-cannot-reuse-first-pool-for-second-executing-group");
        let mut foreign_inputs = inputs(&tx);
        foreign_inputs.push(second_provider.a.clone());
        let mut foreign_outputs = outputs(&tx);
        foreign_outputs.push(ledger.live[&second_provider.a].clone());
        let bad = ledger.build(&foreign_inputs, foreign_outputs, 8, tx.witnesses().into_iter().collect());
        ledger.reject(&bad, &["Inputs[6].Lock"], "route-cannot-interpose-an-external-y-wallet-in-middle-accounting");
        let mut stolen_capacity = outputs(&tx);
        let refund = u64::from(stolen_capacity[10].0.capacity()) + 2 * ledger::CELL_CAPACITY;
        stolen_capacity[10].0 = stolen_capacity[10].0.clone().as_builder().capacity(refund).build();
        let bad = ledger.build(&inputs(&tx), stolen_capacity, 8, tx.witnesses().into_iter().collect());
        ledger.reject(&bad, &["Inputs[6].Lock"], "route-capacity-change-cannot-exceed-committed-debit");
        let mut frozen = outputs(&tx);
        frozen[10].0 = frozen[10].0.clone().as_builder().capacity(u64::from(frozen[10].0.capacity()) - ledger::CELL_CAPACITY).build();
        frozen.push(cell(owner_lock(0x63), Some(trap.script(&[])), vec![]));
        let bad = ledger.build(&inputs(&tx), frozen, 8, trap_witness(&tx, &trap, 11));
        ledger.reject(&bad, &["Inputs[6].Lock"], "route-authorizer-capacity-cannot-be-credited-through-a-spend-rejecting-type");
        // Exercise the full 32/32 bound with genuinely signed unrelated Cells.
        // One pair is consolidated without changing its capacity ownership.
        let mut with_inputs = inputs(&tx);
        let mut with_outputs = outputs(&tx);
        for n in 0..22 {
            let point = ledger.seed_funding(0x64, 4600 + mode as u32 * 32 + n);
            let extra = ledger.live[&point].clone();
            if n == 21 {
                let last = with_outputs.last_mut().unwrap();
                let capacity = u64::from(last.0.capacity()) + u64::from(extra.0.capacity());
                last.0 = last.0.clone().as_builder().capacity(capacity).build();
            } else {
                with_outputs.push(extra);
            }
            with_inputs.push(point);
        }
        if mode == 1 {
            with_inputs.swap(10, 11);
            with_outputs.swap(11, 12);
        }
        assert_eq!((with_inputs.len(), with_outputs.len()), (32, 32));
        let tx = ledger.build(&with_inputs, with_outputs, 8, tx.witnesses().into_iter().collect());
        let mut late = outputs(&tx);
        let refund = u64::from(late[10].0.capacity()) + 2 * ledger::CELL_CAPACITY;
        late[10].0 = late[10].0.clone().as_builder().capacity(refund).build();
        let bad = ledger.build(&inputs(&tx), late, 8, tx.witnesses().into_iter().collect());
        ledger.reject(&bad, &["Inputs[6].Lock"], "route-maximum-domain-capacity-change-cannot-exceed-committed-debit");
        let points = checked_commit_with(
            "c3",
            &all,
            &mut ledger,
            &tx,
            if mode == 0 {
                "pure-two-pool-exact-input-with-unrelated-cells"
            } else {
                "pure-two-pool-exact-output-with-unrelated-permutation"
            },
        );
        for (i, pool) in pools.iter_mut().enumerate() {
            pool.points = points[i * 3..i * 3 + 3].to_vec();
            pool.revision += 1;
        }
        assert_eq!(value(&ledger, &points[7]), number(&reference, "received"));
        let authority = ledger.seed_funding(0x63, 4700 + mode as u32);
        let unwrap = ledger.build(
            &[points[8].clone(), points[6].clone(), points[7].clone(), points[9].clone(), authority],
            vec![
                token(&programs.asset, &x, 0x63, value(&ledger, &points[6])),
                token(&programs.asset, &z, 0x63, value(&ledger, &points[7])),
                cell(owner_lock(0x63), None, vec![]),
            ],
            2,
            vec![route.witness(&[Arg::U64(2), Arg::U64(3), Arg::U64(4)]), programs.asset.witness(&[]), programs.asset.witness(&[])],
        );
        let mut frozen = outputs(&unwrap);
        frozen.push(cell(owner_lock(0x63), Some(trap.script(&[])), vec![]));
        let bad = ledger.build(&inputs(&unwrap), frozen, 2, trap_witness(&unwrap, &trap, 3));
        ledger.reject(&bad, &["Inputs[0].Lock"], "route-unwrap-capacity-cannot-be-credited-through-a-spend-rejecting-type");
        let unrelated = ledger.seed_funding(0x64, 9200 + mode as u32);
        let mut leak_sources = inputs(&unwrap);
        leak_sources.push(unrelated.clone());
        let mut leaked = outputs(&unwrap);
        let mut refund = ledger.live[&unrelated].clone();
        refund.0 = refund.0.clone().as_builder().capacity(u64::from(refund.0.capacity()) + 1).build();
        leaked.push(refund);
        let bad = ledger.build(&leak_sources, leaked, 2, unwrap.witnesses().into_iter().collect());
        ledger.reject(&bad, &["Inputs[0].Lock"], "route-unused-fee-allowance-cannot-fund-a-third-party-refund");
        wallet = checked_commit_with("c3", &all, &mut ledger, &unwrap, "route-authorizer-reclaims-x-z-and-capacity")[0].clone();
        models.push(reference);
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/finance-protocol-evidence/c3");
    ledger.archive(&root, &all);
    std::fs::write(root.join("independent-model.json"), serde_json::to_vec_pretty(&models).unwrap()).unwrap();
}
