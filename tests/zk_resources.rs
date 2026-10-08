//! Separate diagnostic replay of an application-generated transaction.
#[allow(dead_code)]
#[path = "support/cost_measurement.rs"]
mod cost_measurement;
#[allow(dead_code)]
#[path = "support/cost_trace.rs"]
mod cost_trace;
use ckb_testtool::{
    ckb_script::ScriptGroupType,
    ckb_types::{bytes::Bytes, packed, prelude::*},
    context::Context,
};

#[test]
#[ignore = "requires the private-counter application fixture; unified gates run both"]
fn exact_application_resource_replay() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("contracts/zk-private-counter/target/resource-fixture.json")).unwrap())
            .unwrap();
    let bytes = |v: &serde_json::Value| hex::decode(v.as_str().unwrap()).unwrap();
    let tx = packed::Transaction::from_slice(&bytes(&fixture["transaction"])).unwrap().into_view();
    let script = packed::Script::from_slice(&bytes(&fixture["script"])).unwrap();
    let mut context = Context::new_with_deterministic_rng();
    let mut capacities = Vec::new();
    for cell in fixture["cells"].as_array().unwrap() {
        let output = packed::CellOutput::from_slice(&bytes(&cell["output"])).unwrap();
        let data_size = bytes(&cell["data"]).len();
        capacities.push(serde_json::json!({"out_point":cell["out_point"],"data_bytes":data_size,"capacity_shannons":Unpack::<u64>::unpack(&output.capacity()),"occupied_capacity_shannons":output.occupied_capacity(ckb_testtool::ckb_types::core::Capacity::bytes(data_size).unwrap()).unwrap().as_u64()}));
        context.create_cell_with_out_point(
            packed::OutPoint::from_slice(&bytes(&cell["out_point"])).unwrap(),
            packed::CellOutput::from_slice(&bytes(&cell["output"])).unwrap(),
            Bytes::from(bytes(&cell["data"])),
        );
    }
    let cycles = context.verify_tx(&tx, 250_000_000).unwrap();
    let trace = cost_trace::measure(&context, &tx, &script, ScriptGroupType::Type, 250_000_000);
    assert_eq!(trace.status, "measured", "{:?}", trace.unavailable_reason);
    assert_eq!(trace.exit_code, Some(0));
    assert!(trace.vms.len() >= 3, "lifecycle EXEC parent plus Spawn child");
    let vms: Vec<_> = trace.vms.iter().map(|vm| serde_json::json!({"vm_id":vm.vm_id,"generation":vm.generation,"program_sha256":vm.program_sha256,"vm_version":vm.vm_version,"isa":vm.isa,"entry_sp":vm.entry_sp,"minimum_sp":vm.minimum_sp,"observed_stack_bytes":vm.observed_stack_bytes})).collect();
    let report = serde_json::json!({"schema":"cellscript-zk-resources-v1","status":"passed","scope":"separate scheduler replay; stack observations are not heap peaks or static bounds","transaction_hash":hex::encode(tx.hash().as_slice()),"transaction_cycles":cycles,"type_group_cycles":trace.authoritative_cycles,"replayed_type_group_cycles":trace.stepped_cycles,"transaction_bytes":tx.data().as_slice().len(),"witness_bytes":tx.witnesses().as_slice().len(),"child_allocator_budget_bytes":532480,"proof_bytes":128,"request_bytes":464,"vk_bytes":744,"max_calls":1,"vms":vms,"cells":capacities});
    std::fs::write(root.join("contracts/zk-private-counter/target/resources.json"), serde_json::to_vec_pretty(&report).unwrap())
        .unwrap();
}

