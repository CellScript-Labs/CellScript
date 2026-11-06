//! Matched whole swaps compare the original bounded arithmetic with an ordinary
//! relation-library variant; changed executable identities are kept explicit.
use super::*;

fn replace_once(source: &mut String, before: &str, after: &str, patches: &mut Vec<Value>) {
    assert_eq!(source.matches(before).count(), 1, "the frozen experiment must match exactly one source fragment: {before}");
    *source = source.replacen(before, after, 1);
    patches.push(json!({"before":before,"after":after}));
}

fn relation_variant(programs: &Programs) -> (Program, Value) {
    let original = programs._directory.path().join("finance");
    let variant = programs._directory.path().join("relation-variant");
    copy_package(&original, &variant.join("finance"));
    copy_package(&programs._directory.path().join("finance_math"), &variant.join("finance_math"));
    let path = variant.join("finance/pool/src/main.cell");
    let original_source = std::fs::read_to_string(&path).unwrap();
    let mut source = original_source.clone();
    let mut patches = Vec::new();
    replace_once(
        &mut source,
        "use finance_math::{mul_div_floor, mul_div_ceil, isqrt}",
        "use finance_math::{mul_div_floor, mul_div_ceil, isqrt, verify_mul_div_floor}",
        &mut patches,
    );
    replace_once(
        &mut source,
        "witness min_a: u128, witness min_b: u128, witness owner: Hash,",
        "witness min_a: u128, witness min_b: u128, witness owner: Hash, witness quotient: u128, witness remainder: u128,",
        &mut patches,
    );
    replace_once(
        &mut source,
        "check_pool(mode, input_base, output_base, a_max, b_max, shares, min_a, min_b, owner)",
        "check_pool(mode, input_base, output_base, a_max, b_max, shares, min_a, min_b, owner, quotient, remainder)",
        &mut patches,
    );
    replace_once(&mut source, "private fn check_pool(mode: u64, ib: u64, ob: u64, a_max: u128, b_max: u128, shares: u128, min_a: u128, min_b: u128, owner: Hash) -> bool {",
        "private fn check_pool(mode: u64, ib: u64, ob: u64, a_max: u128, b_max: u128, shares: u128, min_a: u128, min_b: u128, owner: Hash, quotient: u128, remainder: u128) -> bool {", &mut patches);
    replace_once(&mut source, "    if mode == 1 {\n        if a_max == 0 || b_max != 0 || shares != 0 || min_a != 0 || provided_a < a_max { return false }\n        let scaled = a_max * (10000 - fee) as u128\n        let received = mul_div_floor(b, scaled, a * 10000 + scaled)",
        "    if mode == 1 {\n        if a_max == 0 || b_max != 0 || shares != 0 || min_a != 0 || provided_a < a_max { return false }\n        let scaled = a_max * (10000 - fee) as u128\n        if !verify_mul_div_floor(b, scaled, a * 10000 + scaled, quotient, remainder) { return false }\n        let received = quotient", &mut patches);
    std::fs::write(&path, &source).unwrap();
    let program = Program::compile(&variant.join("finance/pool"), "pool-relation", false);
    assert_ne!(program.code(), programs.pool.code());
    (
        program,
        json!({"original_source":original_source,"variant_source":source,"patches":patches,
        "original_source_hash":hex::encode(cellscript_artifact_checker::ckb_blake2b256(original_source.as_bytes())),
        "variant_source_hash":hex::encode(cellscript_artifact_checker::ckb_blake2b256(source.as_bytes())),
        "production_source_changed":false,"ordinary_library":"finance_math::verify_mul_div_floor"}),
    )
}

fn entry(pool: &Program, relation: bool, mut values: Vec<Arg>, quotient: u128, remainder: u128) -> packed::Bytes {
    if relation {
        values.extend([Arg::U128(quotient), Arg::U128(remainder)]);
    }
    pool.witness(&values)
}

