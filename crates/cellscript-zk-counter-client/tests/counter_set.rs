//! Real independently owned 1/2/4-counter transactions, with every Script group.
use anyhow::Result;
use ark_std::rand::{rngs::StdRng, SeedableRng};
use cellscript_zk_counter_client::{counter_set as client, Deployment};
use cellscript_zk_private_counter::{self as counter, CounterCircuit, Witness};
use ckb_testtool::{builtin::ALWAYS_SUCCESS, context::Context};
use ckb_types::{
    bytes::Bytes,
    core::{TransactionBuilder, TransactionView},
    packed,
    prelude::*,
};
use serde_json::{json, Value};
use std::{fs, path::Path};

fn dep(point: packed::OutPoint) -> packed::CellDep {
    packed::CellDep::new_builder().out_point(point).build()
}
fn reject(context: &Context, tx: &TransactionView, code: i8, name: &str, rows: &mut Vec<Value>) {
    let error = context.verify_tx(tx, 500_000_000).expect_err(name).to_string();
    assert!(error.contains(&format!("error code {code}")), "{name}: {error}");
    rows.push(json!({"case":name,"expected_code":code,"error":error}));
}
fn apply(context: &mut Context, tx: &TransactionView) -> Vec<packed::OutPoint> {
    tx.outputs()
        .into_iter()
        .zip(tx.outputs_data())
        .enumerate()
        .map(|(i, (cell, data))| {
            let point = packed::OutPoint::new_builder().tx_hash(tx.hash()).index(i as u32).build();
            context.create_cell_with_out_point(point.clone(), cell, data.raw_data());
            point
        })
        .collect()
}

