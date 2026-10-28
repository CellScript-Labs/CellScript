//! Application-owned two-pool fixture. No financial names or predicates are
//! special-cased by the compiler. Each pool anchors its own state with Type ID.

use super::{code_candidate, lock, package, source_policy_records};
use camino::Utf8Path;
use cellscript::package::frozen_interface::{
    compile_module, freeze_code_catalog, freeze_instance_policy, freeze_package_sources, freeze_source_code_policy,
    resolve_source_catalog, EntrySelection, FrozenInstancePolicy, FrozenPackageModule,
};
use cellscript::CompileOptions;
use cellscript_artifact_checker::{
    code_origin::{SuppliedDependencyCell, SuppliedInputCell, SuppliedTypeGroupTransaction},
    instance_policy::{encode_selection, ArgsConstraint, ConsumerArgs, InstanceMode, InstancePolicy},
    open_handle_policy::{AuthorizationSet, ScriptRole},
};
use ckb_testtool::{
    ckb_types::{bytes::Bytes, core::Capacity, core::TransactionBuilder, core::TransactionView, packed, prelude::*},
    context::Context,
};
use std::collections::HashMap;
use std::path::Path;

const ASSET: &str = include_str!("../fixtures/instance_policy/pool_asset.cell.in");
const RESERVE: &str = include_str!("../fixtures/instance_policy/pool_reserve.cell.in");
const POOL: &str = include_str!("../fixtures/instance_policy/pool.cell.in");
const OBSERVER: &str = include_str!("../fixtures/instance_policy/pool_observer.cell.in");
const CAPACITY: u64 = 100_000_000_000;
const FEE: u64 = 100_000_000;
const CYCLES: u64 = 12_000_000;

type Cell = (packed::CellOutput, Bytes);

fn plain(key: u8, capacity: u64) -> packed::CellOutput {
    packed::CellOutput::new_builder().capacity(capacity).lock(super::instance_document::signer_lock(key)).build()
}

fn script(code: &[u8], args: &[u8]) -> packed::Script {
    packed::Script::new_builder()
        .code_hash(packed::CellOutput::calc_data_hash(code))
        .hash_type(4u8)
        .args(Bytes::copy_from_slice(args).pack())
        .build()
}

fn words(values: &[u64]) -> Vec<u8> {
    values.iter().flat_map(|value| value.to_le_bytes()).collect()
}

fn payload(values: &[u64]) -> packed::Bytes {
    packed::WitnessArgs::new_builder()
        .input_type(Some(Bytes::from([b"CSARGv1\0".as_slice(), &words(values)].concat())).pack())
        .build()
        .as_bytes()
        .pack()
}

struct Ledger {
    context: Context,
    live: HashMap<packed::OutPoint, Cell>,
    deps: Vec<packed::CellDep>,
    assets: Vec<packed::Script>,
    maximum_cycles: u64,
    records: Vec<serde_json::Value>,
}

impl Ledger {
    fn new(modules: &[&FrozenPackageModule]) -> Self {
        let mut context = Context::new_with_deterministic_rng();
        let mut points =
            modules.iter().map(|module| context.deploy_cell(Bytes::copy_from_slice(module.bundle()[0]))).collect::<Vec<_>>();
        for name in ["specs/cells/secp256k1_blake160_sighash_all", "specs/cells/secp256k1_data"] {
            points.push(context.deploy_cell(Bytes::copy_from_slice(&ckb_system_scripts_v0_6_0::BUNDLED_CELL.get(name).unwrap())));
        }
        Self {
            context,
            live: HashMap::new(),
            assets: Vec::new(),
            maximum_cycles: 0,
            records: Vec::new(),
            deps: points.into_iter().map(|point| packed::CellDep::new_builder().out_point(point).build()).collect(),
        }
    }

    fn seed(&mut self, key: u8, index: u32) -> packed::OutPoint {
        let output = plain(key, 100 * CAPACITY);
        let point = packed::OutPoint::new_builder().tx_hash(output.lock().calc_script_hash()).index(index).build();
        assert!(!self.live.contains_key(&point));
        self.context.create_cell_with_out_point(point.clone(), output.clone(), Bytes::new());
        self.live.insert(point.clone(), (output, Bytes::new()));
        point
    }

