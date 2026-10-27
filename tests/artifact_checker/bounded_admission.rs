use super::*;

#[test]
fn complete_bounded_programs_require_machine_evidence_after_strict_admission() {
    for (name, source, allocated) in [
        ("transfer", include_str!("../../examples/language/batches/multi_asset_transfer.cell"), true),
        ("settlement", include_str!("../../examples/language/batches/mixed_settlement.cell"), false),
        ("byte migration", include_str!("../../examples/language/batches/nested_migration.cell"), false),
        ("record migration", include_str!("../../examples/language/batches/record_migration.cell"), false),
    ] {
        for opt_level in [0, 3] {
            let compiled = cellscript::compile_with_executable_surface_policy(
                source,
                CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
                cellscript::ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap_or_else(|error| panic!("{name} O{opt_level}: {error}"));
            let valid = Fixture::from_result(compiled);
            assert_bounded_admitted(&valid);

            // Outer hashes are rebound: the missing semantic proof, rather than
            // a stale checksum or the retired blanket flag, must cause rejection.
            let mut missing = valid.clone();
            let mut removed = 0;
            for block in &mut missing.record.blocks {
                let labels = if allocated { &mut block.allocated_init_labels } else { &mut block.exact_role_labels };
                removed += labels.len();
                labels.clear();
            }
            assert!(removed > 0, "{name} O{opt_level} must exercise its machine evidence");
            missing.rebind_sidecars();
            assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn strict_nested_successors_still_require_complete_output_comparisons() {
    for opt_level in [0, 3] {
        let compiled = cellscript::compile_with_executable_surface_policy(
            NESTED_CELL_CODEC_SOURCE,
            CompileOptions { opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let valid = Fixture::from_result(compiled);
        assert_bounded_admitted(&valid);
        let mut missing = valid.clone();
        assert!(missing.record.blocks.iter().any(|block| !block.nested_output_labels.is_empty()));
        for block in &mut missing.record.blocks {
            block.nested_output_labels.clear();
        }
        missing.rebind_sidecars();
        assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}
