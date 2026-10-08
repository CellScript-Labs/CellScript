//! Consume the all-group application fixtures produced before this crate by the
//! unified gates. This tests the actual client against independently constructed
//! transactions/proofs; it does not create a second contract oracle.
use anyhow::{ensure, Context, Result};
use cellscript_zk_counter_client::migration::{
    Artifacts, CheckedDeployment, CodeCell, InstanceScripts, Manifest, ResolvedCell, Version,
};
use cellscript_zk_counter_client::Deployment;
use cellscript_zk_private_counter::{self as counter, package};
use ckb_testtool::context::Context as VmContext;
use ckb_types::{bytes::Bytes, core::TransactionView, packed, prelude::*};
use serde_json::{json, Value};
use std::{fs, path::Path};

fn bytes(value: &Value) -> Vec<u8> {
    hex::decode(value.as_str().unwrap()).unwrap()
}
fn transaction(value: &Value) -> TransactionView {
    packed::Transaction::from_slice(&bytes(value)).unwrap().into_view()
}
fn cells(value: &Value) -> Vec<ResolvedCell> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|cell| {
            (
                packed::OutPoint::from_slice(&bytes(&cell["out_point"])).unwrap(),
                packed::CellOutput::from_slice(&bytes(&cell["output"])).unwrap(),
                Bytes::from(bytes(&cell["data"])),
            )
        })
        .collect()
}
fn code(cell: &ResolvedCell) -> CodeCell {
    CodeCell {
        tx_hash: hex::encode(cell.0.tx_hash().as_slice()),
        index: cell.0.index().unpack(),
        data_hash: hex::encode(counter::hash(&cell.2)),
    }
}
fn dep(point: packed::OutPoint) -> packed::CellDep {
    packed::CellDep::new_builder().out_point(point).build()
}
fn failure<T>(result: Result<T>, expected: &str) {
    let error = result.err().expect("mutation must reject");
    assert!(error.to_string().contains(expected), "expected {expected}, got {error:#}");
}

