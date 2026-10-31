//! Native certification command boundary. Plugin evidence generation remains
//! separate from compiler semantics and from generic CLI presentation.
use super::output::{read_json_value, CommandOutcome};
use crate::error::{CompileError, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const NOVASEAL_CERTIFICATION_PLUGIN: &str = "novaseal-profile-v0";
const NOVASEAL_CERTIFICATION_REPORT_SCHEMA: &str = "cellscript-certification-report-v0.1";
const NOVASEAL_PLUGIN_REPORT_SCHEMA: &str = "novaseal-production-gates-v0.4";
const NOVASEAL_PROFILE_CERTIFICATION_SCHEMA: &str = "novaseal-profile-certification-v0.1";
const NOVASEAL_AGREEMENT_PROFILE: &str = "agreement-profile-v0";
const NOVASEAL_CANONICAL_SCHEMA: &str = "NovaSealCanonicalV0";
const NOVASEAL_PROFILE_CERTIFICATION_GATE: &str = "agreement_profile_public_ecosystem_certification_v0";
const NOVASEAL_LOCAL_V1_DIMENSIONS: &[&str] = &[
    "architecture_and_profile_conformance",
    "planned_profiles_and_business_scenarios",
    "security_audit_coverage",
    "devnet_multi_profile_coverage",
    "multi_business_scenario_coverage",
    "full_stateful_acceptance",
    "wallet_signing_vectors",
    "profile_operator_fixtures",
    "service_builder_fixtures",
    "btc_spv_evidence_adapter",
    "external_attestation_adapter",
    "external_evidence_handoff",
    "local_bip340_tcb_review",
    "local_v1_gate",
];
const NOVASEAL_EXTERNAL_V1_DIMENSIONS: &[&str] = &[
    "external_btc_fiber_endpoint_acceptance",
    "all_profiles_production_completeness",
    "public_shared_cell_dep_attestation",
    "external_bip340_tcb_review_attestation",
    "public_btc_spv_evidence",
    "rwa_legal_registry_review_evidence",
];

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
            CompileError::without_span(error.to_string()).with_code("CLI0001").with_category(crate::error::CompileErrorCategory::Usage)
        })?;
        args.json = json;
        Ok(args)
    }
}

pub(super) fn run(args: CertifyArgs) -> Result<()> {
    if args.plugin != NOVASEAL_CERTIFICATION_PLUGIN {
        return Err(crate::error::CompileError::without_span(format!(
            "unknown certification plugin '{}'; available plugins: {}",
            args.plugin, NOVASEAL_CERTIFICATION_PLUGIN
        )));
    }

    let repo_root = args.repo_root.unwrap_or(std::env::current_dir()?);
    let report_provided = args.report.is_some();
    let plugin_report_path = args.report.clone().unwrap_or_else(|| repo_root.join("target/novaseal-production-gates.json"));
    let report_generated = !report_provided;

    let plugin_report = if report_provided {
        read_json_value(&plugin_report_path)?
    } else {
        let report = super::novaseal_certification::build_report(&repo_root)?;
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

    let implementation_path = repo_root.join("src/cli/novaseal_certification.rs");
    let summary = novaseal_certification_summary(
        &plugin_report,
        &repo_root,
        &plugin_report_path,
        &implementation_path,
        report_generated,
        args.require_production,
    )?;
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
    let failure_message = (!passed).then(|| novaseal_certification_failure_message(&summary));
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
fn novaseal_gate_status<'a>(report: &'a serde_json::Value, gate_name: &str) -> Option<&'a str> {
    report.get("gates")?.as_array()?.iter().find_map(|gate| {
        let name = gate.get("name").and_then(serde_json::Value::as_str)?;
        if name == gate_name {
            gate.get("status").and_then(serde_json::Value::as_str)
        } else {
            None
        }
    })
}

fn novaseal_certification_failure_message(summary: &serde_json::Value) -> String {
    let reason = summary.get("failure_reason").unwrap_or(&serde_json::Value::Null);
    if let Some(message) = json_pointer_str(reason, "/message") {
        return message.to_string();
    }
    if let Some(message) = reason.as_str() {
        return message.to_string();
    }
    if !reason.is_null() {
        return serde_json::to_string(reason).unwrap_or_else(|_| "certification failed".to_string());
    }
    "certification failed".to_string()
}

fn novaseal_failed_dimensions(plugin_report: &serde_json::Value, v1_readiness: &serde_json::Value) -> serde_json::Value {
    let mut seen = BTreeSet::new();
    let mut dimensions = Vec::new();
    for source in [plugin_report.get("failed_dimensions"), v1_readiness.get("failed_dimensions")] {
        let Some(items) = source.and_then(serde_json::Value::as_array) else {
            continue;
        };
        for item in items {
            let Some(name) = item.as_str() else {
                continue;
            };
            if seen.insert(name.to_string()) {
                dimensions.push(serde_json::Value::String(name.to_string()));
            }
        }
    }
    serde_json::Value::Array(dimensions)
}

