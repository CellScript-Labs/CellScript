//! Source-level equivalent probes. These do not enable a compiler rewrite.

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;
#[path = "support/cost_measurement.rs"]
#[allow(dead_code)]
mod cost_measurement;
#[path = "support/cost_memory.rs"]
mod cost_memory;
#[path = "support/cost_stack.rs"]
mod cost_stack;

use cellscript::{CompileOptions, EntryWitnessArg, ExecutableSurfacePolicy};
use ckb_testtool::{
    ckb_script::ScriptGroupType,
    ckb_types::{bytes::Bytes, packed, prelude::*},
};
use serde_json::json;

#[test]
fn repeated_pure_expression_probe_preserves_values_and_rejections() {
    let mut rows = Vec::new();
    for reused in [false, true] {
        let source=format!("module cse_probe\naction verify(witness seed: u64,witness expected: u64) {{ verification\nlet first = seed & 255\nlet second = {}\nrequire first == expected\nrequire second == expected\n}}",if reused {"first"} else {"seed & 255"});
        let compiled = cellscript::compile_with_executable_surface_policy(
            &source,
            CompileOptions {
                opt_level: 3,
                edition: cellscript::CellScriptEdition::Edition2027,
                target: Some("riscv64-elf".into()),
                target_profile: Some("ckb".into()),
                ..Default::default()
            },
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        compiled.validate().unwrap();
        let elf = cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes);
        let mut executions = Vec::new();
        for seed in [0, 1, 255, 256, u64::MAX] {
            for valid in [false, true] {
                let expected = if valid { seed & 255 } else { (seed & 255) ^ 1 };
                let payload = compiled.metadata.actions[0]
                    .entry_witness_args(&[EntryWitnessArg::U64(seed), EntryWitnessArg::U64(expected)])
                    .unwrap();
                let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                fixture.witnesses =
                    vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
                let (oracle, group) = ckb_script_runner::execute_cellscript_script_with_observer(
                    elf,
                    &fixture,
                    |tx, _| tx,
                    |context, tx, script| cost_measurement::measure_group(context, tx, script, ScriptGroupType::Type, 10_000_000),
                );
                assert_eq!(oracle.exit_code == 0, valid);
                executions.push(json!({"seed":seed,"valid":valid,"exit":oracle.exit_code,"group_cycles":group.required_cycles()}));
            }
        }
        rows.push(json!({"reused":reused,"source":source,"elf_bytes":elf.len(),"memory":cost_memory::measure(elf),
            "static_stack":cost_stack::measure(&compiled),"executions":executions}));
    }
    for (before, after) in rows[0]["executions"].as_array().unwrap().iter().zip(rows[1]["executions"].as_array().unwrap()) {
        assert_eq!(before["exit"], after["exit"], "preserve exact error behavior");
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/cellscript-cost/pure-expression-experiment.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path,serde_json::to_vec_pretty(&json!({"schema":"cellscript-pure-expression-experiment-v1",
        "scope":"explicit source rewrite of repeated nontrapping scalar AND; not a general compiler CSE pass",
        "decision":"defer-general-cse","blocker":"value numbering must account for mutable definitions, calls, traps, resource effects and first-failure order",
        "rows":rows})).unwrap()).unwrap();
}