fn initialise_matched(
    programs: &Programs,
    pool_program: &Program,
    relation: bool,
    ledger: &mut Ledger,
    owner: &mut Wallet,
    assets: [packed::Script; 2],
    all: &[&Program],
    namespace: &str,
    expected: &Value,
) -> Pool {
    let funding = ledger.seed_funding(owner.key, 100);
    let bootstrap_funding = ledger.seed_funding(owner.key, 101);
    let first = packed::CellInput::new_builder().previous_output(funding.clone()).build();
    let anchor = cellscript_artifact_checker::ckb_blake2b256(&[first.as_slice(), &0u64.to_le_bytes()].concat());
    let identity = packed::Script::new_builder()
        .code_hash(cellscript::ckb_abi::type_id::CODE_HASH)
        .hash_type(1u8)
        .args(Bytes::copy_from_slice(&anchor).pack())
        .build();
    let lp_configuration =
        [identity.calc_script_hash().as_slice(), packed::CellOutput::calc_data_hash(pool_program.code()).as_slice()].concat();
    let mut pool = Pool {
        points: vec![],
        identity,
        controller: pool_program.script(&[]),
        lp: programs.lp.script(&cellscript_artifact_checker::ckb_blake2b256(&lp_configuration)),
        minimum: programs.minimum.script(&[]),
        assets,
        fee: 30,
        nonce: 1,
        revision: 0,
    };
    let supply = number(expected, "supply");
    let state = pool_data(pool_program, &pool, 100000, 500000, supply, 0);
    pool.controller = pool_program.script(&cellscript_artifact_checker::ckb_blake2b256(&state[..208]));
    let bootstrap = ledger.build(
        &[bootstrap_funding],
        vec![cell(pool.controller.clone(), None, state.clone()), cell(owner_lock(owner.key), None, vec![])],
        1,
        vec![],
    );
    let bootstrap = ledger.commit(&bootstrap, "whole-math-comparison-bootstrap")[0].clone();
    let genesis = ledger.build(
        &[funding, bootstrap, owner.a.clone(), owner.b.clone()],
        vec![
            cell(pool.controller.clone(), Some(pool.identity.clone()), state),
            reserve(&programs.reserve, &pool, 0, 100000),
            reserve(&programs.reserve, &pool, 1, 500000),
            token(&programs.asset, &pool.assets[0], owner.key, value(ledger, &owner.a) - 100000),
            token(&programs.asset, &pool.assets[1], owner.key, value(ledger, &owner.b) - 500000),
            shares(&programs.lp, &pool, owner_lock(owner.key), supply - 1000),
            shares(&programs.lp, &pool, pool.minimum.clone(), 1000),
            cell(owner_lock(owner.key), None, vec![]),
        ],
        7,
        vec![
            Bytes::new().pack(),
            entry(
                pool_program,
                relation,
                vec![
                    Arg::U64(0),
                    Arg::U64(1),
                    Arg::U64(0),
                    Arg::U128(100000),
                    Arg::U128(500000),
                    Arg::U128(supply - 1000),
                    Arg::U128(0),
                    Arg::U128(0),
                    Arg::Hash(owner_lock(owner.key).calc_script_hash().unpack()),
                ],
                0,
                0,
            ),
            programs.asset.witness(&[]),
            programs.asset.witness(&[]),
            Bytes::new().pack(),
            programs.lp.witness(&[]),
        ],
    );
    let points = checked_commit_with(namespace, all, ledger, &genesis, "whole-math-comparison-authentic-genesis");
    pool.points = points[..3].to_vec();
    owner.a = points[3].clone();
    owner.b = points[4].clone();
    owner.lp = Some(points[5].clone());
    pool
}