    fn transaction(
        &self,
        inputs: &[packed::OutPoint],
        mut outputs: Vec<Cell>,
        change: usize,
        witnesses: Vec<packed::Bytes>,
    ) -> TransactionView {
        let input_capacity: u64 = inputs.iter().map(|point| u64::from(self.live[point].0.capacity())).sum();
        let fixed: u64 =
            outputs.iter().enumerate().filter(|(index, _)| *index != change).map(|(_, cell)| u64::from(cell.0.capacity())).sum();
        outputs[change].0 = outputs[change].0.clone().as_builder().capacity(input_capacity - fixed - FEE).build();
        let tx = TransactionBuilder::default()
            .cell_deps(self.deps.clone())
            .inputs(inputs.iter().map(|point| packed::CellInput::new_builder().previous_output(point.clone()).build()))
            .outputs(outputs.iter().map(|cell| cell.0.clone()))
            .outputs_data(outputs.iter().map(|cell| cell.1.pack()))
            .witnesses(witnesses)
            .build();
        self.authorize(&tx)
    }

    fn authorize(&self, tx: &TransactionView) -> TransactionView {
        let mut witnesses = tx.witnesses().into_iter().collect::<Vec<_>>();
        witnesses.resize(witnesses.len().max(tx.inputs().len()).max(tx.outputs().len()), Bytes::new().pack());
        for asset in &self.assets {
            let inputs = tx
                .inputs()
                .into_iter()
                .enumerate()
                .filter_map(|(index, input)| {
                    let cell = &self.live[&input.previous_output()];
                    (cell.0.type_().to_opt().as_ref() == Some(asset))
                        .then(|| (index, u64::from_le_bytes(cell.1[..8].try_into().unwrap())))
                })
                .collect::<Vec<_>>();
            let outputs = tx
                .outputs()
                .into_iter()
                .enumerate()
                .filter_map(|(index, output)| {
                    (output.type_().to_opt().as_ref() == Some(asset)).then(|| {
                        (index, u64::from_le_bytes(tx.outputs_data().get(index).unwrap().raw_data()[..8].try_into().unwrap()))
                    })
                })
                .collect::<Vec<_>>();
            if inputs.is_empty() && outputs.is_empty() {
                continue;
            }
            assert!(inputs.len() <= 4 && outputs.len() <= 4);
            let mut amounts = [0u64; 8];
            for (index, (_, amount)) in inputs.iter().enumerate() {
                amounts[index] = *amount;
            }
            for (index, (_, amount)) in outputs.iter().enumerate() {
                amounts[4 + index] = *amount;
            }
            let index = inputs.first().or_else(|| outputs.first()).unwrap().0;
            witnesses[index] = payload(&amounts);
        }
        let mut signed = tx.as_advanced_builder().set_witnesses(witnesses).build();
        for key in [0x61, 0x62, 0x71, 0x72] {
            let owner = super::instance_document::signer_lock(key);
            let indices = tx
                .inputs()
                .into_iter()
                .enumerate()
                .filter_map(|(index, input)| (self.live[&input.previous_output()].0.lock() == owner).then_some(index))
                .collect::<Vec<_>>();
            if !indices.is_empty() {
                signed = super::instance_document::sign(&signed, &owner, indices, key);
            }
        }
        signed
    }

    fn commit(&mut self, tx: &TransactionView, name: &str) -> Vec<packed::OutPoint> {
        let input_capacity: u64 =
            tx.inputs().into_iter().map(|input| u64::from(self.live[&input.previous_output()].0.capacity())).sum();
        assert_eq!(input_capacity - tx.outputs().into_iter().map(|cell| u64::from(cell.capacity())).sum::<u64>(), FEE);
        let cycles = self.context.verify_tx(tx, CYCLES).unwrap_or_else(|error| panic!("pool transaction: {error:?}"));
        self.maximum_cycles = self.maximum_cycles.max(cycles);
        let mut record = super::instance_document::transaction_record(&self.context, tx, "accepted");
        record["case"] = serde_json::json!(name);
        record["cycles"] = serde_json::json!(cycles);
        record["transaction_bytes"] = serde_json::json!(tx.data().as_slice().len());
        record["input_capacity"] = serde_json::json!(input_capacity);
        record["fee"] = serde_json::json!(FEE);
        self.records.push(record);
        for input in tx.inputs() {
            self.live.remove(&input.previous_output()).unwrap();
        }
        tx.outputs()
            .into_iter()
            .enumerate()
            .map(|(index, output)| {
                let data = tx.outputs_data().get(index).unwrap().raw_data();
                assert!(
                    u64::from(output.capacity()) >= output.occupied_capacity(Capacity::bytes(data.len()).unwrap()).unwrap().as_u64()
                );
                let point = packed::OutPoint::new(tx.hash(), index as u32);
                self.context.create_cell_with_out_point(point.clone(), output.clone(), data.clone());
                self.live.insert(point.clone(), (output, data));
                point
            })
            .collect()
    }

