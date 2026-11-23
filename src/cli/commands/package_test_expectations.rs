use super::CheckArgs;
use crate::error::Result;
use camino::Utf8Path;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub(super) struct CompileTestExpectation {
    expect_success: bool,
    expect_fail: bool,
    expected_errors: Vec<String>,
    pub(super) target: Option<String>,
    pub(super) production: bool,
    pub(super) deny_fail_closed: bool,
    deny_ckb_runtime: bool,
    deny_runtime_obligations: bool,
    expect_standalone: Option<bool>,
    expect_ckb_runtime: Option<bool>,
    expect_fail_closed: Option<bool>,
    expected_runtime_features: Vec<String>,
    forbidden_runtime_features: Vec<String>,
    expected_verifier_obligations: Vec<String>,
    forbidden_verifier_obligations: Vec<String>,
    expected_runtime_required_obligations: Vec<String>,
    forbidden_runtime_required_obligations: Vec<String>,
    expected_artifact_format: Option<String>,
    expected_actions: Vec<String>,
    forbidden_actions: Vec<String>,
    expected_functions: Vec<String>,
    forbidden_functions: Vec<String>,
    expected_locks: Vec<String>,
    forbidden_locks: Vec<String>,
}

impl CompileTestExpectation {
    pub(super) fn check_args(&self) -> CheckArgs {
        CheckArgs {
            all_targets: false,
            artifact: None,
            target_profile: None,
            features: Vec::new(),
            all_features: false,
            no_default_features: false,
            locked: true,
            frozen: false,
            offline: false,
            environment: None,
            json: false,
            production: self.production,
            deny_fail_closed: self.deny_fail_closed,
            deny_ckb_runtime: self.deny_ckb_runtime,
            deny_runtime_obligations: self.deny_runtime_obligations,
            primitive_compat: None,
            package: None,
            workspace: false,
        }
    }
}

pub(super) fn read_test_expectation(path: &Path) -> Result<CompileTestExpectation> {
    let source = std::fs::read_to_string(path)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to read test '{}': {}", path.display(), error)))?;
    parse_test_expectation(path, &source)
}

