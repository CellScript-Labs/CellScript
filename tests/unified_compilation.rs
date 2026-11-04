//! The normal strategy and pass isolation retain checked execution behavior.
use cellscript::{CompileOptions, CompilerDiagnostics, EntryWitnessArg, OptimizationPass};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

const SOURCE: &str = r#"module unified_strategy
fn increment(value: u64) -> u64 { return value + 1 }
fn unused() -> u64 { return 99 }
action inspect(witness value: u64) -> u64 {
    verification
    let constant = 2 + 3
    let next = increment(value)
    require next > value
    require next == constant
    return 0
}
"#;

#[test]
fn source_byte_loop_preserves_shift_traps_and_zero_iteration_behavior() {
    let source = r#"module byte_loop_traps
resource ByteData has store { value: u8 }
fn same(a: InputView<ByteData>, b: InputView<ByteData>, length: u64, amount: u64) -> bool {
    let mut index: u64 = 0
    while index < length {
        let dead = 1 << amount
        if ckb::cell_data_u8(a, index) != ckb::cell_data_u8(b, index) { return false }
        index += 1
    }
    return true
}
action inspect(witness amount: u64, witness length: u64) -> u64 {
    verification
    require same(ckb::input<ByteData>(0), ckb::input<ByteData>(0), length, amount)
    return 0
}
"#;
    for diagnostics in [
        CompilerDiagnostics::default(),
        CompilerDiagnostics::default().without(OptimizationPass::SourceByteEquality),
        CompilerDiagnostics::without_optimizations(),
    ] {
        let compiled =
            cellscript::compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                .unwrap();
        for (amount, length, expected_exit) in [(0, 1, 0), (63, 1, 0), (64, 1, 65), (u64::MAX, 1, 65), (64, 0, 0)] {
            let payload = compiled.metadata.actions[0]
                .entry_witness_args(&[EntryWitnessArg::U64(amount), EntryWitnessArg::U64(length)])
                .unwrap();
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.inputs[0].data = Bytes::from(vec![7]);
            fixture.current_type_script_input_indices = vec![0];
            fixture.witnesses =
                vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
            let execution = ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code, expected_exit, "{diagnostics}, shift={amount}, length={length}: {execution:?}");
        }
    }
}

#[test]
fn unified_strategy_and_each_diagnostic_preserve_success_failure_and_overflow() {
    let mut configurations = vec![CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()];
    configurations.extend(
        [
            OptimizationPass::ConstantFolding,
            OptimizationPass::Inlining,
            OptimizationPass::DeadCode,
            OptimizationPass::ExactCellEquality,
            OptimizationPass::SourceByteEquality,
            OptimizationPass::ExactReadHotCache,
            OptimizationPass::StackReloads,
            OptimizationPass::BranchFusion,
        ]
        .map(|pass| CompilerDiagnostics::default().without(pass)),
    );
    for diagnostics in configurations {
        let compiled = cellscript::compile_with_executable_surface_policy(
            SOURCE,
            CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        cellscript_artifact_checker::check_bundle_values(
            &compiled.artifact_bytes,
            &serde_json::to_value(&compiled.metadata).unwrap(),
            compiled.verified_lowering_record.as_ref().unwrap(),
            compiled.source_artifact_map.as_ref().unwrap(),
            &cellscript_artifact_checker::CheckerBudgets::default(),
        )
        .unwrap();
        for (value, succeeds) in [(4, true), (5, false), (u64::MAX, false)] {
            let payload = compiled.metadata.actions[0].entry_witness_args(&[EntryWitnessArg::U64(value)]).unwrap();
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.current_type_script_input_indices = vec![0];
            fixture.witnesses =
                vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
            let execution = ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code == 0, succeeds, "{diagnostics}, value={value}: {execution:?}");
        }
    }
}

#[test]
fn normal_strategy_is_default_and_compilation_is_reproducible() {
    assert!(CompileOptions::default().diagnostics.is_standard());
    let options = CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() };
    let first = cellscript::compile(SOURCE, options.clone()).unwrap();
    let second = cellscript::compile(SOURCE, options).unwrap();
    assert_eq!(first.artifact_bytes, second.artifact_bytes);
    assert_eq!(first.verified_lowering_record, second.verified_lowering_record);
    assert_eq!(first.metadata.source_content_hash, second.metadata.source_content_hash);
    let diagnostic = cellscript::compile(
        SOURCE,
        CompileOptions {
            diagnostics: CompilerDiagnostics::without_optimizations(),
            target: Some("riscv64-elf".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_ne!(first.artifact_bytes, diagnostic.artifact_bytes);
}

#[test]
fn package_build_settings_reject_retired_levels_and_diagnostic_controls() {
    for field in ["opt_level", "optimization_level", "opt", "diagnostics"] {
        let config = format!("{field} = 0");
        assert!(toml::from_str::<cellscript::package::BuildConfig>(&config).is_err(), "accepted {field}");
    }
}

#[test]
fn dead_code_cannot_hide_retired_effect_attributes() {
    let source = "module invalid_effect\n#[effect(Pure)]\nfn ignored() -> u64 { return env::current_timepoint() }\naction inspect() { verification }";
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        let error = cellscript::compile(source, CompileOptions { diagnostics, ..Default::default() }).unwrap_err();
        assert!(error.message.contains("effect declarations have been removed"), "{error}");
    }
}

#[test]
fn epoch_comparisons_keep_fraction_semantics_when_branches_are_optimized() {
    let source = r#"module semantic_epoch_branch
fn equal_fraction() -> bool {
    return ckb::since_absolute_epoch(42, 1, 2) == ckb::since_absolute_epoch(42, 2, 4)
}
fn later_fraction() -> bool {
    return ckb::since_absolute_epoch(42, 1, 2) > ckb::since_absolute_epoch(42, 2, 5)
}
action inspect() -> u64 {
    verification
    require equal_fraction()
    require later_fraction()
    require ckb::since_absolute_epoch(42, 2, 5) < ckb::since_absolute_epoch(42, 1, 2)
    return 0
}"#;
    for diagnostics in [
        CompilerDiagnostics::default(),
        CompilerDiagnostics::without_optimizations(),
        CompilerDiagnostics::default().without(OptimizationPass::BranchFusion),
    ] {
        for wrong in [false, true] {
            let source = if wrong { source.replace("require later_fraction()", "require !later_fraction()") } else { source.into() };
            let compiled = cellscript::compile_with_executable_surface_policy(
                &source,
                CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                cellscript::ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap();
            let fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            let execution = ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code == 0, !wrong, "{diagnostics}: {execution:?}");
        }
    }
}
