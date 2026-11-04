//! Annotated and inferred bounded witness views retain the same owner, maximum,
//! typed wire spelling and machine behavior without IR string reparsing.
use cellscript::{CellScriptEdition, CompileOptions, CompilerDiagnostics, ExecutableSurfacePolicy};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

#[test]
fn structured_witness_owners_preserve_inferred_bytes_and_execute_explicit_annotations() {
    let payload = Bytes::from(42_u64.to_le_bytes().to_vec());
    let witness = packed::WitnessArgs::new_builder()
        .lock(Some(payload.clone()).pack())
        .input_type(Some(payload.clone()).pack())
        .output_type(Some(payload).pack())
        .build()
        .as_bytes();
    for owner in ["raw", "lock", "entry", "output_type"] {
        let (length, expected) =
            if owner == "raw" { (witness.len(), u64::from_le_bytes(witness[..8].try_into().unwrap())) } else { (8, 42) };
        for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
            for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
                let mut baseline = None;
                for annotation in [String::new(), format!(": WitnessBytesView<{owner}, 64>")] {
                    let source = format!(
                        "module structured_witness\naction verify() -> u64 {{ verification\n\
                         let args = witness::args(0)\nlet view{annotation} = witness::bounded_{owner}(args, 64)\n\
                         require view.size == {length}\nrequire witness::u64_le(view, 0) == {expected}\nreturn 0\n}}"
                    );
                    let result = cellscript::compile_with_executable_surface_policy(
                        &source,
                        CompileOptions { edition, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                        ExecutableSurfacePolicy::DenyFailClosed,
                    )
                    .unwrap_or_else(|error| panic!("{edition}/{diagnostics}/{owner}: {error:?}"));
                    result.validate().unwrap();
                    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                    fixture.witnesses = vec![witness.clone()];
                    let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
                    assert_eq!(execution.exit_code, 0, "{edition}/{diagnostics}/{owner}: {execution:?}");
                    if let Some(baseline) = &baseline {
                        assert_eq!(&result.artifact_bytes, baseline);
                    } else {
                        baseline = Some(result.artifact_bytes);
                    }
                }
            }
        }
    }
}

#[test]
fn structured_witness_arguments_reject_wrong_roles_extents_and_owner_conversion() {
    for ty in [
        "WitnessBytesView<u64, 64>",
        "WitnessBytesView<entry, u64>",
        "WitnessBytesView<entry, 65537>",
        "WitnessBytesView<entry, 340282366920938463463374607431768211455>",
        "WitnessBytesView<entry>",
        "WitnessBytesView<entry, 64, 1>",
        "WitnessBytesView<lock, 64>",
    ] {
        let source = format!(
            "module wrong_witness\naction verify() -> u64 {{ verification\n\
             let view: {ty} = witness::bounded_entry(witness::args(0), 64)\nreturn 0\n}}"
        );
        for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
            for target in [None, Some("riscv64-elf".into())] {
                assert!(cellscript::compile(&source, CompileOptions { edition, target, ..Default::default() }).is_err(), "{source}");
            }
        }
    }
}
