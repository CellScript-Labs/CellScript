//! Scale measurements for bounded abstractions; no claim about arbitrary programs.

use cellscript::{
    artifact::{
        compile_artifact, encode_policy_action_record, ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch,
    },
    strip_vm_abi_trailer, CompileResult, EntryWitnessArg, ExecutableSurfacePolicy,
};
use cellscript_ckb_adapter::policy_witness::{encode_policy_witness_bundle, PolicyScriptRole, PolicyWitnessRecord};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Budget {
    elf_bytes: u64,
    positive_cycles: u64,
    max_stack_frame_bytes: u64,
    witness_bytes: u64,
}

use super::{
    ckb_script_runner::{build_simple_fixture, execute_cellscript_script, execute_cellscript_script_with_transaction_transform},
    compile_cellscript, options, witness_for,
};

fn metrics(name: String, compiled: &CompileResult, cycles: u64, witness_bytes: usize) -> Value {
    let frame = compiled
        .verified_lowering_record
        .iter()
        .flat_map(|record| &record.entries)
        .map(|entry| entry.frame_size_bytes)
        .max()
        .expect("cost fixture has a verified lowering entry");
    let row = json!({
        "name": name,
        "elf_bytes": strip_vm_abi_trailer(&compiled.artifact_bytes).len(),
        "max_stack_frame_bytes": frame,
        "positive_cycles": cycles,
        "witness_bytes": witness_bytes,
    });
    eprintln!("[cost-growth] {row}");
    row
}

fn action(index: usize, width: usize) -> String {
    format!(
        "action check_{index}(input before: Token, witness payload: [u8; {width}]) {{\n\
         verification\nrequire before.amount == {}\n\
         require payload == before.expected\nconsume before\n}}\n",
        index + 1
    )
}

