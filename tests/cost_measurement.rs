//! The rejection metric must come from executed work, not an error placeholder.

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;
#[path = "support/cost_measurement.rs"]
mod cost_measurement;
#[path = "support/cost_stack.rs"]
mod cost_stack;

use ckb_script_runner::{build_simple_fixture, compile_cellscript_source_to_elf, execute_cellscript_script_with_observer};
use ckb_testtool::{
    ckb_script::ScriptGroupType,
    ckb_types::{bytes::Bytes, prelude::*},
};
use cost_measurement::{measure_group, CycleMeasurement, CycleObservation, CycleScope, ExitCategory};

#[test]
fn nonzero_exits_retain_real_group_cycles_and_transaction_oracle() {
    let mut observed = Vec::new();
    for (body, expected_exit) in
        [("return 0", 0), ("return 7", 7), ("let capacity = ckb::cell_capacity(source::input(0))\nrequire capacity > 0\nreturn 7", 7)]
    {
        let source = format!("module measured_exit\naction verify() -> u64 {{ verification\n{body}\n}}");
        let elf = compile_cellscript_source_to_elf(&source, "verify", None);
        let fixture = build_simple_fixture(Bytes::default(), 1, 1);
        let (oracle, group) = execute_cellscript_script_with_observer(
            &elf,
            &fixture,
            |tx, _| tx,
            |context, tx, script| measure_group(context, tx, script, ScriptGroupType::Type, 10_000_000),
        );
        assert_eq!(oracle.exit_code, expected_exit);
        let CycleObservation::Measured { cycles, exit_code } = group.observation else {
            panic!("ordinary exit must have a scheduler cycle count: {group:?}");
        };
        assert_eq!(i64::from(exit_code), expected_exit);
        assert!(cycles > 0);
        assert_eq!(group.scope, CycleScope::ScriptGroupSchedulerIncludingChildren);
        assert_eq!(group.group.as_ref().unwrap().output_indices, vec![0]);
        let transaction = CycleMeasurement::transaction(oracle.exit_code, oracle.cycles);
        if expected_exit == 0 {
            assert_eq!(group.exit_category, ExitCategory::Success);
            assert!(transaction.required_cycles() > cycles, "transaction also executes the harness Lock");
        } else {
            assert_eq!(group.exit_category, ExitCategory::NonzeroExit);
            assert!(matches!(transaction.observation, CycleObservation::Unavailable { .. }));
        }
        observed.push(cycles);
        let encoded = serde_json::to_string(&group).expect("measurement JSON");
        assert_eq!(serde_json::from_str::<CycleMeasurement>(&encoded).unwrap(), group);
    }
    assert!(observed[2] > observed[1], "the extra syscall and check must increase rejection cost");
}

#[test]
fn limits_and_missing_groups_are_unavailable_not_zero_measurements() {
    let elf =
        compile_cellscript_source_to_elf("module measured_limit\naction verify() -> u64 { verification\nreturn 0\n}", "verify", None);
    let fixture = build_simple_fixture(Bytes::default(), 1, 1);
    let (oracle, (limited, missing)) = execute_cellscript_script_with_observer(
        &elf,
        &fixture,
        |tx, _| tx,
        |context, tx, script| {
            let limited = measure_group(context, tx, script, ScriptGroupType::Type, 1);
            let absent = script.clone().as_builder().args(Bytes::from_static(b"absent-group").pack()).build();
            let missing = measure_group(context, tx, &absent, ScriptGroupType::Type, 10_000_000);
            (limited, missing)
        },
    );
    assert_eq!(oracle.exit_code, 0);
    assert_eq!(limited.exit_category, ExitCategory::CycleLimit);
    assert_eq!(missing.exit_category, ExitCategory::SetupFailure);
    for measurement in [limited, missing] {
        assert!(matches!(measurement.observation, CycleObservation::Unavailable { .. }));
        let json = serde_json::to_value(&measurement).unwrap();
        assert!(json["observation"].get("cycles").is_none());
    }
}

#[test]
fn static_stack_bound_includes_nested_entry_and_action_frames() {
    let compiled = cellscript::compile(
        "module stack_bound\naction verify(witness value: u64) -> u64 { verification\nrequire value > 0\nreturn 0\n}",
        cellscript::CompileOptions { target: Some("riscv64-elf".into()), target_profile: Some("ckb".into()), ..Default::default() },
    )
    .expect("stack fixture compiles");
    compiled.validate().expect("independent artifact validation");
    let maximum_frame =
        compiled.verified_lowering_record.as_ref().unwrap().entries.iter().map(|entry| entry.frame_size_bytes).max().unwrap();
    let bound = cost_stack::measure(&compiled);
    let cost_stack::StaticStackBound::Bounded { static_call_chain_stack_bound_bytes } = bound else {
        panic!("closed call graph must have a static bound: {bound:?}");
    };
    assert!(static_call_chain_stack_bound_bytes > u64::from(maximum_frame));
}

#[test]
fn illegal_instruction_traps_are_unavailable() {
    let mut elf =
        compile_cellscript_source_to_elf("module measured_trap\naction verify() -> u64 { verification\nreturn 0\n}", "verify", None);
    let parsed = cellscript_artifact_checker::parse_elf(&elf, 100_000).unwrap();
    let offset = (parsed.text.offset + parsed.entry - parsed.text.address) as usize;
    elf[offset..offset + 4].fill(0);
    let fixture = build_simple_fixture(Bytes::default(), 1, 1);
    let (oracle, group) = execute_cellscript_script_with_observer(
        &elf,
        &fixture,
        |tx, _| tx,
        |context, tx, script| measure_group(context, tx, script, ScriptGroupType::Type, 10_000_000),
    );
    assert_ne!(oracle.exit_code, 0);
    assert_eq!(group.exit_category, ExitCategory::VmTrap);
    assert!(matches!(group.observation, CycleObservation::Unavailable { .. }));
}
