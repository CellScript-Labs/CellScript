//! Strict backend audit implementation used by the repository gate.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::Instant;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use time::OffsetDateTime;

use crate::shared::{lexical_path, stable_json_pretty};

const FEATURE_IDS: &[&str] = &[
    "ir.cfg.block-id-uniqueness",
    "ir.cfg.terminator-targets",
    "ir.cfg.reachability",
    "ir.defs.must-define-before-use",
    "ir.abi.call-arg-types",
    "ir.abi.return-types",
    "codegen.psabi.sp-delta-alignment",
    "codegen.psabi.outgoing-stack-args-0-through-20",
    "codegen.tuple-return-register-contract",
    "codegen.runtime-fail-closed-syscall-contracts",
    "riscv.oracle.core-instruction-bytes",
    "riscv.oracle.immediate-boundaries",
    "riscv.branch-relaxation.near-and-far",
    "riscv.machine-cfg.layout-coverage",
    "riscv.elf.header-and-segment-layout",
    "edge.match-wildcard-order",
    "edge.tuple-projection-through-branching",
    "edge.bytestring-length",
    "edge.import-alias-callable-rename",
    "metamorphic.numeric-type-equality-commutative",
    "acceptance.syntax-combo",
    "acceptance.ckb-stateful-scenarios",
];

#[derive(Clone, Debug)]
struct CommandSpec {
    id: &'static str,
    feature_ids: &'static [&'static str],
    argv: &'static [&'static str],
}