fn parse_test_expectation(path: &Path, source: &str) -> Result<CompileTestExpectation> {
    let mut expectation = CompileTestExpectation::default();
    for (line_number, line) in source.lines().enumerate() {
        let Some(marker) = line.split("//").nth(1).map(str::trim) else {
            continue;
        };
        let Some(directive) = marker.strip_prefix("cellscript-test:").map(str::trim) else {
            continue;
        };

        if directive == "expect-success" {
            expectation.expect_success = true;
        } else if directive == "expect-fail" {
            expectation.expect_fail = true;
        } else if let Some(expected) = directive.strip_prefix("expect-error:").map(str::trim) {
            expectation.expect_fail = true;
            if !expected.is_empty() {
                expectation.expected_errors.push(expected.to_string());
            }
        } else if let Some(target) = directive.strip_prefix("target:").map(str::trim) {
            if target.is_empty() {
                return Err(compile_test_directive_error(path, line_number, "target directive requires a non-empty target"));
            }
            if expectation.target.replace(target.to_string()).is_some() {
                return Err(compile_test_directive_error(path, line_number, "target directive may only appear once"));
            }
        } else if directive == "production" {
            expectation.production = true;
        } else if directive == "deny-fail-closed" {
            expectation.deny_fail_closed = true;
        } else if directive == "deny-ckb-runtime" {
            expectation.deny_ckb_runtime = true;
        } else if directive == "deny-runtime-obligations" {
            expectation.deny_runtime_obligations = true;
        } else if directive == "expect-standalone" {
            expectation.expect_standalone = Some(true);
        } else if directive == "expect-not-standalone" {
            expectation.expect_standalone = Some(false);
        } else if directive == "expect-ckb-runtime" {
            expectation.expect_ckb_runtime = Some(true);
        } else if directive == "expect-no-ckb-runtime" {
            expectation.expect_ckb_runtime = Some(false);
        } else if directive == "expect-fail-closed-runtime" {
            expectation.expect_fail_closed = Some(true);
        } else if directive == "expect-no-fail-closed-runtime" {
            expectation.expect_fail_closed = Some(false);
        } else if let Some(feature) = directive.strip_prefix("expect-runtime-feature:").map(str::trim) {
            if feature.is_empty() {
                return Err(compile_test_directive_error(path, line_number, "expect-runtime-feature requires non-empty text"));
            }
            expectation.expected_runtime_features.push(feature.to_string());
        } else if let Some(feature) = directive.strip_prefix("expect-no-runtime-feature:").map(str::trim) {
            if feature.is_empty() {
                return Err(compile_test_directive_error(path, line_number, "expect-no-runtime-feature requires non-empty text"));
            }
            expectation.forbidden_runtime_features.push(feature.to_string());
        } else if let Some(obligation) = directive.strip_prefix("expect-verifier-obligation:").map(str::trim) {
            push_non_empty_test_directive(
                path,
                line_number,
                "expect-verifier-obligation",
                obligation,
                &mut expectation.expected_verifier_obligations,
            )?;
        } else if let Some(obligation) = directive.strip_prefix("expect-no-verifier-obligation:").map(str::trim) {
            push_non_empty_test_directive(
                path,
                line_number,
                "expect-no-verifier-obligation",
                obligation,
                &mut expectation.forbidden_verifier_obligations,
            )?;
        } else if let Some(obligation) = directive.strip_prefix("expect-runtime-required-obligation:").map(str::trim) {
            push_non_empty_test_directive(
                path,
                line_number,
                "expect-runtime-required-obligation",
                obligation,
                &mut expectation.expected_runtime_required_obligations,
            )?;
        } else if let Some(obligation) = directive.strip_prefix("expect-no-runtime-required-obligation:").map(str::trim) {
            push_non_empty_test_directive(
                path,
                line_number,
                "expect-no-runtime-required-obligation",
                obligation,
                &mut expectation.forbidden_runtime_required_obligations,
            )?;
        } else if let Some(format) = directive.strip_prefix("expect-artifact-format:").map(str::trim) {
            if format.is_empty() {
                return Err(compile_test_directive_error(path, line_number, "expect-artifact-format requires non-empty text"));
            }
            if expectation.expected_artifact_format.replace(format.to_string()).is_some() {
                return Err(compile_test_directive_error(path, line_number, "expect-artifact-format may only appear once"));
            }
        } else if let Some(name) = directive.strip_prefix("expect-action:").map(str::trim) {
            push_non_empty_test_directive(path, line_number, "expect-action", name, &mut expectation.expected_actions)?;
        } else if let Some(name) = directive.strip_prefix("expect-no-action:").map(str::trim) {
            push_non_empty_test_directive(path, line_number, "expect-no-action", name, &mut expectation.forbidden_actions)?;
        } else if let Some(name) = directive.strip_prefix("expect-function:").map(str::trim) {
            push_non_empty_test_directive(path, line_number, "expect-function", name, &mut expectation.expected_functions)?;
        } else if let Some(name) = directive.strip_prefix("expect-no-function:").map(str::trim) {
            push_non_empty_test_directive(path, line_number, "expect-no-function", name, &mut expectation.forbidden_functions)?;
        } else if let Some(name) = directive.strip_prefix("expect-lock:").map(str::trim) {
            push_non_empty_test_directive(path, line_number, "expect-lock", name, &mut expectation.expected_locks)?;
        } else if let Some(name) = directive.strip_prefix("expect-no-lock:").map(str::trim) {
            push_non_empty_test_directive(path, line_number, "expect-no-lock", name, &mut expectation.forbidden_locks)?;
        } else {
            return Err(compile_test_directive_error(
                path,
                line_number,
                &format!("unknown cellscript-test directive '{}'", directive),
            ));
        }
    }
    if expectation.expect_success && expectation.expect_fail {
        return Err(crate::error::CompileError::without_span(format!(
            "{}: conflicting cellscript-test directives: expect-success cannot be combined with expect-fail/expect-error",
            path.display()
        )));
    }
    Ok(expectation)
}