fn novaseal_failed_dimension_matches(failed_dimensions: &serde_json::Value, expected: &[&str]) -> bool {
    failed_dimensions.as_array().is_some_and(|dimensions| {
        dimensions.iter().filter_map(serde_json::Value::as_str).any(|dimension| expected.contains(&dimension))
    })
}

#[allow(clippy::too_many_arguments)]
fn novaseal_certification_summary(
    plugin_report: &serde_json::Value,
    repo_root: &Path,
    plugin_report_path: &Path,
    implementation_path: &Path,
    report_generated: bool,
    require_production: bool,
) -> Result<serde_json::Value> {
    let plugin_report_hash = ckb_blake2b_file_hash(plugin_report_path)?.ok_or_else(|| {
        crate::error::CompileError::without_span(format!(
            "NovaSeal plugin report '{}' is not a regular file",
            plugin_report_path.display()
        ))
    })?;
    let implementation_hash = ckb_blake2b_file_hash(implementation_path)?;
    let profile_certification = plugin_report.get("profile_certification").unwrap_or(&serde_json::Value::Null);
    let v1_readiness = plugin_report.get("v1_readiness").unwrap_or(&serde_json::Value::Null);

    let mut checks = vec![
        ("plugin_report_schema", json_pointer_str(plugin_report, "/schema") == Some(NOVASEAL_PLUGIN_REPORT_SCHEMA)),
        (
            "profile_certification_schema",
            json_pointer_str(profile_certification, "/schema") == Some(NOVASEAL_PROFILE_CERTIFICATION_SCHEMA),
        ),
        ("profile_id", json_pointer_str(profile_certification, "/profile") == Some(NOVASEAL_AGREEMENT_PROFILE)),
        ("canonical_target", json_pointer_str(profile_certification, "/conforms_to") == Some(NOVASEAL_CANONICAL_SCHEMA)),
        ("profile_certification_passed", json_pointer_str(profile_certification, "/status") == Some("passed")),
        ("public_ecosystem_gate_passed", novaseal_gate_status(plugin_report, NOVASEAL_PROFILE_CERTIFICATION_GATE) == Some("passed")),
        ("local_production_prep_ready", json_pointer_bool(plugin_report, "/local_production_prep_ready")),
    ];
    if !v1_readiness.is_null() {
        checks.push(("v1_readiness_local_ready", json_pointer_bool(v1_readiness, "/local_v1_ready")));
    }

    let production_statement_eligible = plugin_report
        .pointer("/production_statement_eligible")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or_else(|| json_pointer_bool(profile_certification, "/production_statement_eligible"));

    if require_production {
        checks.push(("production_ready", json_pointer_bool(plugin_report, "/production_ready")));
        checks.push(("production_statement_eligible", production_statement_eligible));
    }

    let checks_json =
        checks.iter().map(|(name, passed)| ((*name).to_string(), serde_json::Value::Bool(*passed))).collect::<serde_json::Map<_, _>>();
    let failed_checks: Vec<serde_json::Value> =
        checks.iter().filter(|(_, passed)| !*passed).map(|(name, _)| serde_json::Value::String((*name).to_string())).collect();
    let passed = failed_checks.is_empty();
    let external_blockers = plugin_report
        .get("external_blockers")
        .cloned()
        .or_else(|| v1_readiness.get("external_blockers").cloned())
        .or_else(|| profile_certification.get("production_statement_blockers").cloned())
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    let failed_dimensions = novaseal_failed_dimensions(plugin_report, v1_readiness);
    let planned_missing =
        v1_readiness.pointer("/planned_profile_matrix/missing").cloned().unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    let planned_missing_non_empty = planned_missing.as_array().is_some_and(|items| !items.is_empty());
    let external_blockers_non_empty = external_blockers.as_array().is_some_and(|items| !items.is_empty());
    let failed_local_v1_dimension = novaseal_failed_dimension_matches(&failed_dimensions, NOVASEAL_LOCAL_V1_DIMENSIONS);
    let failed_external_or_endpoint_dimension = novaseal_failed_dimension_matches(&failed_dimensions, NOVASEAL_EXTERNAL_V1_DIMENSIONS);
    let certification_level = json_pointer_str(profile_certification, "/certification_level").unwrap_or("unknown");

    let failure_reason = if passed {
        serde_json::Value::Null
    } else if !v1_readiness.is_null() && !json_pointer_bool(v1_readiness, "/local_v1_ready") && planned_missing_non_empty {
        serde_json::json!({
            "message": "NovaSeal V1 readiness requires remaining planned profiles and business scenarios",
            "v1_status": json_pointer_str(v1_readiness, "/status"),
            "missing": planned_missing,
            "failed_dimensions": failed_dimensions.clone(),
            "external_blockers": external_blockers.clone(),
            "failed_checks": failed_checks,
        })
    } else if !v1_readiness.is_null() && !json_pointer_bool(v1_readiness, "/local_v1_ready") && failed_local_v1_dimension {
        serde_json::json!({
            "message": "NovaSeal V1 readiness requires fresh local evidence reports",
            "remediation": [
                "rerun live devnet reports for core, Agreement, and planned profiles after source or report changes",
                "rerun NovaSeal wallet, operator fixture, service-builder, BTC SPV adapter, external attestation adapter, BIP340 TCB review, and handoff bundle generators"
            ],
            "v1_status": json_pointer_str(v1_readiness, "/status"),
            "missing": planned_missing,
            "failed_dimensions": failed_dimensions.clone(),
            "external_blockers": external_blockers.clone(),
            "failed_checks": failed_checks,
        })
    } else if !v1_readiness.is_null()
        && !json_pointer_bool(v1_readiness, "/local_v1_ready")
        && (external_blockers_non_empty || failed_external_or_endpoint_dimension)
    {
        serde_json::json!({
            "message": "NovaSeal V1 readiness requires external production evidence and endpoint acceptance",
            "v1_status": json_pointer_str(v1_readiness, "/status"),
            "missing": planned_missing,
            "failed_dimensions": failed_dimensions.clone(),
            "external_blockers": external_blockers.clone(),
            "failed_checks": failed_checks,
        })
    } else if require_production && json_pointer_bool(plugin_report, "/local_production_prep_ready") {
        serde_json::json!({
            "message": "NovaSeal production certification requires remaining external attestations",
            "external_blockers": external_blockers.clone(),
            "failed_dimensions": failed_dimensions.clone(),
            "failed_checks": failed_checks,
        })
    } else {
        serde_json::json!({
            "message": "NovaSeal profile certification failed deterministic compiler checks",
            "failed_dimensions": failed_dimensions.clone(),
            "external_blockers": external_blockers.clone(),
            "failed_checks": failed_checks,
        })
    };

    Ok(serde_json::json!({
        "schema": NOVASEAL_CERTIFICATION_REPORT_SCHEMA,
        "status": if passed { "passed" } else { "failed" },
        "plugin": {
            "id": NOVASEAL_CERTIFICATION_PLUGIN,
            "kind": "compiler-builtin-rust",
            "implementation": super::novaseal_certification::IMPLEMENTATION_ID,
            "implementation_path": implementation_path.display().to_string(),
            "implementation_hash_algorithm": "ckb_blake2b_256",
            "implementation_hash": implementation_hash,
            "report_generated": report_generated,
        },
        "plugin_report": {
            "path": plugin_report_path.display().to_string(),
            "schema": json_pointer_str(plugin_report, "/schema"),
            "hash_algorithm": "ckb_blake2b_256",
            "hash": plugin_report_hash,
            "status": json_pointer_str(plugin_report, "/status"),
            "production_ready": json_pointer_bool(plugin_report, "/production_ready"),
            "production_gates_passed": json_pointer_bool(plugin_report, "/production_gates_passed"),
            "local_production_prep_ready": json_pointer_bool(plugin_report, "/local_production_prep_ready"),
            "v1_status": json_pointer_str(v1_readiness, "/status"),
            "local_v1_ready": json_pointer_bool(v1_readiness, "/local_v1_ready"),
        },
        "profile": NOVASEAL_AGREEMENT_PROFILE,
        "conforms_to": NOVASEAL_CANONICAL_SCHEMA,
        "certification_level": certification_level,
        "production_statement_eligible": production_statement_eligible,
        "failed_dimensions": failed_dimensions,
        "external_blockers": external_blockers,
        "require_production": require_production,
        "repo_root": repo_root.display().to_string(),
        "checks": checks_json,
        "failure_reason": failure_reason,
    }))
}

fn json_pointer_str<'a>(value: &'a serde_json::Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(serde_json::Value::as_str)
}

fn json_pointer_bool(value: &serde_json::Value, pointer: &str) -> bool {
    value.pointer(pointer).and_then(serde_json::Value::as_bool).unwrap_or(false)
}

fn ckb_blake2b_file_hash(path: &Path) -> Result<Option<String>> {
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(path)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to read '{}': {}", path.display(), error)))?;
    Ok(Some(crate::hex_encode(&crate::ckb_blake2b256(&bytes))))
}
