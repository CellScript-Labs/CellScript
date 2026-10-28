//! Real CKB-VM regressions for fatal verification versus ordinary return values.
//! These bounded cases do not establish general source/machine equivalence.

use cellscript::{
    compile_path_with_executable_surface_policy, strip_vm_abi_trailer, CellScriptEdition, CompileEntryScope, CompileOptions,
    CompileResult, EntryWitnessArg, ExecutableSurfacePolicy,
};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

use ckb_script_runner::{build_simple_fixture, execute_cellscript_script, FixtureCell};

const EDITIONS: [CellScriptEdition; 2] = [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027];

fn compile_source(source: &str, edition: CellScriptEdition, diagnostics: cellscript::CompilerDiagnostics) -> CompileResult {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.cell");
    std::fs::write(&path, source).unwrap();
    compile_path_with_executable_surface_policy(
        path.to_str().unwrap(),
        CompileOptions { edition, diagnostics, target: Some("riscv64-elf".to_string()), ..Default::default() },
        Some(CompileEntryScope::Action("verify".to_string())),
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap_or_else(|error| panic!("edition={edition:?}, opt={diagnostics}: {error}\n{source}"))
}

fn exit_code(compiled: &CompileResult, args: &[EntryWitnessArg]) -> i64 {
    exit_code_with_deps(compiled, args, &[])
}

fn exit_code_with_deps(compiled: &CompileResult, args: &[EntryWitnessArg], dependencies: &[Vec<u8>]) -> i64 {
    let payload = compiled.metadata.actions.iter().find(|entry| entry.name == "verify").unwrap().entry_witness_args(args).unwrap();
    let mut fixture = build_simple_fixture(Bytes::new(), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.witnesses = vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
    fixture.cell_deps = dependencies
        .iter()
        .map(|data| FixtureCell { capacity: 100_000_000_000, type_script: None, data: Bytes::copy_from_slice(data) })
        .collect();
    execute_cellscript_script(strip_vm_abi_trailer(&compiled.artifact_bytes), &fixture).exit_code
}

#[test]
fn called_unit_action_require_failure_is_fatal_even_when_discarded() {
    let source = r#"
module verifier_failure
action verify(witness value: u64) -> u64 {
    verification
    outer(value)
    return 0
}
action outer(value: u64) {
    verification
    inner(value)
}
action inner(value: u64) {
    verification
    require value == 7
}
"#;
    let mut failures = Vec::new();
    for edition in EDITIONS {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let compiled = compile_source(source, edition, diagnostics);
            assert_eq!(exit_code(&compiled, &[EntryWitnessArg::U64(7)]), 0, "normal Unit return: {edition:?}, opt={diagnostics}");
            let actual = exit_code(&compiled, &[EntryWitnessArg::U64(8)]);
            if actual != 5 {
                failures.push(format!("{edition:?}, opt={diagnostics}: expected assertion error 5, got {actual}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn discarded_division_helper_failure_is_fatal() {
    let source = r#"
module verifier_failure
fn divide(value: u64) -> u64 { 10 / value }
action verify(witness value: u64) -> u64 {
    verification
    let ignored = divide(value)
    return 0
}
"#;
    let mut failures = Vec::new();
    for edition in EDITIONS {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let compiled = compile_source(source, edition, diagnostics);
            assert_eq!(exit_code(&compiled, &[EntryWitnessArg::U64(2)]), 0, "normal scalar return: {edition:?}, opt={diagnostics}");
            let actual = exit_code(&compiled, &[EntryWitnessArg::U64(0)]);
            if actual != 20 {
                failures.push(format!("{edition:?}, opt={diagnostics}: expected numeric error 20, got {actual}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[derive(Clone, Copy, Debug)]
enum ResultUse {
    Statement,
    Unused,
    Wildcard,
    IgnoredArgument,
}

impl ResultUse {
    fn source(self, call: &str) -> String {
        match self {
            Self::Statement => call.to_string(),
            Self::Unused => format!("let unused = {call}"),
            Self::Wildcard => format!("let _ = {call}"),
            Self::IgnoredArgument => format!("let _ = ignore({call})"),
        }
    }
}

const RESULT_USES: [ResultUse; 4] = [ResultUse::Statement, ResultUse::Unused, ResultUse::Wildcard, ResultUse::IgnoredArgument];

#[test]
fn arithmetic_failures_survive_transitive_calls_and_every_discard_shape() {
    let cases = [
        ("division", "u64", "u64", "100 / value", EntryWitnessArg::U64(5), EntryWitnessArg::U64(0), 20),
        ("modulo", "u64", "u64", "100 % value", EntryWitnessArg::U64(5), EntryWitnessArg::U64(0), 20),
        ("narrow_cast", "u64", "u8", "value as u8", EntryWitnessArg::U64(20), EntryWitnessArg::U64(256), 20),
        ("shift", "u64", "u64", "1 << value", EntryWitnessArg::U64(3), EntryWitnessArg::U64(64), 65),
        ("overflow", "u128", "u128", "value + 1", EntryWitnessArg::U128(48), EntryWitnessArg::U128(u128::MAX), 49),
        ("wide_division", "u128", "u128", "100 / value", EntryWitnessArg::U128(5), EntryWitnessArg::U128(0), 20),
    ];
    let mut failures = Vec::new();
    for (name, parameter_type, return_type, expression, good, bad, expected) in cases {
        for result_use in RESULT_USES {
            let use_call = result_use.source("outer(value)");
            let source = format!(
                "module verifier_failure\n\
                 fn calculate(value: {parameter_type}) -> {return_type} {{ {expression} }}\n\
                 fn outer(value: {parameter_type}) -> {return_type} {{ calculate(value) }}\n\
                 fn ignore(value: {return_type}) -> u64 {{ 0 }}\n\
                 action verify(witness value: {parameter_type}) -> u64 {{\n\
                 verification\n{use_call}\nreturn 0\n}}"
            );
            for edition in EDITIONS {
                for diagnostics in
                    [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()]
                {
                    let compiled = compile_source(&source, edition, diagnostics);
                    let valid = exit_code(&compiled, std::slice::from_ref(&good));
                    let invalid = exit_code(&compiled, std::slice::from_ref(&bad));
                    if valid != 0 || invalid != expected {
                        failures.push(format!(
                            "{name}/{result_use:?}, {edition:?}, opt={diagnostics}: valid={valid}, invalid={invalid}, expected={expected}"
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn unit_failure_survives_scalar_wrappers_and_discarded_results() {
    let mut failures = Vec::new();
    for result_use in RESULT_USES {
        let use_call = result_use.source("outer(value)");
        let source = format!(
            "module verifier_failure\n\
             action checked(value: u64) {{ verification require value == 7 }}\n\
             action bridge(value: u64) -> u64 {{ verification checked(value)\nreturn 49 }}\n\
             fn outer(value: u64) -> u64 {{ bridge(value) }}\n\
             fn ignore(value: u64) -> u64 {{ 0 }}\n\
             action verify(witness value: u64) -> u64 {{ verification\n{use_call}\nreturn 0 }}"
        );
        for edition in EDITIONS {
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let compiled = compile_source(&source, edition, diagnostics);
                let valid = exit_code(&compiled, &[EntryWitnessArg::U64(7)]);
                let invalid = exit_code(&compiled, &[EntryWitnessArg::U64(8)]);
                if valid != 0 || invalid != 5 {
                    failures.push(format!("{result_use:?}, {edition:?}, opt={diagnostics}: valid={valid}, invalid={invalid}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn failed_helpers_cannot_supply_error_shaped_values_to_successful_predicates() {
    let cases = [
        ("fn result(value: u64) -> u64 { 100 / value }", "result(value) == 20", 5, 0, 20),
        ("action result(value: u64) -> bool { verification require value == 7\nreturn true }", "result(value)", 7, 8, 5),
    ];
    for (helper, predicate, good, bad, expected) in cases {
        let source = format!(
            "module verifier_failure\n{helper}\n\
             action verify(witness value: u64) -> u64 {{ verification\nrequire {predicate}\nreturn 0 }}"
        );
        for edition in EDITIONS {
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let compiled = compile_source(&source, edition, diagnostics);
                assert_eq!(exit_code(&compiled, &[EntryWitnessArg::U64(good)]), 0, "valid predicate: {edition:?}, opt={diagnostics}");
                assert_eq!(
                    exit_code(&compiled, &[EntryWitnessArg::U64(bad)]),
                    expected,
                    "fatal error must not become a matching scalar or true: {edition:?}, opt={diagnostics}, predicate={predicate}"
                );
            }
        }
    }
}

#[test]
fn normal_error_shaped_scalars_false_unit_and_wide_values_remain_values() {
    let source = r#"
module verifier_failure
fn scalar(value: u64) -> u64 { value }
fn no() -> bool { false }
fn unit() -> () { () }
fn wide(value: u128) -> u128 { value }
action number() -> u64 { verification return 49 }
action verify(witness value: u64, witness large: u128) -> u64 {
    verification
    require scalar(value) == value
    require scalar(5) == 5
    require scalar(20) == 20
    require scalar(49) == 49
    require number() == 49
    require !no()
    unit()
    require wide(large) == large
    return 0
}
"#;
    for edition in EDITIONS {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let compiled = compile_source(source, edition, diagnostics);
            for value in [5, 20, 49] {
                assert_eq!(
                    exit_code(&compiled, &[EntryWitnessArg::U64(value), EntryWitnessArg::U128((1u128 << 96) + value as u128)]),
                    0,
                    "ordinary value must not be treated as an error: {edition:?}, opt={diagnostics}, value={value}"
                );
            }
        }
    }
}

#[test]
fn normal_enum_tuple_returns_and_reference_arguments_preserve_their_abi() {
    let cases = [
        (
            "enum",
            "enum Choice { Empty, Number(u64) }\nfn choose(value: u64) -> Choice { Choice::Number(value) }",
            "let choice = choose(value)\n\
             let actual = match choice { Choice::Number(amount) => { amount }, Choice::Empty => { 0 } }\n\
             require actual == value",
        ),
        (
            "tuple",
            "fn pair(value: u64) -> (u64, u64) { (value, 49) }",
            "let (first, second) = pair(value)\nrequire first == value\nrequire second == 49",
        ),
        (
            "repeated_tuple_projection",
            "fn pair(value: u64) -> (u64, u64) { (value, 49) }",
            "let result = pair(value)\n\
             require result.0 == value\nrequire result.1 == 49\n\
             require result.0 == value\nrequire result.1 == 49\n\
             let (first, second) = result\nrequire first == value\nrequire second == 49",
        ),
        (
            "reference_argument",
            "struct Point { value: u64 }\nfn point(value: &Point) -> u64 { value.value }",
            "let original: Point = Point { value: value }\nrequire point(&original) == value",
        ),
        (
            "reference_aliases",
            "struct Point { value: u64 }\nfn point(value: &Point) -> u64 { value.value }",
            "let original: Point = Point { value: value }\n\
             let first = &original\nlet second = first\nlet third = second\n\
             require point(third) == value\nrequire point(second) == value\nrequire point(first) == value",
        ),
    ];
    let mut failures = Vec::new();
    for (name, definitions, checks) in cases {
        let source = format!(
            "module verifier_failure\n{definitions}\n\
             action verify(witness value: u64) -> u64 {{ verification\n{checks}\nreturn 0 }}"
        );
        for edition in EDITIONS {
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let compiled = compile_source(&source, edition, diagnostics);
                for value in [5, 20, 49] {
                    let actual = exit_code(&compiled, &[EntryWitnessArg::U64(value)]);
                    if actual != 0 {
                        failures.push(format!(
                            "{name}: {edition:?}, opt={diagnostics}, value={value}: expected normal return, exit={actual}"
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn external_schema_reference_arguments_keep_real_lengths_through_aliases() {
    let source = r#"
module verifier_failure
shared Config { value: u64 }
fn value(config: &Config) -> u64 { config.value }
fn forward(config: &Config) -> u64 { let alias = config
value(alias) }
action verify(read config: Config, witness expected: u64) -> u64 {
    verification
    let alias = config
    require forward(alias) == expected
    return 0
}
"#;
    for edition in EDITIONS {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let compiled = compile_source(source, edition, diagnostics);
            for expected in [5u64, 20, 49] {
                assert_eq!(
                    exit_code_with_deps(&compiled, &[EntryWitnessArg::U64(expected)], &[expected.to_le_bytes().to_vec()]),
                    0,
                    "valid external schema reference: {edition:?}, opt={diagnostics}"
                );
            }
            assert_eq!(
                exit_code_with_deps(&compiled, &[EntryWitnessArg::U64(49)], &[vec![49, 0, 0]]),
                4,
                "external short data cannot receive a fabricated fixed width: {edition:?}, opt={diagnostics}"
            );
            assert_ne!(
                exit_code_with_deps(&compiled, &[EntryWitnessArg::U64(49)], &[vec![49, 0, 0, 0, 0, 0, 0, 0, 0]]),
                0,
                "external trailing data remains invalid: {edition:?}, opt={diagnostics}"
            );
        }
    }
}

#[test]
fn discarded_schema_reads_cannot_hide_malformed_cell_data() {
    let mut failures = Vec::new();
    for result_use in RESULT_USES {
        let use_call = result_use.source("outer()");
        let source = format!(
            "module verifier_failure\n\
             shared Config {{ value: u64 }}\n\
             fn load() -> u64 {{ let config = read_ref<Config>()\nconfig.value }}\n\
             fn outer() -> u64 {{ load() }}\n\
             fn ignore(value: u64) -> u64 {{ 0 }}\n\
             action verify() -> u64 {{ verification\n{use_call}\nreturn 0 }}"
        );
        for edition in EDITIONS {
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let compiled = compile_source(&source, edition, diagnostics);
                let valid = exit_code_with_deps(&compiled, &[], &[49u64.to_le_bytes().to_vec()]);
                let invalid = exit_code_with_deps(&compiled, &[], &[vec![49, 0, 0]]);
                if valid != 0 || invalid != 4 {
                    failures.push(format!(
                        "{result_use:?}, {edition:?}, opt={diagnostics}: valid={valid}, malformed={invalid}, expected=4"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn imported_alias_failures_survive_local_wrappers_and_discarded_results() {
    let helper = r#"
module verifier_failure::helper
fn divide(value: u64) -> u64 { 100 / value }
action checked(value: u64) { verification require value == 7 }
fn checked_value(value: u64) -> u64 { checked(value)
49 }
"#;
    let mut failures = Vec::new();
    for edition in EDITIONS {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("Cell.toml"),
            format!(
                "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nname = \"verifier_failure\"\nversion = \"0.1.0\"\nedition = \"{}\"\n",
                if edition == CellScriptEdition::Edition2026 { "2026" } else { "2027" }
            ),
        )
        .unwrap();
        let source_dir = directory.path().join("src");
        std::fs::create_dir(&source_dir).unwrap();
        std::fs::write(source_dir.join("helper.cell"), helper).unwrap();
        let entry = source_dir.join("main.cell");
        for (name, good, bad, expected) in [("divide", 5, 0, 20), ("checked_value", 7, 8, 5)] {
            for result_use in RESULT_USES {
                let use_call = result_use.source("outer(value)");
                let source = format!(
                    "module verifier_failure::main\n\
                     use verifier_failure::helper::{name} as imported\n\
                     fn outer(value: u64) -> u64 {{ imported(value) }}\n\
                     fn ignore(value: u64) -> u64 {{ 0 }}\n\
                     action verify(witness value: u64) -> u64 {{ verification\n{use_call}\nreturn 0 }}"
                );
                std::fs::write(&entry, &source).unwrap();
                for diagnostics in
                    [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()]
                {
                    let compiled = compile_path_with_executable_surface_policy(
                        entry.to_str().unwrap(),
                        CompileOptions { edition, diagnostics, target: Some("riscv64-elf".to_string()), ..Default::default() },
                        Some(CompileEntryScope::Action("verify".to_string())),
                        ExecutableSurfacePolicy::DenyFailClosed,
                    )
                    .unwrap_or_else(|error| panic!("{edition:?}, opt={diagnostics}, {name}/{result_use:?}: {error}"));
                    let valid = exit_code(&compiled, &[EntryWitnessArg::U64(good)]);
                    let invalid = exit_code(&compiled, &[EntryWitnessArg::U64(bad)]);
                    if valid != 0 || invalid != expected {
                        failures.push(format!(
                            "{name}/{result_use:?}, {edition:?}, opt={diagnostics}: valid={valid}, invalid={invalid}, expected={expected}"
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn declared_close_status_return_is_not_a_verifier_abort() {
    // `close` deliberately returns its normalized status (1 for an invalid fd).
    // This is distinct from compiler-generated errors in a checked operation.
    let source = r#"
module verifier_failure
fn status() -> u64 { close(99999) }
action verify() -> u64 {
    verification
    require status() == 1
    return 0
}
"#;
    for edition in EDITIONS {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let compiled = compile_source(source, edition, diagnostics);
            assert_eq!(exit_code(&compiled, &[]), 0, "declared status return: {edition:?}, opt={diagnostics}");
        }
    }
}