    fn reject(&mut self, tx: &TransactionView, group: &str, name: &str) {
        let signed = self.authorize(tx);
        let input_capacity: u64 =
            signed.inputs().into_iter().map(|input| u64::from(self.live[&input.previous_output()].0.capacity())).sum();
        assert_eq!(input_capacity - signed.outputs().into_iter().map(|cell| u64::from(cell.capacity())).sum::<u64>(), FEE);
        let error = self.context.verify_tx(&signed, CYCLES).unwrap_err();
        assert!(group.split('|').any(|expected| format!("{error:?}").contains(expected)), "expected {group}: {error:?}");
        let mut record = super::instance_document::transaction_record(&self.context, &signed, "rejected");
        record["case"] = serde_json::json!(name);
        record["expected_groups"] = serde_json::json!(group.split('|').collect::<Vec<_>>());
        record["error"] = serde_json::json!(format!("{error:?}"));
        self.records.push(record);
    }
}

struct Pool {
    points: Vec<packed::OutPoint>,
    a: u64,
    b: u64,
    revision: u64,
    fee: u64,
}

fn custody(amount: u64, pool: &packed::Script, controller: &packed::Script, asset: &packed::Script, reserve: &[u8]) -> Cell {
    let configuration =
        [pool.calc_script_hash().as_slice(), controller.calc_script_hash().as_slice(), asset.calc_script_hash().as_slice()].concat();
    let lock = script(reserve, &cellscript_artifact_checker::ckb_blake2b256(&configuration));
    (
        packed::CellOutput::new_builder().capacity(CAPACITY).lock(lock).type_(Some(asset.clone()).pack()).build(),
        Bytes::from([words(&[amount]), configuration].concat()),
    )
}