fn matched_swap(
    programs: &Programs,
    pool_program: &Program,
    relation: bool,
    ledger: &mut Ledger,
    pool: &Pool,
    owner: &Wallet,
    expected: &Value,
    quotient: u128,
    remainder: u128,
) -> TransactionView {
    let funding = ledger.seed_funding(owner.key, 200);
    let selected = [pool.points.clone(), vec![funding, owner.a.clone(), owner.b.clone()]].concat();
    let created = vec![
        cell(
            pool.controller.clone(),
            Some(pool.identity.clone()),
            pool_data(pool_program, pool, number(expected, "a"), number(expected, "b"), number(expected, "supply"), 1),
        ),
        reserve(&programs.reserve, pool, 0, number(expected, "a")),
        reserve(&programs.reserve, pool, 1, number(expected, "b")),
        token(&programs.asset, &pool.assets[0], owner.key, value(ledger, &owner.a) - 1234),
        token(&programs.asset, &pool.assets[1], owner.key, value(ledger, &owner.b) + number(expected, "received")),
        cell(owner_lock(owner.key), None, vec![]),
    ];
    ledger.build(
        &selected,
        created,
        5,
        vec![
            entry(
                pool_program,
                relation,
                vec![
                    Arg::U64(1),
                    Arg::U64(0),
                    Arg::U64(0),
                    Arg::U128(1234),
                    Arg::U128(0),
                    Arg::U128(0),
                    Arg::U128(0),
                    Arg::U128(number(expected, "received")),
                    Arg::Hash(owner_lock(owner.key).calc_script_hash().unpack()),
                ],
                quotient,
                remainder,
            ),
            programs.asset.witness(&[]),
            programs.asset.witness(&[]),
        ],
    )
}

