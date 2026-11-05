//! Focused backend tests for dev; CI/backend already run the complete compiler suite.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::Instant;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use time::OffsetDateTime;

use crate::evidence_retention::{prune_report_files, write_latest_index};
use crate::shared::{lexical_path, stable_json_pretty};

#[derive(Clone, Debug)]
struct CommandSpec {
    id: &'static str,
    argv: &'static [&'static str],
}

fn command_plan() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "elf-entry-contract",
            argv: &[
                "cargo",
                "test",
                "--locked",
                "-p",
                "cellscript",
                "--lib",
                "strict_audit_internal_elf_entry_preserves_ckb_stack_pointer",
                "--",
                "--nocapture",
            ],
        },
        CommandSpec {
            id: "assembler-emitted-surface",
            argv: &[
                "cargo",
                "test",
                "--locked",
                "-p",
                "cellscript",
                "--lib",
                "internal_assembler_encodes_emitted_instruction_surface",
                "--",
                "--nocapture",
            ],
        },
        CommandSpec {
            id: "branch-relaxation-contracts",
            argv: &["cargo", "test", "--locked", "-p", "cellscript", "--lib", "internal_assembler_relaxes_", "--", "--nocapture"],
        },
        CommandSpec {
            id: "backend-shape-contracts",
            argv: &[
                "cargo",
                "test",
                "--locked",
                "-p",
                "cellscript",
                "--test",
                "examples",
                "bundled_examples_stay_within_backend_shape_budgets",
                "--",
                "--exact",
                "--nocapture",
            ],
        },
        CommandSpec {
            id: "wildcard-match-order-contract",
            argv: &[
                "cargo",
                "test",
                "--locked",
                "-p",
                "cellscript",
                "--lib",
                "compile_rejects_invalid_enum_match_patterns",
                "--",
                "--nocapture",
            ],
        },
        CommandSpec {
            id: "lowering-preservation-contracts",
            argv: &["cargo", "test", "--locked", "-p", "cellscript", "--lib", "compile_preserves_", "--", "--nocapture"],
        },
        CommandSpec {
            id: "bytestring-boundaries",
            argv: &[
                "cargo",
                "test",
                "--locked",
                "-p",
                "cellscript",
                "--lib",
                "--test",
                "frontend_boundaries",
                "--test",
                "lsp_responsiveness",
                "byte_string",
                "--",
                "--nocapture",
            ],
        },
    ]
}

// A successful Cargo exit also occurs when a filter matches no tests. Every
// selected test target must report at least one executed, passing test.
fn executed_test_count(stdout: &str) -> Option<usize> {
    let mut counts = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("test result: ok. ")?.split_once(" passed;")?.0.parse::<usize>().ok())
        .peekable();
    counts.peek()?;
    counts.try_fold(0usize, |total, count| if count == 0 { None } else { total.checked_add(count) })
}

fn exit_code(status: ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        -status.signal().unwrap_or(1)
    }
    #[cfg(not(unix))]
    {
        1
    }
}

fn tail_chars(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    let count = trimmed.chars().count();
    trimmed.chars().skip(count.saturating_sub(limit)).collect()
}

fn run_command(root: &Path, spec: &CommandSpec) -> Result<Value> {
    let started = Instant::now();
    let output = Command::new(spec.argv[0])
        .args(&spec.argv[1..])
        .current_dir(root)
        .output()
        .with_context(|| format!("failed to run {}", spec.argv.join(" ")))?;
    let duration = (started.elapsed().as_secs_f64() * 1000.0).round() / 1000.0;
    let stdout = String::from_utf8(output.stdout).context("strict audit command stdout is not UTF-8")?;
    let stderr = String::from_utf8(output.stderr).context("strict audit command stderr is not UTF-8")?;
    let combined = format!("{stdout}\n{stderr}");
    let code = exit_code(output.status);
    let matched_tests = executed_test_count(&stdout);
    let passed = code == 0 && matched_tests.is_some();
    Ok(json!({
        "id": spec.id,
        "matched_tests": matched_tests,
        "empty_test_target": code == 0 && matched_tests.is_none(),
        "argv": spec.argv,
        "status": if passed { "passed" } else { "failed" },
        "exit_code": code,
        "duration_seconds": duration,
        "output_tail": tail_chars(&combined, 12_000),
    }))
}