fn create_pool(
    ledger: &mut Ledger,
    owner: u8,
    nonce: u64,
    fee: u64,
    wallets: [packed::OutPoint; 2],
    pool_code: &[u8],
    reserve_code: &[u8],
) -> Pool {
    let funding = ledger.seed(owner, 0x90);
    let bootstrap_funding = ledger.seed(owner, 0x91);
    let first_input = packed::CellInput::new_builder().previous_output(funding.clone()).build();
    let identity = cellscript_artifact_checker::ckb_blake2b256(&[first_input.as_slice(), &0u64.to_le_bytes()].concat());
    let mut type_id = [0u8; 32];
    type_id[25..].copy_from_slice(b"TYPE_ID");
    let state_type =
        packed::Script::new_builder().code_hash(type_id.pack()).hash_type(1u8).args(Bytes::copy_from_slice(&identity).pack()).build();
    let assets = [ledger.assets[0].clone(), ledger.assets[1].clone()];
    let owner_script = super::instance_document::signer_lock(owner);
    let config = [
        state_type.calc_script_hash().as_slice(),
        assets[0].calc_script_hash().as_slice(),
        assets[1].calc_script_hash().as_slice(),
        owner_script.calc_script_hash().as_slice(),
        &[0x11; 32],
        &words(&[fee, nonce]),
    ]
    .concat();
    assert_eq!(config.len(), 176);
    let controller = script(pool_code, &cellscript_artifact_checker::ckb_blake2b256(&config));
    let a = 1_000;
    let b = 5_000;
    let state_data = Bytes::from([config, words(&[a, b, 0])].concat());
    let bootstrap_output = packed::CellOutput::new_builder().capacity(CAPACITY).lock(controller.clone()).build();
    let bootstrap = ledger.transaction(
        &[bootstrap_funding],
        vec![(bootstrap_output.clone(), state_data.clone()), (plain(owner, CAPACITY), Bytes::new())],
        1,
        vec![],
    );
    let bootstrap_points = ledger.commit(&bootstrap, &format!("bootstrap-{nonce}"));
    let bootstrap = bootstrap_points[0].clone();
    let outputs = vec![
        (bootstrap_output.as_builder().type_(Some(state_type.clone()).pack()).build(), state_data),
        custody(a, &state_type, &controller, &assets[0], reserve_code),
        custody(b, &state_type, &controller, &assets[1], reserve_code),
        (plain(owner, CAPACITY), Bytes::new()),
        (ledger.live[&wallets[0]].0.clone(), Bytes::from(words(&[40_000 - a]))),
        (ledger.live[&wallets[1]].0.clone(), Bytes::from(words(&[40_000 - b]))),
    ];
    let genesis = ledger.transaction(
        &[funding, bootstrap, wallets[0].clone(), wallets[1].clone()],
        outputs,
        3,
        vec![Bytes::new().pack(), payload(&[a, b, a, b, 0, 1, 0, fee])],
    );
    // All owner/asset signatures remain valid. Only the first-input genesis
    // anchor changes, so the authentic built-in Type ID group must reject.
    let wrong_anchor = ledger.transaction(
        &[bootstrap_points[1].clone(), genesis.inputs().get(1).unwrap().previous_output(), wallets[0].clone(), wallets[1].clone()],
        genesis
            .outputs()
            .into_iter()
            .enumerate()
            .map(|(index, output)| (output, genesis.outputs_data().get(index).unwrap().raw_data()))
            .collect(),
        3,
        genesis.witnesses().into_iter().collect(),
    );
    ledger.reject(&wrong_anchor, "Outputs[0].Type", &format!("forged-genesis-anchor-{nonce}"));
    let points = ledger.commit(&genesis, &format!("genesis-{nonce}"));
    Pool { points, a, b, revision: 0, fee }
}

fn swap_transaction(ledger: &Ledger, pools: &[&Pool]) -> TransactionView {
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut witnesses = Vec::new();
    for pool in pools {
        let base = inputs.len();
        let deposited = 100;
        let net = deposited - deposited * pool.fee / 10_000;
        let received = pool.b * net / (pool.a + net);
        let mut cells = pool.points.iter().map(|point| ledger.live[point].clone()).collect::<Vec<_>>();
        let mut state = cells[0].1[..176].to_vec();
        state.extend(words(&[pool.a + deposited, pool.b - received, pool.revision + 1]));
        cells[0].1 = Bytes::from(state);
        for (index, amount) in [(1, pool.a + deposited), (2, pool.b - received)] {
            cells[index].1 = Bytes::from([words(&[amount]), cells[index].1[8..].to_vec()].concat());
        }
        for (index, delta, add) in [(4, deposited, false), (5, received, true)] {
            let before = u64::from_le_bytes(cells[index].1[..8].try_into().unwrap());
            cells[index].1 = Bytes::from(words(&[if add { before + delta } else { before - delta }]));
        }
        inputs.extend(pool.points.clone());
        outputs.extend(cells);
        witnesses.push(payload(&[
            pool.a,
            pool.b,
            pool.a + deposited,
            pool.b - received,
            pool.revision,
            base as u64,
            base as u64,
            pool.fee,
        ]));
        witnesses.extend((0..5).map(|_| Bytes::new().pack()));
    }
    ledger.transaction(&inputs, outputs, 3, witnesses)
}

fn observed(ledger: &Ledger, tx: &TransactionView, observations: &[(packed::OutPoint, Vec<u8>)]) -> TransactionView {
    let mut inputs = tx.inputs().into_iter().collect::<Vec<_>>();
    let mut outputs = tx.outputs().into_iter().collect::<Vec<_>>();
    let mut data = tx.outputs_data().into_iter().collect::<Vec<_>>();
    let mut witnesses = tx.witnesses().into_iter().collect::<Vec<_>>();
    for (point, selection) in observations {
        assert_eq!(inputs.len(), witnesses.len());
        inputs.push(packed::CellInput::new_builder().previous_output(point.clone()).build());
        outputs.push(plain(0x61, u64::from(ledger.live[point].0.capacity())));
        data.push(Bytes::new().pack());
        witnesses.push(
            packed::WitnessArgs::new_builder()
                .input_type(Some(Bytes::from([b"CSARGv1\0".as_slice(), selection].concat())).pack())
                .build()
                .as_bytes()
                .pack(),
        );
    }
    ledger.authorize(
        &tx.as_advanced_builder().set_inputs(inputs).set_outputs(outputs).set_outputs_data(data).set_witnesses(witnesses).build(),
    )
}