#[test]
fn whole_signed_swap_compares_direct_arithmetic_and_checked_quotient_relation() {
    let programs = Programs::compile();
    let (variant, patches) = relation_variant(&programs);
    let model = reference(100000, 500000, 30, vec![json!({"kind":"swap_a","amount":"1234"})]);
    let quotient = number(&model[1], "received");
    let scaled = 1234u128 * 9970;
    let denominator = 100000u128 * 10000 + scaled;
    let remainder = 500000u128 * scaled % denominator;
    let mut observations = Vec::new();
    for (relation, pool_program, namespace) in [(false, &programs.pool, "pool-math-direct"), (true, &variant, "pool-math-relation")] {
        let all = [&programs.asset, &programs.reserve, &programs.lp, &programs.minimum, pool_program];
        let mut ledger = Ledger::new(&all, &[0x61, 0x62, 0x71, 0x72]);
        let (a, wa) = issue(&mut ledger, &programs.asset, 0x71);
        let (b, wb) = issue(&mut ledger, &programs.asset, 0x72);
        let mut owner = Wallet { key: 0x61, a: wa[0].clone(), b: wb[0].clone(), lp: None };
        let pool = initialise_matched(&programs, pool_program, relation, &mut ledger, &mut owner, [a, b], &all, namespace, &model[0]);
        let transaction = matched_swap(&programs, pool_program, relation, &mut ledger, &pool, &owner, &model[1], quotient, remainder);
        if relation {
            for (field, q, r) in
                [("quotient", quotient + 1, remainder), ("remainder", quotient, if remainder == 0 { 1 } else { remainder - 1 })]
            {
                let mut entries = transaction.witnesses().into_iter().collect::<Vec<_>>();
                entries[0] = entry(
                    pool_program,
                    true,
                    vec![
                        Arg::U64(1),
                        Arg::U64(0),
                        Arg::U64(0),
                        Arg::U128(1234),
                        Arg::U128(0),
                        Arg::U128(0),
                        Arg::U128(0),
                        Arg::U128(quotient),
                        Arg::Hash(owner_lock(owner.key).calc_script_hash().unpack()),
                    ],
                    q,
                    r,
                );
                ledger.reject(
                    &transaction.as_advanced_builder().set_witnesses(entries).build(),
                    &["Inputs[0].Lock"],
                    &format!("whole-swap-forged-{field}-relation-rejects"),
                );
                assert!(ledger.records.last().unwrap()["error"].as_str().unwrap().contains("error code 49 "));
            }
        }
        // The low half is the exact admitted input. Silently narrowing this
        // witness would accept the unchanged valid control transaction.
        let mut high_input = transaction.witnesses().into_iter().collect::<Vec<_>>();
        high_input[0] = entry(
            pool_program,
            relation,
            vec![
                Arg::U64(1),
                Arg::U64(0),
                Arg::U64(0),
                Arg::U128((1u128 << 64) + 1234),
                Arg::U128(0),
                Arg::U128(0),
                Arg::U128(0),
                Arg::U128(quotient),
                Arg::Hash(owner_lock(owner.key).calc_script_hash().unpack()),
            ],
            quotient,
            remainder,
        );
        ledger.reject(
            &transaction.as_advanced_builder().set_witnesses(high_input).build(),
            &["Inputs[0].Lock"],
            "whole-swap-high-u128-input-cannot-narrow-to-valid-low-half",
        );
        assert!(ledger.records.last().unwrap()["error"].as_str().unwrap().contains("error code 5 "));
        let points = checked_commit_with(namespace, &all, &mut ledger, &transaction, "whole-signed-swap-mathematical-output-agrees");
        assert_eq!(u128::from_le_bytes(ledger.live[&points[0]].1[208..224].try_into().unwrap()), number(&model[1], "a"));
        assert_eq!(u128::from_le_bytes(ledger.live[&points[0]].1[224..240].try_into().unwrap()), number(&model[1], "b"));
        let record = ledger.records.last().unwrap();
        observations.push(json!({"variant":if relation{"witness-relation"}else{"original-direct"},"namespace":namespace,
            "whole_transaction_cycles":record["transaction_cycles"],"transaction_bytes":transaction.data().as_slice().len(),
            "witness_bytes":transaction.witnesses().into_iter().map(|w|w.raw_data().len()).sum::<usize>(),
            "pool_elf_bytes":pool_program.code().len(),"application_elf_bytes":all.iter().map(|p|p.code().len()).sum::<usize>(),
            "pool_elf_hash":hex::encode(cellscript_artifact_checker::ckb_blake2b256(pool_program.code())),
            "pool_identity":hex::encode(pool.identity.as_slice()),"pool_controller":hex::encode(pool.controller.as_slice()),
            "lp_identity":hex::encode(pool.lp.as_slice()),"reserve_locks":transaction.outputs().into_iter().skip(1).take(2).map(|o|hex::encode(o.lock().as_slice())).collect::<Vec<_>>(),
            "asset_identities":pool.assets.iter().map(|a|hex::encode(a.as_slice())).collect::<Vec<_>>(),
            "authorizer":hex::encode(owner_lock(owner.key).as_slice()),"transaction_hash":hex::encode(transaction.hash().as_slice()),
            "serialized_transaction_hash":hex::encode(cellscript_artifact_checker::ckb_blake2b256(transaction.data().as_slice())),
            "independent_model":model[1],"quotient":quotient.to_string(),"remainder":remainder.to_string(),
            "standard_signature":"actual-upstream-secp-sighash-all","generated_entry_argument_count":if relation{11}else{9},
            "vm_stack":null,"node_acceptance":null}));
        ledger.archive(&Path::new(env!("CARGO_MANIFEST_DIR")).join("target/finance-protocol-evidence").join(namespace), &all);
    }
    assert_eq!(observations[0]["asset_identities"], observations[1]["asset_identities"]);
    assert_eq!(observations[0]["pool_identity"], observations[1]["pool_identity"]);
    assert_eq!(observations[0]["authorizer"], observations[1]["authorizer"]);
    assert_ne!(observations[0]["pool_controller"], observations[1]["pool_controller"]);
    let report = json!({"schema":"cellscript-finance-whole-operation-math-comparison-v1","operation":"A-for-B-basic-pool-swap",
        "profile":{"reserve_a":"100000","reserve_b":"500000","input_a":"1234","fee":"30","denominator":"10000"},
        "method":"two independent authentic deployment histories with matching economics, real signatures and all Script groups",
        "identity_boundary":"new pool ELF/ABI changes controllers, LP configuration, reserve Locks, predecessors and serialized signed transactions; these are not byte-identical operations",
        "source_variant":patches,"observations":observations,"financial_profile_changed":false,"global_algorithm_winner":null,
        "scope":"one matched operation and two hostile relation witnesses; not a universal performance bound or node benchmark"});
    std::fs::write(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/finance-math-operation-comparison.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
