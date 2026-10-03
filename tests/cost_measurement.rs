//! The rejection metric must come from executed work, not an error placeholder.

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;
#[path = "support/cost_measurement.rs"]
mod cost_measurement;
#[path = "support/cost_memory.rs"]
mod cost_memory;
#[path = "support/cost_stack.rs"]
mod cost_stack;
#[path = "support/cost_trace.rs"]
mod cost_trace;

use ckb_script_runner::{build_simple_fixture, compile_cellscript_source_to_elf, execute_cellscript_script_with_observer};
use ckb_testtool::{
    ckb_script::ScriptGroupType,
    ckb_types::{bytes::Bytes, prelude::*},
};
use cost_measurement::{measure_group, CycleMeasurement, CycleObservation, CycleScope, ExitCategory};

#[test]
fn diagnostic_trace_preserves_scheduler_verdict_and_cycles() {
    for exit in [0, 7] {
        let source = format!("module traced\naction verify() -> u64 {{ verification\nlet capacity = ckb::cell_capacity(source::input(0))\nrequire capacity > 0\nreturn {exit}\n}}");
        let elf = compile_cellscript_source_to_elf(&source, "verify", None);
        let fixture = build_simple_fixture(Bytes::default(), 1, 1);
        let (oracle, trace) = execute_cellscript_script_with_observer(
            &elf,
            &fixture,
            |tx, _| tx,
            |context, tx, script| cost_trace::measure(context, tx, script, ScriptGroupType::Type, 10_000_000),
        );
        assert_eq!(oracle.exit_code, exit);
        assert_eq!(trace.status, "measured", "{trace:?}");
        assert_eq!(trace.exit_code, Some(exit as i8));
        assert_eq!(trace.authoritative_cycles, trace.stepped_cycles);
        assert_eq!(trace.vms.len(), 1);
        let vm = &trace.vms[0];
        assert_eq!(vm.vm_id, 0);
        assert_eq!(vm.program_sha256.len(), 64);
        assert!(vm.observed_stack_bytes > 0);
        assert!(vm.syscalls.contains_key(&2081));
        let roundtrip: cost_trace::ExecutionTrace = serde_json::from_str(&serde_json::to_string(&trace).unwrap()).unwrap();
        assert_eq!(roundtrip.authoritative_cycles, trace.authoritative_cycles);
    }
}