fn check_native(
    policy: &FrozenInstancePolicy,
    ledger: &Ledger,
    tx: &TransactionView,
    observations: &[(packed::OutPoint, Vec<u8>)],
) -> Vec<serde_json::Value> {
    let dep_cells = ledger.deps.iter().map(|dep| ledger.context.get_cell(&dep.out_point()).unwrap()).collect::<Vec<_>>();
    let deps = ledger
        .deps
        .iter()
        .zip(&dep_cells)
        .map(|(dep, (output, data))| SuppliedDependencyCell {
            out_point: dep.out_point().as_slice().try_into().unwrap(),
            output: output.as_slice(),
            data,
        })
        .collect::<Vec<_>>();
    let inputs = tx
        .inputs()
        .into_iter()
        .map(|input| {
            let cell = &ledger.live[&input.previous_output()];
            SuppliedInputCell {
                out_point: input.previous_output().as_slice().try_into().unwrap(),
                output: cell.0.as_slice(),
                data: &cell.1,
            }
        })
        .collect::<Vec<_>>();
    let transaction = tx.data();
    observations
        .iter()
        .map(|(point, selection)| {
            let committed = ledger.live[point].0.lock().args().raw_data();
            let checked = policy
                .check_selection(&committed, selection, &Default::default())
                .unwrap()
                .check_direct_dependency(transaction.raw().as_slice(), &deps, &Default::default())
                .unwrap()
                .check_script_group(
                    &SuppliedTypeGroupTransaction { full_transaction: transaction.as_slice(), dependencies: &deps, inputs: &inputs },
                    &Default::default(),
                )
                .unwrap();
            serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap()
        })
        .collect()
}

