//! ProofPlan soundness checks for v0.16 assurance metadata.

use crate::error::{CompileError, Result};
use crate::{CompileMetadata, ProofPlanMetadata, VerifierObligationMetadata};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofPlanSoundnessReport {
    pub status: String,
    pub strict: bool,
    pub checked_records: usize,
    pub checked_obligations: usize,
    pub issue_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<ProofPlanSoundnessIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofPlanSoundnessIssue {
    pub severity: String,
    pub code: String,
    pub origin: String,
    pub feature: String,
    pub message: String,
}

pub fn check_metadata(metadata: &CompileMetadata, strict: bool) -> ProofPlanSoundnessReport {
    let mut issues = Vec::new();
    let proof_plan = &metadata.runtime.proof_plan;
    let obligations = &metadata.runtime.verifier_obligations;

    if proof_plan.is_empty() && !obligations.is_empty() {
        push_issue(
            &mut issues,
            "error",
            "PP0001",
            "runtime",
            "*",
            "runtime verifier obligations exist but runtime.proof_plan is empty",
        );
    }

    let proof_index = proof_plan
        .iter()
        .map(|plan| (obligation_key(&plan.origin, &plan.category, &plan.feature, &plan.status, &plan.detail), plan))
        .collect::<BTreeMap<_, _>>();

    for obligation in obligations {
        let key = obligation_key(&obligation.scope, &obligation.category, &obligation.feature, &obligation.status, &obligation.detail);
        if !proof_index.contains_key(&key) {
            push_issue(
                &mut issues,
                "error",
                "PP0002",
                &obligation.scope,
                &obligation.feature,
                "runtime verifier obligation has no matching ProofPlan record with the same origin/scope, category, feature, status, and detail",
            );
        }
    }

    check_local_runtime_plan_consistency(metadata, &mut issues);

    for plan in proof_plan {
        check_plan_record(plan, strict, &mut issues);
    }

    let issue_count = issues.len();
    let status = if issues.iter().any(|issue| issue.severity == "error") { "failed" } else { "passed" };

    ProofPlanSoundnessReport {
        status: status.to_string(),
        strict,
        checked_records: proof_plan.len(),
        checked_obligations: obligations.len(),
        issue_count,
        issues,
    }
}

pub fn validate_metadata(metadata: &CompileMetadata, strict: bool) -> Result<()> {
    let report = check_metadata(metadata, strict);
    if report.status == "passed" {
        return Ok(());
    }

    let messages = report
        .issues
        .iter()
        .filter(|issue| issue.severity == "error")
        .map(|issue| format!("{} {}:{} - {}", issue.code, issue.origin, issue.feature, issue.message))
        .collect::<Vec<_>>();
    Err(CompileError::without_span(format!("ProofPlan soundness check failed:\n  - {}", messages.join("\n  - "))))
}

fn check_plan_record(plan: &ProofPlanMetadata, strict: bool, issues: &mut Vec<ProofPlanSoundnessIssue>) {
    if plan.on_chain_checked && plan.status == "runtime-required" {
        push_issue(
            issues,
            "error",
            "PP0101",
            &plan.origin,
            &plan.feature,
            "ProofPlan marks a runtime-required obligation as on-chain checked",
        );
    }

    if plan.on_chain_checked && plan.codegen_coverage_status != "covered" {
        push_issue(
            issues,
            "error",
            "PP0102",
            &plan.origin,
            &plan.feature,
            "on-chain checked ProofPlan record must have codegen_coverage_status='covered'",
        );
    }

    if strict && plan.status == "checked-runtime" && !plan.on_chain_checked {
        push_issue(
            issues,
            "error",
            "PP0103",
            &plan.origin,
            &plan.feature,
            "checked ProofPlan status is not reflected in on_chain_checked",
        );
    }

    if plan.codegen_coverage_status.starts_with("gap:") && plan.on_chain_checked {
        push_issue(issues, "error", "PP0104", &plan.origin, &plan.feature, "ProofPlan coverage gap cannot be marked on-chain checked");
    }

    if strict && (plan.status == "runtime-required" || plan.codegen_coverage_status == "gap:metadata-only") {
        push_issue(
            issues,
            "error",
            "PP0150",
            &plan.origin,
            &plan.feature,
            "strict v0.16 ProofPlan mode rejects metadata-only or runtime-required obligations",
        );
    }

    if strict && !plan.lock_args_fields.is_empty() && !plan.reads.iter().any(|read| read == "lock_args") {
        push_issue(
            issues,
            "error",
            "PP0201",
            &plan.origin,
            &plan.feature,
            "ProofPlan exposes lock_args fields but reads does not include lock_args",
        );
    }

    if strict && !plan.witness_fields.is_empty() && !plan.reads.iter().any(|read| read == "witness") {
        push_issue(
            issues,
            "error",
            "PP0202",
            &plan.origin,
            &plan.feature,
            "ProofPlan exposes witness fields but reads does not include witness",
        );
    }

    if plan.on_chain_checked
        && plan
            .builder_assumptions
            .iter()
            .any(|assumption| assumption.contains("runtime-required") || assumption.contains("metadata-only"))
    {
        push_issue(
            issues,
            "error",
            "PP0301",
            &plan.origin,
            &plan.feature,
            "on-chain checked ProofPlan record carries unchecked runtime/metadata-only builder assumptions",
        );
    }
}

fn check_local_runtime_plan_consistency(metadata: &CompileMetadata, issues: &mut Vec<ProofPlanSoundnessIssue>) {
    let mut runtime_by_identity = BTreeMap::<String, Vec<&ProofPlanMetadata>>::new();
    for plan in &metadata.runtime.proof_plan {
        runtime_by_identity.entry(plan_identity_key(plan)).or_default().push(plan);
    }
    let runtime_identities = runtime_by_identity.keys().cloned().collect::<BTreeSet<_>>();
    let runtime_full = metadata.runtime.proof_plan.iter().map(plan_full_key).collect::<BTreeSet<_>>();

    let mut local_by_identity = BTreeMap::<String, Vec<&ProofPlanMetadata>>::new();

    for action in &metadata.actions {
        for plan in &action.proof_plan {
            local_by_identity.entry(plan_identity_key(plan)).or_default().push(plan);
        }
    }
    for function in &metadata.functions {
        for plan in &function.proof_plan {
            local_by_identity.entry(plan_identity_key(plan)).or_default().push(plan);
        }
    }
    for lock in &metadata.locks {
        for plan in &lock.proof_plan {
            local_by_identity.entry(plan_identity_key(plan)).or_default().push(plan);
        }
    }
    let local_identities = local_by_identity.keys().cloned().collect::<BTreeSet<_>>();
    let local_full = local_by_identity.values().flat_map(|plans| plans.iter().copied()).map(plan_full_key).collect::<BTreeSet<_>>();

    for key in &local_identities {
        if !runtime_identities.contains(key) {
            let (origin, feature, status) = split_plan_identity_key(key);
            push_issue(
                issues,
                "error",
                "PP0401",
                origin,
                feature,
                &format!("local ProofPlan record with status '{}' is missing from runtime.proof_plan", status),
            );
        } else if let Some(plans) = local_by_identity.get(key) {
            for plan in plans {
                if !runtime_full.contains(&plan_full_key(plan)) {
                    push_issue(
                        issues,
                        "error",
                        "PP0403",
                        &plan.origin,
                        &plan.feature,
                        "local ProofPlan record differs from runtime.proof_plan in trigger, scope, reads, coverage, assumptions, group cardinality, detail, or codegen coverage",
                    );
                }
            }
        }
    }

    for key in runtime_identities {
        let (origin, feature, status) = split_plan_identity_key(&key);
        if origin.starts_with("invariant:") {
            continue;
        }
        if !local_identities.contains(&key) {
            push_issue(
                issues,
                "error",
                "PP0402",
                origin,
                feature,
                &format!("runtime ProofPlan record with status '{}' is missing from local action/function/lock metadata", status),
            );
        } else if let Some(plans) = runtime_by_identity.get(&key) {
            for plan in plans {
                if !local_full.contains(&plan_full_key(plan)) {
                    push_issue(
                        issues,
                        "error",
                        "PP0404",
                        &plan.origin,
                        &plan.feature,
                        "runtime ProofPlan record differs from local action/function/lock metadata in trigger, scope, reads, coverage, assumptions, group cardinality, detail, or codegen coverage",
                    );
                }
            }
        }
    }
}

fn obligation_key(scope_or_origin: &str, category: &str, feature: &str, status: &str, detail: &str) -> String {
    format!("{scope_or_origin}\u{1f}{category}\u{1f}{feature}\u{1f}{status}\u{1f}{detail}")
}

fn plan_identity_key(plan: &ProofPlanMetadata) -> String {
    format!("{}\u{1f}{}\u{1f}{}", plan.origin, plan.feature, plan.status)
}

fn split_plan_identity_key(key: &str) -> (&str, &str, &str) {
    let mut parts = key.split('\u{1f}');
    (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""))
}

