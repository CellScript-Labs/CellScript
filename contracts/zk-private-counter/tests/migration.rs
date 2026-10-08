//! Continuous state lineage with two exact VKs and an old-proof-authorized switch.
//! Testtool executes all Script groups; it does not establish node liveness.
#[allow(dead_code)] // Shared helpers also cover the unchanged legacy lifecycle.
mod support;
use ark_std::rand::{rngs::StdRng, SeedableRng};
use cellscript_zk_private_counter::{self as counter, package, CounterCircuit, Witness};
use ckb_testtool::{
    builtin::ALWAYS_SUCCESS,
    ckb_types::{
        bytes::Bytes,
        core::{ScriptHashType, TransactionBuilder, TransactionView},
        packed,
        prelude::*,
    },
    context::Context,
};
use serde_json::{json, Value};
use support::*;

const CYCLES: u64 = 250_000_000;
const MAX_TX_BYTES: usize = 16_384;
const MAX_SCRIPT_BYTES: usize = 131_072;
const FEE: u64 = 100_000;

fn script_bytes(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("script/target/riscv64imac-unknown-none-elf/release").join(name),
    )
    .unwrap()
}

fn config_data(selector: u8, parent: &[u8]) -> Bytes {
    let mut data = b"CSZKCFG1".to_vec();
    data.push(selector);
    data.extend(counter::hash(parent));
    Bytes::from(data)
}

fn paired_scripts(
    context: &mut Context,
    first: &packed::CellInput,
    lifecycle_out: &packed::OutPoint,
    guard_out: &packed::OutPoint,
    parents: [&[u8]; 2],
) -> (packed::Script, packed::Script) {
    let type_id = |index: u64| {
        let mut bytes = first.as_slice().to_vec();
        bytes.extend(index.to_le_bytes());
        counter::hash(&bytes)
    };
    let mut config_args = type_id(1).to_vec();
    config_args.extend(type_id(0));
    config_args.extend(counter::hash(&context.get_cell(lifecycle_out).unwrap().1));
    config_args.extend(counter::hash(parents[0]));
    config_args.extend(counter::hash(parents[1]));
    let config = context.build_script_with_hash_type(guard_out, ScriptHashType::Data2, Bytes::from(config_args)).unwrap();
    let mut state_args = type_id(0).to_vec();
    state_args.extend(config.calc_script_hash().as_slice());
    let state = context.build_script_with_hash_type(lifecycle_out, ScriptHashType::Data2, Bytes::from(state_args)).unwrap();
    (state, config)
}

fn apply(context: &mut Context, tx: &TransactionView) -> Vec<packed::OutPoint> {
    tx.outputs()
        .into_iter()
        .zip(tx.outputs_data())
        .enumerate()
        .map(|(index, (output, data))| {
            let point = packed::OutPoint::new_builder().tx_hash(tx.hash()).index(index as u32).build();
            context.create_cell_with_out_point(point.clone(), output, data.raw_data());
            point
        })
        .collect()
}

fn accept(context: &Context, tx: &TransactionView, label: &str, rows: &mut Vec<Value>) {
    assert!(tx.data().as_slice().len() <= MAX_TX_BYTES);
    let cycles = context.verify_tx(tx, CYCLES).unwrap_or_else(|error| panic!("{label}: {error}"));
    rows.push(json!({"case":label,"cycles":cycles,"transaction_hash":hex::encode(tx.hash().as_slice()),
        "transaction_bytes":tx.data().as_slice().len(),"witness_bytes":tx.witnesses().as_slice().len()}));
}

fn reject(context: &Context, tx: &TransactionView, codes: &[i8], label: &str, rows: &mut Vec<Value>) {
    let error = context.verify_tx(tx, CYCLES).expect_err(label).to_string();
    assert!(
        codes.iter().any(|code| error.contains(&format!("error code {code})"))
            || error.contains(&format!("error code {code} "))
            || error.ends_with(&format!("error code {code}"))),
        "{label}: expected one of {codes:?}, got {error}"
    );
    rows.push(json!({"case":label,"expected_codes":codes,"error":error}));
}