fn push_non_empty_test_directive(
    path: &Path,
    zero_based_line: usize,
    directive: &str,
    value: &str,
    values: &mut Vec<String>,
) -> Result<()> {
    if value.is_empty() {
        return Err(compile_test_directive_error(path, zero_based_line, &format!("{} requires non-empty text", directive)));
    }
    values.push(value.to_string());
    Ok(())
}

fn compile_test_directive_error(path: &Path, zero_based_line: usize, message: &str) -> crate::error::CompileError {
    crate::error::CompileError::without_span(format!("{}:{}: {}", path.display(), zero_based_line + 1, message))
}

pub(super) fn evaluate_compile_test_result(
    path: &Utf8Path,
    expectation: &CompileTestExpectation,
    result: Result<crate::CompileResult>,
) -> Result<()> {
    match (expectation.expect_fail, result) {
        (false, Ok(result)) => validate_compile_test_metadata(path, expectation, &result.metadata),
        (false, Err(error)) => {
            Err(crate::error::CompileError::without_span(format!("{}: expected compile success, got error: {}", path, error)))
        }
        (true, Ok(_)) => Err(crate::error::CompileError::without_span(format!("{}: expected compile failure, got success", path))),
        (true, Err(error)) => {
            let missing = expectation
                .expected_errors
                .iter()
                .filter(|expected| !compile_error_contains_text(&error, expected))
                .cloned()
                .collect::<Vec<_>>();
            if missing.is_empty() {
                Ok(())
            } else {
                Err(crate::error::CompileError::without_span(format!(
                    "{}: expected error text not found: {}; actual error: {}",
                    path,
                    missing.join(", "),
                    error
                )))
            }
        }
    }
}

fn compile_error_contains_text(error: &crate::error::CompileError, expected: &str) -> bool {
    error.message.contains(expected) || error.related.iter().any(|related| compile_error_contains_text(related, expected))
}

fn validate_compile_test_metadata(
    path: &Utf8Path,
    expectation: &CompileTestExpectation,
    metadata: &crate::CompileMetadata,
) -> Result<()> {
    if let Some(expected) = &expectation.expected_artifact_format
        && &metadata.artifact_format != expected
    {
        return Err(crate::error::CompileError::without_span(format!(
            "{}: expected artifact_format='{}', got '{}'",
            path, expected, metadata.artifact_format
        )));
    }

    if let Some(expected) = expectation.expect_standalone
        && metadata.runtime.standalone_runner_compatible != expected
    {
        return Err(crate::error::CompileError::without_span(format!(
            "{}: expected standalone_runner_compatible={}, got {}",
            path, expected, metadata.runtime.standalone_runner_compatible
        )));
    }
    if let Some(expected) = expectation.expect_ckb_runtime
        && metadata.runtime.ckb_runtime_required != expected
    {
        return Err(crate::error::CompileError::without_span(format!(
            "{}: expected ckb_runtime_required={}, got {}",
            path, expected, metadata.runtime.ckb_runtime_required
        )));
    }
    if let Some(expected) = expectation.expect_fail_closed {
        let actual = !metadata.runtime.fail_closed_runtime_features.is_empty()
            || metadata.runtime.verifier_obligations.iter().any(|obligation| obligation.status == "fail-closed");
        if actual != expected {
            return Err(crate::error::CompileError::without_span(format!(
                "{}: expected fail_closed_runtime={}, got {}",
                path, expected, actual
            )));
        }
    }

    let runtime_summary = compile_test_runtime_summary(metadata);
    for expected in &expectation.expected_runtime_features {
        if !runtime_summary.contains(expected) {
            return Err(crate::error::CompileError::without_span(format!(
                "{}: expected runtime metadata to contain '{}'",
                path, expected
            )));
        }
    }
    for forbidden in &expectation.forbidden_runtime_features {
        if runtime_summary.contains(forbidden) {
            return Err(crate::error::CompileError::without_span(format!(
                "{}: expected runtime metadata not to contain '{}'",
                path, forbidden
            )));
        }
    }

    validate_compile_test_summary_contains(
        path,
        "verifier obligation",
        &compile_test_obligation_summary(metadata, None),
        &expectation.expected_verifier_obligations,
        &expectation.forbidden_verifier_obligations,
    )?;
    validate_compile_test_summary_contains(
        path,
        "runtime-required verifier obligation",
        &compile_test_obligation_summary(metadata, Some("runtime-required")),
        &expectation.expected_runtime_required_obligations,
        &expectation.forbidden_runtime_required_obligations,
    )?;

    validate_named_metadata_set(
        path,
        "action",
        &metadata.actions.iter().map(|action| action.name.as_str()).collect::<Vec<_>>(),
        &expectation.expected_actions,
        &expectation.forbidden_actions,
    )?;
    validate_named_metadata_set(
        path,
        "function",
        &metadata.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>(),
        &expectation.expected_functions,
        &expectation.forbidden_functions,
    )?;
    validate_named_metadata_set(
        path,
        "lock",
        &metadata.locks.iter().map(|lock| lock.name.as_str()).collect::<Vec<_>>(),
        &expectation.expected_locks,
        &expectation.forbidden_locks,
    )?;

    Ok(())
}

