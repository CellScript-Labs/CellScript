//! Complete protected-set accounting, separate from single-state cost ceilings.
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
use std::{collections::BTreeMap, fs, path::Path};
fn bytes(v: &Value) -> Vec<u8> {
    hex::decode(v.as_str().unwrap()).unwrap()
}

#[test]
#[ignore = "requires native counter_set producer; explicit resource replay only"]
fn protected_set_resources_include_every_group_and_vm_generation() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/counter-set");
    let raw = fs::read(directory.join("fixtures.json")).unwrap();
    let fixtures: Vec<Value> = serde_json::from_slice(&raw).unwrap();
    assert_eq!(fixtures.len(), 3);
    let mut rows = Vec::new();
    for fixture in fixtures {
        let count = fixture["count"].as_u64().unwrap();
        let original = packed::Transaction::from_slice(&bytes(&fixture["transaction"])).unwrap().into_view();
        let mut context = Context::new_with_deterministic_rng();
        let mut inputs = BTreeMap::new();
        let mut capacities = Vec::new();
        for cell in fixture["cells"].as_array().unwrap() {
            let point = packed::OutPoint::from_slice(&bytes(&cell["out_point"])).unwrap();
            let output = packed::CellOutput::from_slice(&bytes(&cell["output"])).unwrap();
            let data = Bytes::from(bytes(&cell["data"]));
            capacities.push(json!({"out_point":cell["out_point"],"data_bytes":data.len(),"is_elf":data.starts_with(b"\x7fELF"),
                "data_hash":hex::encode(cellscript::ckb_blake2b256(&data)),"capacity_shannons":Unpack::<u64>::unpack(&output.capacity()),
                "occupied_capacity_shannons":output.occupied_capacity(Capacity::bytes(data.len()).unwrap()).unwrap().as_u64()}));
            inputs.insert(point.as_slice().to_vec(), output.clone());
            context.create_cell_with_out_point(point, output, data);
        }
        let mut groups = BTreeMap::new();
        for input in original.inputs() {
            let cell = &inputs[input.previous_output().as_slice()];
            groups.insert((0u8, cell.lock().as_slice().to_vec()), cell.lock());
            if let Some(script) = cell.type_().to_opt() {
                groups.insert((1u8, script.as_slice().to_vec()), script);
            }
        }
        assert_eq!(groups.len(), count as usize + 1);
        for invalid in [false, true] {
            let tx = if invalid {
                let mut outputs = original.outputs().into_iter().collect::<Vec<_>>();
                let fee = count as usize;
                let capacity = Unpack::<u64>::unpack(&outputs[fee].capacity());
                outputs[fee] = outputs[fee].clone().as_builder().capacity(capacity - 1).build();
                original.as_advanced_builder().set_outputs(outputs).build()
            } else {
                original.clone()
            };
            let total = context.verify_tx(&tx, 500_000_000);
            if invalid {
                assert!(total.as_ref().unwrap_err().to_string().contains("error code 79"));
            } else {
                assert!(total.is_ok());
            }
            let mut measured = Vec::new();
            let mut sum = 0;
            let mut children = 0;
            for ((role, _), script) in &groups {
                let trace = cost_trace::measure(
                    &context,
                    &tx,
                    script,
                    if *role == 0 { ScriptGroupType::Lock } else { ScriptGroupType::Type },
                    500_000_000,
                );
                assert_eq!(trace.status, "measured", "{:?}", trace.unavailable_reason);
                assert_eq!(trace.authoritative_cycles, trace.stepped_cycles);
                assert_eq!(trace.exit_code, Some(if invalid && *role == 1 { 79 } else { 0 }));
                sum += trace.authoritative_cycles.unwrap();
                if *role == 1 {
                    assert_eq!(trace.vms.len(), 3);
                    children += 1;
                    assert!(trace.authoritative_cycles.unwrap() <= 250_000_000);
                }
                let vms=trace.vms.iter().map(|vm| {
                    assert!(vm.observed_stack_bytes<=32_768);
                    json!({"vm_id":vm.vm_id,"generation":vm.generation,"program_sha256":vm.program_sha256,"vm_version":vm.vm_version,"isa":vm.isa,
                        "entry_sp":vm.entry_sp,"minimum_sp":vm.minimum_sp,"observed_stack_bytes":vm.observed_stack_bytes,
                        "instruction_cycles":vm.pcs.values().map(|p|p.instruction_cycles).sum::<u64>(),
                        "synchronous_syscall_cycles":vm.pcs.values().map(|p|p.synchronous_syscall_cycles).sum::<u64>()})
                }).collect::<Vec<_>>();
                measured.push(json!({"role":if *role==0 {"lock"}else{"type"},"script_hash":hex::encode(script.calc_script_hash().as_slice()),
                    "exit_code":trace.exit_code,"authoritative_cycles":trace.authoritative_cycles,"replayed_cycles":trace.stepped_cycles,
                    "scheduler_and_loading_cycles":trace.scheduler_and_loading_cycles,"vms":vms}));
            }
            assert_eq!(children, count);
            if let Ok(cycles) = total {
                assert_eq!(cycles, sum);
            }
            assert!(tx.data().as_slice().len() <= 16_384);
            rows.push(json!({"protected_inputs":count,"proofs":count,"late_invalid":invalid,"transaction_cycles":total.ok(),
                "transaction_cycles_unavailable_reason":if invalid {Some("all-group rejection API does not return total; independently replayed rejected groups are not an executed total")}else{None},
                "transaction_hash":hex::encode(tx.hash().as_slice()),"transaction_bytes":tx.data().as_slice().len(),"witness_bytes":tx.witnesses().as_slice().len(),
                "groups":measured,"cells":capacities}));
        }
    }
    fs::write(directory.join("resources.json"),serde_json::to_vec_pretty(&json!({"schema":"cellscript-counter-set-resources-v1","status":"passed",
        "fixture_hash":hex::encode(cellscript::ckb_blake2b256(&raw)),"max_transaction_cycles":500000000,"max_per_vm_stack_bytes":32768,
        "heap_peak_bytes":null,"heap_peak_unavailable_reason":"allocator ceilings are not measured heap peaks",
        "scope":"all-group acceptance and individual authoritative/diagnostic group replay; no sum of stack peaks; not live node acceptance",
        "fee_lock":"always-success fixture","rows":rows,"ci_or_gates_run":false})) .unwrap()).unwrap();
}