#[test]
fn old_proof_authorizes_exact_migration_and_new_key_updates_its_successor() {
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    std::fs::write(target.join("migration-evidence.json"), b"{\"status\":\"running\"}\n").unwrap();
    std::fs::write(target.join("migration-resource-fixtures.json"), b"[]\n").unwrap();
    std::fs::write(target.join("migration-client-fixtures.json"), b"{\"status\":\"running\"}\n").unwrap();
    let mut rng = StdRng::seed_from_u64(0x4d494752415445); // Public fixture randomness only.
    let mut setup_kinds = Vec::new();
    let mut setup_manifests = Vec::new();
    let keys = ["CELLSCRIPT_COUNTER_PACKAGE", "CELLSCRIPT_COUNTER_MIGRATION_PACKAGE"].map(|env| {
        if let Some(path) = std::env::var_os(env) {
            let (manifest, key) = package::load_package(std::path::Path::new(&path)).unwrap();
            setup_kinds.push(serde_json::to_value(&manifest.setup_kind).unwrap());
            setup_manifests.push(manifest);
            key
        } else {
            setup_kinds.push(json!("public-test-seed"));
            let key = counter::setup(&mut rng).unwrap().0;
            let vk = counter::serialize(&key.vk).unwrap();
            setup_manifests.push(package::Manifest {
                schema: "cellscript-counter-setup-v1".into(),
                circuit: counter::circuit_identity().unwrap(),
                rust_toolchain: "1.97.1".into(),
                dependency_lock_sha256: package::sha256(include_bytes!("../Cargo.lock")),
                proving_key_sha256: package::sha256(&counter::serialize(&key).unwrap()),
                verification_key_sha256: package::sha256(&vk),
                verification_key_data_hash: hex::encode(counter::hash(&vk)),
                setup_kind: package::SetupKind::PublicTestSeed,
                setup_unix_seconds: 0,
                production_admitted: false,
            });
            key
        }
    });
    let vks = keys.each_ref().map(|key| counter::serialize(&key.vk).unwrap());
    assert_ne!(vks[0], vks[1], "migration must exercise a different exact verification key");
    let mut context = Context::new_with_deterministic_rng();
    let child = child_bytes();
    let lifecycle = script_bytes("cellscript-counter-migratable-lifecycle");
    let guard = script_bytes("cellscript-counter-config");
    assert!(lifecycle.len() <= MAX_SCRIPT_BYTES && guard.len() <= MAX_SCRIPT_BYTES);
    let child_out = context.deploy_cell(Bytes::from(child.clone()));
    let vk_outs = vks.each_ref().map(|key| context.deploy_cell(Bytes::from(key.clone())));
    let parents = vks.each_ref().map(|key| parent(&child, key, &child_out, [0x11; 32]));
    let parent_bytes = parents.each_ref().map(|(compiled, _)| cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes));
    assert_ne!(counter::hash(parent_bytes[0]), counter::hash(parent_bytes[1]));
    let parent_outs = parent_bytes.map(|bytes| context.deploy_cell(Bytes::copy_from_slice(bytes)));
    let lifecycle_out = context.deploy_cell(Bytes::from(lifecycle.clone()));
    let guard_out = context.deploy_cell(Bytes::from(guard.clone()));
    let lock_out = context.deploy_cell(ALWAYS_SUCCESS.clone());
    let lock = context.build_script(&lock_out, Bytes::new()).unwrap();
    let funding = packed::CellOutput::new_builder().capacity(100_000_000_000u64).lock(lock.clone()).build();
    let funding_point = context.create_cell(funding, Bytes::new());
    let first = packed::CellInput::new_builder().previous_output(funding_point).build();
    let (state_script, config_script) = paired_scripts(&mut context, &first, &lifecycle_out, &guard_out, parent_bytes);
    let state_cell = packed::CellOutput::new_builder()
        .capacity(40_000_000_000u64)
        .lock(lock.clone())
        .type_(Some(state_script.clone()).pack())
        .build();
    let config_cell = state_cell.clone().as_builder().type_(Some(config_script.clone()).pack()).build();
    let fee_cell = |amount: u64| packed::CellOutput::new_builder().capacity(amount).lock(lock.clone()).build();
    let secret = [0x23; 32];
    let owner = counter::owner(&secret);
    let state_data = |value| Bytes::copy_from_slice(&counter::state(&owner, value));
    let deps = vec![
        dep(child_out),
        dep(vk_outs[0].clone()),
        dep(parent_outs[0].clone()),
        dep(vk_outs[1].clone()),
        dep(parent_outs[1].clone()),
        dep(lifecycle_out.clone()),
        dep(guard_out.clone()),
    ];
    let mut fee_capacity = 20_000_000_000 - FEE;
    let creation = context.complete_tx(
        TransactionBuilder::default()
            .input(first)
            .output(state_cell.clone())
            .output_data(state_data(0).pack())
            .output(config_cell.clone())
            .output_data(config_data(0, parent_bytes[0]).pack())
            .output(fee_cell(fee_capacity))
            .output_data(Bytes::new().pack())
            .set_cell_deps(deps.clone())
            .build(),
    );
    let mut rows = Vec::new();
    accept(&context, &creation, "create-paired-state", &mut rows);
    reject(
        &context,
        &creation
            .as_advanced_builder()
            .set_outputs_data(vec![state_data(1).pack(), config_data(0, parent_bytes[0]).pack(), Bytes::new().pack()])
            .build(),
        &[108],
        "create-nonzero",
        &mut rows,
    );
    reject(
        &context,
        &creation
            .as_advanced_builder()
            .set_outputs_data(vec![state_data(0).pack(), config_data(1, parent_bytes[1]).pack(), Bytes::new().pack()])
            .build(),
        &[108, 109],
        "create-at-new-version",
        &mut rows,
    );
    let created = apply(&mut context, &creation);
    let mut state_point = created[0].clone();
    let mut config_point = created[1].clone();
    let mut fee_point = created[2].clone();
    let mut previous_proof: Option<Vec<u8>> = None;
    let mut resource_fixtures = Vec::new();
    let mut client_fixtures = Vec::new();

    for old_value in 0..3 {
        let switching = old_value == 1;
        let selected = usize::from(old_value == 2);
        let mut tx = TransactionBuilder::default()
            .input(packed::CellInput::new_builder().previous_output(state_point.clone()).build())
            .output(state_cell.clone())
            .output_data(state_data(old_value + 1).pack())
            .set_cell_deps(deps.clone());
        if switching {
            tx = tx
                .input(packed::CellInput::new_builder().previous_output(config_point.clone()).build())
                .output(config_cell.clone())
                .output_data(config_data(1, parent_bytes[1]).pack());
        } else {
            tx = tx.cell_dep(dep(config_point.clone()));
        }
        fee_capacity -= FEE;
        let mut tx = context.complete_tx(
            tx.input(packed::CellInput::new_builder().previous_output(fee_point.clone()).build())
                .output(fee_cell(fee_capacity))
                .output_data(Bytes::new().pack())
                .build(),
        );
        if switching {
            // Freeze all raw fields at the admitted maxima BEFORE proving.
            let mut builder = tx.as_advanced_builder();
            for _ in tx.inputs().len()..64 {
                let fee = fee_cell(10_000_000_000);
                let point = context.create_cell(fee.clone(), Bytes::new());
                builder = builder
                    .input(packed::CellInput::new_builder().previous_output(point).build())
                    .output(fee)
                    .output_data(Bytes::new().pack());
            }
            for index in tx.cell_deps().len()..64 {
                let point = context.deploy_cell(Bytes::from(vec![0x5a, index as u8]));
                builder = builder.cell_dep(dep(point));
            }
            tx = builder.build();
            assert_eq!((tx.inputs().len(), tx.outputs().len(), tx.cell_deps().len()), (64, 64, 64));
        }
        let statement = statement(&state_script, &state_point, &state_data(old_value), &state_data(old_value + 1), &tx);
        let circuit = || CounterCircuit {
            statement: statement.clone(),
            witness: Witness { secret, old_counter: old_value, new_counter: old_value + 1 },
        };
        let proof = counter::prove(&keys[selected], circuit(), &mut rng).unwrap();
        counter::verify(&vks[selected], &statement, &proof).unwrap();
        let with_proof = |proof: &[u8]| {
            tx.as_advanced_builder().set_witnesses(vec![witness(&parents[selected].0, proof, &parents[selected].1).pack()]).build()
        };
        let authorize_mutation = |changed: TransactionView, rng: &mut StdRng| {
            let statement =
                support::statement(&state_script, &state_point, &state_data(old_value), &state_data(old_value + 1), &changed);
            let proof = counter::prove(
                &keys[selected],
                CounterCircuit {
                    statement: statement.clone(),
                    witness: Witness { secret, old_counter: old_value, new_counter: old_value + 1 },
                },
                rng,
            )
            .unwrap();
            counter::verify(&vks[selected], &statement, &proof).unwrap();
            changed
                .as_advanced_builder()
                .set_witnesses(vec![witness(&parents[selected].0, &proof, &parents[selected].1).pack()])
                .build()
        };
        let valid = with_proof(&proof);
        let label = ["old-key-update", "old-key-authorized-migration", "new-key-successor-update"][old_value as usize];
        accept(&context, &valid, label, &mut rows);
        let cells: Vec<_> = valid
            .inputs()
            .into_iter()
            .map(|input| input.previous_output())
            .chain(valid.cell_deps().into_iter().map(|dep| dep.out_point()))
            .map(|point| {
                let (output, data) = context.get_cell(&point).unwrap();
                json!({"out_point":hex::encode(point.as_slice()),"output":hex::encode(output.as_slice()),"data":hex::encode(data)})
            })
            .collect();
        client_fixtures.push(json!({"case":label,"transaction":hex::encode(valid.data().as_slice()),"cells":cells}));
        if let Some(previous) = &previous_proof {
            reject(&context, &with_proof(previous), &[79], "previous-proof-replay", &mut rows);
        }
        if switching || selected == 1 {
            let wrong = 1 - selected;
            let wrong_proof = counter::prove(&keys[wrong], circuit(), &mut rng).unwrap();
            counter::verify(&vks[wrong], &statement, &wrong_proof).unwrap();
            reject(
                &context,
                &with_proof(&wrong_proof),
                &[79],
                if switching { "new-key-cannot-authorize-its-installation" } else { "old-key-cannot-update-migrated-state" },
                &mut rows,
            );
        }
        if switching {
            let extra = valid.as_advanced_builder().witness(Bytes::new().pack()).build();
            let pad = MAX_TX_BYTES.checked_sub(extra.data().as_slice().len()).unwrap();
            let maximum = valid.as_advanced_builder().witness(Bytes::from(vec![0; pad]).pack()).build();
            assert_eq!(maximum.data().as_slice().len(), MAX_TX_BYTES);
            assert_eq!(maximum.hash(), valid.hash(), "unbound witness does not change the raw transaction");
            accept(&context, &maximum, "max-cells-dependencies-and-transaction-bytes", &mut rows);
            let too_large = valid.as_advanced_builder().witness(Bytes::from(vec![0; pad + 1]).pack()).build();
            assert_eq!(too_large.data().as_slice().len(), MAX_TX_BYTES + 1);
            reject(&context, &too_large, &[102], "transaction-byte-limit-plus-one", &mut rows);
            let extra_fee = fee_cell(10_000_000_000);
            let extra_point = context.create_cell(extra_fee.clone(), Bytes::new());
            reject(
                &context,
                &valid
                    .as_advanced_builder()
                    .input(packed::CellInput::new_builder().previous_output(extra_point.clone()).build())
                    .build(),
                &[102],
                "input-limit-plus-one",
                &mut rows,
            );
            reject(
                &context,
                &valid.as_advanced_builder().output(extra_fee).output_data(Bytes::new().pack()).build(),
                &[102],
                "output-limit-plus-one",
                &mut rows,
            );
            reject(
                &context,
                &valid.as_advanced_builder().cell_dep(dep(extra_point)).build(),
                &[102],
                "dependency-limit-plus-one",
                &mut rows,
            );
            let mut overlapping = valid.cell_deps().into_iter().collect::<Vec<_>>();
            *overlapping.last_mut().unwrap() = dep(config_point.clone());
            reject(
                &context,
                &valid.as_advanced_builder().set_cell_deps(overlapping).build(),
                &[108],
                "config-input-dependency-overlap",
                &mut rows,
            );
            let missing_parent = valid.cell_deps().into_iter().filter(|dep| dep.out_point() != parent_outs[0]).collect::<Vec<_>>();
            reject(
                &context,
                &valid.as_advanced_builder().set_cell_deps(missing_parent).build(),
                &[106],
                "missing-old-parent-code",
                &mut rows,
            );
            let copied_parent = context.create_cell(fee_cell(2_000_000_000_000), Bytes::copy_from_slice(parent_bytes[0]));
            let mut duplicated_parent = valid.cell_deps().into_iter().collect::<Vec<_>>();
            *duplicated_parent.last_mut().unwrap() = dep(copied_parent);
            reject(
                &context,
                &valid.as_advanced_builder().set_cell_deps(duplicated_parent).build(),
                &[103],
                "ambiguous-parent-code",
                &mut rows,
            );
            let mut changed_fee = valid.outputs().into_iter().collect::<Vec<_>>();
            changed_fee[2] = fee_cell(fee_capacity - 1);
            reject(
                &context,
                &valid.as_advanced_builder().set_outputs(changed_fee).build(),
                &[79],
                "post-proof-raw-transaction-change",
                &mut rows,
            );
            let mut changed_owner = valid.outputs_data().into_iter().collect::<Vec<_>>();
            changed_owner[0] = Bytes::copy_from_slice(&counter::state(&[7; 32], old_value + 1)).pack();
            reject(
                &context,
                &valid.as_advanced_builder().set_outputs_data(changed_owner).build(),
                &[104],
                "owner-substitution",
                &mut rows,
            );
            for index in [0usize, 1] {
                let mut outputs = valid.outputs().into_iter().collect::<Vec<_>>();
                let mut data = valid.outputs_data().into_iter().collect::<Vec<_>>();
                outputs[2] = outputs[index].clone();
                data[2] = data[index].clone();
                reject(
                    &context,
                    &valid.as_advanced_builder().set_outputs(outputs).set_outputs_data(data).build(),
                    &[103],
                    if index == 0 { "duplicate-counter-successor" } else { "duplicate-config-successor" },
                    &mut rows,
                );
                let mut outputs = valid.outputs().into_iter().collect::<Vec<_>>();
                let mut data = valid.outputs_data().into_iter().collect::<Vec<_>>();
                outputs.remove(index);
                data.remove(index);
                reject(
                    &context,
                    &valid.as_advanced_builder().set_outputs(outputs).set_outputs_data(data).build(),
                    &[103, 108],
                    if index == 0 { "burn-counter" } else { "burn-config" },
                    &mut rows,
                );
                let mut outputs = valid.outputs().into_iter().collect::<Vec<_>>();
                outputs[index] = outputs[index].clone().as_builder().capacity(39_999_999_999u64).build();
                let changed = valid.as_advanced_builder().set_outputs(outputs).build();
                // The configuration guard may execute after the counter. Give
                // the changed raw transaction a VALID owner proof so rejection
                // demonstrates custody enforcement, not a stale proof binding.
                let changed = if index == 1 { authorize_mutation(changed, &mut rng) } else { changed };
                reject(
                    &context,
                    &changed,
                    &[110],
                    if index == 0 { "counter-capacity-change" } else { "config-capacity-change" },
                    &mut rows,
                );
                let mut outputs = valid.outputs().into_iter().collect::<Vec<_>>();
                let changed_lock = outputs[index].lock().as_builder().args(Bytes::from_static(b"other-owner").pack()).build();
                outputs[index] = outputs[index].clone().as_builder().lock(changed_lock).build();
                let changed = valid.as_advanced_builder().set_outputs(outputs).build();
                let changed = if index == 1 { authorize_mutation(changed, &mut rng) } else { changed };
                reject(&context, &changed, &[110], if index == 0 { "counter-lock-change" } else { "config-lock-change" }, &mut rows);
            }
            let mut wrong_parent = valid.outputs_data().into_iter().collect::<Vec<_>>();
            let mut changed = config_data(1, parent_bytes[1]).to_vec();
            changed[9] ^= 1;
            wrong_parent[1] = Bytes::from(changed).pack();
            reject(
                &context,
                &valid.as_advanced_builder().set_outputs_data(wrong_parent).build(),
                &[105],
                "unadmitted-parent-hash",
                &mut rows,
            );
            let mut data = valid.outputs_data().into_iter().collect::<Vec<_>>();
            data[1] = config_data(0, parent_bytes[0]).pack();
            reject(
                &context,
                &valid.as_advanced_builder().set_outputs_data(data).build(),
                &[109],
                "switch-must-advance-selector",
                &mut rows,
            );
            let mut inputs = valid.inputs().into_iter().collect::<Vec<_>>();
            inputs.remove(0);
            let mut outputs = valid.outputs().into_iter().collect::<Vec<_>>();
            outputs.remove(0);
            let mut data = valid.outputs_data().into_iter().collect::<Vec<_>>();
            data.remove(0);
            reject(
                &context,
                &valid
                    .as_advanced_builder()
                    .set_inputs(inputs)
                    .set_outputs(outputs)
                    .set_outputs_data(data)
                    .set_witnesses(vec![])
                    .build(),
                &[108],
                "config-only-spend",
                &mut rows,
            );
            let mut cells = Vec::new();
            for point in
                maximum.inputs().into_iter().map(|i| i.previous_output()).chain(maximum.cell_deps().into_iter().map(|d| d.out_point()))
            {
                let (output, data) = context.get_cell(&point).unwrap();
                cells.push(json!({"out_point":hex::encode(point.as_slice()),"output":hex::encode(output.as_slice()),"data":hex::encode(data)}));
            }
            resource_fixtures.push(json!({"transaction":hex::encode(maximum.data().as_slice()),"counter_script":hex::encode(state_script.as_slice()),"config_script":hex::encode(config_script.as_slice()),"cells":cells}));
        } else {
            let duplicate = context.create_cell(config_cell.clone(), config_data(selected as u8, parent_bytes[selected]));
            reject(
                &context,
                &valid.as_advanced_builder().cell_dep(dep(duplicate)).build(),
                &[103],
                "duplicate-config-dependency-views",
                &mut rows,
            );
            let absent = valid.cell_deps().into_iter().filter(|d| d.out_point() != config_point).collect::<Vec<_>>();
            reject(
                &context,
                &valid.as_advanced_builder().set_cell_deps(absent).build(),
                &[108],
                "missing-config-dependency",
                &mut rows,
            );
        }
        let points = apply(&mut context, &valid);
        state_point = points[0].clone();
        if switching {
            config_point = points[1].clone();
        }
        fee_point = points[if switching { 2 } else { 1 }].clone();
        previous_proof = Some(proof.to_vec());
    }
    for selector in [0, 1] {
        let rollback = context.complete_tx(
            TransactionBuilder::default()
                .input(packed::CellInput::new_builder().previous_output(state_point.clone()).build())
                .input(packed::CellInput::new_builder().previous_output(config_point.clone()).build())
                .input(packed::CellInput::new_builder().previous_output(fee_point.clone()).build())
                .output(state_cell.clone())
                .output_data(state_data(4).pack())
                .output(config_cell.clone())
                .output_data(config_data(selector, parent_bytes[selector as usize]).pack())
                .output(fee_cell(fee_capacity - FEE))
                .output_data(Bytes::new().pack())
                .set_cell_deps(deps.clone())
                .build(),
        );
        reject(
            &context,
            &rollback,
            &[109],
            if selector == 0 { "rollback-forbidden" } else { "second-migration-forbidden" },
            &mut rows,
        );
    }
    // Derive the new pair's Type IDs from this actual input. The previously
    // created legacy input must reject losing its successor at its burn check,
    // rather than only failing on a malformed proof or a new group's identity.
    let legacy_out = context.deploy_cell(Bytes::from(lifecycle_bytes()));
    let funding_point = context.create_cell(fee_cell(100_000_000_000), Bytes::new());
    let funding_input = packed::CellInput::new_builder().previous_output(funding_point).build();
    let legacy_script = context
        .build_script_with_hash_type(&legacy_out, ScriptHashType::Data2, Bytes::from(type_args(&funding_input, 0, parent_bytes[0])))
        .unwrap();
    let legacy_cell = fee_cell(100_000_000_000).as_builder().type_(Some(legacy_script).pack()).build();
    let legacy_creation = context
        .complete_tx(TransactionBuilder::default().input(funding_input).output(legacy_cell).output_data(state_data(0).pack()).build());
    accept(&context, &legacy_creation, "legacy-instance-created", &mut rows);
    let legacy_point = apply(&mut context, &legacy_creation)[0].clone();
    let legacy_input = packed::CellInput::new_builder().previous_output(legacy_point).build();
    let (fresh_state, fresh_config) = paired_scripts(&mut context, &legacy_input, &lifecycle_out, &guard_out, parent_bytes);
    let attempted = context.complete_tx(
        TransactionBuilder::default()
            .input(legacy_input)
            .output(state_cell.as_builder().type_(Some(fresh_state).pack()).build())
            .output_data(state_data(0).pack())
            .output(config_cell.as_builder().type_(Some(fresh_config).pack()).build())
            .output_data(config_data(0, parent_bytes[0]).pack())
            .output(fee_cell(20_000_000_000 - FEE))
            .output_data(Bytes::new().pack())
            .set_cell_deps(deps)
            .build(),
    );
    reject(&context, &attempted, &[94], "legacy-instance-cannot-migrate", &mut rows);
    let report = json!({"schema":"cellscript-counter-migration-evidence-v1","status":"passed",
        "scope":"testtool all-group execution; does not prove node liveness or admit production setup",
        "production_admitted":false,"setup_kinds":setup_kinds,"counter_script":hex::encode(state_script.as_slice()),
        "config_script":hex::encode(config_script.as_slice()),"verification_key_hashes":vks.each_ref().map(|v|hex::encode(counter::hash(v))),
        "parent_hashes":parent_bytes.map(|p|hex::encode(counter::hash(p))),
        "lifecycle_hash":hex::encode(counter::hash(&lifecycle)),"config_guard_hash":hex::encode(counter::hash(&guard)),
        "lifecycle_bytes":lifecycle.len(),"config_guard_bytes":guard.len(),
        "circuit":counter::circuit_identity().unwrap(),
        "environment":{"ckb_testtool":"1.1.1","rust":"1.97.1","vm_version":2},
        "budgets":{"cycles":CYCLES,"transaction_bytes":MAX_TX_BYTES,"each_new_script_bytes":MAX_SCRIPT_BYTES,"inputs_outputs_resolved_deps_each":64},"rows":rows});
    std::fs::write(target.join("migration-evidence.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    std::fs::write(target.join("migration-resource-fixtures.json"), serde_json::to_vec_pretty(&resource_fixtures).unwrap()).unwrap();
    let creation_cells: Vec<_> = creation
        .inputs()
        .into_iter()
        .map(|input| input.previous_output())
        .chain(creation.cell_deps().into_iter().map(|dep| dep.out_point()))
        .map(|point| {
            let (output, data) = context.get_cell(&point).unwrap();
            json!({"out_point":hex::encode(point.as_slice()),"output":hex::encode(output.as_slice()),"data":hex::encode(data)})
        })
        .collect();
    std::fs::write(
        target.join("migration-client-fixtures.json"),
        serde_json::to_vec_pretty(&json!({
            "status":"passed", "scope":"public-secret testtool fixture; no node liveness or production admission",
            "creation":hex::encode(creation.data().as_slice()), "creation_cells":creation_cells,
            "setup_manifests":setup_manifests, "transactions":client_fixtures,
        }))
        .unwrap(),
    )
    .unwrap();
}