fn command_plan(mode: &str) -> Vec<CommandSpec> {
    let mut commands = vec![
        CommandSpec {
            id: "strict-rust-contract-tests",
            feature_ids: &[
                "ir.cfg.block-id-uniqueness",
                "ir.cfg.terminator-targets",
                "ir.cfg.reachability",
                "ir.defs.must-define-before-use",
                "ir.abi.call-arg-types",
                "ir.abi.return-types",
                "codegen.psabi.sp-delta-alignment",
                "riscv.oracle.core-instruction-bytes",
                "riscv.oracle.immediate-boundaries",
                "riscv.elf.header-and-segment-layout",
            ],
            argv: &["cargo", "test", "--locked", "-p", "cellscript", "strict_audit", "--", "--nocapture"],
        },
        CommandSpec {
            id: "outgoing-stack-abi-matrix",
            feature_ids: &["codegen.psabi.outgoing-stack-args-0-through-20"],
            argv: &[
                "cargo",
                "test",
                "--locked",
                "-p",
                "cellscript",
                "outgoing_stack_arg_area_is_16_byte_aligned_at_call_boundaries",
                "--",
                "--nocapture",
            ],
        },
        CommandSpec {
            id: "assembler-emitted-surface",
            feature_ids: &["riscv.machine-cfg.layout-coverage"],
            argv: &[
                "cargo",
                "test",
                "--locked",
                "-p",
                "cellscript",
                "internal_assembler_encodes_emitted_instruction_surface",
                "--",
                "--nocapture",
            ],
        },
        CommandSpec {
            id: "branch-relaxation-contracts",
            feature_ids: &["riscv.branch-relaxation.near-and-far"],
            argv: &["cargo", "test", "--locked", "-p", "cellscript", "relaxes", "--", "--nocapture"],
        },
        CommandSpec {
            id: "tuple-return-abi-contracts",
            feature_ids: &["codegen.tuple-return-register-contract"],
            argv: &[
                "cargo",
                "test",
                "--locked",
                "-p",
                "cellscript",
                "tuple_return_abi_rejects_more_than_eight_fields",
                "--",
                "--nocapture",
            ],
        },
        CommandSpec {
            id: "runtime-fail-closed-contracts",
            feature_ids: &["codegen.runtime-fail-closed-syscall-contracts"],
            argv: &[
                "cargo",
                "test",
                "--locked",
                "-p",
                "cellscript",
                "ckb_u64_syscall_helpers_check_return_code_and_size",
                "--",
                "--nocapture",
            ],
        },
        CommandSpec {
            id: "backend-shape-contracts",
            feature_ids: &["riscv.machine-cfg.layout-coverage"],
            argv: &[
                "cargo",
                "test",
                "--locked",
                "-p",
                "cellscript",
                "bundled_examples_stay_within_backend_shape_budgets",
                "--",
                "--nocapture",
            ],
        },
        CommandSpec {
            id: "wildcard-match-order-contract",
            feature_ids: &["edge.match-wildcard-order"],
            argv: &[
                "cargo",
                "test",
                "--locked",
                "-p",
                "cellscript",
                "compile_rejects_invalid_enum_match_patterns",
                "--",
                "--nocapture",
            ],
        },
        CommandSpec {
            id: "tuple-projection-branching-contracts",
            feature_ids: &["edge.tuple-projection-through-branching"],
            argv: &["cargo", "test", "--locked", "-p", "cellscript", "compile_preserves_", "--", "--nocapture"],
        },
        CommandSpec {
            id: "bytestring-length-contracts",
            feature_ids: &["edge.bytestring-length"],
            argv: &["cargo", "test", "--locked", "-p", "cellscript", "byte_string", "--", "--nocapture"],
        },
        CommandSpec {
            id: "import-alias-callable-rename-contract",
            feature_ids: &["edge.import-alias-callable-rename"],
            argv: &[
                "cargo",
                "test",
                "--locked",
                "-p",
                "cellscript",
                "compile_package_import_alias_emits_matching_external_callable",
                "--",
                "--nocapture",
            ],
        },
        CommandSpec {
            id: "numeric-type-equality-metamorphic-contract",
            feature_ids: &["metamorphic.numeric-type-equality-commutative"],
            argv: &[
                "cargo",
                "test",
                "--locked",
                "-p",
                "cellscript",
                "numeric_named_type_equality_is_commutative",
                "--",
                "--nocapture",
            ],
        },
    ];
    if matches!(mode, "ci" | "full" | "nightly") {
        commands.push(CommandSpec {
            id: "syntax-combo-audit",
            feature_ids: &["acceptance.syntax-combo"],
            argv: &["scripts/cellscript_syntax_combo_audit.sh", "ci"],
        });
    }
    if matches!(mode, "full" | "nightly") {
        commands.push(CommandSpec {
            id: "ckb-stateful-scenarios",
            feature_ids: &["acceptance.ckb-stateful-scenarios"],
            argv: &["scripts/cellscript_ckb_stateful_scenarios.sh"],
        });
    }
    commands
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
    Ok(json!({
        "id": spec.id,
        "feature_ids": spec.feature_ids,
        "argv": spec.argv,
        "status": if code == 0 { "passed" } else { "failed" },
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
    if !matches!(mode, "quick" | "ci" | "full" | "nightly") {
        eprintln!("usage: cellscript-tools strict-backend [quick|ci|full|nightly]");
        return Ok(2);
    }

    let report_path = match env::var_os("CELLSCRIPT_STRICT_BACKEND_AUDIT_REPORT") {
        // Collapse repeated separators and `.` without resolving symlinks or
        // `..` components.
        Some(path) => lexical_path(&PathBuf::from(path)),
        None => default_report_path(root, mode)?,
    };
    let report_parent = report_path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(report_parent).with_context(|| format!("failed to create report directory {}", report_parent.display()))?;

    let commands = command_plan(mode);
    let mut results = Vec::with_capacity(commands.len());
    let mut tested = BTreeSet::new();
    for spec in &commands {
        println!("==> {}: {}", spec.id, spec.argv.join(" "));
        io::stdout().flush().context("failed to flush strict audit progress")?;
        let result = run_command(root, spec)?;
        if result.get("status").and_then(Value::as_str) == Some("passed") {
            tested.extend(spec.feature_ids.iter().copied());
        }
        results.push(result);
    }

    let mut missing: Vec<&str> = FEATURE_IDS.iter().copied().filter(|feature| !tested.contains(feature)).collect();
    missing.sort_unstable();
    let failed: Vec<&str> = results
        .iter()
        .filter(|result| result.get("status").and_then(Value::as_str) != Some("passed"))
        .filter_map(|result| result.get("id").and_then(Value::as_str))
        .collect();
    let passed = failed.is_empty();
    let report = json!({
        "audit": "cellscript-strict-codegen-ir-riscv",
        "mode": mode,
        "status": if passed { "passed" } else { "failed" },
        "feature_ids": FEATURE_IDS,
        "tested_feature_ids": tested,
        "missing_feature_ids": missing,
        "failed_commands": failed,
        "artifact_hashes": [],
        "ckb_vm": {"cycles": Value::Null, "transaction_size_bytes": Value::Null},
        "commands": results,
    });
    fs::write(&report_path, format!("{}\n", stable_json_pretty(&report)?))
        .with_context(|| format!("failed to write {}", report_path.display()))?;
    println!("strict backend audit report: {}", report_path.display());
    Ok(if passed { 0 } else { 1 })
}
