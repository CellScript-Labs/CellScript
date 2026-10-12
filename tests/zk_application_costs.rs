//! Opt-in application accounting; independent of the frozen general cost corpus.
#[allow(dead_code)]
#[path = "support/cost_measurement.rs"]
mod cost_measurement;
#[allow(dead_code)]
#[path = "support/cost_trace.rs"]
mod cost_trace;
use ckb_testtool::{
    ckb_script::ScriptGroupType,
    ckb_types::{bytes::Bytes, core::Capacity, packed, prelude::*},
    context::Context,
};
use serde_json::{json, Value};
use sha2::Digest;
use std::{collections::BTreeMap, fs, path::Path};

fn bytes(v: &Value) -> Vec<u8> {
    hex::decode(v.as_str().unwrap()).unwrap()
}

#[test]
#[ignore = "requires the explicit host proving pilot and application migration fixtures"]
fn application_cost_accounting_pilot() {
    application_cost_accounting(false);
}

#[test]
#[ignore = "requires host cost acceptance; applies archived frozen resource ceilings"]
fn application_cost_accounting_acceptance() {
    application_cost_accounting(true);
}

fn application_cost_accounting(acceptance: bool) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let phase = if acceptance { "acceptance" } else { "pilot" };
    let directory = root.join(format!("contracts/zk-private-counter/target/zk-cost-{phase}"));
    let limit_bytes = include_bytes!("../docs/reports/0.32/zk-costs/limits.json");
    let limits: Value = serde_json::from_slice(limit_bytes).unwrap();
    let bounds = &limits["application"];
    let metadata: cellscript::CompileMetadata =
        serde_json::from_slice(&fs::read(root.join("contracts/zk-private-counter/target/migration-parent-0-metadata.json")).unwrap())
            .unwrap();
    let mut rows = Vec::new();
    for case in ["ordinary", "maximum"] {
        let fixture: Value = serde_json::from_slice(&fs::read(directory.join(format!("{case}-fixture.json"))).unwrap()).unwrap();
        let original = packed::Transaction::from_slice(&bytes(&fixture["transaction"])).unwrap().into_view();
        let mut context = Context::new_with_deterministic_rng();
        let mut cells = BTreeMap::new();
        let mut capacities = Vec::new();
        for cell in fixture["cells"].as_array().unwrap() {
            let point = packed::OutPoint::from_slice(&bytes(&cell["out_point"])).unwrap();
            let output = packed::CellOutput::from_slice(&bytes(&cell["output"])).unwrap();
            let data = Bytes::from(bytes(&cell["data"]));
            if acceptance && data.starts_with(b"\x7fELF") {
                assert!(data.len() as u64 <= bounds["max_deployed_elf_bytes"].as_u64().unwrap());
            }
            capacities.push(json!({"out_point":cell["out_point"],"data_bytes":data.len(),
                "data_hash":hex::encode(cellscript::ckb_blake2b256(&data)),"is_elf":data.starts_with(b"\x7fELF"),
                "capacity_shannons":Unpack::<u64>::unpack(&output.capacity()),
                "occupied_capacity_shannons":output.occupied_capacity(Capacity::bytes(data.len()).unwrap()).unwrap().as_u64()}));
            cells.insert(point.as_slice().to_vec(), output.clone());
            context.create_cell_with_out_point(point, output, data);
        }
        let mut groups = BTreeMap::new();
        for input in original.inputs() {
            let cell = &cells[input.previous_output().as_slice()];
            groups.insert((0u8, cell.lock().as_slice().to_vec()), cell.lock());
            if let Some(script) = cell.type_().to_opt() {
                groups.insert((1u8, script.as_slice().to_vec()), script);
            }
        }
        for cell in original.outputs() {
            if let Some(script) = cell.type_().to_opt() {
                groups.insert((1u8, script.as_slice().to_vec()), script);
            }
        }
        let witness = packed::WitnessArgs::from_slice(&original.witnesses().get(0).unwrap().raw_data()).unwrap();
        let original_payload = witness.input_type().to_opt().unwrap().raw_data();
        assert_eq!(&original_payload[..8], b"CSARGv1\0");
        let handle = original_payload[136..].to_vec();
        let encode = |proof: Vec<u8>| {
            metadata.actions[0]
                .entry_witness_args(&[cellscript::EntryWitnessArg::Bytes(proof), cellscript::EntryWitnessArg::Bytes(handle.clone())])
                .unwrap()
        };
        assert_eq!(encode(original_payload[8..136].to_vec()), original_payload.as_ref());
        // Every host policy produced byte-identical proofs. Bind the observed
        // result through the actual metadata encoder and execute all groups.
        let proof = fs::read(directory.join(format!("{case}-reference-1.proof"))).unwrap();
        for candidate in ["four-workers", "reuse-key", "combined"] {
            assert_eq!(proof, fs::read(directory.join(format!("{case}-{candidate}-1.proof"))).unwrap());
        }
        let mut witnesses: Vec<_> = original.witnesses().into_iter().collect();
        witnesses[0] = witness.as_builder().input_type(Some(Bytes::from(encode(proof))).pack()).build().as_bytes().pack();
        let valid = original.as_advanced_builder().set_witnesses(witnesses).build();
        assert_eq!(valid.hash(), original.hash(), "proving must preserve the complete raw transaction");
        assert_eq!(valid.data().as_slice().len(), original.data().as_slice().len());
        for late_invalid in [false, true] {
            let tx = if late_invalid {
                // A fee-only output change preserves both protected state
                // transitions but invalidates the old proof's raw-tx binding.
                let mut outputs: Vec<_> = valid.outputs().into_iter().collect();
                let fee = outputs.iter().position(|o| o.type_().to_opt().is_none()).unwrap();
                let capacity = Unpack::<u64>::unpack(&outputs[fee].capacity());
                outputs[fee] = outputs[fee].clone().as_builder().capacity(capacity - 1).build();
                valid.as_advanced_builder().set_outputs(outputs).build()
            } else {
                valid.clone()
            };
            let total = context.verify_tx(&tx, 250_000_000);
            if acceptance {
                assert!(tx.data().as_slice().len() as u64 <= bounds["max_transaction_bytes"].as_u64().unwrap());
                if let Ok(cycles) = &total {
                    assert!(*cycles <= bounds["max_transaction_cycles"].as_u64().unwrap());
                }
            }
            if late_invalid {
                let error = total.as_ref().unwrap_err().to_string();
                assert!(error.contains("error code 79"), "late pairing rejection: {error}");
            } else {
                assert!(total.is_ok(), "{total:?}");
            }
            let mut measurements = Vec::new();
            let mut measured_sum = 0;
            let mut saw_rejection = false;
            for ((role, _), script) in &groups {
                let trace = cost_trace::measure(
                    &context,
                    &tx,
                    script,
                    if *role == 0 { ScriptGroupType::Lock } else { ScriptGroupType::Type },
                    250_000_000,
                );
                assert_eq!(trace.status, "measured", "{:?}", trace.unavailable_reason);
                assert_eq!(trace.authoritative_cycles, trace.stepped_cycles);
                if acceptance {
                    assert!(trace.authoritative_cycles.unwrap() <= bounds["max_group_cycles"].as_u64().unwrap());
                }
                measured_sum += trace.authoritative_cycles.unwrap();
                saw_rejection |= trace.exit_code != Some(0);
                let vms: Vec<_> = trace
                    .vms
                    .iter()
                    .map(|vm| {
                        assert!(vm.observed_stack_bytes <= 32_768);
                        if acceptance {
                            assert!(vm.observed_stack_bytes <= bounds["max_vm_stack_bytes"].as_u64().unwrap());
                        }
                        json!({"vm_id":vm.vm_id,"generation":vm.generation,"program_sha256":vm.program_sha256,
                        "observed_stack_bytes":vm.observed_stack_bytes,"entry_sp":vm.entry_sp,"minimum_sp":vm.minimum_sp,
                        "vm_version":vm.vm_version,"isa":vm.isa,
                        "instruction_cycles":vm.pcs.values().map(|pc|pc.instruction_cycles).sum::<u64>(),
                        "synchronous_syscall_cycles":vm.pcs.values().map(|pc|pc.synchronous_syscall_cycles).sum::<u64>()})
                    })
                    .collect();
                measurements.push(json!({"role":if *role == 0 {"lock"} else {"type"},"script_hash":hex::encode(script.calc_script_hash().as_slice()),
                    "exit_code":trace.exit_code,"authoritative_cycles":trace.authoritative_cycles,
                    "replayed_cycles":trace.stepped_cycles,"scheduler_and_loading_cycles":trace.scheduler_and_loading_cycles,"vms":vms}));
            }
            assert_eq!(saw_rejection, late_invalid);
            if let Ok(total) = total {
                assert_eq!(total, measured_sum);
            }
            rows.push(json!({"case":case,"late_invalid":late_invalid,"transaction_hash":hex::encode(tx.hash().as_slice()),
                "transaction_bytes":tx.data().as_slice().len(),"witness_bytes":tx.witnesses().as_slice().len(),
                "transaction_cycles":total.ok(),"transaction_cycles_unavailable_reason":if late_invalid {Some("ordinary all-group verifier does not return rejection cycles; independent group observations are not a completed transaction total")} else {None},
                "groups":measurements,"cells":capacities,"all_group_acceptance":!late_invalid,
                "fee_lock":"always-success fixture; real signed node costs are a separately identified observation"}));
        }
    }
    fs::write(directory.join(format!("application-{phase}.json")), serde_json::to_vec_pretty(&json!({
        "schema":"cellscript-zk-application-cost-observations-v1","status":"passed","rows":rows,"phase":phase,
        "frozen_limits_sha256":if acceptance {Some(hex::encode(sha2::Sha256::digest(limit_bytes)))} else {None},
        "source_sha256":hex::encode(sha2::Sha256::digest(include_bytes!("zk_application_costs.rs"))),
        "vm_heap_peak_bytes":null,"vm_heap_peak_unavailable_reason":"no heap instrumentation; allocator budgets are not observed peaks",
        "ci_or_gates_run":false
    })).unwrap()).unwrap();
}
