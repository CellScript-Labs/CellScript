//! Direct admission/role negatives and exact LP-minimum/maximum-deposit bounds.
use super::*;

fn issue_domain(ledger: &mut Ledger, asset: &Program, issuer: u8) -> (packed::Script, [packed::OutPoint; 2]) {
    let identity = asset.script(owner_lock(issuer).calc_script_hash().as_slice());
    let funding = ledger.seed_funding(issuer, 8000);
    let transaction = ledger.build(
        &[funding],
        vec![token(asset, &identity, 0x61, MAXIMUM), token(asset, &identity, 0x62, MAXIMUM), cell(owner_lock(issuer), None, vec![])],
        2,
        vec![asset.witness(&[])],
    );
    let points = ledger.commit(&transaction, &format!("issue-pool-admission-enforcing-asset-{issuer}"));
    (identity, [points[0].clone(), points[1].clone()])
}

fn require_group_acceptance(
    ledger: &Ledger,
    transaction: &TransactionView,
    identity: &packed::Script,
    role: ckb_testtool::ckb_script::ScriptGroupType,
) -> Value {
    if identity.code_hash() == cellscript::ckb_abi::type_id::CODE_HASH.into() && u8::from(identity.hash_type()) == 1 {
        use ckb_testtool::{
            ckb_chain_spec::consensus::ConsensusBuilder,
            ckb_script::{TransactionScriptsVerifier, TxVerifyEnv},
            ckb_types::core::{
                hardfork::{HardForks, CKB2021, CKB2023},
                HeaderBuilder,
            },
        };
        use std::sync::Arc;
        let resolved = cost_measurement::resolve_transaction(&ledger.context, transaction).unwrap();
        let consensus = ConsensusBuilder::default()
            .hardfork_switch(HardForks { ckb2021: CKB2021::new_dev_default(), ckb2023: CKB2023::new_dev_default() })
            .build();
        let verifier = TransactionScriptsVerifier::new(
            Arc::new(resolved),
            ledger.context.clone(),
            Arc::new(consensus),
            Arc::new(TxVerifyEnv::new_submit(&HeaderBuilder::default().number(0).build())),
        );
        let cycles = verifier
            .verify_single(role, &identity.calc_script_hash(), 100_000_000)
            .expect("the actual native Type ID must accept this same genesis transaction");
        return json!({"script_hash":hex::encode(identity.calc_script_hash().as_slice()),"role":"type",
            "implementation":"native-system-script","cycles":cycles,"exit_code":0,"guest_stack":null});
    }
    let observed = cost_measurement::measure_group(&ledger.context, transaction, identity, role, 100_000_000);
    assert!(
        matches!(observed.observation, cost_measurement::CycleObservation::Measured { exit_code: 0, .. }),
        "this independent peer must accept the same hostile transaction: {observed:?}"
    );
    serde_json::to_value(observed).unwrap()
}

fn policy_rejection(ledger: &mut Ledger, transaction: &TransactionView, groups: &[&str], code: u64, name: &str) {
    ledger.reject(transaction, groups, name);
    let error = ledger.records.last().unwrap()["error"].as_str().unwrap();
    assert!(error.contains(&format!("error code {code} ")), "{name}: expected exact policy code {code}, observed {error}");
}