pub fn measure() -> Vec<Value> {
    let mut rows = Vec::new();
    for width in [8, 32, 128] {
        let prefix = format!("module cost_growth\nresource Token has store, consume {{ amount: u64\nexpected: [u8; {width}] }}\n");
        let single = compile_cellscript(&format!("{prefix}{}", action(0, width)));
        single.validate().expect("single-action artifact validation");
        let mut fixture = build_simple_fixture(Bytes::default(), 1, 0);
        fixture.current_type_script_input_indices = vec![0];
        let mut payload = vec![0; width];
        payload[width - 1] = 7;
        let mut data = 1u64.to_le_bytes().to_vec();
        data.extend_from_slice(&payload);
        fixture.inputs[0].data = Bytes::from(data.clone());
        let witness = witness_for(&single, &[EntryWitnessArg::Bytes(payload.clone())]);
        fixture.witnesses = vec![witness.clone()];
        let single_run = execute_cellscript_script(strip_vm_abi_trailer(&single.artifact_bytes), &fixture);
        assert_eq!(single_run.exit_code, 0, "single width {width}");
        rows.push(metrics(format!("single-width-{width}"), &single, single_run.cycles, witness.len()));
        payload[width - 1] ^= 1;
        fixture.witnesses = vec![witness_for(&single, &[EntryWitnessArg::Bytes(payload)])];
        assert_ne!(execute_cellscript_script(strip_vm_abi_trailer(&single.artifact_bytes), &fixture).exit_code, 0);

        for count in [1, 2, 4, 8] {
            let source = format!("{prefix}{}", (0..count).map(|index| action(index, width)).collect::<String>());
            let policy = compile_artifact(
                &source,
                options(),
                ArtifactDeclaration {
                    name: "CostPolicy".to_string(),
                    context: ArtifactContext::TypeGroup { resource: "Token".to_string() },
                    dispatch: ArtifactDispatch::PolicyWitnessV1,
                    actions: (0..count).map(|index| ArtifactAction { tag: index as u32, action: format!("check_{index}") }).collect(),
                    common_checks: Vec::new(),
                },
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .expect("bounded cost policy compiles");
            policy.validate().expect("policy artifact validation");
            let mut max_cycles = 0;
            let mut policy_witness_bytes = 0;
            for index in 0..count {
                data[..8].copy_from_slice(&((index + 1) as u64).to_le_bytes());
                fixture.inputs[0].data = Bytes::from(data.clone());
                for valid in [true, false] {
                    let mut payload = vec![0; width];
                    payload[width - 1] = if valid { 7 } else { 6 };
                    let run = execute_cellscript_script_with_transaction_transform(
                        strip_vm_abi_trailer(&policy.artifact_bytes),
                        &fixture,
                        |tx, script| {
                            let selected = encode_policy_action_record(
                                &policy.metadata,
                                &script.calc_script_hash().unpack(),
                                &format!("check_{index}"),
                                &[EntryWitnessArg::Bytes(payload)],
                            )
                            .expect("policy action record");
                            let bundle = encode_policy_witness_bundle(&[PolicyWitnessRecord {
                                role: PolicyScriptRole::Type,
                                script_hash: selected.script_hash,
                                tag: selected.tag,
                                args: selected.args,
                            }])
                            .expect("policy witness bundle");
                            let encoded =
                                packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(bundle)).pack()).build().as_bytes();
                            policy_witness_bytes = encoded.len();
                            tx.as_advanced_builder().set_witnesses(vec![encoded.pack()]).build()
                        },
                    );
                    assert_eq!(run.exit_code == 0, valid, "policy {count}/{width} action {index}, valid={valid}");
                    if valid {
                        max_cycles = max_cycles.max(run.cycles);
                    }
                }
            }
            // A single record has constant protocol overhead; action count must
            // not multiply the witness or change the existing wire format.
            assert_eq!(policy_witness_bytes, witness.len() + 77);
            rows.push(metrics(format!("policy-actions-{count}-width-{width}"), &policy, max_cycles, policy_witness_bytes));
        }
    }
    for bound in [1, 4, 16] {
        let source = include_str!("../fixtures/bounded_group_input.cell")
            .replace("BoundedCellSet<Token, 3>", &format!("BoundedCellSet<Token, {bound}>"));
        let compiled = compile_cellscript(&source);
        compiled.validate().expect("bounded group artifact validation");
        for (count, valid_amount, expected_ok) in [(bound, true, true), (bound + 1, true, false), (bound, false, false)] {
            let mut fixture = build_simple_fixture(Bytes::default(), count, 0);
            fixture.current_type_script_input_indices = (0..count).collect();
            for (index, cell) in fixture.inputs.iter_mut().enumerate() {
                let mut data = (index as u64).to_le_bytes().to_vec();
                let amount: u64 = if valid_amount || index != count - 1 { 1 } else { 0 };
                data.extend_from_slice(&amount.to_le_bytes());
                cell.data = Bytes::from(data);
            }
            let run = execute_cellscript_script(strip_vm_abi_trailer(&compiled.artifact_bytes), &fixture);
            assert_eq!(run.exit_code == 0, expected_ok, "group bound {bound}, count {count}, valid amount={valid_amount}");
            if expected_ok {
                rows.push(metrics(format!("group-bound-{bound}"), &compiled, run.cycles, 0));
            }
        }
    }
    assert_eq!(rows.len(), 18, "all growth dimensions executed");
    let mut budgets: BTreeMap<String, Budget> =
        serde_json::from_str(include_str!("../fixtures/cost_corpus/growth_budgets.json")).expect("checked growth budgets");
    for row in &mut rows {
        let name = row["name"].as_str().expect("row name");
        let budget = budgets.remove(name).unwrap_or_else(|| panic!("missing or duplicate budget: {name}"));
        for (field, ceiling) in [
            ("elf_bytes", budget.elf_bytes),
            ("positive_cycles", budget.positive_cycles),
            ("max_stack_frame_bytes", budget.max_stack_frame_bytes),
        ] {
            let measured = row[field].as_u64().expect("measured cost");
            assert!(measured > 0 && measured <= ceiling, "{name} {field}: {measured} exceeds budget {ceiling}");
        }
        assert_eq!(row["witness_bytes"].as_u64(), Some(budget.witness_bytes), "{name} witness ABI footprint");
        row["budget"] = serde_json::to_value(budget).expect("serialize budget");
    }
    assert!(budgets.is_empty(), "budgeted growth samples must not disappear");
    rows
}
