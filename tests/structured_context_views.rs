//! Context source identity must survive annotations, helper transport and field
//! projection without reconstructing an encoded nominal name.
use cellscript::{CellScriptEdition, CompileOptions, CompilerDiagnostics, ExecutableSurfacePolicy};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

const CELLS: &str = r#"
module context_cells
resource Token has store { amount: u64 }
fn inspect(input: InputView<Token>, output: OutputView<Token>) -> u64 {
    return input.data.amount + output.data.amount
}
fn shape(input: InputView<Token>, output: OutputView<Token>) -> bool {
    return input.capacity > 0 && output.output_index == 0 && input.lock.args_empty && output.type_script.hash == output.type_hash
}
action verify() -> u64 {
    verification
    let input = ckb::group_input<Token>(0)
    let output = ckb::group_output<Token>(0)
    require shape(input, output)
    require inspect(input, output) == 84
    return 0
}
"#;

const FORWARD: &str = r#"
module context_forward
resource Token has store { amount: u64 }
fn capacity(input: InputView<Token>) -> u64 { return ckb::cell_capacity(input) }
fn delegated(input: InputView<Token>) -> u64 { return capacity(input) }
fn select_capacity(group: bool) -> u64 {
    if group { return delegated(ckb::group_input<Token>(0)) }
    return delegated(ckb::input<Token>(0))
}
action verify() -> u64 {
    verification
    require select_capacity(true) > 0
    require select_capacity(false) > 0
    return 0
}
"#;

const SOURCES: &str = r#"
module context_sources
fn dep_size(dep: CellDepView) -> u64 { return dep.data_size }
fn witness_size(args: WitnessArgsView) -> u64 { return args.size }
action verify() -> u64 {
    verification
    require dep_size(ckb::cell_dep(0)) > 0
    require witness_size(witness::args(0)) > 0
    return 0
}
"#;

fn execute(source: &str, reject_changed_data: bool) {
    {
        let edition = CellScriptEdition::Edition2027;
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            let result = cellscript::compile_with_executable_surface_policy(
                source,
                CompileOptions {
                    edition,
                    diagnostics,
                    source_contracts: true,
                    target: Some("riscv64-elf".into()),
                    ..Default::default()
                },
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap_or_else(|error| panic!("{edition:?}/{diagnostics:?}: {error}\n{source}"));
            result.validate().unwrap();
            for param in result.metadata.functions.iter().flat_map(|function| &function.params).filter(|param| {
                matches!(param.ty.as_str(), "InputView<Token>" | "OutputView<Token>" | "CellDepView" | "WitnessArgsView")
            }) {
                assert!(param.schema_pointer_abi && param.schema_length_abi, "{param:?}");
                assert!(!param.cell_bound_abi, "{param:?}");
            }
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.current_type_script_input_indices = vec![0];
            fixture.inputs[0].data = 42u64.to_le_bytes().to_vec().into();
            fixture.outputs[0].data = fixture.inputs[0].data.clone();
            fixture.witnesses = vec![packed::WitnessArgs::new_builder().build().as_bytes()];
            let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code, 0, "{edition:?}/{diagnostics:?}: {execution:?}\n{source}");
            if reject_changed_data {
                fixture.outputs[0].data = 43u64.to_le_bytes().to_vec().into();
                let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
                assert_eq!(execution.exit_code, 5, "{edition:?}/{diagnostics:?}: {execution:?}\n{source}");
            }
        }
    }
}

#[test]
fn context_cell_parameters_and_data_fields_execute_in_both_profiles() {
    execute(CELLS, true);
}

#[test]
fn context_source_substitution_executes_in_both_profiles() {
    execute(FORWARD, false);
}

#[test]
fn context_unparameterized_sources_keep_their_roles_through_calls() {
    execute(SOURCES, false);
}

#[test]
fn context_representation_matches_current_bytes_and_declared_identity() {
    use sha2::{Digest, Sha256};
    let cases: serde_json::Value = serde_json::from_str(include_str!("fixtures/context_view_identity_current.json")).unwrap();
    for case in cases["cases"].as_array().unwrap() {
        let result = cellscript::compile(
            case["source"].as_str().unwrap(),
            CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        result.validate().unwrap();
        assert_eq!(hex::encode(Sha256::digest(&result.artifact_bytes)), case["elf_sha256"].as_str().unwrap());
        assert_eq!(result.metadata.interface_hash, case["interface_hash"].as_str().unwrap());
        assert_eq!(result.metadata.typed_semantics_hash, case["typed_semantics_hash"].as_str().unwrap());
    }
}

#[test]
fn context_type_arguments_require_a_real_cell_schema_and_exact_arity() {
    for declaration in [
        "fn wrong(value: InputView<u64>) {}",
        "fn wrong(value: OutputView<Plain>) {}",
        "fn wrong(value: CellDataView<Missing>) {}",
        "fn wrong(value: InputView<Token, Token>) {}",
        "fn wrong(value: InputView<16>) {}",
        "fn wrong(value: CellDepView<Token>) {}",
        "fn wrong(value: InputView<Token>) -> InputView<Token> { return value }",
        "fn wrong(value: CellDataView<Token>) -> CellDataView<Token> { return value }",
    ] {
        let source = format!("module wrong_view\nstruct Plain {{ amount: u64 }}\nresource Token has store {{ amount: u64 }}\n{declaration}\naction verify() -> u64 {{ verification return 0 }}");
        {
            let edition = CellScriptEdition::Edition2027;
            for target in [None, Some("riscv64-elf".into())] {
                assert!(cellscript::compile(&source, CompileOptions { edition, target, ..Default::default() }).is_err(), "{source}");
            }
        }
    }
    for shadow in ["InputView", "CellDepView", "WitnessArgsView", "ScriptView"] {
        let source = format!("module shadowed_view\nstruct {shadow} {{ amount: u64 }}\nresource Token has store {{ amount: u64 }}\naction verify(value: {shadow}{}) -> u64 {{ verification return 0 }}", if shadow == "InputView" { "<Token>" } else { "" });
        assert!(cellscript::compile(&source, CompileOptions::default()).is_err(), "{source}");
    }
    for expression in ["ckb::lock_script(ckb::input<Token>(0))", "ckb::input<Token>(0).lock"] {
        let source = format!("module shadowed_script\nstruct ScriptView {{ amount: u64 }}\nresource Token has store {{ amount: u64 }}\naction verify() -> u64 {{ verification let script = {expression} return 0 }}");
        {
            let edition = CellScriptEdition::Edition2027;
            for target in [None, Some("riscv64-elf".into())] {
                let error = cellscript::compile(&source, CompileOptions { edition, target, ..Default::default() }).unwrap_err();
                assert_eq!(error.code, Some(cellscript::error::DiagnosticCode::E2111), "{error}\n{source}");
            }
        }
    }
}