#[test]
fn checked_migration_client_binds_manifest_selection_dependencies_and_proof_snapshot() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = root.join("contracts/zk-private-counter/target");
    fs::write(target.join("migration-client-evidence.json"), b"{\"status\":\"running\"}\n")?;
    let fixture_bytes = fs::read(target.join("migration-client-fixtures.json"))
        .context("run the private-counter migration test first; unified gates run producer before client")?;
    let fixture: Value = serde_json::from_slice(&fixture_bytes)?;
    ensure!(fixture["status"] == "passed", "migration application fixture is incomplete");
    let cases = fixture["transactions"].as_array().context("transactions missing")?;
    ensure!(cases.len() == 3, "all three successive transitions are required");
    let creation = transaction(&fixture["creation"]);
    let counter_script = creation.outputs().get(0).unwrap().type_().to_opt().unwrap();
    let config_script = creation.outputs().get(1).unwrap().type_().to_opt().unwrap();
    let config_args = config_script.args().raw_data();
    let all = cells(&cases[1]["cells"]);
    let find_hash = |hash: &[u8]| all.iter().find(|cell| counter::hash(&cell.2) == hash).unwrap();
    let lifecycle = find_hash(counter_script.code_hash().as_slice());
    let guard = find_hash(config_script.code_hash().as_slice());
    let child = &all[64]; // Fixture raw dependency zero, after 64 resolved inputs.
    let setups: [package::Manifest; 2] = serde_json::from_value(fixture["setup_manifests"].clone())?;
    let keys = setups.each_ref().map(|setup| find_hash(&hex::decode(&setup.verification_key_data_hash).unwrap()));
    let versions = [0usize, 1].map(|i| Version {
        verifier: Deployment {
            chain_id: "ckb-testnet".into(),
            genesis_hash: hex::encode([0x11; 32]),
            child_tx_hash: hex::encode(child.0.tx_hash().as_slice()),
            child_index: child.0.index().unpack(),
            child_data_hash: hex::encode(counter::hash(&child.2)),
            verification_key_hash: hex::encode(counter::hash(&keys[i].2)),
        },
        parent: code(find_hash(&config_args[96 + i * 32..128 + i * 32])),
        key: code(keys[i]),
        setup: setups[i].clone(),
    });
    let manifest = Manifest {
        schema: "cellscript-counter-migration-v1".into(),
        chain_id: "ckb-testnet".into(),
        genesis_hash: hex::encode([0x11; 32]),
        lifecycle: code(lifecycle),
        config_guard: code(guard),
        instance: InstanceScripts {
            counter: hex::encode(counter_script.as_slice()),
            configuration: hex::encode(config_script.as_slice()),
        },
        versions,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    let artifacts =
        || Artifacts { lifecycle: &lifecycle.2, config_guard: &guard.2, children: [&child.2; 2], keys: [&keys[0].2, &keys[1].2] };
    let load = |bytes: &[u8]| CheckedDeployment::load(bytes, counter::hash(bytes), "ckb-testnet", [0x11; 32], artifacts());
    let deployment = load(&manifest_bytes)?;
    let instance = deployment.bind_instance(counter_script.clone(), config_script.clone())?;
    let generated = deployment.instance(&creation.inputs().get(0).unwrap(), 0, 1)?;
    assert_eq!(generated.counter(), &counter_script);
    assert_eq!(generated.config(), &config_script);
    let creation_cells = cells(&fixture["creation_cells"]);
    let (creation_inputs, creation_deps) = creation_cells.split_at(creation.inputs().len());
    deployment.check_creation(&creation, creation_inputs, creation_deps, &instance)?;
    let mut changed_creation = creation.outputs_data().into_iter().collect::<Vec<_>>();
    changed_creation[1] = deployment.config_data(1)?.pack();
    failure(
        deployment.check_creation(
            &creation.as_advanced_builder().set_outputs_data(changed_creation).build(),
            creation_inputs,
            creation_deps,
            &instance,
        ),
        "parent 0",
    );
    let mut changed_first = creation.inputs().get(0).unwrap().as_builder().since(1u64).build();
    let changed_creation = creation.as_advanced_builder().set_inputs(vec![changed_first.clone()]).build();
    failure(deployment.check_creation(&changed_creation, creation_inputs, creation_deps, &instance), "regenerate both Type IDs");
    changed_first = creation.inputs().get(0).unwrap();
    failure(deployment.instance(&changed_first, 1, 0), "trusted instance Scripts");
    failure(deployment.instance(&creation.inputs().get(0).unwrap(), 1, 1), "indices");
    failure(deployment.instance(&creation.inputs().get(0).unwrap(), 0, 64), "indices");
    failure(CheckedDeployment::load(&manifest_bytes, [0; 32], "ckb-testnet", [0x11; 32], artifacts()), "trusted digest");
    failure(
        CheckedDeployment::load(&manifest_bytes, counter::hash(&manifest_bytes), "ckb-testnet", [0x12; 32], artifacts()),
        "network",
    );
    failure(
        CheckedDeployment::load(&manifest_bytes, counter::hash(&manifest_bytes), "ckb-mainnet", [0x11; 32], artifacts()),
        "network",
    );
    let mut changed = serde_json::to_value(&manifest)?;
    changed["config_guard"]["data_hash"] = json!(hex::encode([0x44; 32]));
    failure(load(&serde_json::to_vec(&changed)?), "config guard data hash");
    changed = serde_json::to_value(&manifest)?;
    changed["versions"][0]["setup"]["production_admitted"] = json!(true);
    failure(load(&serde_json::to_vec(&changed)?), "self-admission");
    changed = serde_json::to_value(&manifest)?;
    changed["unknown"] = json!(true);
    failure(load(&serde_json::to_vec(&changed)?), "unknown field");
    failure(
        deployment.bind_instance(counter_script.clone(), config_script.clone().as_builder().code_hash([0x55; 32]).build()),
        "pinned code",
    );
    failure(
        deployment.bind_instance(counter_script.clone().as_builder().hash_type(1u8).build(), config_script.clone()),
        "pinned code",
    );
    // A different, internally consistent pair with the same admitted code and
    // parents is still a different instance and cannot replace the pinned one.
    let mut other_config_args = config_args.to_vec();
    other_config_args[..64].fill(0x66);
    let other_config = config_script.clone().as_builder().args(Bytes::from(other_config_args).pack()).build();
    let mut other_counter_args = vec![0x66; 32];
    other_counter_args.extend(other_config.calc_script_hash().as_slice());
    let other_counter = counter_script.clone().as_builder().args(Bytes::from(other_counter_args).pack()).build();
    failure(deployment.bind_instance(other_counter, other_config), "trusted instance Scripts");
    let mut rows = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        let tx = transaction(&case["transaction"]);
        let resolved = cells(&case["cells"]);
        let (inputs, deps) = resolved.split_at(tx.inputs().len());
        let mut prepared = deployment.prepare(tx.clone(), inputs, deps, &instance)?;
        assert_eq!(prepared.selected_version(), usize::from(i == 2));
        assert_eq!(prepared.is_migration(), i == 1);
        let original =
            packed::WitnessArgs::from_slice(&tx.witnesses().get(0).unwrap().raw_data())?.input_type().to_opt().unwrap().raw_data();
        // Profile's fixed proof is the first argument, after the 8-byte ABI magic.
        // attach_proof uses the authoritative encoder; complete byte equality
        // below ensures this fixture extraction cannot invent a new wire format.
        assert_eq!(&original[..8], b"CSARGv1\0");
        let proof = &original[8..136];
        failure(prepared.check_signed(&tx, &tx), "no completed proof");
        let proved = prepared.attach_proof(proof)?;
        assert_eq!(proved.data().as_slice(), tx.data().as_slice());
        prepared.check_signed(&proved, &proved)?;
        let mut context = VmContext::new_with_deterministic_rng();
        for (point, output, data) in &resolved {
            context.create_cell_with_out_point(point.clone(), output.clone(), data.clone());
        }
        let cycles = context.verify_tx(&proved, 250_000_000)?;
        let changed_raw = proved.as_advanced_builder().version(1u32).build();
        failure(prepared.check_signed(&proved, &changed_raw), "raw transaction changed");
        let args = packed::WitnessArgs::from_slice(&proved.witnesses().get(0).unwrap().raw_data())?;
        let forged = proved
            .as_advanced_builder()
            .set_witnesses(vec![args
                .clone()
                .as_builder()
                .input_type(Some(Bytes::from(vec![0; original.len()])).pack())
                .build()
                .as_bytes()
                .pack()])
            .build();
        failure(prepared.check_signed(&forged, &forged), "non-Lock witness");
        let oversized = proved
            .as_advanced_builder()
            .set_witnesses(vec![args.as_builder().lock(Some(Bytes::from(vec![0; 16384])).pack()).build().as_bytes().pack()])
            .build();
        failure(prepared.check_signed(&proved, &oversized), "16384 bytes");
        if i == 1 {
            let maximum = proved.as_advanced_builder().witness(Bytes::new().pack()).build();
            let mut witnesses = maximum.witnesses().into_iter().collect::<Vec<_>>();
            *witnesses.last_mut().unwrap() = Bytes::from(vec![0; 16384 - maximum.data().as_slice().len()]).pack();
            let maximum = maximum.as_advanced_builder().set_witnesses(witnesses).build();
            let mut max_prepared = deployment.prepare(maximum.clone(), inputs, deps, &instance)?;
            assert_eq!(max_prepared.attach_proof(proof)?.data().as_slice(), maximum.data().as_slice());
            max_prepared.check_signed(&maximum, &maximum)?;
            let mut witnesses = maximum.witnesses().into_iter().collect::<Vec<_>>();
            let removed = witnesses[0].raw_data().len();
            witnesses[0] = Bytes::new().pack();
            let last = witnesses.last_mut().unwrap();
            *last = Bytes::from(vec![0; last.raw_data().len() + removed]).pack();
            let unproved_maximum = maximum.as_advanced_builder().set_witnesses(witnesses).build();
            let mut overflow = deployment.prepare(unproved_maximum, inputs, deps, &instance)?;
            failure(overflow.attach_proof(proof), "16384 bytes");
            failure(overflow.check_signed(&proved, &proved), "no completed proof");
        }
        failure(prepared.attach_proof(&[0; 128]), "proof rejected locally");
        failure(prepared.check_signed(&proved, &proved), "no completed proof");
        prepared.attach_proof(proof)?;
        prepared.check_signed(&proved, &proved)?;
        rows.push(json!({"case":case["case"],"selected_version":prepared.selected_version(),"migration":prepared.is_migration(),"cycles":cycles,"transaction_hash":hex::encode(proved.hash().as_slice()),"byte_identical_proof_attachment":true}));
        if i != 1 {
            continue;
        }
        let mut missing = deps.to_vec();
        missing.remove(0);
        failure(deployment.prepare(tx.clone(), inputs, &missing, &instance), "observation missing");
        let mut wrong = inputs.to_vec();
        wrong[0].0 = packed::OutPoint::default();
        failure(deployment.prepare(tx.clone(), &wrong, deps, &instance), "input resolution");
        let mut wrong = deps.to_vec();
        wrong[0].2 = Bytes::from_static(b"substituted child");
        failure(deployment.prepare(tx.clone(), inputs, &wrong, &instance), "child dependency");
        let mut wrong = inputs.to_vec();
        wrong[1].2 = deployment.config_data(1)?;
        failure(deployment.prepare(tx.clone(), &wrong, deps, &instance), "0 -> 1");
        let mut output_data = tx.outputs_data().into_iter().collect::<Vec<_>>();
        output_data[1] = deployment.config_data(0)?.pack();
        failure(deployment.prepare(tx.as_advanced_builder().set_outputs_data(output_data).build(), inputs, deps, &instance), "0 -> 1");
        let mut outputs = tx.outputs().into_iter().collect::<Vec<_>>();
        outputs[1] = outputs[1].clone().as_builder().capacity(39_999_999_999u64).build();
        failure(deployment.prepare(tx.as_advanced_builder().set_outputs(outputs).build(), inputs, deps, &instance), "Lock/capacity");
        let mut overlap = tx.cell_deps().into_iter().collect::<Vec<_>>();
        *overlap.last_mut().unwrap() = dep(inputs[1].0.clone());
        let mut observations = deps.to_vec();
        observations.push(inputs[1].clone());
        failure(
            deployment.prepare(tx.as_advanced_builder().set_cell_deps(overlap).build(), inputs, &observations, &instance),
            "without overlap",
        );
        // Expand all 64 dependencies through a Molecule dep-group, preserving
        // child slot zero. Structural preparation succeeds, but the prior proof
        // must reject because replacing raw CellDeps changes the statement.
        let group_point = packed::OutPoint::new_builder().tx_hash([0xa5; 32]).build();
        let points: Vec<_> = tx.cell_deps().into_iter().map(|dep| dep.out_point()).collect();
        let group = (group_point.clone(), packed::CellOutput::default(), points.clone().pack().as_bytes());
        let mut grouped_cells = deps.to_vec();
        grouped_cells.push(group);
        let grouped = tx.as_advanced_builder().set_cell_deps(vec![dep(group_point).as_builder().dep_type(1u8).build()]).build();
        let mut prepared_group = deployment.prepare(grouped.clone(), inputs, &grouped_cells, &instance)?;
        assert_eq!(prepared_group.selected_version(), 0);
        failure(prepared_group.attach_proof(proof), "proof rejected locally");
        let mut over = points;
        over.push(over[0].clone());
        grouped_cells.last_mut().unwrap().2 = over.pack().as_bytes();
        failure(deployment.prepare(grouped, inputs, &grouped_cells, &instance), "dep-group exceeds bounds");
        // Duplicate actual parent bytes at a different OutPoint are ambiguous,
        // even though the declared parent is still in the transaction.
        let parent = deps.iter().find(|cell| counter::hash(&cell.2) == config_args[96..128]).unwrap();
        let copied = (packed::OutPoint::new_builder().tx_hash([0xa6; 32]).build(), parent.1.clone(), parent.2.clone());
        let mut ambiguous = tx.cell_deps().into_iter().collect::<Vec<_>>();
        *ambiguous.last_mut().unwrap() = dep(copied.0.clone());
        let mut copied_deps = deps.to_vec();
        copied_deps.push(copied);
        failure(
            deployment.prepare(tx.as_advanced_builder().set_cell_deps(ambiguous).build(), inputs, &copied_deps, &instance),
            "parent dependency",
        );
    }
    fs::write(target.join("migration-client-manifest.json"), &manifest_bytes)?;
    for (i, key) in keys.iter().enumerate() {
        let parent = cellscript_zk_counter_client::parent(&manifest.versions[i].verifier, &child.2, &key.2)?;
        fs::write(target.join(format!("migration-parent-{i}-metadata.json")), serde_json::to_vec_pretty(&parent.compiled.metadata)?)?;
        fs::write(target.join(format!("migration-parent-{i}-handle.bin")), &parent.handle)?;
    }
    fs::write(
        target.join("migration-client-evidence.json"),
        serde_json::to_vec_pretty(&json!({
            "schema":"cellscript-counter-migration-client-evidence-v1","status":"passed",
            "scope":"native client and all-group testtool; no node liveness, CCC migration or production admission",
            "fixture_data_hash":hex::encode(counter::hash(&fixture_bytes)),"manifest_data_hash":hex::encode(counter::hash(&manifest_bytes)),"rows":rows,
        }))?,
    )?;
    Ok(())
}