fn exact_minimum_rejects(programs: &Programs, ledger: &mut Ledger, assets: [packed::Script; 2], owner: &Wallet) {
    let funding = ledger.seed_funding(owner.key, 9000);
    let bootstrap_funding = ledger.seed_funding(owner.key, 9001);
    let first = packed::CellInput::new_builder().previous_output(funding.clone()).build();
    let anchor = cellscript_artifact_checker::ckb_blake2b256(&[first.as_slice(), &0u64.to_le_bytes()].concat());
    let identity = packed::Script::new_builder()
        .code_hash(cellscript::ckb_abi::type_id::CODE_HASH)
        .hash_type(1u8)
        .args(Bytes::copy_from_slice(&anchor).pack())
        .build();
    let lp_configuration =
        [identity.calc_script_hash().as_slice(), packed::CellOutput::calc_data_hash(programs.pool.code()).as_slice()].concat();
    let mut pool = Pool {
        points: vec![],
        identity,
        controller: programs.pool.script(&[]),
        lp: programs.lp.script(&cellscript_artifact_checker::ckb_blake2b256(&lp_configuration)),
        minimum: programs.minimum.script(&[]),
        assets,
        fee: 30,
        nonce: 9000,
        revision: 0,
    };
    let state = pool_data(&programs.pool, &pool, 1000, 1000, 1000, 0);
    pool.controller = programs.pool.script(&cellscript_artifact_checker::ckb_blake2b256(&state[..208]));
    let bootstrap = ledger.build(
        &[bootstrap_funding],
        vec![cell(pool.controller.clone(), None, state.clone()), cell(owner_lock(owner.key), None, vec![])],
        1,
        vec![],
    );
    let bootstrap = ledger.commit(&bootstrap, "create-exact-minimum-pool-bootstrap")[0].clone();
    let transaction = ledger.build(
        &[funding, bootstrap, owner.a.clone(), owner.b.clone()],
        vec![
            cell(pool.controller.clone(), Some(pool.identity.clone()), state),
            reserve(&programs.reserve, &pool, 0, 1000),
            reserve(&programs.reserve, &pool, 1, 1000),
            token(&programs.asset, &pool.assets[0], owner.key, value(ledger, &owner.a) - 1000),
            token(&programs.asset, &pool.assets[1], owner.key, value(ledger, &owner.b) - 1000),
            shares(&programs.lp, &pool, owner_lock(owner.key), 0),
            shares(&programs.lp, &pool, pool.minimum.clone(), 1000),
            cell(owner_lock(owner.key), None, vec![]),
        ],
        7,
        vec![
            Bytes::new().pack(),
            witness(&programs.pool, 0, 1, 1000, 1000, 0, 0, 0, owner.key),
            programs.asset.witness(&[]),
            programs.asset.witness(&[]),
            Bytes::new().pack(),
            programs.lp.witness(&[]),
        ],
    );
    let valid_anchor = require_group_acceptance(ledger, &transaction, &pool.identity, ckb_testtool::ckb_script::ScriptGroupType::Type);
    let valid_lp = require_group_acceptance(ledger, &transaction, &pool.lp, ckb_testtool::ckb_script::ScriptGroupType::Type);
    policy_rejection(
        ledger,
        &transaction,
        &["Inputs[1].Lock"],
        5,
        "genesis-at-exact-locked-minimum-rejects-with-valid-anchor-and-lp-group",
    );
    ledger.records.last_mut().unwrap()["separate_valid_genesis_peers"] = json!([valid_anchor, valid_lp]);
}

fn maximum_deposit(programs: &Programs, ledger: &mut Ledger, pool: &mut Pool, owner: &mut Wallet, expected: &Value) {
    let maximum = MAXIMUM - 1001;
    let quotation = quote::deposit(quoted_state(ledger, pool), maximum, maximum, maximum).unwrap();
    assert_model_state(quotation.next, expected);
    assert_eq!((quotation.minted, quotation.used_a, quotation.used_b), (maximum, maximum, maximum));
    let funding = ledger.seed_funding(owner.key, 9200);
    let selected = [pool.points.clone(), vec![funding, owner.a.clone(), owner.b.clone()]].concat();
    let created = vec![
        cell(pool.controller.clone(), Some(pool.identity.clone()), pool_data(&programs.pool, pool, MAXIMUM, MAXIMUM, MAXIMUM, 1)),
        reserve(&programs.reserve, pool, 0, MAXIMUM),
        reserve(&programs.reserve, pool, 1, MAXIMUM),
        token(&programs.asset, &pool.assets[0], owner.key, value(ledger, &owner.a) - maximum),
        token(&programs.asset, &pool.assets[1], owner.key, value(ledger, &owner.b) - maximum),
        shares(&programs.lp, pool, owner_lock(owner.key), maximum),
        cell(owner_lock(owner.key), None, vec![]),
    ];
    let transaction = ledger.build(
        &selected,
        created,
        6,
        vec![
            witness(&programs.pool, 3, 0, maximum, maximum, maximum, 0, 0, owner.key),
            programs.asset.witness(&[]),
            programs.asset.witness(&[]),
        ],
    );
    let points =
        checked_commit(programs, ledger, &transaction, "minimum-live-pool-accepts-maximum-deposit-to-reserve-and-supply-ceilings");
    pool.points = points[..3].to_vec();
    pool.revision = 1;
    owner.a = points[3].clone();
    owner.b = points[4].clone();
    owner.lp = Some(points[5].clone());
    assert_eq!((value(ledger, &owner.a), value(ledger, &owner.b)), (1001, 1001));
}

