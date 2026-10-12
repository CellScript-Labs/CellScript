//! Bridge to the existing package deployment record; no network I/O or mutation.
use super::*;
use cellscript::package::{DeployedManifest, DeploymentStatus};
use ckb_types::{packed, prelude::*};

/// SHA-256 of the canonical JSON projection of a parsed Deployed.toml record.
/// This is deliberately distinct from the hash of its original TOML formatting.
pub fn deployment_record_digest(record: &DeployedManifest) -> Result<String> {
    ensure!(record.deployments.len() <= 64, "ZK lifecycle deployment record cardinality");
    record.validate_schema().map_err(|e| anyhow::anyhow!("ZK lifecycle deployment schema: {e}"))?;
    let bytes = serde_json::to_vec(&serde_json::to_value(record)?)?;
    ensure!(bytes.len() <= 262_144, "ZK lifecycle deployment record byte budget");
    Ok(sha256(&bytes))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeploymentUse {
    /// Discovery status does not revoke the rules of existing on-chain Cells.
    ExistingSpend,
    /// Newly selected deployments must have an explicit active record.
    Candidate,
}

impl CheckedManifest {
    /// Check a parsed existing Deployed.toml against this externally pinned
    /// lifecycle manifest and an observed genesis identity. Callers obtain the
    /// observation from their trusted chain provider; this is not liveness proof.
    pub fn check_deployment_record(&self, record: &DeployedManifest, observed_genesis: [u8; 32], usage: DeploymentUse) -> Result<()> {
        let m = &self.manifest;
        ensure!(
            deployment_record_digest(record)? == m.package.deployed_manifest_sha256,
            "ZK lifecycle deployed record differs from pinned digest"
        );
        ensure!(hex::encode(observed_genesis) == m.deployment.genesis_hash, "ZK lifecycle observed genesis mismatch");
        let point = packed::OutPoint::from_slice(&hex::decode(&m.deployment.code_out_point)?)?;
        let candidates: Vec<_> = record
            .deployments
            .iter()
            .filter(|r| {
                r.chain_id == m.deployment.chain_id
                    && crate::hash32(&r.tx_hash, "deployment tx hash").is_ok_and(|h| h.as_slice() == point.tx_hash().as_slice())
                    && r.output_index == Unpack::<u32>::unpack(&point.index())
            })
            .collect();
        let [selected] = candidates.as_slice() else {
            anyhow::bail!("ZK lifecycle missing or ambiguous deployed parent record");
        };
        let parent = crate::hash32(&m.deployment.parent_data_hash, "parent data hash")?;
        ensure!(
            crate::hash32(&selected.data_hash, "deployment data hash")? == parent
                && crate::hash32(&selected.code_hash, "deployment code hash")? == parent
                && selected.dep_type == "code"
                && matches!(selected.hash_type.as_str(), "data" | "data1" | "data2"),
            "ZK lifecycle deployed parent/code identity mismatch"
        );
        let expected_point = format!("0x{}:{}", hex::encode(point.tx_hash().as_slice()), Unpack::<u32>::unpack(&point.index()));
        ensure!(selected.out_point == expected_point, "ZK lifecycle redundant deployment OutPoint mismatch");
        if let Some(artifact) = &selected.artifact_hash {
            ensure!(crate::hash32(artifact, "deployment artifact hash")? == parent, "ZK lifecycle deployed artifact mismatch");
        }
        if let Some(build) = &record.build
            && let Some(artifact) = &build.artifact_hash
        {
            ensure!(crate::hash32(artifact, "deployment build hash")? == parent, "ZK lifecycle deployed build mismatch");
        }
        if usage == DeploymentUse::Candidate {
            ensure!(selected.status == Some(DeploymentStatus::Active), "ZK lifecycle candidate deployment is not active");
        }
        Ok(())
    }
}