#[test]
fn complete_counter_sets_execute_and_reject_boundaries() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = root.join("target/counter-set");
    fs::create_dir_all(&target)?;
    fs::write(target.join("evidence.json"), b"{\"status\":\"running\"}")?;
    let child = fs::read(
        root.join("contracts/zk-transition-verifier/target/riscv64imac-unknown-none-elf/release/cellscript-zk-transition-verifier"),
    )?;
    let lifecycle =
        fs::read(root.join("contracts/zk-private-counter/script/target/riscv64imac-unknown-none-elf/release/cellscript-counter-set"))?;
    assert!(lifecycle.len() <= 131_072 && child.len() <= 262_144);
    let mut rng = StdRng::seed_from_u64(0x53455401);
    let pk = counter::setup(&mut rng)?.0;
    let vk = counter::serialize(&pk.vk)?;
    let mut fixtures = Vec::new();
    let mut rows = Vec::new();
    for count in [1usize, 2, 4] {
        let mut context = Context::new_with_deterministic_rng();
        let child_point = context.deploy_cell(Bytes::from(child.clone()));
        let key_point = context.deploy_cell(Bytes::from(vk.clone()));
        let parent = cellscript_zk_counter_client::parent(
            &Deployment {
                chain_id: "ckb-testnet".into(),
                genesis_hash: hex::encode([0x11; 32]),
                child_tx_hash: hex::encode(child_point.tx_hash().as_slice()),
                child_index: child_point.index().unpack(),
                child_data_hash: hex::encode(counter::hash(&child)),
                verification_key_hash: hex::encode(counter::hash(&vk)),
            },
            &child,
            &vk,
        )?;
        let parent_bytes = cellscript::strip_vm_abi_trailer(&parent.compiled.artifact_bytes);
        assert!(parent_bytes.len() <= 32_768);
        if count == 1 {
            fs::write(target.join("parent-metadata.json"), serde_json::to_vec_pretty(&parent.compiled.metadata)?)?;
            fs::write(target.join("parent-handle.bin"), &parent.handle)?;
            fs::write(target.join("parent.elf"), parent_bytes)?;
            fs::write(target.join("verification-key.bin"), &vk)?;
        }
        let parent_point = context.deploy_cell(Bytes::copy_from_slice(parent_bytes));
        let lifecycle_point = context.deploy_cell(Bytes::from(lifecycle.clone()));
        let lock_point = context.deploy_cell(ALWAYS_SUCCESS.clone());
        let lock = context.build_script(&lock_point, Bytes::new()).unwrap();
        let funding = packed::CellOutput::new_builder().capacity(1_000_000_000_000u64).lock(lock.clone()).build();
        let funding_point = context.create_cell(funding.clone(), Bytes::new());
        let first = packed::CellInput::new_builder().previous_output(funding_point).build();
        let family = client::family_id(&first, 0);
        let scripts = (0..count)
            .map(|i| client::script(counter::hash(&lifecycle), family, i as u8, counter::hash(parent_bytes)))
            .collect::<Result<Vec<_>>>()?;
        let cells = scripts
            .iter()
            .map(|s| {
                packed::CellOutput::new_builder().capacity(40_000_000_000u64).lock(lock.clone()).type_(Some(s.clone()).pack()).build()
            })
            .collect::<Vec<_>>();
        let data = |lane: usize, value: u64| Bytes::from(counter::state(&counter::owner(&[0x21 + lane as u8; 32]), value).to_vec());
        let mut creation = TransactionBuilder::default().input(first).set_cell_deps(vec![
            dep(child_point),
            dep(key_point),
            dep(parent_point),
            dep(lifecycle_point),
        ]);
        for (i, cell) in cells.iter().enumerate() {
            creation = creation.output(cell.clone()).output_data(data(i, 0).pack());
        }
        let fee = funding.clone().as_builder().capacity(1_000_000_000_000u64 - 40_000_000_000u64 * count as u64 - 100_000).build();
        let creation = context.complete_tx(creation.output(fee.clone()).output_data(Bytes::new().pack()).build());
        let creation_cycles = context.verify_tx(&creation, 500_000_000)?;
        if count > 1 {
            let mut outputs = creation.outputs().into_iter().collect::<Vec<_>>();
            outputs[0] = outputs[0].clone().as_builder().type_(None::<packed::Script>.pack()).build();
            reject(
                &context,
                &creation.as_advanced_builder().set_outputs(outputs).build(),
                117,
                "creation-without-lane-zero",
                &mut rows,
            );
        }
        let mut nonzero = creation.outputs_data().into_iter().collect::<Vec<_>>();
        nonzero[0] = data(0, 1).pack();
        reject(&context, &creation.as_advanced_builder().set_outputs_data(nonzero).build(), 117, "nonzero-creation", &mut rows);
        let points = apply(&mut context, &creation);
        let mut inputs = Vec::new();
        let mut update = TransactionBuilder::default().set_cell_deps(creation.cell_deps().into_iter().collect());
        for (i, cell) in cells.iter().enumerate() {
            inputs.push((points[i].clone(), cell.clone(), data(i, 0)));
            update = update
                .input(packed::CellInput::new_builder().previous_output(points[i].clone()).build())
                .output(cell.clone())
                .output_data(data(i, 1).pack());
        }
        inputs.push((points[count].clone(), fee.clone(), Bytes::new()));
        let fee_capacity: u64 = fee.capacity().unpack();
        update = update
            .input(packed::CellInput::new_builder().previous_output(points[count].clone()).build())
            .output(fee.clone().as_builder().capacity(fee_capacity - 100_000).build())
            .output_data(Bytes::new().pack());
        if count == 4 {
            while inputs.len() < 64 {
                let cell = fee.clone().as_builder().capacity(10_000_000_000u64).build();
                let point = context.create_cell(cell.clone(), Bytes::new());
                inputs.push((point.clone(), cell.clone(), Bytes::new()));
                update = update
                    .input(packed::CellInput::new_builder().previous_output(point).build())
                    .output(cell)
                    .output_data(Bytes::new().pack());
            }
        }
        let mut update = context.complete_tx(update.build());
        if count == 4 {
            let mut deps = update.cell_deps().into_iter().collect::<Vec<_>>();
            while deps.len() < 64 {
                let point = context.create_cell(fee.clone(), Bytes::from(vec![deps.len() as u8]));
                deps.push(dep(point));
            }
            update = update.as_advanced_builder().set_cell_deps(deps).build();
        }
        let payload = parent.compiled.metadata.actions[0].entry_witness_args(&[
            cellscript::EntryWitnessArg::Bytes(vec![0; 128]),
            cellscript::EntryWitnessArg::Bytes(parent.handle.clone()),
        ])?;
        let placeholder = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes();
        let mut witnesses =
            (0..inputs.len()).map(|i| if i < count { placeholder.clone().pack() } else { Bytes::new().pack() }).collect::<Vec<_>>();
        if count == 4 {
            witnesses.push(Bytes::new().pack());
        }
        update = update.as_advanced_builder().set_witnesses(witnesses).build();
        if count == 4 {
            let padding = 16_384usize.checked_sub(update.data().as_slice().len()).expect("frozen maximum transaction budget");
            let mut witnesses = update.witnesses().into_iter().collect::<Vec<_>>();
            *witnesses.last_mut().unwrap() = Bytes::from(vec![0; padding]).pack();
            update = update.as_advanced_builder().set_witnesses(witnesses).build();
            assert_eq!(
                (update.inputs().len(), update.outputs().len(), update.cell_deps().len(), update.data().as_slice().len()),
                (64, 64, 64, 16_384)
            );
        }
        let mut prepared = client::PreparedCounterSet::new(update.clone(), &inputs, &scripts[0])?;
        assert_eq!(prepared.statements().len(), count);
        let proofs = prepared
            .statements()
            .iter()
            .enumerate()
            .map(|(i, s)| {
                counter::prove(
                    &pk,
                    CounterCircuit {
                        statement: s.clone(),
                        witness: Witness { secret: [0x21 + i as u8; 32], old_counter: 0, new_counter: 1 },
                    },
                    &mut rng,
                )
                .map(|p| p.to_vec())
            })
            .collect::<Result<Vec<_>>>()?;
        assert!(prepared.attach_proofs(&parent, &vk, &proofs[..count - 1]).is_err());
        let proved = prepared.attach_proofs(&parent, &vk, &proofs)?;
        prepared.check_signed(&proved, &proved)?;
        let cycles = context.verify_tx(&proved, 500_000_000)?;
        if count == 4 {
            let mut too_many = proved.inputs().into_iter().collect::<Vec<_>>();
            too_many.push(too_many.last().unwrap().clone());
            reject(&context, &proved.as_advanced_builder().set_inputs(too_many).build(), 113, "input-bound-plus-one", &mut rows);
            let mut too_many = proved.outputs().into_iter().collect::<Vec<_>>();
            too_many.push(too_many.last().unwrap().clone());
            let mut values = proved.outputs_data().into_iter().collect::<Vec<_>>();
            values.push(Bytes::new().pack());
            reject(
                &context,
                &proved.as_advanced_builder().set_outputs(too_many).set_outputs_data(values).build(),
                113,
                "output-bound-plus-one",
                &mut rows,
            );
            let extra = context.create_cell(fee.clone(), Bytes::from(vec![0xfe]));
            let mut too_many = proved.cell_deps().into_iter().collect::<Vec<_>>();
            too_many.push(dep(extra));
            reject(
                &context,
                &proved.as_advanced_builder().set_cell_deps(too_many).build(),
                113,
                "dependency-bound-plus-one",
                &mut rows,
            );
            let mut oversized = proved.witnesses().into_iter().collect::<Vec<_>>();
            let mut padding = oversized.last().unwrap().raw_data().to_vec();
            padding.push(0);
            *oversized.last_mut().unwrap() = Bytes::from(padding).pack();
            let oversized = proved.as_advanced_builder().set_witnesses(oversized).build();
            assert_eq!(oversized.data().as_slice().len(), 16_385);
            reject(&context, &oversized, 113, "transaction-bound-plus-one", &mut rows);
            assert!(client::PreparedCounterSet::new(oversized, &inputs, &scripts[0]).is_err());
        }
        rows.push(json!({"case":format!("valid-{count}"),"protected_inputs":count,"successors":count,"creation_cycles":creation_cycles,
            "cycles":cycles,"transaction_bytes":proved.data().as_slice().len(),"witness_bytes":proved.witnesses().as_slice().len(),"transaction_hash":hex::encode(proved.hash().as_slice())}));
        let mut snapshot = Vec::new();
        for point in
            proved.inputs().into_iter().map(|i| i.previous_output()).chain(proved.cell_deps().into_iter().map(|d| d.out_point()))
        {
            let (output, data) = context.get_cell(&point).unwrap();
            snapshot.push(
                json!({"out_point":hex::encode(point.as_slice()),"output":hex::encode(output.as_slice()),"data":hex::encode(data)}),
            );
        }
        fixtures.push(json!({"count":count,"transaction":hex::encode(proved.data().as_slice()),"cells":snapshot,
            "selected_script":hex::encode(scripts[0].as_slice()),"set_statement":hex::encode(prepared.encoded_statement()),
            "public_inputs":prepared.statements().iter().map(|s|hex::encode(s.public_inputs())).collect::<Vec<_>>() }));
        // Bound-plus-one cases above use the exact maximum. Later semantic
        // mutations need room to grow without masking their intended boundary.
        // Only opaque extra padding is removed; raw tx and all proofs agree.
        let proved = if count == 4 {
            let mut witnesses = proved.witnesses().into_iter().collect::<Vec<_>>();
            *witnesses.last_mut().unwrap() = Bytes::new().pack();
            proved.as_advanced_builder().set_witnesses(witnesses).build()
        } else {
            proved
        };
        // A fee-only mutation reaches the pairing boundary, never a malformed codec.
        let mut outputs = proved.outputs().into_iter().collect::<Vec<_>>();
        outputs[count] = outputs[count].clone().as_builder().capacity(fee_capacity - 100_001).build();
        let changed = proved.as_advanced_builder().set_outputs(outputs).build();
        reject(&context, &changed, 79, "fee-change-after-proof", &mut rows);
        assert!(prepared.check_signed(&changed, &changed).is_err());
        // Full snapshot selection cannot be made smaller by a witness list.
        let snapshots = inputs
            .iter()
            .map(|(p, c, d)| cellscript_ckb_adapter::counter_set::SuppliedInputCell {
                out_point: p.as_slice().try_into().unwrap(),
                output: c.as_slice(),
                data: d,
            })
            .collect::<Vec<_>>();
        let checked =
            cellscript_ckb_adapter::counter_set::check_counter_set(proved.data().as_slice(), &snapshots, scripts[0].as_slice())?;
        checked.check_statement(prepared.encoded_statement())?;
        assert!(checked.check_unchanged(changed.data().as_slice(), &snapshots).is_err());
        let mut wrong = prepared.encoded_statement().to_vec();
        *wrong.last_mut().unwrap() ^= 1;
        assert!(checked.check_statement(&wrong).is_err());
        if count > 1 {
            let mut outputs = proved.outputs().into_iter().collect::<Vec<_>>();
            outputs.swap(0, 1);
            reject(
                &context,
                &proved.as_advanced_builder().set_outputs(outputs).build(),
                114,
                "reordered-protected-outputs",
                &mut rows,
            );
            let mut raw_inputs = proved.inputs().into_iter().collect::<Vec<_>>();
            raw_inputs.swap(0, 1);
            reject(
                &context,
                &proved.as_advanced_builder().set_inputs(raw_inputs).build(),
                114,
                "reordered-protected-inputs",
                &mut rows,
            );
            let mut outputs = proved.outputs().into_iter().collect::<Vec<_>>();
            outputs[1] = outputs[0].clone();
            reject(&context, &proved.as_advanced_builder().set_outputs(outputs).build(), 114, "duplicate-successor-Script", &mut rows);
            let mut raw_inputs = proved.inputs().into_iter().collect::<Vec<_>>();
            raw_inputs[1] = raw_inputs[0].clone();
            reject(&context, &proved.as_advanced_builder().set_inputs(raw_inputs).build(), 114, "duplicate-input-identity", &mut rows);
        }
        let mut outputs = proved.outputs().into_iter().collect::<Vec<_>>();
        outputs[0] = outputs[0].clone().as_builder().type_(None::<packed::Script>.pack()).build();
        reject(&context, &proved.as_advanced_builder().set_outputs(outputs).build(), 115, "state-hidden-as-fee", &mut rows);
        let mut outputs = proved.outputs().into_iter().collect::<Vec<_>>();
        outputs[count] = outputs[count].clone().as_builder().type_(Some(scripts[0].clone()).pack()).build();
        reject(
            &context,
            &proved.as_advanced_builder().set_outputs(outputs).build(),
            114,
            "fee-inserted-as-protected-successor",
            &mut rows,
        );
        let mut outputs = proved.outputs().into_iter().collect::<Vec<_>>();
        let mut args = scripts[0].args().raw_data().to_vec();
        args[41] ^= 1;
        let wrong_parent = scripts[0].clone().as_builder().args(Bytes::from(args).pack()).build();
        outputs[0] = outputs[0].clone().as_builder().type_(Some(wrong_parent).pack()).build();
        reject(
            &context,
            &proved.as_advanced_builder().set_outputs(outputs).build(),
            112,
            "same-family-parent-substitution",
            &mut rows,
        );
        let mut outputs = proved.outputs().into_iter().collect::<Vec<_>>();
        outputs[0] = outputs[0].clone().as_builder().capacity(39_999_999_999u64).build();
        reject(&context, &proved.as_advanced_builder().set_outputs(outputs).build(), 120, "protected-capacity-change", &mut rows);
        let mut outputs = proved.outputs().into_iter().collect::<Vec<_>>();
        let other_lock = lock.clone().as_builder().args(Bytes::from(vec![1]).pack()).build();
        outputs[0] = outputs[0].clone().as_builder().lock(other_lock).build();
        reject(&context, &proved.as_advanced_builder().set_outputs(outputs).build(), 120, "protected-lock-change", &mut rows);
        let mut data_values = proved.outputs_data().into_iter().collect::<Vec<_>>();
        data_values[0] = Bytes::from(counter::state(&[7; 32], 1).to_vec()).pack();
        reject(&context, &proved.as_advanced_builder().set_outputs_data(data_values).build(), 116, "substituted-owner", &mut rows);
        let successors = apply(&mut context, &proved);
        let mut replay_inputs = inputs.clone();
        let mut raw_inputs = proved.inputs().into_iter().collect::<Vec<_>>();
        let mut output_data = proved.outputs_data().into_iter().collect::<Vec<_>>();
        for i in 0..count {
            replay_inputs[i] = (successors[i].clone(), cells[i].clone(), data(i, 1));
            raw_inputs[i] = raw_inputs[i].clone().as_builder().previous_output(successors[i].clone()).build();
            output_data[i] = data(i, 2).pack();
        }
        for i in count..inputs.len() {
            replay_inputs[i] =
                (successors[i].clone(), proved.outputs().get(i).unwrap(), proved.outputs_data().get(i).unwrap().raw_data());
            raw_inputs[i] = raw_inputs[i].clone().as_builder().previous_output(successors[i].clone()).build();
        }
        let mut replay_outputs = proved.outputs().into_iter().collect::<Vec<_>>();
        replay_outputs[count] = replay_outputs[count].clone().as_builder().capacity(fee_capacity - 200_000).build();
        let replay =
            proved.as_advanced_builder().set_inputs(raw_inputs).set_outputs(replay_outputs).set_outputs_data(output_data).build();
        reject(&context, &replay, 79, "old-proof-successor-replay", &mut rows);
        let mut rebuilt = client::PreparedCounterSet::new(replay, &replay_inputs, &scripts[0])?;
        let fresh = rebuilt
            .statements()
            .iter()
            .enumerate()
            .map(|(i, s)| {
                counter::prove(
                    &pk,
                    CounterCircuit {
                        statement: s.clone(),
                        witness: Witness { secret: [0x21 + i as u8; 32], old_counter: 1, new_counter: 2 },
                    },
                    &mut rng,
                )
                .map(|p| p.to_vec())
            })
            .collect::<Result<Vec<_>>>()?;
        let recovered = rebuilt.attach_proofs(&parent, &vk, &fresh)?;
        rows.push(json!({"case":format!("rebuilt-successors-{count}"),"cycles":context.verify_tx(&recovered,500_000_000)?,"scope":"testtool successor rebuild; live conflicting-spend rejection requires node evidence"}));
    }
    fs::write(target.join("fixtures.json"), serde_json::to_vec(&fixtures)?)?;
    fs::write(
        target.join("evidence.json"),
        serde_json::to_vec_pretty(&json!({"schema":"cellscript-counter-set-execution-v1","status":"passed",
        "profile":"cellscript-zk-counter-set-v1","proof_composition":"1..4 unchanged exact counter proofs; not aggregation",
        "circuit":counter::circuit_identity()?,"verification_key_data_hash":hex::encode(counter::hash(&vk)),
        "lifecycle_bytes":lifecycle.len(),"lifecycle_hash":hex::encode(counter::hash(&lifecycle)),"child_bytes":child.len(),
        "setup":"public deterministic test only","node_conflict_acceptance":false,"rows":rows,"ci_or_gates_run":false}))?,
    )?;
    Ok(())
}
