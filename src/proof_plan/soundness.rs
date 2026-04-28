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

    let proof_index =
        proof_plan.iter().map(|plan| (obligation_key(&plan.category, &plan.feature, &plan.status), plan)).collect::<BTreeMap<_, _>>();

    for obligation in obligations {
        let key = obligation_key(&obligation.category, &obligation.feature, &obligation.status);
        if !proof_index.contains_key(&key) {
            push_issue(
                &mut issues,
                "error",
                "PP0002",
                &obligation.scope,
                &obligation.feature,
                "runtime verifier obligation has no matching ProofPlan record with the same category, feature, and status",
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

    if matches!(plan.status.as_str(), "checked-runtime" | "checked-static" | "ckb-runtime") && !plan.on_chain_checked {
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

    if !plan.lock_args_fields.is_empty() && !plan.reads.iter().any(|read| read == "lock_args") {
        push_issue(
            issues,
            "error",
            "PP0201",
            &plan.origin,
            &plan.feature,
            "ProofPlan exposes lock_args fields but reads does not include lock_args",
        );
    }

    if !plan.witness_fields.is_empty() && !plan.reads.iter().any(|read| read == "witness") {
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
    let runtime_keys = metadata.runtime.proof_plan.iter().map(plan_key).collect::<BTreeSet<_>>();
    let mut local_keys = BTreeSet::new();

    for action in &metadata.actions {
        local_keys.extend(action.proof_plan.iter().map(plan_key));
    }
    for function in &metadata.functions {
        local_keys.extend(function.proof_plan.iter().map(plan_key));
    }
    for lock in &metadata.locks {
        local_keys.extend(lock.proof_plan.iter().map(plan_key));
    }

    for key in &local_keys {
        if !runtime_keys.contains(key) {
            let (origin, feature, status) = split_plan_key(key);
            push_issue(
                issues,
                "error",
                "PP0401",
                origin,
                feature,
                &format!("local ProofPlan record with status '{}' is missing from runtime.proof_plan", status),
            );
        }
    }

    for key in runtime_keys {
        let (origin, feature, status) = split_plan_key(&key);
        if origin.starts_with("invariant:") {
            continue;
        }
        if !local_keys.contains(&key) {
            push_issue(
                issues,
                "error",
                "PP0402",
                origin,
                feature,
                &format!("runtime ProofPlan record with status '{}' is missing from local action/function/lock metadata", status),
            );
        }
    }
}

fn obligation_key(category: &str, feature: &str, status: &str) -> String {
    format!("{category}\u{1f}{feature}\u{1f}{status}")
}

fn plan_key(plan: &ProofPlanMetadata) -> String {
    format!("{}\u{1f}{}\u{1f}{}", plan.origin, plan.feature, plan.status)
}

fn split_plan_key(key: &str) -> (&str, &str, &str) {
    let mut parts = key.split('\u{1f}');
    (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""))
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
    obligation_key(&obligation.category, &obligation.feature, &obligation.status)
}