#[test]
#[ignore = "requires the counter migration fixtures; unified gates run the producer first"]
fn exact_migration_resource_replay() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = root.join("contracts/zk-private-counter/target");
    std::fs::write(directory.join("migration-resources.json"), b"{\"status\":\"running\"}\n").unwrap();
    let input = std::fs::read(directory.join("migration-resource-fixtures.json")).unwrap();
    let fixtures: Vec<serde_json::Value> = serde_json::from_slice(&input).unwrap();
    assert_eq!(fixtures.len(), 1, "a successful maximum-bound migration fixture is required");
    let mut rows = Vec::new();
    for fixture in fixtures {
        let bytes = |value: &serde_json::Value| hex::decode(value.as_str().unwrap()).unwrap();
        let tx = packed::Transaction::from_slice(&bytes(&fixture["transaction"])).unwrap().into_view();
        assert_eq!((tx.inputs().len(), tx.outputs().len(), tx.cell_deps().len()), (64, 64, 64));
        assert_eq!(tx.data().as_slice().len(), 16_384);
        let mut context = Context::new_with_deterministic_rng();
        let mut capacities = Vec::new();
        for cell in fixture["cells"].as_array().unwrap() {
            let output = packed::CellOutput::from_slice(&bytes(&cell["output"])).unwrap();
            let data = bytes(&cell["data"]);
            capacities.push(serde_json::json!({
                "out_point": cell["out_point"], "data_bytes": data.len(),
                "capacity_shannons": Unpack::<u64>::unpack(&output.capacity()),
                "occupied_capacity_shannons": output.occupied_capacity(ckb_testtool::ckb_types::core::Capacity::bytes(data.len()).unwrap()).unwrap().as_u64()
            }));
            context.create_cell_with_out_point(
                packed::OutPoint::from_slice(&bytes(&cell["out_point"])).unwrap(),
                output,
                Bytes::from(data),
            );
        }
        let total_cycles = context.verify_tx(&tx, 250_000_000).unwrap();
        let mut groups = Vec::new();
        let mut type_cycles = 0;
        for (field, expected_vms) in [("counter_script", 3), ("config_script", 1)] {
            let script = packed::Script::from_slice(&bytes(&fixture[field])).unwrap();
            let trace = cost_trace::measure(&context, &tx, &script, ScriptGroupType::Type, 250_000_000);
            assert_eq!(trace.status, "measured", "{field}: {:?}", trace.unavailable_reason);
            assert_eq!(trace.exit_code, Some(0));
            assert_eq!(trace.authoritative_cycles, trace.stepped_cycles);
            assert_eq!(trace.vms.len(), expected_vms);
            for vm in &trace.vms {
                assert!(vm.observed_stack_bytes <= 32_768, "{field}: VM {} generation {}", vm.vm_id, vm.generation);
            }
            type_cycles += trace.authoritative_cycles.unwrap();
            let vms: Vec<_> = trace
                .vms
                .iter()
                .map(|vm| {
                    serde_json::json!({
                        "vm_id": vm.vm_id, "generation": vm.generation, "program_sha256": vm.program_sha256,
                        "vm_version": vm.vm_version, "isa": vm.isa, "entry_sp": vm.entry_sp,
                        "minimum_sp": vm.minimum_sp, "observed_stack_bytes": vm.observed_stack_bytes
                    })
                })
                .collect();
            groups.push(serde_json::json!({"role":field,"script_hash":hex::encode(script.calc_script_hash().as_slice()),
                "authoritative_cycles":trace.authoritative_cycles,"replayed_cycles":trace.stepped_cycles,"vms":vms}));
        }
        assert!(type_cycles <= total_cycles);
        rows.push(serde_json::json!({"transaction_hash":hex::encode(tx.hash().as_slice()),"transaction_cycles":total_cycles,
            "type_group_cycles":type_cycles,"remaining_lock_group_cycles":total_cycles-type_cycles,
            "transaction_bytes":tx.data().as_slice().len(),"witness_bytes":tx.witnesses().as_slice().len(),
            "groups":groups,"cells":capacities}));
    }
    let report = serde_json::json!({"schema":"cellscript-counter-migration-resources-v1","status":"passed",
        "fixture_ckb_data_hash":hex::encode(cellscript::ckb_blake2b256(&input)),
        "scope":"all-group scheduler acceptance plus separate per-Type-group instruction replay; not node liveness",
        "max_transaction_cycles":250000000,"max_transaction_bytes":16384,"max_observed_vm_stack_bytes":32768,
        "observed_peak_heap_bytes":null,"observed_peak_heap_availability":"not-measured",
        "lifecycle_and_config_allocator_budget_each":36864,"child_allocator_budget_bytes":532480,"rows":rows});
    std::fs::write(directory.join("migration-resources.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}
