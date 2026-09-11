//! Evidence-grade validation for the 0.30 business-scenario inventory.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

pub const MANIFEST: &str = "tests/fixtures/business_scenario_evidence.json";
const SCHEMA: &str = "cellscript-business-scenario-evidence-v1";
const INVENTORY: &str = "tests/fixtures/business_transaction_inventory.json";
const INVENTORY_SCHEMA: &str = "cellscript-business-transaction-inventory-v1";
const REQUIRED_FAMILIES: [&str; 8] = [
    "authorization",
    "committed_state",
    "external_verifier",
    "fungible_asset",
    "multi_script_composition",
    "nft_dob",
    "order_amm",
    "temporal",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransactionInventory {
    schema: String,
    serialization: String,
    families: BTreeMap<String, InventoryFamily>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InventoryFamily {
    positive: Vec<String>,
    adversarial: Vec<String>,
    fixture_owners: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioEvidence {
    schema: String,
    status: String,
    transaction_inventory: String,
    claim: String,
    reference_reproducibility: Reproducibility,
    families: BTreeMap<String, ScenarioFamilyEvidence>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reproducibility {
    status: String,
    blockers: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioFamilyEvidence {
    coverage_status: String,
    records: Vec<ScenarioRecord>,
    gaps: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioRecord {
    scenario: String,
    outcome: String,
    status: String,
    fixture: String,
    evidence: Vec<String>,
    raw_transaction_hash: Option<String>,
    serialized_transaction_hash: Option<String>,
    artifact_hashes: Vec<String>,
}

pub fn load(root: &Path) -> Result<ScenarioEvidence> {
    let bytes = fs::read(root.join(MANIFEST)).with_context(|| format!("failed to read {MANIFEST}"))?;
    serde_json::from_slice(&bytes).with_context(|| format!("invalid {MANIFEST}"))
}

fn load_inventory(root: &Path, evidence: &ScenarioEvidence) -> Result<TransactionInventory> {
    if evidence.transaction_inventory != INVENTORY {
        bail!("business scenario evidence transaction inventory must be {INVENTORY}");
    }
    let bytes = fs::read(root.join(INVENTORY)).with_context(|| format!("failed to read {INVENTORY}"))?;
    serde_json::from_slice(&bytes).with_context(|| format!("invalid {INVENTORY}"))
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

fn validate_hash(value: &str, label: &str) -> Result<()> {
    let Some(hex) = value.strip_prefix("0x") else {
        bail!("{label} must use a 0x-prefixed 32-byte hash");
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("{label} must use a 0x-prefixed 32-byte hash");
    }
    Ok(())
}

fn validate_path(root: &Path, relative: &str) -> Result<()> {
    let path = Path::new(relative);
    if path.is_absolute() || relative.contains('\\') || path.components().any(|component| !matches!(component, Component::Normal(_))) {
        bail!("business scenario evidence path is not normalized and repository-relative: {relative}");
    }
    let metadata =
        fs::symlink_metadata(root.join(path)).with_context(|| format!("business scenario evidence path is missing: {relative}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!("business scenario evidence path must be a regular non-symlink file: {relative}");
    }
    Ok(())
}

pub fn validate(root: &Path, evidence: &ScenarioEvidence, release: bool) -> Result<()> {
    if evidence.schema != SCHEMA || !matches!(evidence.status.as_str(), "candidate" | "accepted") || evidence.claim.trim().is_empty() {
        bail!("business scenario evidence must use the v1 schema, candidate or accepted status, and an explicit claim");
    }
    let inventory = load_inventory(root, evidence)?;
    if inventory.schema != INVENTORY_SCHEMA || inventory.serialization.trim().is_empty() {
        bail!("business transaction inventory has an invalid schema or serialization contract");
    }
    if !matches!(evidence.reference_reproducibility.status.as_str(), "reproducible" | "blocked") {
        bail!("business scenario reference reproducibility has an invalid status");
    }
    if evidence.reference_reproducibility.status == "blocked" {
        validate_nonempty(&evidence.reference_reproducibility.blockers, "business scenario reproducibility blockers")?;
    }
    if release && evidence.reference_reproducibility.status != "reproducible" {
        bail!("release business scenario references are not reproducible");
    }

    let required = REQUIRED_FAMILIES.into_iter().collect::<BTreeSet<_>>();
    if inventory.families.keys().map(String::as_str).collect::<BTreeSet<_>>() != required
        || evidence.families.keys().map(String::as_str).collect::<BTreeSet<_>>() != required
    {
        bail!("business transaction inventory and scenario evidence must contain every required family");
    }

    for (family_id, family) in &inventory.families {
        validate_nonempty(&family.positive, &format!("{family_id} positive scenarios"))?;
        validate_nonempty(&family.adversarial, &format!("{family_id} adversarial scenarios"))?;
        validate_nonempty(&family.fixture_owners, &format!("{family_id} fixture owners"))?;
        for owner in &family.fixture_owners {
            validate_path(root, owner)?;
        }
        let family_evidence = &evidence.families[family_id];
        if !matches!(
            family_evidence.coverage_status.as_str(),
            "owner-test-inventory-only" | "partial-transaction-hashes" | "exact-artifact-fixtures"
        ) {
            bail!("business scenario family {family_id} has an invalid coverage status");
        }
        if family_evidence.coverage_status != "exact-artifact-fixtures" {
            validate_nonempty(&family_evidence.gaps, &format!("business scenario family {family_id} gaps"))?;
        }

        let positive = family.positive.iter().map(String::as_str).collect::<BTreeSet<_>>();
        let adversarial = family.adversarial.iter().map(String::as_str).collect::<BTreeSet<_>>();
        let mut recorded = BTreeSet::new();
        let mut exact = BTreeSet::new();
        for record in &family_evidence.records {
            if !recorded.insert((record.outcome.as_str(), record.scenario.as_str())) {
                bail!("business scenario family {family_id} contains a duplicate evidence record");
            }
            let known = match record.outcome.as_str() {
                "positive" => positive.contains(record.scenario.as_str()),
                "adversarial" => adversarial.contains(record.scenario.as_str()),
                _ => false,
            };
            if !known {
                bail!("business scenario family {family_id} record {} is not in its {} inventory", record.scenario, record.outcome);
            }
            if !matches!(record.status.as_str(), "owner-test-mapped" | "transaction-hashes-pinned" | "exact-artifact-fixture") {
                bail!("business scenario family {family_id} record {} has an invalid status", record.scenario);
            }
            validate_path(root, &record.fixture)?;
            validate_nonempty(&record.evidence, &format!("business scenario {} evidence", record.scenario))?;
            for path in &record.evidence {
                validate_path(root, path)?;
            }
            if matches!(record.status.as_str(), "transaction-hashes-pinned" | "exact-artifact-fixture") {
                validate_hash(
                    record.raw_transaction_hash.as_deref().unwrap_or_default(),
                    &format!("business scenario {} raw transaction hash", record.scenario),
                )?;
                validate_hash(
                    record.serialized_transaction_hash.as_deref().unwrap_or_default(),
                    &format!("business scenario {} serialized transaction hash", record.scenario),
                )?;
            }
            if record.status == "exact-artifact-fixture" {
                validate_nonempty(&record.artifact_hashes, &format!("business scenario {} artifact hashes", record.scenario))?;
                for hash in &record.artifact_hashes {
                    validate_hash(hash, &format!("business scenario {} artifact hash", record.scenario))?;
                }
                exact.insert((record.outcome.as_str(), record.scenario.as_str()));
            } else if !record.artifact_hashes.is_empty() {
                bail!("business scenario {} must not attach artifact hashes below exact-artifact-fixture status", record.scenario);
            }
        }

        let required_records = positive
            .iter()
            .map(|scenario| ("positive", *scenario))
            .chain(adversarial.iter().map(|scenario| ("adversarial", *scenario)))
            .collect::<BTreeSet<_>>();
        if family_evidence.coverage_status == "exact-artifact-fixtures" && exact != required_records {
            bail!("business scenario family {family_id} claims exact coverage without an exact artifact fixture for every scenario");
        }
        if release && family_evidence.coverage_status != "exact-artifact-fixtures" {
            bail!("release business scenario family {family_id} is only {}", family_evidence.coverage_status);
        }
    }
    if release && evidence.status != "accepted" {
        bail!("release business scenario evidence status must be accepted");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence() -> ScenarioEvidence {
        serde_json::from_str(include_str!("../../../tests/fixtures/business_scenario_evidence.json"))
            .expect("checked-in business scenario evidence")
    }

    #[test]
    fn candidate_scenario_evidence_is_honest_and_release_rejects_its_gaps() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let evidence = evidence();
        validate(&root, &evidence, false).expect("honest candidate evidence");
        assert!(validate(&root, &evidence, true).unwrap_err().to_string().contains("not reproducible"));
    }

    #[test]
    fn scenario_evidence_rejects_unknown_rows_and_false_exact_coverage() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut unknown = evidence();
        unknown.families.get_mut("nft_dob").expect("NFT family").records.push(ScenarioRecord {
            scenario: "not-in-the-frozen-inventory".to_string(),
            outcome: "positive".to_string(),
            status: "owner-test-mapped".to_string(),
            fixture: "tests/examples.rs".to_string(),
            evidence: vec!["tests/examples.rs".to_string()],
            raw_transaction_hash: None,
            serialized_transaction_hash: None,
            artifact_hashes: Vec::new(),
        });
        assert!(validate(&root, &unknown, false).unwrap_err().to_string().contains("not in its positive inventory"));

        let mut false_exact = evidence();
        false_exact.families.get_mut("order_amm").expect("order/AMM family").records.retain(|record| record.scenario != "wrong_price");
        assert!(validate(&root, &false_exact, false).unwrap_err().to_string().contains("without an exact artifact fixture"));
    }
}
