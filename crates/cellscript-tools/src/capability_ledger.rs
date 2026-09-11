//! Validation for the versioned 0.30 product-capability ledger.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

pub const MANIFEST: &str = "tests/fixtures/capability_ledger.json";
const SCHEMA: &str = "cellscript-capability-ledger-v1";
const DOCUMENTATION: &str = "docs/CELLSCRIPT_0_30_CAPABILITY_LEDGER.md";
const REQUIRED_RELEASE_GATES: [&str; 4] = ["independent_review", "release", "repository_gates", "selected_network_evidence"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityLedger {
    schema: String,
    release_line: String,
    status: String,
    claim: String,
    documentation: String,
    base_commit: String,
    entries: Vec<CapabilityEntry>,
    release_requirements: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityEntry {
    issue: u64,
    issue_url: String,
    title: String,
    capability_id: String,
    category: String,
    priority: String,
    admission_state: String,
    on_chain_status: String,
    builder_status: String,
    evidence_status: String,
    interface_status: String,
    owner: String,
    reviewer: String,
    dependencies: Vec<u64>,
    release_scope: String,
    release_eligibility: String,
    issue_disposition: String,
    design_records: Vec<String>,
    evidence: Vec<String>,
    non_goals: Vec<String>,
    remaining_work: Vec<String>,
}

pub fn load(root: &Path) -> Result<CapabilityLedger> {
    let bytes = fs::read(root.join(MANIFEST)).with_context(|| format!("failed to read {MANIFEST}"))?;
    serde_json::from_slice(&bytes).with_context(|| format!("invalid {MANIFEST}"))
}

fn validate_nonempty(values: &[String], label: &str) -> Result<()> {
    if values.is_empty() || values.iter().any(|value| value.trim().is_empty()) {
        bail!("{label} must contain nonempty entries");
    }
    if values.iter().collect::<BTreeSet<_>>().len() != values.len() {
        bail!("{label} contains duplicates");
    }
    Ok(())
}

fn validate_local_path(root: &Path, relative: &str) -> Result<()> {
    let path = Path::new(relative);
    if path.is_absolute() || relative.contains('\\') || path.components().any(|component| !matches!(component, Component::Normal(_))) {
        bail!("capability-ledger path is not normalized and repository-relative: {relative}");
    }
    let metadata = fs::symlink_metadata(root.join(path)).with_context(|| format!("capability-ledger link is broken: {relative}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!("capability-ledger path must be a regular non-symlink file: {relative}");
    }
    Ok(())
}

pub fn validate(root: &Path, ledger: &CapabilityLedger, release: bool) -> Result<()> {
    if ledger.schema != SCHEMA {
        bail!("capability ledger schema must be {SCHEMA}");
    }
    if ledger.release_line != "0.30" || !matches!(ledger.status.as_str(), "candidate" | "accepted") {
        bail!("capability ledger must identify the 0.30 line and candidate or accepted status");
    }
    if ledger.claim.trim().is_empty() || ledger.documentation != DOCUMENTATION {
        bail!("capability ledger claim and canonical documentation must be explicit");
    }
    if ledger.base_commit.len() != 40 || !ledger.base_commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("capability ledger base_commit must be a 40-character hexadecimal Git identity");
    }

    let expected_issues = (7u64..=27).collect::<BTreeSet<_>>();
    let actual_issues = ledger.entries.iter().map(|entry| entry.issue).collect::<BTreeSet<_>>();
    if actual_issues != expected_issues || ledger.entries.len() != expected_issues.len() {
        bail!("capability ledger must contain GitHub issues #7 through #27 exactly once");
    }
    let capability_ids = ledger.entries.iter().map(|entry| entry.capability_id.as_str()).collect::<BTreeSet<_>>();
    if capability_ids.len() != ledger.entries.len() {
        bail!("capability ledger capability IDs must be unique");
    }

    for entry in &ledger.entries {
        let context = format!("capability ledger issue #{}", entry.issue);
        let expected_url = format!("https://github.com/CellScript-Labs/CellScript/issues/{}", entry.issue);
        if entry.issue_url != expected_url {
            bail!("{context} has a broken or noncanonical issue link");
        }
        if entry.title.trim().is_empty()
            || entry.capability_id.trim().is_empty()
            || entry.owner.trim().is_empty()
            || entry.reviewer.trim().is_empty()
        {
            bail!("{context} must name its title, capability ID, owner, and reviewer state");
        }
        if !matches!(
            entry.category.as_str(),
            "language" | "consensus-runtime" | "artifact" | "builder" | "tooling" | "assurance" | "ecosystem" | "product"
        ) {
            bail!("{context} has an invalid category");
        }
        if !matches!(entry.priority.as_str(), "P0" | "P1" | "P2" | "research") {
            bail!("{context} has an invalid priority");
        }
        if !matches!(entry.admission_state.as_str(), "absent" | "research" | "reserved" | "shape-gated" | "bounded" | "complete") {
            bail!("{context} has an invalid admission state");
        }
        if !matches!(entry.on_chain_status.as_str(), "none" | "metadata-only" | "fail-closed" | "executable") {
            bail!("{context} has an invalid on-chain status");
        }
        if !matches!(entry.builder_status.as_str(), "none" | "scaffold" | "checked" | "complete-transaction-path") {
            bail!("{context} has an invalid builder status");
        }
        if !matches!(entry.evidence_status.as_str(), "none" | "simulator" | "ckb-vm" | "stateful" | "chain") {
            bail!("{context} has an invalid evidence status");
        }
        if !matches!(entry.interface_status.as_str(), "absent" | "private" | "package" | "public-versioned") {
            bail!("{context} has an invalid interface status");
        }
        if !matches!(entry.release_scope.as_str(), "required" | "non-blocking" | "deferred")
            || !matches!(entry.release_eligibility.as_str(), "experimental" | "candidate" | "stable")
            || !matches!(entry.issue_disposition.as_str(), "keep-open" | "close-after-merge" | "split-and-close" | "deferred")
        {
            bail!("{context} has an invalid release classification");
        }
        validate_nonempty(&entry.design_records, &format!("{context} design records"))?;
        validate_nonempty(&entry.non_goals, &format!("{context} non-goals"))?;
        for relative in entry.design_records.iter().chain(&entry.evidence) {
            validate_local_path(root, relative)?;
        }
        if matches!(entry.admission_state.as_str(), "absent" | "research")
            && (entry.on_chain_status == "executable"
                || matches!(entry.builder_status.as_str(), "checked" | "complete-transaction-path")
                || entry.release_eligibility == "stable")
        {
            bail!("{context} advertises an absent or research capability as implemented");
        }
        if matches!(entry.issue_disposition.as_str(), "keep-open" | "split-and-close") && entry.remaining_work.is_empty() {
            bail!("{context} must state the work that remains before closure or follow-up split");
        }
        if entry.dependencies.iter().any(|dependency| !expected_issues.contains(dependency) || *dependency == entry.issue) {
            bail!("{context} references an invalid dependency issue");
        }
        if release && entry.release_scope == "required" {
            let review_waived = ledger.release_requirements.get("independent_review").map(String::as_str) == Some("waived");
            if entry.release_eligibility != "stable"
                || entry.issue_disposition == "keep-open"
                || (entry.reviewer == "unassigned" && !review_waived)
            {
                bail!("release capability ledger is incomplete at issue #{}", entry.issue);
            }
        }
    }

    let release_gates = ledger.release_requirements.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if release_gates != REQUIRED_RELEASE_GATES.into_iter().collect() {
        bail!("capability ledger must classify every release requirement");
    }
    for (gate, status) in &ledger.release_requirements {
        if !matches!(status.as_str(), "passed" | "pending" | "not-authorized" | "waived")
            || (status == "waived" && gate != "independent_review")
        {
            bail!("capability ledger release requirement {gate} has an invalid status");
        }
        if release && status != "passed" && !(gate == "independent_review" && status == "waived") {
            bail!("release capability ledger is incomplete: {gate} is {status}");
        }
    }
    if release && ledger.status != "accepted" {
        bail!("release capability ledger status must be accepted");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger() -> CapabilityLedger {
        serde_json::from_str(include_str!("../../../tests/fixtures/capability_ledger.json")).expect("checked-in capability ledger")
    }

    #[test]
    fn ledger_rejects_invalid_states_broken_issue_links_and_false_research_claims() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

        let mut invalid_state = ledger();
        invalid_state.entries[0].admission_state = "done-ish".to_string();
        assert!(validate(&root, &invalid_state, false).unwrap_err().to_string().contains("invalid admission state"));

        let mut broken_link = ledger();
        broken_link.entries[0].issue_url = "https://example.invalid/7".to_string();
        assert!(validate(&root, &broken_link, false).unwrap_err().to_string().contains("noncanonical issue link"));

        let mut false_claim = ledger();
        let research = false_claim.entries.iter_mut().find(|entry| entry.issue == 22).expect("research entry");
        research.on_chain_status = "executable".to_string();
        assert!(validate(&root, &false_claim, false).unwrap_err().to_string().contains("advertises"));

        let mut invalid_waiver = ledger();
        invalid_waiver.release_requirements.insert("release".to_string(), "waived".to_string());
        assert!(validate(&root, &invalid_waiver, false).unwrap_err().to_string().contains("invalid status"));
    }

    #[test]
    fn candidate_ledger_is_valid_for_development_and_rejected_for_release() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let ledger = ledger();
        validate(&root, &ledger, false).expect("candidate ledger");
        assert!(validate(&root, &ledger, true).unwrap_err().to_string().contains("incomplete"));
    }
}
