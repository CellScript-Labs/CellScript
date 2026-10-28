//! A non-financial instance policy with real issuance, custody and retirement.
//! Only ordinary funding Cells are seeded. Document and observer Cells are
//! outputs of verified signed transactions; the local live set is not a node.

use super::{code_candidate, lock, package, source_policy_records};
use camino::Utf8Path;
use cellscript::package::frozen_interface::{
    compile_module, freeze_code_catalog, freeze_instance_policy, freeze_package_sources, freeze_source_code_policy,
    resolve_source_catalog, EntrySelection,
};
use cellscript::CompileOptions;
use cellscript_artifact_checker::{
    code_origin::SuppliedDependencyCell,
    instance_policy::{encode_selection, ArgsConstraint, ConsumerArgs, InstanceMode, InstancePolicy},
    open_handle_policy::AuthorizationSet,
};
use ckb_sdk::{traits::SecpCkbRawKeySigner, types::ScriptGroup, unlock::ScriptSigner, unlock::SecpSighashScriptSigner, SECP256K1};
use ckb_testtool::{
    ckb_types::{bytes::Bytes, core::Capacity, core::TransactionBuilder, core::TransactionView, packed, prelude::*},
    context::Context,
};
use std::collections::HashMap;

const DOCUMENT: &str = include_str!("../fixtures/instance_policy/document.cell.in");
const OBSERVER: &str = include_str!("../fixtures/instance_policy/document_observer.cell.in");
const CAPACITY: u64 = 100_000_000_000;
const FEE: u64 = 100_000_000;
const CYCLES: u64 = 2_000_000;

fn witness(payload: &[u8]) -> packed::Bytes {
    packed::WitnessArgs::new_builder().input_type(Some(Bytes::copy_from_slice(payload)).pack()).build().as_bytes().pack()
}

fn document_witness(expected: &[u8; 32]) -> packed::Bytes {
    witness(&[b"CSARGv1\0".as_slice(), expected].concat())
}

pub(super) fn sign(tx: &TransactionView, script: &packed::Script, indices: Vec<usize>, key: u8) -> TransactionView {
    let signer =
        SecpSighashScriptSigner::new(Box::new(SecpCkbRawKeySigner::new_with_secret_keys(vec![secp256k1::SecretKey::from_slice(
            &[key; 32],
        )
        .unwrap()])));
    let mut group = ScriptGroup::from_lock_script(script);
    group.input_indices = indices;
    signer.sign_tx(tx, &group).unwrap()
}

pub(super) fn signer_lock(byte: u8) -> packed::Script {
    let code = ckb_system_scripts_v0_6_0::BUNDLED_CELL.get("specs/cells/secp256k1_blake160_sighash_all").unwrap();
    let key = secp256k1::SecretKey::from_slice(&[byte; 32]).unwrap();
    let public = secp256k1::PublicKey::from_secret_key(&SECP256K1, &key);
    let hash = cellscript_artifact_checker::ckb_blake2b256(&public.serialize());
    packed::Script::new_builder()
        .code_hash(packed::CellOutput::calc_data_hash(&code))
        .hash_type(4u8)
        .args(Bytes::copy_from_slice(&hash[..20]).pack())
        .build()
}

pub(super) fn transaction_record(context: &Context, tx: &TransactionView, status: &str) -> serde_json::Value {
    let hex = cellscript_artifact_checker::hex_encode;
    serde_json::json!({
        "status": status,
        "transaction": hex(tx.data().as_slice()),
        "transaction_hash": hex(tx.hash().as_slice()),
        "inputs": tx.inputs().into_iter().map(|input| {
            let (output, data) = context.get_cell(&input.previous_output()).unwrap();
            serde_json::json!({"out_point": hex(input.previous_output().as_slice()), "output": hex(output.as_slice()), "data": hex(&data)})
        }).collect::<Vec<_>>(),
    })
}