fn direct_identity_and_role_negatives(
    programs: &Programs,
    ledger: &mut Ledger,
    pool: &Pool,
    owner: &mut Wallet,
    foreign: &packed::OutPoint,
    foreign_identity: &packed::Script,
    expected: &Value,
) {
    let unsolicited = unsolicited_reserve(programs, ledger, pool, owner);
    // A genuine holder can create a well-formed custody-sized token Cell under
    // its standard Lock. It has real B value, but no reserve-spending authority.
    let counterfeit_capacity = ledger.seed_funding(owner.key, 9401);
    let old_b = value(ledger, &pool.points[2]);
    let data = programs.asset.data(
        "CustodyData",
        &[
            ("amount", old_b.to_le_bytes().to_vec()),
            ("subject", pool.identity.calc_script_hash().as_slice().to_vec()),
            ("controller", pool.controller.calc_script_hash().as_slice().to_vec()),
            ("asset", pool.assets[1].calc_script_hash().as_slice().to_vec()),
        ],
    );
    let holder_cell = ledger.build(
        &[owner.b.clone(), counterfeit_capacity],
        vec![
            cell(owner_lock(owner.key), Some(pool.assets[1].clone()), data),
            token(&programs.asset, &pool.assets[1], owner.key, value(ledger, &owner.b) - old_b),
            cell(owner_lock(owner.key), None, vec![]),
        ],
        2,
        vec![programs.asset.witness(&[])],
    );
    let holder_cell = ledger.commit(&holder_cell, "fund-real-b-custody-sized-cell-with-standard-holder-authority");
    owner.b = holder_cell[1].clone();
    let valid = swap_transaction(programs, ledger, pool, owner, expected, 9400);
    let reserve_amount = number(expected, "a");
    let all_inputs = inputs(&valid);
    let all_outputs = cells(&valid);

    // Different issuer args mean a different full asset identity, even though
    // the same genuine enforcing code validates both complete Type groups.
    let mut selected = all_inputs.clone();
    selected.push(foreign.clone());
    let mut created = all_outputs.clone();
    created[1].0 = created[1].0.clone().as_builder().type_(Some(foreign_identity.clone()).pack()).build();
    created.push(token(&programs.asset, &pool.assets[0], owner.key, reserve_amount));
    created.push(token(&programs.asset, foreign_identity, owner.key, value(ledger, foreign) - reserve_amount));
    let mut entries = valid.witnesses().into_iter().collect::<Vec<_>>();
    entries.resize(7, Bytes::new().pack());
    entries[6] = programs.asset.witness(&[]);
    let foreign_asset = ledger.build(&selected, created, 5, entries);
    let a = require_group_acceptance(ledger, &foreign_asset, &pool.assets[0], ckb_testtool::ckb_script::ScriptGroupType::Type);
    let c = require_group_acceptance(ledger, &foreign_asset, foreign_identity, ckb_testtool::ckb_script::ScriptGroupType::Type);
    policy_rejection(
        ledger,
        &foreign_asset,
        &["Inputs[0].Lock"],
        17,
        "full-asset-identity-substitution-rejects-despite-two-conserved-enforcing-types",
    );
    ledger.records.last_mut().unwrap()["separate_valid_asset_groups"] = json!([a, c]);

    let mut created = all_outputs.clone();
    created[1].0 = created[1].0.clone().as_builder().lock(owner_lock(owner.key)).build();
    let alternate_policy = ledger.build(&all_inputs, created, 5, valid.witnesses().into_iter().collect());
    policy_rejection(
        ledger,
        &alternate_policy,
        &["Inputs[0].Lock"],
        41,
        "reserve-successor-cannot-substitute-authentic-owner-lock-for-custody-policy",
    );

    // The omitted B reserve remains live. The real owner supplies its entire
    // successor from a signed B wallet, preserving B without authorising the
    // absent custody role as recognised inventory.
    let mut selected = all_inputs.clone();
    selected[2] = holder_cell[0].clone();
    assert!(!selected.contains(&pool.points[2]));
    let omitted = ledger.build(&selected, all_outputs.clone(), 5, valid.witnesses().into_iter().collect());
    let b = require_group_acceptance(ledger, &omitted, &pool.assets[1], ckb_testtool::ckb_script::ScriptGroupType::Type);
    policy_rejection(
        ledger,
        &omitted,
        &["Inputs[0].Lock"],
        41,
        "omitted-authentic-b-reserve-rejects-even-when-owner-funds-conserved-successor",
    );
    ledger.records.last_mut().unwrap()["separate_valid_asset_group"] = b;

    let mut selected = all_inputs;
    selected.push(unsolicited.clone());
    let mut created = all_outputs;
    created.push(token(&programs.asset, &pool.assets[0], owner.key, value(ledger, &unsolicited)));
    let duplicated = ledger.build(&selected, created, 5, valid.witnesses().into_iter().collect());
    let a = require_group_acceptance(ledger, &duplicated, &pool.assets[0], ckb_testtool::ckb_script::ScriptGroupType::Type);
    policy_rejection(
        ledger,
        &duplicated,
        &["Inputs[1].Lock"],
        5,
        "duplicated-reserve-group-rejects-real-unsolicited-extra-input-with-balanced-refund",
    );
    ledger.records.last_mut().unwrap()["separate_valid_asset_group"] = a;
    checked_commit(programs, ledger, &valid, "admission-negative-controls-authentic-swap-still-accepts");
}