#[test]
fn two_pools_issue_assets_authenticate_genesis_and_swap_independently_and_together() {
    let directory = tempfile::tempdir().unwrap();
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let asset = compile(directory.path(), "asset", ASSET, opt, true);
        let reserve = compile(directory.path(), "reserve", RESERVE, opt, false);
        let mut type_id = [0u8; 32];
        type_id[25..].copy_from_slice(b"TYPE_ID");
        let source = POOL
            .replace("@RESERVE_CODE_HASH@", &literal(&cellscript_artifact_checker::ckb_blake2b256(reserve.bundle()[0])))
            .replace("@NETWORK_DOMAIN@", &literal(&[0x11; 32]))
            .replace("@TYPE_ID_CODE_HASH@", &literal(&type_id));
        let pool = compile(directory.path(), "library", &source, opt, false);
        let consumer = directory.path().join("consumer");
        package(&consumer, "client", OBSERVER, "[dependencies]\nlibrary = { path = \"../library\" }\n");
        lock(&consumer);
        let candidate = code_candidate(compile(directory.path(), "library", &source, opt, false), vec![1]);
        let catalog = freeze_code_catalog(pool, vec![candidate], &Default::default()).unwrap();
        let sources =
            resolve_source_catalog(freeze_package_sources(Utf8Path::from_path(&consumer).unwrap(), "dev").unwrap(), catalog).unwrap();
        let (mut header, members) = source_policy_records(&sources, false, 0);
        header.role = ScriptRole::Lock;
        let implementation = freeze_source_code_policy(sources, AuthorizationSet::new(header, &members).unwrap()).unwrap();
        let selection = implementation.policy().selection(&members[0].receipt).unwrap();
        let constraints = InstancePolicy {
            implementation_root: implementation.policy().root(),
            mode: InstanceMode::Constrained,
            args_length: 32,
            config_length: 0,
            constraints: vec![ArgsConstraint::Any { offset: 0, length: 32 }],
        };
        let policy = freeze_instance_policy(implementation, constraints).unwrap();
        let provider = &policy.implementation().sources().catalog().candidates()[0];
        let pool = provider.module();
        let observer = compile_module(
            Utf8Path::from_path(&consumer).unwrap(),
            "dev",
            CompileOptions { diagnostics: opt, ..Default::default() },
            EntrySelection::Lock("verify".into()),
        )
        .unwrap();
        let mut ledger = Ledger::new(&[&asset, &reserve, pool, &observer]);
        let creation = packed::RawTransaction::from_slice(provider.raw_transaction()).unwrap();
        let code_point = packed::OutPoint::new(creation.calc_tx_hash(), provider.origin().output_index());
        ledger.context.create_cell_with_out_point(
            code_point.clone(),
            creation.outputs().get(0).unwrap(),
            Bytes::copy_from_slice(pool.bundle()[0]),
        );
        ledger.deps[2] = packed::CellDep::new_builder().out_point(code_point).build();
        let mut wallets = Vec::new();
        for issuer in [0x71, 0x72] {
            let asset_script = script(asset.bundle()[0], super::instance_document::signer_lock(issuer).calc_script_hash().as_slice());
            ledger.assets.push(asset_script.clone());
            let funding = ledger.seed(issuer, 0x80);
            let mint = ledger.transaction(
                &[funding],
                vec![
                    (
                        plain(0x61, CAPACITY).as_builder().type_(Some(asset_script.clone()).pack()).build(),
                        Bytes::from(words(&[40_000])),
                    ),
                    (plain(0x62, CAPACITY).as_builder().type_(Some(asset_script).pack()).build(), Bytes::from(words(&[40_000]))),
                    (plain(issuer, CAPACITY), Bytes::new()),
                ],
                2,
                vec![],
            );
            wallets.push(ledger.commit(&mint, &format!("asset-issuance-{issuer}")));
        }
        let mut a = create_pool(
            &mut ledger,
            0x61,
            1,
            30,
            [wallets[0][0].clone(), wallets[1][0].clone()],
            pool.bundle()[0],
            reserve.bundle()[0],
        );
        let mut b = create_pool(
            &mut ledger,
            0x62,
            2,
            100,
            [wallets[0][1].clone(), wallets[1][1].clone()],
            pool.bundle()[0],
            reserve.bundle()[0],
        );
        assert_ne!(ledger.live[&a.points[0]].0.lock(), ledger.live[&b.points[0]].0.lock());
        let mut observer_outputs = Vec::new();
        let mut selections = Vec::new();
        for (instance, index) in [(&a, 0), (&a, 0), (&b, 0), (&b, 6)] {
            let controller = ledger.live[&instance.points[0]].0.lock();
            let committed = ConsumerArgs {
                policy_root: policy.policy().root().unwrap(),
                subject: controller.calc_script_hash().unpack(),
                config: vec![],
            }
            .encode()
            .unwrap();
            let wire = encode_selection(policy.policy(), &selection, &controller.args().raw_data()).unwrap();
            policy.check_selection(&committed, &wire, &Default::default()).unwrap();
            observer_outputs.push((
                packed::CellOutput::new_builder().capacity(CAPACITY).lock(script(observer.bundle()[0], &committed)).build(),
                Bytes::from(words(&[index])),
            ));
            selections.push(wire.to_vec());
        }
        observer_outputs.push((plain(0x61, CAPACITY), Bytes::new()));
        let observer_funding = ledger.seed(0x61, 0x92);
        let observer_creation = ledger.transaction(&[observer_funding], observer_outputs, 4, vec![]);
        let observers = ledger.commit(&observer_creation, "create-instance-observers");
        let mut native_records = Vec::new();
        // All of B's actual pool, custody, asset and holder predicates accept.
        // Only A's separately committed instance observation rejects B.
        let substituted = observed(&ledger, &swap_transaction(&ledger, &[&b]), &[(observers[0].clone(), selections[2].clone())]);
        ledger.reject(&substituted, "Inputs[6].Lock", "policy-valid-B-substituted-for-A");
        let valid_a = observed(&ledger, &swap_transaction(&ledger, &[&a]), &[(observers[0].clone(), selections[0].clone())]);
        // B has the same asset balances, so this substitution preserves both
        // actual asset Type groups. Pool A's custody commitment rejects it.
        for replaced in [1, 2, 3] {
            let mut inputs = valid_a.inputs().into_iter().collect::<Vec<_>>();
            inputs[replaced] = packed::CellInput::new_builder().previous_output(b.points[replaced].clone()).build();
            let groups = if replaced == 3 { "Inputs[0].Lock".to_owned() } else { format!("Inputs[0].Lock|Inputs[{replaced}].Lock") };
            ledger.reject(
                &valid_a.as_advanced_builder().set_inputs(inputs).build(),
                &groups,
                &format!("B-substituted-input-{replaced}"),
            );
        }
        let mut wrong_data = valid_a.outputs_data().into_iter().collect::<Vec<_>>();
        let mut state = wrong_data[0].raw_data().to_vec();
        state[160] ^= 1;
        wrong_data[0] = Bytes::from(state).pack();
        ledger.reject(
            &valid_a.as_advanced_builder().set_outputs_data(wrong_data).build(),
            "Inputs[0].Lock",
            "changed-fee-configuration",
        );
        let reserve_only = ledger.transaction(
            &[a.points[1].clone(), a.points[3].clone()],
            vec![
                (plain(0x61, CAPACITY).as_builder().type_(Some(ledger.assets[0].clone()).pack()).build(), Bytes::from(words(&[a.a]))),
                (plain(0x61, CAPACITY), Bytes::new()),
            ],
            1,
            vec![],
        );
        ledger.reject(&reserve_only, "Inputs[0].Lock", "reserve-spend-without-pool-state");
        let mut minted = valid_a.outputs_data().into_iter().collect::<Vec<_>>();
        let amount = u64::from_le_bytes(minted[4].raw_data()[..8].try_into().unwrap());
        minted[4] = Bytes::from(words(&[amount + 1])).pack();
        ledger.reject(
            &valid_a.as_advanced_builder().set_outputs_data(minted).build(),
            "Inputs[1].Type",
            "valid-admission-rejecting-asset-peer",
        );
        for (index, current) in [(0, &mut a), (2, &mut b)] {
            let tx =
                observed(&ledger, &swap_transaction(&ledger, &[current]), &[(observers[index].clone(), selections[index].clone())]);
            native_records.extend(check_native(&policy, &ledger, &tx, &[(observers[index].clone(), selections[index].clone())]));
            current.points = ledger.commit(&tx, &format!("independent-swap-{index}"))[..6].to_vec();
            current.a = u64::from_le_bytes(ledger.live[&current.points[0]].1[176..184].try_into().unwrap());
            current.b = u64::from_le_bytes(ledger.live[&current.points[0]].1[184..192].try_into().unwrap());
            current.revision += 1;
        }
        let concurrent = observed(
            &ledger,
            &swap_transaction(&ledger, &[&a, &b]),
            &[(observers[1].clone(), selections[1].clone()), (observers[3].clone(), selections[3].clone())],
        );
        native_records.extend(check_native(
            &policy,
            &ledger,
            &concurrent,
            &[(observers[1].clone(), selections[1].clone()), (observers[3].clone(), selections[3].clone())],
        ));
        ledger.commit(&concurrent, "concurrent-two-pool-swap");
        assert!(ledger.maximum_cycles < 7_000_000);
        assert!(concurrent.data().as_slice().len() < 9_000);
        eprintln!(
            "two real pools {opt}: max_cycles={} concurrent_bytes={}",
            ledger.maximum_cycles,
            concurrent.data().as_slice().len()
        );
        let evidence = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/instance-policy-evidence/pools").join(format!("{opt}"));
        std::fs::create_dir_all(&evidence).unwrap();
        for (name, module, source) in [
            ("asset", &asset, ASSET),
            ("reserve", &reserve, RESERVE),
            ("pool", pool, source.as_str()),
            ("observer", &observer, OBSERVER),
        ] {
            std::fs::write(evidence.join(format!("{name}.cell")), source).unwrap();
            for (suffix, bytes) in ["elf", "metadata.json", "lowering.json", "sourcemap.json"].into_iter().zip(module.bundle()) {
                std::fs::write(evidence.join(format!("{name}.{suffix}")), bytes).unwrap();
            }
            std::fs::write(evidence.join(format!("{name}.source-context.json")), module.context_bytes().unwrap()).unwrap();
        }
        let dependencies = ledger.deps.iter().enumerate().map(|(index, dep)| {
            let (output, data) = ledger.context.get_cell(&dep.out_point()).unwrap();
            let file = format!("dependency-{index}.bin");
            std::fs::write(evidence.join(&file), &data).unwrap();
            serde_json::json!({"out_point": cellscript_artifact_checker::hex_encode(dep.out_point().as_slice()), "output": cellscript_artifact_checker::hex_encode(output.as_slice()), "data_file": file, "data_hash": cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(&data))})
        }).collect::<Vec<_>>();
        std::fs::write(evidence.join("implementation-creation.molecule"), provider.raw_transaction()).unwrap();
        std::fs::write(evidence.join("instance-policy.molecule"), policy.policy().encode().unwrap()).unwrap();
        let report = serde_json::json!({
            "schema": "cellscript-instance-pool-evidence-v1", "scope": "local-all-group-ckb-vm", "diagnostics": opt.to_string(),
            "compiler_version": env!("CARGO_PKG_VERSION"), "maximum_cycles": ledger.maximum_cycles,
            "source_policy": policy.implementation().identity(), "instance_policy": policy.identity(),
            "cases": ledger.records, "dependencies": dependencies, "native_group_records": native_records,
        });
        std::fs::write(evidence.join("report.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        replay_evidence(&evidence);
    }
}