fn reject(ledger: &mut Ledger, tx: &TransactionView, group: &str) {
    let error = ledger.context.verify_tx(tx, CYCLES).unwrap_err();
    assert!(format!("{error:?}").contains(group), "expected {group}, got {error:?}");
    let mut record = transaction_record(&ledger.context, tx, "rejected");
    record["expected_group"] = serde_json::json!(group);
    record["error"] = serde_json::json!(format!("{error:?}"));
    ledger.records.push(record);
}

struct Ledger {
    context: Context,
    live: HashMap<packed::OutPoint, (packed::CellOutput, Bytes)>,
    maximum_cycles: u64,
    maximum_transaction_bytes: usize,
    records: Vec<serde_json::Value>,
}

impl Ledger {
    fn seed(&mut self, script: &packed::Script) -> packed::OutPoint {
        let output = packed::CellOutput::new_builder().capacity(10 * CAPACITY).lock(script.clone()).build();
        // Context's deterministic option covers deploy_cell, not create_cell.
        // Freeze funding points too so sighashes and measured cycles replay.
        let point = packed::OutPoint::new_builder().tx_hash(script.calc_script_hash()).index(0x80u32).build();
        self.context.create_cell_with_out_point(point.clone(), output.clone(), Bytes::new());
        self.live.insert(point.clone(), (output, Bytes::new()));
        point
    }

    fn commit(&mut self, tx: &TransactionView) -> Vec<packed::OutPoint> {
        let mut capacity = 0;
        for input in tx.inputs() {
            capacity += u64::from(self.live.get(&input.previous_output()).expect("input must still be live").0.capacity());
        }
        let output_capacity: u64 = tx.outputs().into_iter().map(|cell| u64::from(cell.capacity())).sum();
        assert_eq!(capacity - output_capacity, FEE);
        let cycles = self.context.verify_tx(tx, CYCLES).unwrap();
        self.maximum_cycles = self.maximum_cycles.max(cycles);
        let mut record = transaction_record(&self.context, tx, "accepted");
        record["cycles"] = serde_json::json!(cycles);
        self.records.push(record);
        self.maximum_transaction_bytes = self.maximum_transaction_bytes.max(tx.data().as_slice().len());
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
                assert!(!self.live.contains_key(&point));
                self.context.create_cell_with_out_point(point.clone(), output.clone(), data.clone());
                self.live.insert(point.clone(), (output, data));
                point
            })
            .collect()
    }
}