#[test]
fn pool_admission_proves_full_identities_complete_roles_and_exact_liquidity_bounds() {
    let mut programs = Programs::compile();
    programs.evidence_namespace = "pool-admission";
    let mut ledger = Ledger::new(&programs.all(), &[0x61, 0x62, 0x71, 0x72, 0x73]);
    let (a, wa) = issue_domain(&mut ledger, &programs.asset, 0x71);
    let (b, wb) = issue_domain(&mut ledger, &programs.asset, 0x72);
    let (c, wc) = issue_domain(&mut ledger, &programs.asset, 0x73);
    assert_eq!(a.code_hash(), c.code_hash());
    assert_ne!(a.calc_script_hash(), c.calc_script_hash());
    let mut first = Wallet { key: 0x61, a: wa[0].clone(), b: wb[0].clone(), lp: None };
    let mut second = Wallet { key: 0x62, a: wa[1].clone(), b: wb[1].clone(), lp: None };
    exact_minimum_rejects(&programs, &mut ledger, [a.clone(), b.clone()], &first);
    let minimum =
        reference(1001, 1001, 30, vec![json!({"kind":"deposit","a":(MAXIMUM-1001).to_string(),"b":(MAXIMUM-1001).to_string()})]);
    let (mut minimum_pool, locked) =
        initialise_configured(&programs, &mut ledger, [a.clone(), b.clone()], &mut first, &minimum[0], 30, 91, 9100);
    assert_eq!(value(&ledger, first.lp.as_ref().unwrap()), 1);
    assert_eq!(value(&ledger, &locked), 1000);
    maximum_deposit(&programs, &mut ledger, &mut minimum_pool, &mut second, &minimum[1]);
    assert_eq!(quoted_state(&ledger, &minimum_pool).total_lp, MAXIMUM);
    let representative = reference(100000, 500000, 30, vec![json!({"kind":"swap_a","amount":"10"})]);
    let (pool, _) = initialise_configured(&programs, &mut ledger, [a, b], &mut first, &representative[0], 30, 93, 9300);
    direct_identity_and_role_negatives(&programs, &mut ledger, &pool, &mut first, &wc[0], &c, &representative[1]);
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/finance-protocol-evidence/pool-admission");
    ledger.archive(&directory, &programs.all());
    std::fs::write(directory.join("independent-model.json"), serde_json::to_vec_pretty(&json!({
        "minimum_initialisation_and_maximum_deposit":minimum,"representative_swap":representative,
        "genesis_rejects_total_lp":1000,"genesis_minimum_admitted_total_lp":1001,
        "maximum_final_reserve_a":MAXIMUM.to_string(),"maximum_final_reserve_b":MAXIMUM.to_string(),"maximum_final_total_lp":MAXIMUM.to_string(),
        "source_invariant_changes":false,"node_acceptance":null,"outside_developer":null,
    })).unwrap()).unwrap();
}