fn validate_compile_test_summary_contains(
    path: &Utf8Path,
    label: &str,
    summary: &str,
    expected: &[String],
    forbidden: &[String],
) -> Result<()> {
    for expected in expected {
        if !summary.contains(expected) {
            return Err(crate::error::CompileError::without_span(format!(
                "{}: expected {} metadata to contain '{}'",
                path, label, expected
            )));
        }
    }
    for forbidden in forbidden {
        if summary.contains(forbidden) {
            return Err(crate::error::CompileError::without_span(format!(
                "{}: expected {} metadata not to contain '{}'",
                path, label, forbidden
            )));
        }
    }
    Ok(())
}

fn validate_named_metadata_set(path: &Utf8Path, kind: &str, actual: &[&str], expected: &[String], forbidden: &[String]) -> Result<()> {
    for name in expected {
        if !actual.iter().any(|actual_name| actual_name == name) {
            return Err(crate::error::CompileError::without_span(format!(
                "{}: expected {} metadata to contain '{}'",
                path, kind, name
            )));
        }
    }
    for name in forbidden {
        if actual.iter().any(|actual_name| actual_name == name) {
            return Err(crate::error::CompileError::without_span(format!(
                "{}: expected {} metadata not to contain '{}'",
                path, kind, name
            )));
        }
    }
    Ok(())
}

fn compile_test_runtime_summary(metadata: &crate::CompileMetadata) -> String {
    let mut values = Vec::new();
    values.extend(metadata.runtime.ckb_runtime_features.iter().cloned());
    values.extend(metadata.runtime.fail_closed_runtime_features.iter().cloned());
    for access in &metadata.runtime.ckb_runtime_accesses {
        values.push(format!(
            "{}:{}:{}:{}:{}:{}",
            access.operation,
            access.syscall,
            access.source,
            access.index,
            access.binding,
            serde_json::to_string(&access.provenance).expect("runtime access provenance must serialize")
        ));
    }
    for obligation in &metadata.runtime.verifier_obligations {
        values.push(format!(
            "{}:{}:{}:{}:{}",
            obligation.scope, obligation.category, obligation.feature, obligation.status, obligation.detail
        ));
    }
    values.join("\n")
}

fn compile_test_obligation_summary(metadata: &crate::CompileMetadata, status: Option<&str>) -> String {
    metadata
        .runtime
        .verifier_obligations
        .iter()
        .filter(|obligation| match status {
            Some(status) => obligation.status == status,
            None => true,
        })
        .map(|obligation| {
            format!("{}:{}:{}:{}:{}", obligation.scope, obligation.category, obligation.feature, obligation.status, obligation.detail)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn collect_cell_files(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    if root.is_file() {
        return Ok(if root.extension().and_then(|ext| ext.to_str()) == Some("cell") { vec![root.to_path_buf()] } else { Vec::new() });
    }

    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("cell") {
                files.push(path);
            }
        }
    }
    Ok(files)
}