#[test]
fn issued_document_instances_require_real_issuer_custody_and_peer_execution() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("library");
    let consumer = directory.path().join("consumer");
    let issuer = signer_lock(0x51);
    let attacker = signer_lock(0x52);
    let digest = cellscript_artifact_checker::ckb_blake2b256(b"CellScript constrained document: signed issuance and retirement.");
    let mut wrong_digest = digest;
    wrong_digest[31] ^= 1;
    let issuer_literal = issuer.calc_script_hash().as_slice().iter().map(|byte| format!("\\x{byte:02x}")).collect::<String>();
    package(&library, "library", &DOCUMENT.replace("@ISSUER_LOCK_HASH@", &issuer_literal), "");
    lock(&library);
    package(&consumer, "client", OBSERVER, "[dependencies]\nlibrary = { path = \"../library\" }\n");
    lock(&consumer);

    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compile_document = || {
            compile_module(
                Utf8Path::from_path(&library).unwrap(),
                "dev",
                CompileOptions { diagnostics, ..Default::default() },
                EntrySelection::Action("verify".into()),
            )
            .unwrap()
        };
        let candidate = code_candidate(compile_document(), vec![1]);
        let catalog = freeze_code_catalog(compile_document(), vec![candidate], &Default::default()).unwrap();
        let sources =
            resolve_source_catalog(freeze_package_sources(Utf8Path::from_path(&consumer).unwrap(), "dev").unwrap(), catalog).unwrap();
        let (header, members) = source_policy_records(&sources, false, 0);
        let implementation = freeze_source_code_policy(sources, AuthorizationSet::new(header, &members).unwrap()).unwrap();
        let implementation_selection = implementation.policy().selection(&members[0].receipt).unwrap();
        let constraints = InstancePolicy {
            implementation_root: implementation.policy().root(),
            mode: InstanceMode::Constrained,
            args_length: 32,
            config_length: 16,
            constraints: vec![
                ArgsConstraint::Config { offset: 0, length: 16, config_offset: 0 },
                ArgsConstraint::Unsigned { offset: 16, width: 8, minimum: 1, maximum: 5 },
                ArgsConstraint::Unsigned { offset: 24, width: 8, minimum: 1, maximum: u64::MAX },
            ],
        };
        let policy = freeze_instance_policy(implementation, constraints).unwrap();
        let provider = &policy.implementation().sources().catalog().candidates()[0];
        let observer = compile_module(
            Utf8Path::from_path(&consumer).unwrap(),
            "dev",
            CompileOptions { diagnostics, ..Default::default() },
            EntrySelection::Lock("verify".into()),
        )
        .unwrap();
        let mut ledger = Ledger {
            context: Context::new_with_deterministic_rng(),
            live: HashMap::new(),
            maximum_cycles: 0,
            maximum_transaction_bytes: 0,
            records: Vec::new(),
        };
        let code_creation = packed::RawTransaction::from_slice(provider.raw_transaction()).unwrap();
        let code_point =
            packed::OutPoint::new_builder().tx_hash(code_creation.calc_tx_hash()).index(provider.origin().output_index()).build();
        ledger.context.create_cell_with_out_point(
            code_point.clone(),
            code_creation.outputs().get(0).unwrap(),
            Bytes::copy_from_slice(provider.module().bundle()[0]),
        );
        let observer_point = ledger.context.deploy_cell(Bytes::copy_from_slice(observer.bundle()[0]));
        let lock_code = ckb_system_scripts_v0_6_0::BUNDLED_CELL.get("specs/cells/secp256k1_blake160_sighash_all").unwrap();
        let secp_data = ckb_system_scripts_v0_6_0::BUNDLED_CELL.get("specs/cells/secp256k1_data").unwrap();
        let lock_point = ledger.context.deploy_cell(Bytes::copy_from_slice(&lock_code));
        let secp_point = ledger.context.deploy_cell(Bytes::copy_from_slice(&secp_data));
        let mut funding = ledger.seed(&issuer);
        let attacker_point = ledger.seed(&attacker);
        let deps =
            [code_point, observer_point, lock_point, secp_point].map(|point| packed::CellDep::new_builder().out_point(point).build());
        let plain = |capacity| packed::CellOutput::new_builder().capacity(capacity).lock(issuer.clone()).build();
        let mut instances = Vec::new();

        for (serial, category) in [(1u64, 3u64), (2, 3), (3, 6)] {
            let args = [[0x41; 16].as_slice(), &category.to_le_bytes(), &serial.to_le_bytes()].concat();
            let script = packed::Script::from_slice(provider.selected_script())
                .unwrap()
                .as_builder()
                .args(Bytes::from(args.clone()).pack())
                .build();
            assert_ne!(script.as_slice(), provider.selected_script(), "neither future instance was enumerated in the old member");
            let committed = ConsumerArgs {
                policy_root: policy.policy().root().unwrap(),
                subject: script.calc_script_hash().unpack(),
                config: vec![0x41; 16],
            }
            .encode()
            .unwrap();
            let selection = encode_selection(policy.policy(), &implementation_selection, &args).unwrap();
            assert_eq!(policy.check_selection(&committed, &selection, &Default::default()).is_ok(), category <= 5);
            let observer_script = packed::Script::new_builder()
                .code_hash(packed::CellOutput::calc_data_hash(observer.bundle()[0]))
                .hash_type(4u8)
                .args(Bytes::copy_from_slice(&committed).pack())
                .build();
            let data = [args.as_slice(), &digest].concat();
            let available = u64::from(ledger.live[&funding].0.capacity());
            let unsigned = TransactionBuilder::default()
                .cell_deps(deps.clone())
                .input(packed::CellInput::new_builder().previous_output(funding.clone()).build())
                .outputs([
                    plain(CAPACITY).as_builder().type_(Some(script.clone()).pack()).build(),
                    plain(CAPACITY).as_builder().lock(observer_script).build(),
                    plain(available - 2 * CAPACITY - FEE),
                ])
                .outputs_data([Bytes::from(data.clone()).pack(), Bytes::new().pack(), Bytes::new().pack()])
                .witness(document_witness(&digest))
                .build();
            if serial == 1 {
                let forged = unsigned
                    .as_advanced_builder()
                    .set_inputs(vec![packed::CellInput::new_builder().previous_output(attacker_point.clone()).build()])
                    .build();
                reject(&mut ledger, &sign(&forged, &attacker, vec![0], 0x52), "Outputs[0].Type");
                let wrong_custody = unsigned
                    .as_advanced_builder()
                    .set_outputs(vec![
                        unsigned.outputs().get(0).unwrap().as_builder().lock(attacker.clone()).build(),
                        unsigned.outputs().get(1).unwrap(),
                        unsigned.outputs().get(2).unwrap(),
                    ])
                    .build();
                reject(&mut ledger, &sign(&wrong_custody, &issuer, vec![0], 0x51), "Outputs[0].Type");
                let mut rebound_data = data.clone();
                rebound_data[24] = 9;
                let wrong_configuration = unsigned
                    .as_advanced_builder()
                    .set_outputs_data(vec![Bytes::from(rebound_data.clone()).pack(), Bytes::new().pack(), Bytes::new().pack()])
                    .build();
                reject(&mut ledger, &sign(&wrong_configuration, &issuer, vec![0], 0x51), "Outputs[0].Type");
                let malformed = unsigned
                    .as_advanced_builder()
                    .set_outputs_data(vec![Bytes::copy_from_slice(&data[..63]).pack(), Bytes::new().pack(), Bytes::new().pack()])
                    .build();
                reject(&mut ledger, &sign(&malformed, &issuer, vec![0], 0x51), "Outputs[0].Type");
            }
            let points = ledger.commit(&sign(&unsigned, &issuer, vec![0], 0x51));
            funding = points[2].clone();
            instances.push((script, committed, selection, points));
        }
        assert_ne!(instances[0].0.calc_script_hash(), instances[1].0.calc_script_hash());

        for index in 0..instances.len() {
            let (_, committed, selection, points) = &instances[index];
            let unsigned = TransactionBuilder::default()
                .cell_deps(deps.clone())
                .inputs(
                    [points[1].clone(), points[0].clone()]
                        .map(|point| packed::CellInput::new_builder().previous_output(point).build()),
                )
                .output(plain(2 * CAPACITY - FEE))
                .output_data(Bytes::new().pack())
                .witness(witness(&[b"CSARGv1\0".as_slice(), selection].concat()))
                .witness(document_witness(&digest))
                .build();
            if index == 2 {
                // Issuer authorization permits this real document, but the
                // observer admits only categories 1..=5. Its concrete identity
                // and correctly signed peer cannot override that policy.
                assert!(policy.check_selection(committed, selection, &Default::default()).is_err());
                reject(&mut ledger, &sign(&unsigned, &issuer, vec![1], 0x51), "Inputs[0].Lock");
                assert!(ledger.live.contains_key(&points[0]));
                continue;
            }
            let resolved = deps.iter().map(|dep| ledger.context.get_cell(&dep.out_point()).unwrap()).collect::<Vec<_>>();
            let supplied = deps
                .iter()
                .zip(&resolved)
                .map(|(dep, (output, data))| SuppliedDependencyCell {
                    out_point: dep.out_point().as_slice().try_into().unwrap(),
                    output: output.as_slice(),
                    data,
                })
                .collect::<Vec<_>>();
            policy
                .check_selection(committed, selection, &Default::default())
                .unwrap()
                .check_direct_dependency(unsigned.data().raw().as_slice(), &supplied, &Default::default())
                .unwrap();
            let bad_peer = unsigned
                .as_advanced_builder()
                .set_witnesses(vec![unsigned.witnesses().get(0).unwrap(), document_witness(&wrong_digest)])
                .build();
            reject(&mut ledger, &sign(&bad_peer, &issuer, vec![1], 0x51), "Inputs[1].Type");
            let (document, data) = &ledger.live[&points[0]];
            let unauthorized_successor = unsigned
                .as_advanced_builder()
                .set_outputs(vec![document.clone().as_builder().capacity(2 * CAPACITY - FEE).build()])
                .set_outputs_data(vec![data.pack()])
                .build();
            reject(&mut ledger, &sign(&unauthorized_successor, &issuer, vec![1], 0x51), "Inputs[1].Type");
            if index == 0 {
                let (_, _, other_selection, other_points) = &instances[1];
                let substitute = unsigned
                    .as_advanced_builder()
                    .set_inputs(vec![
                        unsigned.inputs().get(0).unwrap(),
                        packed::CellInput::new_builder().previous_output(other_points[0].clone()).build(),
                    ])
                    .set_witnesses(vec![witness(&[b"CSARGv1\0".as_slice(), other_selection].concat()), document_witness(&digest)])
                    .build();
                reject(&mut ledger, &sign(&substitute, &issuer, vec![1], 0x51), "Inputs[0].Lock");
                let missing = unsigned
                    .as_advanced_builder()
                    .set_inputs(vec![unsigned.inputs().get(0).unwrap()])
                    .set_outputs(vec![plain(CAPACITY - FEE)])
                    .set_witnesses(vec![unsigned.witnesses().get(0).unwrap()])
                    .build();
                reject(&mut ledger, &missing, "Inputs[0].Lock");
            }
            ledger.commit(&sign(&unsigned, &issuer, vec![1], 0x51));
            assert!(!ledger.live.contains_key(&points[0]), "retired document cannot be spent again");
        }
        eprintln!(
            "document instances {diagnostics}: maximum_cycles={} maximum_tx_bytes={}",
            ledger.maximum_cycles, ledger.maximum_transaction_bytes
        );
        assert!(ledger.maximum_transaction_bytes <= 2_400);
        let evidence = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/instance-policy-evidence/document")
            .join(format!("{diagnostics}"));
        std::fs::create_dir_all(&evidence).unwrap();
        std::fs::write(evidence.join("document.cell"), std::fs::read(library.join("src/main.cell")).unwrap()).unwrap();
        std::fs::write(evidence.join("observer.cell"), OBSERVER).unwrap();
        for (name, module) in [("document", provider.module()), ("observer", &observer)] {
            for (suffix, bytes) in ["elf", "metadata.json", "lowering.json", "sourcemap.json"].into_iter().zip(module.bundle()) {
                std::fs::write(evidence.join(format!("{name}.{suffix}")), bytes).unwrap();
            }
            std::fs::write(evidence.join(format!("{name}.source-context.json")), module.context_bytes().unwrap()).unwrap();
        }
        std::fs::write(evidence.join("implementation-creation.molecule"), provider.raw_transaction()).unwrap();
        std::fs::write(evidence.join("instance-policy.molecule"), policy.policy().encode().unwrap()).unwrap();
        let dependencies = deps
            .iter()
            .enumerate()
            .map(|(index, dep)| {
                let (output, data) = ledger.context.get_cell(&dep.out_point()).unwrap();
                let file = format!("dependency-{index}.bin");
                std::fs::write(evidence.join(&file), &data).unwrap();
                serde_json::json!({"out_point": cellscript_artifact_checker::hex_encode(dep.out_point().as_slice()),
                "output": cellscript_artifact_checker::hex_encode(output.as_slice()), "data_file": file,
                "data_hash": cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(&data))})
            })
            .collect::<Vec<_>>();
        let report = serde_json::json!({
            "schema": "cellscript-instance-document-evidence-v1", "diagnostics": diagnostics.to_string(),
            "compiler_version": env!("CARGO_PKG_VERSION"), "scope": "local-all-group-ckb-vm",
            "source_policy": policy.implementation().identity(), "instance_policy": policy.identity(),
            "policy_record": serde_json::from_slice::<serde_json::Value>(&policy.canonical_bytes().unwrap()).unwrap(),
            "maximum_cycles": ledger.maximum_cycles, "maximum_transaction_bytes": ledger.maximum_transaction_bytes,
            "cases": ledger.records, "dependencies": dependencies,
        });
        std::fs::write(evidence.join("report.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}