// Replace only the entry text of a sufficiently large compiler ELF. These
// hand-calculated programs deliberately test the observer independently of
// compiler stack-frame metadata and are not checked source/ELF bundles.
fn hand_program(words: &[u32]) -> Vec<u8> {
    let mut elf = compile_cellscript_source_to_elf(
        "module trace_fixture\naction verify(witness x: u64) -> u64 { verification\nrequire x > 0\nreturn 0\n}",
        "verify",
        None,
    );
    let parsed = cellscript_artifact_checker::parse_elf(&elf, 100_000).unwrap();
    let offset = (parsed.text.offset + parsed.entry - parsed.text.address) as usize;
    assert!(words.len() * 4 <= parsed.text.size as usize);
    for (i, word) in words.iter().enumerate() {
        elf[offset + i * 4..offset + i * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    elf
}

#[test]
fn observed_stack_and_memory_match_hand_calculated_nested_outgoing_frames() {
    let elf = hand_program(&[
        0xfe010113, // addi sp,sp,-32
        0x00113023, // sd ra,0(sp)
        0xff010113, // addi sp,sp,-16 (outgoing area)
        0x018000ef, // jal ra,+24 (callee at byte 36)
        0x01010113, // addi sp,sp,16
        0x00013083, // ld ra,0(sp)
        0x02010113, // addi sp,sp,32
        0x0140006f, // j +20 (exit at byte 48; skips callee)
        0x00000013, // unreachable nop
        0xfd010113, // addi sp,sp,-48
        0x03010113, // addi sp,sp,48
        0x00008067, // ret
        0x00000513, // li a0,0
        0x05d00893, // li a7,93
        0x00000073, // ecall
    ]);
    let fixture = build_simple_fixture(Bytes::default(), 1, 1);
    let (_, trace) = execute_cellscript_script_with_observer(
        &elf,
        &fixture,
        |tx, _| tx,
        |context, tx, script| cost_trace::measure(context, tx, script, ScriptGroupType::Type, 10_000_000),
    );
    assert_eq!(trace.status, "measured", "{trace:?}");
    assert_eq!(trace.vms[0].observed_stack_bytes, 96);
    assert_eq!(trace.vms[0].instructions["SD"], 1);
    assert_eq!(trace.vms[0].instructions["LD_VERSION1"], 1);
}

#[test]
fn observed_loop_peak_is_not_iterations_times_frame_and_failure_keeps_peak() {
    for exit in [0u32, 7] {
        let elf = hand_program(&[
            0x00300293,           // li t0,3
            0xfe810113,           // loop: addi sp,sp,-24
            0x01810113,           // addi sp,sp,24
            0xfff28293,           // addi t0,t0,-1
            0xfe029ae3,           // bnez t0,loop (-12)
            (exit << 20) | 0x513, // li a0,exit
            0x05d00893,           // li a7,93
            0x00000073,           // ecall
        ]);
        let fixture = build_simple_fixture(Bytes::default(), 1, 1);
        let (_, trace) = execute_cellscript_script_with_observer(
            &elf,
            &fixture,
            |tx, _| tx,
            |context, tx, script| cost_trace::measure(context, tx, script, ScriptGroupType::Type, 10_000_000),
        );
        assert_eq!(trace.status, "measured", "{trace:?}");
        assert_eq!(trace.exit_code, Some(exit as i8));
        assert_eq!(trace.vms[0].observed_stack_bytes, 24);
        assert_eq!(trace.vms[0].instructions["BNE"], 3);
    }
}

#[test]
fn unavailable_trace_never_substitutes_zero_for_a_stack_or_cycle_measurement() {
    let elf = hand_program(&[0x00000513, 0x05d00893, 0x00000073]);
    let fixture = build_simple_fixture(Bytes::default(), 1, 1);
    let (_, trace) = execute_cellscript_script_with_observer(
        &elf,
        &fixture,
        |tx, _| tx,
        |context, tx, script| cost_trace::measure(context, tx, script, ScriptGroupType::Type, 1),
    );
    assert_eq!(trace.status, "unavailable");
    assert!(trace.authoritative_cycles.is_none());
    assert!(trace.stepped_cycles.is_none());
    assert!(trace.vms.is_empty());
    assert!(trace.unavailable_reason.is_some());
}

#[test]
fn traced_spawn_and_exec_preserve_separate_vm_stack_identities() {
    use ckb_script_runner::FixtureCell;
    let child = hand_program(&[0xfc010113, 0x04010113, 0x00000513, 0x05d00893, 0x00000073]);
    let hash = ckb_testtool::ckb_hash::blake2b_256(&child);
    let literal: String = hash.iter().map(|byte| format!("\\x{byte:02x}")).collect();
    for spawn in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let manager = cellscript::package::PackageManager::new(directory.path());
        manager.init("traced_process").unwrap();
        let mut manifest = manager.read_manifest().unwrap();
        manifest.package.edition = cellscript::CellScriptEdition::Edition2027;
        manifest.deploy.ckb = Some(cellscript::package::CkbDeployConfig {
            trusted_external_verifiers: vec![cellscript::package::CkbTrustedExternalVerifierConfig {
                schema: "cellscript-trusted-external-verifier-v1".into(),
                name: "trace-fixture".into(),
                scope: "action:verify".into(),
                operation: if spawn { "spawn-wait" } else { "exec" }.into(),
                adapter: if spawn { "hex4-v1" } else { "u8-args-v1" }.into(),
                code_hash: hex::encode(hash),
                hash_type: "data".into(),
                source_identity: "hand-calculated observer fixture".into(),
                applicability: "per-VM stack trace test".into(),
                trust_basis: "exact executable bytes".into(),
                guarantees: vec!["child reserves exactly 64 bytes".into()],
            }],
            ..Default::default()
        });
        manager.write_manifest(&manifest).unwrap();
        let call = if spawn {
            format!("let mut bytes = Vec::new()\nbytes.push(0 as u8)\nckb::trusted_spawn_wait_cell_dep_hex4(0, Hash::from_bytes(b\"{literal}\"), bytes, 1, 0, 0, 0)")
        } else {
            format!("ckb::trusted_exec_cell_dep_u8_args(0, Hash::from_bytes(b\"{literal}\"), 0, 0, 0, 0, 0)")
        };
        std::fs::write(
            directory.path().join("src/main.cell"),
            format!("module traced_process\naction verify() -> u64 {{ verification\n{call}\nreturn 0\n}}"),
        )
        .unwrap();
        let compiled = cellscript::compile_path_with_executable_surface_policy(
            camino::Utf8Path::from_path(directory.path()).unwrap(),
            cellscript::CompileOptions {
                edition: cellscript::CellScriptEdition::Edition2027,
                target: Some("riscv64-elf".into()),
                target_profile: Some("ckb".into()),
                ..Default::default()
            },
            Some(cellscript::CompileEntryScope::Action("verify".into())),
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
        fixture.cell_deps.push(FixtureCell { capacity: 100_000_000_000, type_script: None, data: Bytes::copy_from_slice(&child) });
        let (oracle, trace) = execute_cellscript_script_with_observer(
            cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes),
            &fixture,
            |tx, _| tx,
            |context, tx, script| cost_trace::measure(context, tx, script, ScriptGroupType::Type, 10_000_000),
        );
        assert_eq!(oracle.exit_code, 0);
        assert_eq!(trace.status, "measured", "spawn={spawn}: {trace:?}");
        assert_eq!(trace.vms.len(), 2);
        let child = &trace.vms[1];
        assert_eq!(child.observed_stack_bytes, 64);
        assert_ne!(child.program_sha256, trace.vms[0].program_sha256);
        assert_eq!((child.vm_id, child.generation), if spawn { (1, 0) } else { (0, 1) });
    }
}

#[test]
fn static_memory_counts_decode_text_without_executing_or_scanning_data() {
    let mut elf =
        compile_cellscript_source_to_elf("module memory_count\naction verify() -> u64 { verification\nreturn 0\n}", "verify", None);
    let parsed = cellscript_artifact_checker::parse_elf(&elf, 100_000).unwrap();
    let instruction =
        parsed.instructions.iter().find(|instruction| instruction.word & 0x707f == 0x13 && (instruction.word >> 7) & 31 != 2).unwrap();
    let offset = (parsed.text.offset + instruction.address - parsed.text.address) as usize;
    let before = cost_memory::measure(&elf);
    // Replace a non-stack ADDI, preserving paired AUIPC/JALR calls. These loads
    // need not be executable: the metric counts static text, not a run trace.
    elf[offset..offset + 4].copy_from_slice(&0x0000_3083u32.to_le_bytes()); // ld ra, 0(zero)
    let loaded = cost_memory::measure(&elf);
    assert_eq!(loaded["load_instruction_count"].as_u64().unwrap(), before["load_instruction_count"].as_u64().unwrap() + 1);
    assert_eq!(loaded["store_instruction_count"], before["store_instruction_count"]);
    elf[offset..offset + 4].copy_from_slice(&0x0010_3023u32.to_le_bytes()); // sd ra, 0(zero)
    let stored = cost_memory::measure(&elf);
    assert_eq!(stored["store_instruction_count"].as_u64().unwrap(), before["store_instruction_count"].as_u64().unwrap() + 1);
    assert_eq!(stored["load_instruction_count"], before["load_instruction_count"]);
    assert_eq!(stored["instruction_count"], before["instruction_count"]);
    elf.extend_from_slice(&0x0000_3083u32.to_le_bytes());
    assert_eq!(cost_memory::measure(&elf), stored, "bytes outside text are not instructions");
}

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
