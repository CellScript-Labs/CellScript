//! Real pool groups must not change a route's committed economic amount mode.
use super::*;

fn issue_route_asset(ledger: &mut Ledger, asset: &Program, issuer: u8) -> (packed::Script, packed::OutPoint) {
    let identity = asset.script(owner_lock(issuer).calc_script_hash().as_slice());
    let funding = ledger.seed_funding(issuer, 1);
    let transaction = ledger.build(
        &[funding],
        vec![token(asset, &identity, 0x61, 2_000_000_000), cell(owner_lock(issuer), None, vec![])],
        1,
        vec![asset.witness(&[])],
    );
    let points = ledger.commit(&transaction, &format!("issue-route-mode-boundary-asset-{issuer}"));
    (identity, points[0].clone())
}

fn initial_state(a: u128, b: u128, fee: u64) -> Value {
    let quotation = quote::initialise(a, b, fee, 1).unwrap();
    json!({"a":a.to_string(),"b":b.to_string(),"supply":quotation.next.total_lp.to_string()})
}

fn substituted_pool_modes(
    programs: &Programs,
    route: &Program,
    ledger: &Ledger,
    pools: &[Pool; 2],
    intent: &Intent,
    authority: &packed::OutPoint,
    wrapper_mode: u64,
) -> (TransactionView, Value) {
    let public = reference(ledger, pools, wrapper_mode, 1);
    let quotations = if wrapper_mode == 0 {
        assert_eq!((number(&public, "spent"), number(&public, "middle"), number(&public, "received")), (1, 9969, 90240458));
        std::array::from_fn(|index| quote::swap_exact_out(quoted_state(ledger, &pools[index]), quote::Direction::AForB, 1, 1).unwrap())
    } else {
        assert_eq!((number(&public, "spent"), number(&public, "middle"), number(&public, "received")), (10234, 101, 1));
        let first = quote::swap_exact_in(quoted_state(ledger, &pools[0]), quote::Direction::AForB, 10235, 101).unwrap();
        let second = quote::swap_exact_in(quoted_state(ledger, &pools[1]), quote::Direction::AForB, 101, 1).unwrap();
        [first, second]
    };
    let [first, second] = quotations;
    assert_eq!(first.amount_out, second.amount_in);
    assert_eq!((first.amount_in, first.amount_out, second.amount_out), if wrapper_mode == 0 { (1, 1, 1) } else { (10235, 101, 1) });
    let mut created = Vec::new();
    for (pool, quotation) in pools.iter().zip(quotations) {
        let next = quotation.next;
        created.extend([
            cell(
                pool.controller.clone(),
                Some(pool.identity.clone()),
                pool_data(&programs.pool, pool, next.reserve_a, next.reserve_b, next.total_lp, 1),
            ),
            reserve(&programs.reserve, pool, 0, next.reserve_a),
            reserve(&programs.reserve, pool, 1, next.reserve_b),
        ]);
    }
    created.extend([
        cell(
            intent.controller.clone(),
            Some(pools[0].assets[0].clone()),
            amount(&programs.asset, value(ledger, &intent.points[0]) - first.amount_in),
        ),
        cell(intent.controller.clone(), Some(pools[1].assets[1].clone()), amount(&programs.asset, second.amount_out)),
        cell(intent.controller.clone(), None, vec![]),
        cell(intent.controller.clone(), None, intent.data.clone()),
        ledger.live[authority].clone(),
    ]);
    let pool_entry = |index: usize| {
        let quotation = quotations[index];
        programs.pool.witness(&[
            Arg::U64(if wrapper_mode == 0 { 7 } else { 5 }),
            Arg::U64(index as u64 * 3),
            Arg::U64(index as u64 * 3),
            Arg::U128(quotation.amount_in),
            Arg::U128(if wrapper_mode == 0 { quotation.amount_out } else { 0 }),
            Arg::U128(0),
            Arg::U128(0),
            Arg::U128(if wrapper_mode == 0 { 0 } else { quotation.amount_out }),
            Arg::Hash([0; 32]),
        ])
    };
    let selected = [pools[0].points.clone(), pools[1].points.clone(), intent.points.clone(), vec![authority.clone()]].concat();
    let transaction = ledger.build(
        &selected,
        created,
        8,
        vec![
            pool_entry(0),
            programs.asset.witness(&[]),
            programs.asset.witness(&[]),
            pool_entry(1),
            Bytes::new().pack(),
            programs.asset.witness(&[]),
            route.witness(&[Arg::U64(wrapper_mode), Arg::U64(8), Arg::U64(9)]),
        ],
    );
    (transaction, public)
}

