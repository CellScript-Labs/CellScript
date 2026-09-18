//! Offline validation for the selected-network CellScript 0.30 deployment evidence.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

const MANIFEST: &str = "tests/fixtures/cellscript_0_30_pudge_deployment_manifest.json";
const REPORT: &str = "tests/fixtures/cellscript_0_30_pudge_deployment.json";
const MANIFEST_SCHEMA: &str = "cellscript-pudge-release-deployment-manifest-v1";
const BATCH_SCHEMA: &str = "cellscript-pudge-release-deployment-batch-v1";
const REPORT_SCHEMA: &str = "cellscript-pudge-release-deployment-v1";
const PUDGE_GENESIS: &str = "0x10639e0895502b5688a6be8cf69460d76541bfa4821629d86d62ba0aae3f9606";
const PUDGE_SECP_LOCK: &str = "0x9bd7e06f3ecf4be0f2fcd2188b23f1b9fcc88e5d4b65a8637b17723bbda3cce8";
const ARTIFACT_SOURCE_COMMIT: &str = "3cf60a105c80a33016b8280c990a479657c3bc13";
const SHANNONS_PER_BYTE: u64 = 100_000_000;
const CODE_CELL_BASE_BYTES: u64 = 61;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Network {
    name: String,
    chain_id: String,
    genesis_hash: String,
    rpc_url: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ArtifactSource {
    kind: String,
    path: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Artifact {
    id: String,
    purpose: String,
    source: ArtifactSource,
    bytes: u64,
    sha256: String,
    ckb_data_hash: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ManifestBatch {
    id: String,
    purpose: String,
    artifacts: Vec<Artifact>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeploymentManifest {
    schema: String,
    status: String,
    release_line: String,
    artifact_source_commit: String,
    network: Network,
    required_confirmations: u64,
    fee_rate_shannons_per_kb: u64,
    deployment_scope: String,
    batches: Vec<ManifestBatch>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct LockScript {
    code_hash: String,
    hash_type: String,
    args: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportNetwork {
    name: String,
    chain_id: String,
    genesis_hash: String,
    rpc_url: String,
    tip_at_verification: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeploymentAuthority {
    address: String,
    lock: LockScript,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransactionEvidence {
    tx_hash: String,
    full_hash: String,
    block_hash: String,
    block_number: String,
    required_confirmations: u64,
    inputs: u64,
    outputs: u64,
    code_outputs: u64,
    added_capacity_inputs: u64,
    fee_completion: (u64, bool),
    fee_rate_shannons_per_kb: u64,
    fee_shannons: String,
    serialized_bytes: u64,
    dry_run_cycles: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BalanceEvidence {
    before_shannons: String,
    after_shannons: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OutPoint {
    tx_hash: String,
    index: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeployedArtifact {
    id: String,
    purpose: String,
    source: ArtifactSource,
    bytes: u64,
    sha256: String,
    ckb_data_hash: String,
    out_point: OutPoint,
    capacity_shannons: String,
    lock: LockScript,
    dep_type: String,
    hash_type: String,
    live: bool,
    bytes_match: bool,
}

impl DeployedArtifact {
    fn manifest_artifact(&self) -> Artifact {
        Artifact {
            id: self.id.clone(),
            purpose: self.purpose.clone(),
            source: self.source.clone(),
            bytes: self.bytes,
            sha256: self.sha256.clone(),
            ckb_data_hash: self.ckb_data_hash.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportBatchIdentity {
    id: String,
    purpose: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchReport {
    schema: String,
    status: String,
    release_line: String,
    artifact_source_commit: String,
    batch: ReportBatchIdentity,
    network: ReportNetwork,
    deployment_authority: DeploymentAuthority,
    transaction: TransactionEvidence,
    balance: BalanceEvidence,
    artifacts: Vec<DeployedArtifact>,
    generated_at_utc: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeploymentReport {
    schema: String,
    status: String,
    release_line: String,
    artifact_source_commit: String,
    network: Network,
    required_confirmations: u64,
    deployment_scope: String,
    batches: Vec<BatchReport>,
    artifact_count: u64,
    transaction_count: u64,
    generated_at_utc: String,
}

fn canonical_hex(value: &str, bytes: usize) -> bool {
    value.len() == 2 + bytes * 2
        && value.starts_with("0x")
        && value[2..].bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn canonical_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn parse_decimal(value: &str, label: &str) -> Result<u64> {
    if value.is_empty() || (value.len() > 1 && value.starts_with('0')) || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        bail!("{label} must be canonical unsigned decimal");
    }
    value.parse().with_context(|| format!("{label} exceeds u64"))
}

fn normalized_relative_path(value: &str) -> bool {
    let path = Path::new(value);
    !path.is_absolute() && !value.contains('\\') && path.components().all(|component| matches!(component, Component::Normal(_)))
}

fn validate_network(network: &Network) -> Result<()> {
    if network.name != "pudge"
        || network.chain_id != "ckb_testnet"
        || network.genesis_hash != PUDGE_GENESIS
        || network.rpc_url != "https://testnet.ckb.dev"
    {
        bail!("deployment evidence must bind the selected Pudge network identity");
    }
    Ok(())
}

fn validate_manifest(manifest: &DeploymentManifest) -> Result<()> {
    if manifest.schema != MANIFEST_SCHEMA
        || manifest.status != "accepted-deployment-scope"
        || manifest.release_line != "0.30"
        || manifest.artifact_source_commit != ARTIFACT_SOURCE_COMMIT
    {
        bail!("invalid CellScript 0.30 Pudge deployment manifest identity");
    }
    validate_network(&manifest.network)?;
    if manifest.required_confirmations < 4 || manifest.fee_rate_shannons_per_kb < 1_000 || manifest.deployment_scope.trim().is_empty()
    {
        bail!("Pudge deployment manifest has an unsafe confirmation, fee, or scope policy");
    }
    let required_batches = ["business-product", "consensus-security"].into_iter().collect::<BTreeSet<_>>();
    let batches = manifest.batches.iter().map(|batch| batch.id.as_str()).collect::<BTreeSet<_>>();
    if manifest.batches.len() != 2 || batches != required_batches {
        bail!("Pudge deployment manifest must contain the two accepted release batches exactly once");
    }
    let mut artifacts = BTreeSet::new();
    for batch in &manifest.batches {
        if batch.purpose.trim().is_empty() || batch.artifacts.is_empty() {
            bail!("Pudge deployment batch {} has an empty purpose or artifact set", batch.id);
        }
        for artifact in &batch.artifacts {
            if artifact.id.is_empty()
                || !artifacts.insert(artifact.id.as_str())
                || artifact.purpose.trim().is_empty()
                || !matches!(artifact.source.kind.as_str(), "repository" | "acceptance-report")
                || !normalized_relative_path(&artifact.source.path)
                || artifact.bytes == 0
                || !canonical_sha256(&artifact.sha256)
                || !canonical_hex(&artifact.ckb_data_hash, 32)
            {
                bail!("Pudge deployment manifest contains an invalid artifact {}", artifact.id);
            }
        }
    }
    if artifacts.len() != 12 {
        bail!("Pudge deployment manifest must contain the accepted twelve-artifact scope");
    }
    Ok(())
}

fn validate_report(manifest: &DeploymentManifest, report: &DeploymentReport) -> Result<()> {
    if report.schema != REPORT_SCHEMA
        || report.status != "passed"
        || report.release_line != manifest.release_line
        || report.artifact_source_commit != manifest.artifact_source_commit
        || report.network != manifest.network
        || report.required_confirmations != manifest.required_confirmations
        || report.deployment_scope != manifest.deployment_scope
        || report.transaction_count != manifest.batches.len() as u64
        || report.artifact_count != manifest.batches.iter().map(|batch| batch.artifacts.len() as u64).sum::<u64>()
        || report.generated_at_utc.trim().is_empty()
    {
        bail!("Pudge deployment report does not match the accepted release manifest");
    }

    let mut transaction_hashes = BTreeSet::new();
    let mut reported_batches = BTreeSet::new();
    for batch_report in &report.batches {
        let manifest_batch = manifest
            .batches
            .iter()
            .find(|batch| batch.id == batch_report.batch.id)
            .with_context(|| format!("unexpected Pudge deployment batch {}", batch_report.batch.id))?;
        if !reported_batches.insert(batch_report.batch.id.as_str()) {
            bail!("duplicate Pudge deployment batch {}", batch_report.batch.id);
        }
        if batch_report.schema != BATCH_SCHEMA
            || batch_report.status != "passed"
            || batch_report.release_line != manifest.release_line
            || batch_report.artifact_source_commit != manifest.artifact_source_commit
            || batch_report.batch.purpose != manifest_batch.purpose
            || batch_report.generated_at_utc.trim().is_empty()
        {
            bail!("Pudge deployment batch {} has stale identity", batch_report.batch.id);
        }
        let network = &batch_report.network;
        let tip = parse_decimal(&network.tip_at_verification, "tip_at_verification")?;
        if network.name != manifest.network.name
            || network.chain_id != manifest.network.chain_id
            || network.genesis_hash != manifest.network.genesis_hash
            || network.rpc_url != manifest.network.rpc_url
            || tip == 0
        {
            bail!("Pudge deployment batch {} has stale network evidence", batch_report.batch.id);
        }
        let authority = &batch_report.deployment_authority;
        if !authority.address.starts_with("ckt1")
            || authority.lock.code_hash != PUDGE_SECP_LOCK
            || authority.lock.hash_type != "type"
            || !canonical_hex(&authority.lock.args, 20)
        {
            bail!("Pudge deployment batch {} has an invalid deployment authority", batch_report.batch.id);
        }
        let transaction = &batch_report.transaction;
        let block_number = parse_decimal(&transaction.block_number, "block_number")?;
        if !canonical_hex(&transaction.tx_hash, 32)
            || !transaction_hashes.insert(transaction.tx_hash.as_str())
            || !canonical_hex(&transaction.full_hash, 32)
            || !canonical_hex(&transaction.block_hash, 32)
            || block_number == 0
            || tip < block_number
            || tip - block_number + 1 < manifest.required_confirmations
            || transaction.required_confirmations != manifest.required_confirmations
            || transaction.inputs == 0
            || transaction.outputs != manifest_batch.artifacts.len() as u64 + 1
            || transaction.code_outputs != manifest_batch.artifacts.len() as u64
            || transaction.added_capacity_inputs != transaction.inputs
            || transaction.fee_completion != (0, true)
            || transaction.fee_rate_shannons_per_kb != manifest.fee_rate_shannons_per_kb
            || parse_decimal(&transaction.fee_shannons, "fee_shannons")? == 0
            || transaction.serialized_bytes == 0
            || parse_decimal(&transaction.dry_run_cycles, "dry_run_cycles")? == 0
        {
            bail!("Pudge deployment batch {} has incomplete transaction evidence", batch_report.batch.id);
        }
        if batch_report.artifacts.len() != manifest_batch.artifacts.len() {
            bail!("Pudge deployment batch {} has an incomplete artifact report", batch_report.batch.id);
        }
        let mut deployed_capacity = 0u64;
        for (index, (expected, actual)) in manifest_batch.artifacts.iter().zip(&batch_report.artifacts).enumerate() {
            let capacity = parse_decimal(&actual.capacity_shannons, "capacity_shannons")?;
            let minimum_capacity = expected
                .bytes
                .checked_add(CODE_CELL_BASE_BYTES)
                .and_then(|bytes| bytes.checked_mul(SHANNONS_PER_BYTE))
                .context("code Cell occupied capacity overflow")?;
            if actual.manifest_artifact() != *expected
                || actual.out_point.tx_hash != transaction.tx_hash
                || actual.out_point.index != index as u64
                || capacity < minimum_capacity
                || actual.lock != authority.lock
                || actual.dep_type != "code"
                || actual.hash_type != "data2"
                || !actual.live
                || !actual.bytes_match
            {
                bail!("Pudge live Cell evidence mismatch for artifact {}", expected.id);
            }
            deployed_capacity = deployed_capacity.checked_add(capacity).context("deployed capacity overflow")?;
        }
        let before = parse_decimal(&batch_report.balance.before_shannons, "balance.before_shannons")?;
        let after = parse_decimal(&batch_report.balance.after_shannons, "balance.after_shannons")?;
        let fee = parse_decimal(&transaction.fee_shannons, "fee_shannons")?;
        if before <= after || before - after != deployed_capacity.checked_add(fee).context("deployment cost overflow")? {
            bail!("Pudge deployment batch {} has inconsistent capacity accounting", batch_report.batch.id);
        }
    }
    if report.batches.len() != manifest.batches.len() || reported_batches.len() != manifest.batches.len() {
        bail!("Pudge deployment report omits an accepted batch");
    }
    Ok(())
}

pub fn validate(root: &Path) -> Result<()> {
    let manifest: DeploymentManifest =
        serde_json::from_slice(&fs::read(root.join(MANIFEST)).with_context(|| format!("failed to read {MANIFEST}"))?)
            .with_context(|| format!("invalid {MANIFEST}"))?;
    let report: DeploymentReport =
        serde_json::from_slice(&fs::read(root.join(REPORT)).with_context(|| format!("failed to read {REPORT}"))?)
            .with_context(|| format!("invalid {REPORT}"))?;
    validate_manifest(&manifest)?;
    validate_report(&manifest, &report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> DeploymentManifest {
        serde_json::from_str(include_str!("../../../tests/fixtures/cellscript_0_30_pudge_deployment_manifest.json"))
            .expect("checked-in Pudge deployment manifest")
    }

    fn report() -> DeploymentReport {
        serde_json::from_str(include_str!("../../../tests/fixtures/cellscript_0_30_pudge_deployment.json"))
            .expect("checked-in Pudge deployment report")
    }

    #[test]
    fn selected_network_deployment_evidence_is_complete() {
        let manifest = manifest();
        validate_manifest(&manifest).expect("accepted Pudge deployment manifest");
        validate_report(&manifest, &report()).expect("complete Pudge deployment report");
    }

    #[test]
    fn deployment_report_rejects_scope_and_live_cell_mutations() {
        let manifest = manifest();
        let mut stale_count = report();
        stale_count.artifact_count -= 1;
        assert!(validate_report(&manifest, &stale_count).unwrap_err().to_string().contains("does not match"));

        let mut spent = report();
        spent.batches[0].artifacts[0].live = false;
        assert!(validate_report(&manifest, &spent).unwrap_err().to_string().contains("live Cell evidence mismatch"));
    }
}