fn plan_full_key(plan: &ProofPlanMetadata) -> String {
    serde_json::to_string(plan).unwrap_or_else(|_| format!("{plan:?}"))
}

fn push_issue(issues: &mut Vec<ProofPlanSoundnessIssue>, severity: &str, code: &str, origin: &str, feature: &str, message: &str) {
    issues.push(ProofPlanSoundnessIssue {
        severity: severity.to_string(),
        code: code.to_string(),
        origin: origin.to_string(),
        feature: feature.to_string(),
        message: message.to_string(),
    });
}

#[allow(dead_code)]
fn _obligation_debug_key(obligation: &VerifierObligationMetadata) -> String {
    obligation_key(&obligation.scope, &obligation.category, &obligation.feature, &obligation.status, &obligation.detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan_with_status(status: &str, on_chain_checked: bool, codegen_coverage_status: &str) -> ProofPlanMetadata {
        ProofPlanMetadata {
            name: "fixture".to_string(),
            origin: "action:fixture".to_string(),
            category: "fixture".to_string(),
            feature: "fixture".to_string(),
            source_span: None,
            trigger: "explicit_entry".to_string(),
            scope: "selected_cells".to_string(),
            reads: Vec::new(),
            coverage: Vec::new(),
            input_output_relation_checks: Vec::new(),
            group_cardinality: "not a script-group cardinality obligation".to_string(),
            identity_lifecycle_policy: "none".to_string(),
            preserved_fields: Vec::new(),
            witness_fields: Vec::new(),
            lock_args_fields: Vec::new(),
            on_chain_checked,
            on_chain_checked_obligations: Vec::new(),
            executable_evidence: Vec::new(),
            builder_assumptions: Vec::new(),
            codegen_coverage_status: codegen_coverage_status.to_string(),
            status: status.to_string(),
            detail: "fixture detail".to_string(),
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn strict_pp0103_only_applies_to_checked_runtime_records() {
        for status in ["ckb-runtime", "checked-static"] {
            let plan = plan_with_status(status, false, status);
            let mut issues = Vec::new();

            check_plan_record(&plan, true, &mut issues);

            assert!(
                !issues.iter().any(|issue| issue.code == "PP0103"),
                "{status} must not be treated as an on-chain checked runtime proof: {issues:?}"
            );
        }

        let checked_runtime = plan_with_status("checked-runtime", false, "gap:evidence-missing");
        let mut issues = Vec::new();

        check_plan_record(&checked_runtime, true, &mut issues);

        assert!(
            issues.iter().any(|issue| issue.code == "PP0103"),
            "checked-runtime without on_chain_checked must remain a strict soundness error: {issues:?}"
        );
    }
}