fn literal(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("\\x{byte:02x}")).collect()
}

fn compile(root: &Path, name: &str, source: &str, diagnostics: cellscript::CompilerDiagnostics, action: bool) -> FrozenPackageModule {
    let root = root.join(name);
    package(&root, name, source, "");
    lock(&root);
    compile_module(
        Utf8Path::from_path(&root).unwrap(),
        "dev",
        CompileOptions { diagnostics, ..Default::default() },
        if action { EntrySelection::Action("verify".into()) } else { EntrySelection::Lock("verify".into()) },
    )
    .unwrap_or_else(|error| panic!("{name} {diagnostics}: {error:?}"))
}

// Reconstruct a fresh VM from the retained bytes alone. This checks that the
// evidence can actually replay, without rebuilding source policies or signing
// new transactions (whose source snapshots can have different identities).
fn replay_evidence(directory: &Path) {
    let report: serde_json::Value = serde_json::from_slice(&std::fs::read(directory.join("report.json")).unwrap()).unwrap();
    let bytes = |value: &serde_json::Value| hex::decode(value.as_str().unwrap().trim_start_matches("0x")).unwrap();
    let mut context = Context::new_with_deterministic_rng();
    for dep in report["dependencies"].as_array().unwrap() {
        let data = std::fs::read(directory.join(dep["data_file"].as_str().unwrap())).unwrap();
        assert_eq!(cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(&data)), dep["data_hash"]);
        context.create_cell_with_out_point(
            packed::OutPoint::from_slice(&bytes(&dep["out_point"])).unwrap(),
            packed::CellOutput::from_slice(&bytes(&dep["output"])).unwrap(),
            Bytes::from(data),
        );
    }
    for case in report["cases"].as_array().unwrap() {
        for input in case["inputs"].as_array().unwrap() {
            context.create_cell_with_out_point(
                packed::OutPoint::from_slice(&bytes(&input["out_point"])).unwrap(),
                packed::CellOutput::from_slice(&bytes(&input["output"])).unwrap(),
                Bytes::from(bytes(&input["data"])),
            );
        }
        let tx = packed::Transaction::from_slice(&bytes(&case["transaction"])).unwrap().into_view();
        assert_eq!(cellscript_artifact_checker::hex_encode(tx.hash().as_slice()), case["transaction_hash"]);
        let result = context.verify_tx(&tx, CYCLES);
        if case["status"] == "accepted" {
            assert_eq!(result.unwrap(), case["cycles"].as_u64().unwrap(), "{}", case["case"]);
        } else {
            let error = format!("{:?}", result.unwrap_err());
            assert!(
                case["expected_groups"].as_array().unwrap().iter().any(|group| error.contains(group.as_str().unwrap())),
                "{}: {error}",
                case["case"]
            );
        }
    }
}
