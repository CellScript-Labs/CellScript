use crate::error::{Error, Result};
use std::collections::BTreeSet;
use std::path::Path;

const NOVASEAL_CERTIFICATION_PLUGIN: &str = "novaseal-profile-v0";
const NOVASEAL_CERTIFICATION_REPORT_SCHEMA: &str = "cellscript-certification-report-v0.2";
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

pub fn failure_message(summary: &serde_json::Value) -> String {
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

/// Inputs from the command host; evidence rules remain owned by this package.
pub struct SummaryRequest<'a> {
    pub plugin_report: &'a serde_json::Value,
    pub repo_root: &'a Path,
    pub plugin_report_path: &'a Path,
    pub report_generated: bool,
    pub require_production: bool,
}

pub fn summarize(request: SummaryRequest<'_>) -> Result<serde_json::Value> {
    let SummaryRequest { plugin_report, repo_root, plugin_report_path, report_generated, require_production } = request;
    let implementation_path = repo_root.join(crate::ENGINE_SOURCE_PATH);
    let plugin_report_hash = ckb_blake2b_file_hash(plugin_report_path)?
        .ok_or_else(|| Error::invalid(format!("NovaSeal plugin report '{}' is not a regular file", plugin_report_path.display())))?;
    let implementation_hash = ckb_blake2b_file_hash(&implementation_path)?;
    let implementation_sources = crate::IMPLEMENTATION_SOURCES
        .iter()
        .map(|relative| {
            let path = repo_root.join(relative);
            let hash = ckb_blake2b_file_hash(&path)?;
            Ok(serde_json::json!({ "path": relative, "hash": hash }))
        })
        .collect::<Result<Vec<_>>>()?;
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
            "kind": "optional-rust-package",
            "implementation": crate::IMPLEMENTATION_ID,
            "implementation_path": implementation_path.display().to_string(),
            "implementation_hash_algorithm": "ckb_blake2b_256",
            "implementation_hash": implementation_hash,
            "implementation_sources": implementation_sources,
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
    let bytes = std::fs::read(path).map_err(|error| Error::invalid(format!("failed to read '{}': {}", path.display(), error)))?;
    Ok(Some(hex::encode(crate::ckb_blake2b256(&bytes))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn local_report() -> serde_json::Value {
        json!({
            "schema": "novaseal-production-gates-v0.4",
            "profile_certification": {
                "schema": "novaseal-profile-certification-v0.1",
                "profile": "agreement-profile-v0",
                "conforms_to": "NovaSealCanonicalV0",
                "status": "passed",
                "certification_level": "local"
            },
            "gates": [{ "name": "agreement_profile_public_ecosystem_certification_v0", "status": "passed" }],
            "local_production_prep_ready": true,
            "production_ready": false,
            "production_statement_eligible": false
        })
    }

    fn reduce(report: &serde_json::Value, production: bool) -> serde_json::Value {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("report.json");
        std::fs::write(&path, serde_json::to_vec(report).unwrap()).unwrap();
        summarize(SummaryRequest {
            plugin_report: report,
            repo_root: root.path(),
            plugin_report_path: &path,
            report_generated: false,
            require_production: production,
        })
        .unwrap()
    }

    #[test]
    fn local_success_does_not_grant_production_certification() {
        let report = local_report();
        let summary = reduce(&report, false);
        assert_eq!(summary["schema"], "cellscript-certification-report-v0.2");
        assert_eq!(summary["status"], "passed");
        assert_eq!(summary["production_statement_eligible"], false);
        assert_eq!(summary["plugin"]["implementation"], crate::IMPLEMENTATION_ID);
        assert_eq!(summary["plugin"]["report_generated"], false);
        assert_eq!(summary["plugin"]["implementation_sources"].as_array().unwrap().len(), crate::IMPLEMENTATION_SOURCES.len());
        let rejected = reduce(&report, true);
        assert_eq!(rejected["status"], "failed");
        assert!(failure_message(&rejected).contains("remaining external attestations"));
    }

    #[test]
    fn missing_stale_or_incomplete_evidence_stays_failed() {
        assert_eq!(reduce(&json!({}), false)["status"], "failed");
        let mut report = local_report();
        report["gates"] = json!([]);
        assert_eq!(reduce(&report, false)["status"], "failed");
        report = local_report();
        report["v1_readiness"] = json!({ "local_v1_ready": false, "planned_profile_matrix": { "missing": ["dual-seal-profile-v0"] } });
        let summary = reduce(&report, false);
        assert_eq!(summary["status"], "failed");
        assert!(failure_message(&summary).contains("remaining planned profiles"));
    }

    #[test]
    fn request_requires_actual_report_bytes_and_records_their_identity() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("absent.json");
        let report = local_report();
        assert!(summarize(SummaryRequest {
            plugin_report: &report,
            repo_root: root.path(),
            plugin_report_path: &path,
            report_generated: false,
            require_production: false
        })
        .is_err());
        let encoded = serde_json::to_vec(&report).unwrap();
        std::fs::write(&path, &encoded).unwrap();
        let summary = summarize(SummaryRequest {
            plugin_report: &report,
            repo_root: root.path(),
            plugin_report_path: &path,
            report_generated: false,
            require_production: false,
        })
        .unwrap();
        assert_eq!(summary["plugin_report"]["hash"], hex::encode(crate::ckb_blake2b256(&encoded)));
    }
}
