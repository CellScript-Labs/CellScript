use super::super::output::CommandOutcome;
use super::check_policy::validate_check_policy;
use super::package_test_expectations::{collect_cell_files, evaluate_compile_test_result, read_test_expectation};
use super::{test_resolution_options, CommandExecutor, DocArgs, TestArgs};
use crate::docgen::OutputFormat;
use crate::error::Result;
use crate::{compile_path, compile_path_with_executable_surface_policy, CompileOptions, ExecutableSurfacePolicy};
use camino::Utf8Path;
use colored::Colorize;
use std::path::Path;

impl CommandExecutor {
    pub(super) fn test(args: TestArgs) -> Result<()> {
        let options = test_resolution_options(&args);
        crate::package::with_resolution_options(options, || Self::test_inner(args))
    }

    fn test_inner(args: TestArgs) -> Result<()> {
        let doc_output = if args.doc {
            Some(Self::generate_docs(&DocArgs { output_format: OutputFormat::Markdown, ..Default::default() })?)
        } else {
            None
        };

        let mut test_inputs = collect_cell_files(Path::new("tests"))?;
        let mut scenario_inputs = crate::cli::test_runner::collect_scenario_files(Path::new("tests"))?;
        if let Some(filter) = &args.filter {
            test_inputs.retain(|path| path.to_string_lossy().contains(filter));
            scenario_inputs.retain(|path| path.to_string_lossy().contains(filter));
        }
        test_inputs.sort();
        scenario_inputs.sort();
        let backends = if args.no_run {
            Vec::new()
        } else {
            let backend = args.backend.as_deref().ok_or_else(|| {
                crate::error::CompileError::without_span("cellc test requires --backend simulator|ckb-vm|all unless --no-run is used")
            })?;
            if scenario_inputs.is_empty() {
                return Err(crate::error::CompileError::without_span(
                    "cellc test cannot pass without an executable *.scenario.json fixture; use --no-run for compile-only checks",
                ));
            }
            crate::cli::test_runner::TestBackend::parse(backend)?
        };

        if test_inputs.is_empty() {
            compile_path(
                ".",
                CompileOptions {
                    source_contracts: false,
                    edition: crate::CURRENT_EDITION,
                    diagnostics: crate::CompilerDiagnostics::default(),
                    output: None,
                    debug: false,
                    target: None,
                    target_profile: None,
                    primitive_compat: None,
                },
            )?;
            let mut human_lines = Vec::new();
            if let Some(output) = &doc_output {
                human_lines.push("Documentation generated".green().to_string());
                human_lines.push(format!("  Output: {}", output.display()));
            }
            human_lines.push("Test compile complete".green().to_string());
            human_lines.push("  Package check: passed".to_string());
            human_lines.push("  Test files: 0".to_string());
            if !args.no_run {
                human_lines.push("  Execution: skipped; no CellScript test files were found".to_string());
            }
            return CommandOutcome {
                machine: serde_json::json!({
                    "status": "ok",
                    "package_check": "passed",
                    "test_files": 0,
                    "passed": 0,
                    "failed": 0,
                    "fail_fast": args.fail_fast,
                    "no_run": args.no_run,
                    "execution": if args.no_run { "disabled" } else { "skipped-no-test-files" },
                    "docs_generated": args.doc,
                    "doc_output": doc_output.as_ref().map(|path| path.display().to_string()),
                    "scenario_files": scenario_inputs.len(),
                    "tests": [],
                    "scenarios": [],
                }),
                human_lines,
            }
            .emit(args.json);
        }

        let mut passed = 0usize;
        let mut failures = Vec::new();
        let mut test_reports = Vec::new();
        for input in &test_inputs {
            let utf8 = Utf8Path::from_path(input)
                .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input.display())))?;
            if args.nocapture && !args.json {
                println!("  Testing {}", utf8);
            }

            let expectation = read_test_expectation(input)?;
            let result = compile_path_with_executable_surface_policy(
                utf8,
                CompileOptions {
                    source_contracts: false,
                    edition: crate::CURRENT_EDITION,
                    diagnostics: crate::CompilerDiagnostics::default(),
                    output: None,
                    debug: false,
                    target: expectation.target.clone(),
                    target_profile: None,
                    primitive_compat: None,
                },
                None,
                if expectation.production || expectation.deny_fail_closed {
                    ExecutableSurfacePolicy::DenyFailClosed
                } else {
                    ExecutableSurfacePolicy::AllowFailClosed
                },
            )
            .and_then(|result| {
                let policy_args = expectation.check_args();
                validate_check_policy(&result.metadata, &policy_args)?;
                Ok(result)
            });
            match evaluate_compile_test_result(utf8, &expectation, result) {
                Ok(()) => {
                    passed += 1;
                    test_reports.push(serde_json::json!({
                        "path": utf8.to_string(),
                        "status": "passed",
                        "target": expectation.target,
                    }));
                }
                Err(error) => {
                    let message = error.into_message();
                    test_reports.push(serde_json::json!({
                        "path": utf8.to_string(),
                        "status": "failed",
                        "error": message,
                        "target": expectation.target,
                    }));
                    failures.push(message);
                    if args.fail_fast {
                        break;
                    }
                }
            }
        }

        if !failures.is_empty() {
            return Err(crate::error::CompileError::without_span(format!("test failed:\n  - {}", failures.join("\n  - ")))
                .with_details(serde_json::json!({
                    "mode": "test",
                    "test_files": test_inputs.len(),
                    "passed": passed,
                    "failed": failures.len(),
                    "fail_fast": args.fail_fast,
                    "no_run": args.no_run,
                    "tests": test_reports,
                })));
        }

        let mut scenario_reports = Vec::new();
        let mut scenario_failures = Vec::new();
        if !args.no_run {
            for scenario in &scenario_inputs {
                for backend in &backends {
                    match crate::cli::test_runner::run_scenario(scenario, *backend) {
                        Ok(report) => scenario_reports.push(report),
                        Err(error) => {
                            let message = format!("{}: {}", scenario.display(), error);
                            scenario_reports.push(serde_json::json!({
                                "schema": "cellscript-test-report-v1",
                                "status": "failed",
                                "scenario_path": scenario.to_string_lossy(),
                                "error": error.to_string(),
                            }));
                            scenario_failures.push(message);
                            if args.fail_fast {
                                break;
                            }
                        }
                    }
                }
                if args.fail_fast && !scenario_failures.is_empty() {
                    break;
                }
            }
        }
        if !scenario_failures.is_empty() {
            return Err(crate::error::CompileError::without_span(format!(
                "scenario test failed:\n  - {}",
                scenario_failures.join("\n  - ")
            ))
            .with_details(serde_json::json!({
                "mode": "test",
                "compile_test_files": test_inputs.len(),
                "scenario_files": scenario_inputs.len(),
                "backend": args.backend,
                "scenario_runs_passed": scenario_reports.len().saturating_sub(scenario_failures.len()),
                "scenario_runs_failed": scenario_failures.len(),
                "scenarios": scenario_reports,
            })));
        }

        let mut human_lines = Vec::new();
        if let Some(output) = &doc_output {
            human_lines.push("Documentation generated".green().to_string());
            human_lines.push(format!("  Output: {}", output.display()));
        }
        human_lines.push(if args.no_run { "Test compile complete" } else { "Test execution complete" }.green().to_string());
        human_lines.push(format!("  Compiled {} test file(s)", passed));
        if args.no_run {
            human_lines.push("  Execution: disabled by --no-run".to_string());
        } else {
            human_lines.push(format!(
                "  Executed {} scenario/backend run(s) with {}",
                scenario_reports.len(),
                args.backend.as_deref().unwrap_or("unknown")
            ));
        }
        CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "package_check": "not-run",
                "test_files": test_inputs.len(),
                "passed": passed,
                "failed": 0,
                "fail_fast": args.fail_fast,
                "no_run": args.no_run,
                "execution": if args.no_run { "disabled" } else { "executed" },
                "backend": args.backend,
                "scenario_files": scenario_inputs.len(),
                "scenario_runs": scenario_reports.len(),
                "docs_generated": args.doc,
                "doc_output": doc_output.as_ref().map(|path| path.display().to_string()),
                "tests": test_reports,
                "scenarios": scenario_reports,
            }),
            human_lines,
        }
        .emit(args.json)
    }
}