fn assert_real_peers_accept(ledger: &Ledger, transaction: &TransactionView, pools: &[Pool; 2]) -> Value {
    let mut observations = Vec::new();
    for pool in pools {
        let observation = cost_measurement::measure_group(
            &ledger.context,
            transaction,
            &pool.controller,
            ckb_testtool::ckb_script::ScriptGroupType::Lock,
            100_000_000,
        );
        assert!(
            matches!(observation.observation, cost_measurement::CycleObservation::Measured { exit_code: 0, .. }),
            "the genuine pool must accept this same route-mode attack transaction: {observation:?}"
        );
        observations.push(serde_json::to_value(observation).unwrap());
    }
    Value::Array(observations)
}

#[test]
fn route_amount_modes_reject_otherwise_valid_opposite_pool_modes() {
    let mut programs = Programs::compile();
    programs.evidence_namespace = "c3-mode-boundaries";
    let route = Program::compile(&programs._directory.path().join("finance/route"), "route", false);
    let all = [programs.all().to_vec(), vec![&route]].concat();
    let mut ledger = Ledger::new(&all, &[0x61, 0x63, 0x71, 0x72, 0x73]);
    let (x, x_wallet) = issue_route_asset(&mut ledger, &programs.asset, 0x71);
    let (y, y_wallet) = issue_route_asset(&mut ledger, &programs.asset, 0x72);
    let (z, mut z_wallet) = issue_route_asset(&mut ledger, &programs.asset, 0x73);
    let gift_funding = ledger.seed_funding(0x61, 6000);
    let gift = ledger.build(
        &[x_wallet, gift_funding],
        vec![
            token(&programs.asset, &x, 0x63, 20_000),
            token(&programs.asset, &x, 0x63, 20_000),
            token(&programs.asset, &x, 0x61, 1_999_960_000),
            cell(owner_lock(0x61), None, vec![]),
        ],
        3,
        vec![programs.asset.witness(&[])],
    );
    let gifts = ledger.commit(&gift, "fund-two-independent-x-only-route-mode-users");
    let mut provider = Wallet { key: 0x61, a: gifts[2].clone(), b: y_wallet, lp: None };
    for mode in 0..=1 {
        let (a, b) = if mode == 0 { (100_000, 1_000_000_000) } else { (1_000_000, 10_000) };
        let first_step = initial_state(a, b, 30);
        let (first, _) = initialise_configured(
            &programs,
            &mut ledger,
            [x.clone(), y.clone()],
            &mut provider,
            &first_step,
            30,
            100 + mode,
            6100 + mode as u32 * 100,
        );
        let mut second_provider = Wallet { key: 0x61, a: provider.b.clone(), b: z_wallet, lp: None };
        let second_step = initial_state(a, b, 50);
        let (second, _) = initialise_configured(
            &programs,
            &mut ledger,
            [y.clone(), z.clone()],
            &mut second_provider,
            &second_step,
            50,
            200 + mode,
            6120 + mode as u32 * 100,
        );
        provider.b = second_provider.a;
        z_wallet = second_provider.b;
        let pools = [first, second];
        let limits = if mode == 0 { (1, 1) } else { (10235, 1) };
        let intent = prepare(&programs, &route, &mut ledger, &pools, &gifts[mode as usize], mode, limits, 6400 + mode as u32);
        let authority = ledger.seed_funding(0x63, 6500 + mode as u32);
        let (transaction, public) = substituted_pool_modes(&programs, &route, &ledger, &pools, &intent, &authority, mode);
        let accepted_pools = assert_real_peers_accept(&ledger, &transaction, &pools);
        ledger.reject(
            &transaction,
            &["Inputs[6].Lock"],
            if mode == 0 {
                "exact-input-route-rejects-two-valid-exact-output-legs-donating-quote-surplus"
            } else {
                "exact-output-route-rejects-valid-exact-input-plateau-overpayment"
            },
        );
        let record = ledger.records.last_mut().unwrap();
        assert!(
            record["error"].as_str().unwrap().contains("error code 5"),
            "the route must reject by its ordinary application condition: {record}"
        );
        record["separate_pool_group_replays"] = accepted_pools;
        record["independent_public_quotation"] = public;
        record["substituted_pool_mode"] = json!(if mode == 0 { 7 } else { 5 });
    }
    ledger.archive(&Path::new(env!("CARGO_MANIFEST_DIR")).join("target/finance-protocol-evidence/c3-mode-boundaries"), &all);
}
