//! Native certification command boundary. Plugin evidence generation remains
//! separate from compiler semantics and from generic CLI presentation.
#[cfg(feature = "novaseal-certification")]
use super::output::{read_json_value, CommandOutcome};
use crate::error::{CompileError, Result};
use std::path::PathBuf;

const NOVASEAL_CERTIFICATION_PLUGIN: &str = "novaseal-profile-v0";

#[derive(Debug, Default, clap::Args)]
pub struct CertifyArgs {
    #[arg(long, value_name = "PLUGIN", help = "Certification plugin id, e.g. novaseal-profile-v0")]
    pub plugin: String,
    #[arg(id = "repo-root", long = "repo-root", value_name = "DIR", value_parser = utf8_path, help = "Repository root for Rust certification evidence")]
    pub repo_root: Option<PathBuf>,
    #[arg(long, value_name = "JSON", value_parser = utf8_path, help = "Verify an existing plugin report instead of regenerating it")]
    pub report: Option<PathBuf>,
    #[arg(long, short = 'o', value_name = "FILE", value_parser = utf8_path, help = "Write compiler certification report JSON")]
    pub output: Option<PathBuf>,
    #[arg(skip)]
    pub json: bool,
    #[arg(
        id = "require-production",
        long = "require-production",
        help = "Require external production attestations, not only local profile certification"
    )]
    pub require_production: bool,
}

fn utf8_path(value: &str) -> std::result::Result<PathBuf, std::convert::Infallible> {
    Ok(PathBuf::from(value))
}

impl CertifyArgs {
    pub(super) fn from_matches(matches: &clap::ArgMatches, json: bool) -> Result<Self> {
        let mut args = <Self as clap::FromArgMatches>::from_arg_matches(matches).map_err(|error| {
            CompileError::without_span(error.to_string())
                .with_code(crate::error::DiagnosticCode::CLI0001)
                .with_category(crate::error::CompileErrorCategory::Usage)
        })?;
        args.json = json;
        Ok(args)
    }
}

pub(super) fn run(args: CertifyArgs) -> Result<()> {
    validate_plugin(&args.plugin)?;
    run_available(args)
}

fn validate_plugin(plugin: &str) -> Result<()> {
    if plugin != NOVASEAL_CERTIFICATION_PLUGIN {
        return Err(CompileError::without_span(format!(
            "unknown certification plugin '{}'; available plugins: {}",
            plugin, NOVASEAL_CERTIFICATION_PLUGIN
        )));
    }
    Ok(())
}

#[cfg(not(feature = "novaseal-certification"))]
fn run_available(_args: CertifyArgs) -> Result<()> {
    Err(CompileError::without_span(
        "NovaSeal certification is unavailable in this installation; build cellc with --features novaseal-certification",
    )
    .with_code(crate::error::DiagnosticCode::CLI0002)
    .with_category(crate::error::CompileErrorCategory::Usage))
}

#[cfg(feature = "novaseal-certification")]
fn run_available(args: CertifyArgs) -> Result<()> {
    let repo_root = args.repo_root.unwrap_or(std::env::current_dir()?);
    let report_provided = args.report.is_some();
    let plugin_report_path = args.report.clone().unwrap_or_else(|| repo_root.join("target/novaseal-production-gates.json"));
    let report_generated = !report_provided;

    let plugin_report = if report_provided {
        read_json_value(&plugin_report_path)?
    } else {
        let report = cellscript_novaseal_certification::build_report(&repo_root).map_err(certification_error)?;
        if let Some(parent) = plugin_report_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            &plugin_report_path,
            serde_json::to_string_pretty(&report).map_err(|error| {
                crate::error::CompileError::without_span(format!("failed to serialize NovaSeal production-gate report: {}", error))
            })?,
        )?;
        report
    };

    let summary = cellscript_novaseal_certification::summarize(cellscript_novaseal_certification::SummaryRequest {
        plugin_report: &plugin_report,
        repo_root: &repo_root,
        plugin_report_path: &plugin_report_path,
        report_generated,
        require_production: args.require_production,
    })
    .map_err(certification_error)?;
    let output_path = args.output.unwrap_or_else(|| repo_root.join("target/cellscript-certification/novaseal-profile-v0.json"));
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &output_path,
        serde_json::to_string_pretty(&summary).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to serialize certification report: {}", error))
        })?,
    )?;

    let passed = summary["status"].as_str() == Some("passed");
    let failure_message = (!passed).then(|| cellscript_novaseal_certification::failure_message(&summary));
    let outcome = CommandOutcome {
        human_lines: vec![
            "Certification report generated".to_string(),
            format!("  Plugin: {}", args.plugin),
            format!("  Status: {}", summary["status"].as_str().unwrap_or("unknown")),
            format!("  Level: {}", summary["certification_level"].as_str().unwrap_or("unknown")),
            format!("  Output: {}", output_path.display()),
            format!("  Plugin report: {}", plugin_report_path.display()),
        ],
        machine: summary,
    };
    if passed {
        outcome.emit(args.json)
    } else {
        Err(outcome.failure(failure_message.unwrap_or_else(|| "certification failed".to_string())))
    }
}

#[cfg(feature = "novaseal-certification")]
fn certification_error(error: cellscript_novaseal_certification::Error) -> CompileError {
    match error {
        cellscript_novaseal_certification::Error::Io(error) => error.into(),
        cellscript_novaseal_certification::Error::InvalidEvidence(message) => CompileError::without_span(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_plugin_rejects_before_capability_selection() {
        assert!(run(CertifyArgs { plugin: "unknown".into(), ..Default::default() })
            .unwrap_err()
            .message
            .contains("unknown certification plugin"));
    }

    #[cfg(feature = "novaseal-certification")]
    #[test]
    fn supplied_report_uses_the_packaged_policy_and_retains_failure_output() {
        let root = tempfile::tempdir().unwrap();
        let report = root.path().join("input.json");
        let output = root.path().join("summary.json");
        std::fs::write(&report, "{}").unwrap();
        let error = run(CertifyArgs {
            plugin: NOVASEAL_CERTIFICATION_PLUGIN.into(),
            repo_root: Some(root.path().to_owned()),
            report: Some(report),
            output: Some(output.clone()),
            ..Default::default()
        })
        .unwrap_err();
        let summary: serde_json::Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
        assert_eq!(summary["status"], "failed");
        assert_eq!(summary["schema"], "cellscript-certification-report-v0.2");
        assert_eq!(error.details.as_ref(), Some(&summary));
    }

    #[cfg(not(feature = "novaseal-certification"))]
    #[test]
    fn missing_capability_rejects_before_any_report_io() {
        let output = tempfile::tempdir().unwrap();
        let path = output.path().join("must-not-exist.json");
        let error = run(CertifyArgs {
            plugin: NOVASEAL_CERTIFICATION_PLUGIN.into(),
            report: Some(output.path().join("missing-report.json")),
            output: Some(path.clone()),
            ..Default::default()
        })
        .unwrap_err();
        assert_eq!(error.code.as_deref(), Some("CLI0002"));
        assert!(!path.exists());
    }
}