fn default_report_path(root: &Path, mode: &str) -> Result<PathBuf> {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let format = time::format_description::parse("[year][month][day]-[hour][minute][second]")?;
    let stamp = now.format(&format)?;
    Ok(root.join("target/cellscript-strict-backend-audit").join(format!("strict-backend-audit-{mode}-{stamp}.json")))
}

pub fn run(root: &Path, mode: &str) -> Result<i32> {
    if mode != "quick" {
        eprintln!("usage: cellscript-tools strict-backend [quick]");
        return Ok(2);
    }

    let managed_report = env::var_os("CELLSCRIPT_STRICT_BACKEND_AUDIT_REPORT").is_none();
    let report_path = match env::var_os("CELLSCRIPT_STRICT_BACKEND_AUDIT_REPORT") {
        // Collapse repeated separators and `.` without resolving symlinks or
        // `..` components.
        Some(path) => lexical_path(&PathBuf::from(path)),
        None => default_report_path(root, mode)?,
    };
    let report_parent = report_path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(report_parent).with_context(|| format!("failed to create report directory {}", report_parent.display()))?;

    let commands = command_plan();
    let mut results = Vec::with_capacity(commands.len());
    for spec in &commands {
        println!("==> {}: {}", spec.id, spec.argv.join(" "));
        io::stdout().flush().context("failed to flush strict audit progress")?;
        let result = run_command(root, spec)?;
        results.push(result);
    }

    let failed: Vec<&str> = results
        .iter()
        .filter(|result| result.get("status").and_then(Value::as_str) != Some("passed"))
        .filter_map(|result| result.get("id").and_then(Value::as_str))
        .collect();
    let passed = failed.is_empty();
    let report = json!({
        "audit": "cellscript-backend-quick-tests",
        "mode": mode,
        "status": if passed { "passed" } else { "failed" },
        "failed_commands": failed,
        "commands": results,
    });
    fs::write(&report_path, format!("{}\n", stable_json_pretty(&report)?))
        .with_context(|| format!("failed to write {}", report_path.display()))?;
    if managed_report {
        let latest = report_parent.join(format!("latest-{mode}.json"));
        write_latest_index(root, &latest, &report_path, "strict-backend-audit", mode, if passed { "passed" } else { "failed" })?;
        let marker = format!("strict-backend-audit-{mode}-");
        let removed = prune_report_files(root, report_parent, &report_path, &marker)?;
        if !removed.is_empty() {
            println!("strict backend audit retention: pruned {} old {mode} report(s)", removed.len());
        }
    }
    println!("strict backend audit report: {}", report_path.display());
    Ok(if passed { 0 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::executed_test_count;

    #[test]
    fn successful_exit_without_executed_tests_cannot_pass() {
        assert_eq!(executed_test_count("Finished test profile"), None);
        assert_eq!(executed_test_count("running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored;"), None);
        assert_eq!(executed_test_count("test result: ok. 0 passed; 0 failed; 1 ignored;"), None);
    }

    #[test]
    fn an_empty_selected_target_is_not_hidden_by_another_target() {
        let stdout = "test result: ok. 3 passed; 0 failed;\ntest result: ok. 0 passed; 0 failed;";
        assert_eq!(executed_test_count(stdout), None);
    }

    #[test]
    fn every_selected_target_must_execute_successful_tests() {
        let stdout = "running 3 tests\ntest result: ok. 3 passed; 0 failed;\nrunning 2 tests\ntest result: ok. 2 passed; 0 failed;";
        assert_eq!(executed_test_count(stdout), Some(5));
    }

    #[cfg(unix)]
    #[test]
    fn successful_process_with_no_tests_is_reported_as_failed() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let spec = super::CommandSpec {
            id: "empty-test-target",
            argv: &["sh", "-c", "printf '%s\\n' 'test result: ok. 0 passed; 0 failed; 0 ignored;'"],
        };
        let result = super::run_command(root, &spec).unwrap();
        assert_eq!(result["exit_code"], 0);
        assert_eq!(result["status"], "failed");
        assert_eq!(result["empty_test_target"], true);
    }
}
