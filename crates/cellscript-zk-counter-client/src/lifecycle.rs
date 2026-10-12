//! Deterministic off-chain ZK change planning. A plan never authorizes a spend.
//! Manifest digests and discovery status must be obtained from the application's
//! trusted source; neither a Registry record nor this report is consensus evidence.
use anyhow::{ensure, Result};
use cellscript::package::upgrade::{self, UpgradePlan};
use cellscript_zk_private_counter::package::sha256;
use serde::{Deserialize, Serialize};

pub const MANIFEST_SCHEMA: &str = "cellscript-zk-lifecycle-v1";
pub const PLAN_SCHEMA: &str = "cellscript-zk-lifecycle-plan-v1";
pub const MAX_MANIFEST_BYTES: usize = 32_768;
mod deployment;
pub use deployment::{deployment_record_digest, DeploymentUse};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Circuit {
    pub source_sha256: String,
    pub relation_sha256: String,
    pub statement_sha256: String,
    pub codec_sha256: String,
    pub profile: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proving {
    pub key_sha256: String,
    pub executable_sha256: String,
    pub build_sha256: String,
    pub toolchain: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SetupTrust {
    PublicTestOnly,
    LocalSingleParty,
    Unreviewed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setup {
    pub manifest_sha256: String,
    /// None means there is no transcript, not a zero-valued transcript identity.
    #[serde(deserialize_with = "Option::deserialize")]
    pub transcript_sha256: Option<String>,
    pub trust: SetupTrust,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deployment {
    pub chain_id: String,
    pub genesis_hash: String,
    pub verifier_data_hash: String,
    pub parent_data_hash: String,
    pub code_out_point: String,
    /// Complete serialized application Script, not just its code hash/Type ID.
    pub application_script: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Active,
    Yanked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageBinding {
    pub member: String,
    pub lock_sha256: String,
    pub deployed_manifest_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: String,
    pub application: String,
    pub version: u64,
    pub status: Status,
    pub circuit: Circuit,
    pub proving: Proving,
    pub verification_key_sha256: String,
    pub setup: Setup,
    pub verifier_sha256: String,
    pub deployment: Deployment,
    pub package: PackageBinding,
}

/// Private fields prevent callers from manufacturing a checked identity. This
/// validates the trusted manifest's encoding; artifact verification is separate.
pub struct CheckedManifest {
    manifest: Manifest,
    bytes_sha256: String,
}

/// Build provenance supplied by the off-chain package/prover workflow. Its
/// commitment does not imply an independently reviewed build or setup.
pub struct ManifestContext<'a> {
    pub version: u64,
    pub status: Status,
    pub prover_executable: &'a [u8],
    pub prover_build_record: &'a [u8],
    pub package: PackageBinding,
}

fn digest(value: &str, field: &str) -> Result<()> {
    ensure!(
        value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "ZK lifecycle {field}: expected canonical lowercase SHA-256/hash bytes"
    );
    Ok(())
}

impl CheckedManifest {
    pub fn load(bytes: &[u8], trusted_sha256: &str) -> Result<Self> {
        ensure!(bytes.len() <= MAX_MANIFEST_BYTES, "ZK lifecycle manifest exceeds byte budget");
        digest(trusted_sha256, "trusted manifest")?;
        ensure!(sha256(bytes) == trusted_sha256, "ZK lifecycle manifest differs from trusted digest");
        let manifest: Manifest = serde_json::from_slice(bytes)?;
        ensure!(manifest.schema == MANIFEST_SCHEMA, "unsupported ZK lifecycle schema");
        ensure!(manifest.version <= 9_007_199_254_740_991, "ZK lifecycle version exceeds exact cross-client integer range");
        ensure!(!manifest.deployment.chain_id.is_empty() && manifest.deployment.chain_id.len() <= 128, "ZK lifecycle chain ID bounds");
        ensure!(
            !manifest.application.is_empty()
                && manifest.application.len() <= 128
                && !manifest.circuit.profile.is_empty()
                && manifest.circuit.profile.len() <= 128
                && !manifest.proving.toolchain.is_empty()
                && manifest.proving.toolchain.len() <= 128,
            "ZK lifecycle application/profile/toolchain exceeds text bounds"
        );
        for (name, value) in [
            ("source", &manifest.circuit.source_sha256),
            ("relation", &manifest.circuit.relation_sha256),
            ("statement", &manifest.circuit.statement_sha256),
            ("codec", &manifest.circuit.codec_sha256),
            ("PK", &manifest.proving.key_sha256),
            ("prover", &manifest.proving.executable_sha256),
            ("prover build", &manifest.proving.build_sha256),
            ("VK", &manifest.verification_key_sha256),
            ("setup", &manifest.setup.manifest_sha256),
            ("verifier", &manifest.verifier_sha256),
            ("genesis", &manifest.deployment.genesis_hash),
            ("verifier data", &manifest.deployment.verifier_data_hash),
            ("parent data", &manifest.deployment.parent_data_hash),
            ("package lock", &manifest.package.lock_sha256),
            ("deployed manifest", &manifest.package.deployed_manifest_sha256),
        ] {
            digest(value, name)?;
        }
        ensure!(!manifest.package.member.is_empty() && manifest.package.member.len() <= 256, "ZK lifecycle package member bounds");
        if let Some(transcript) = &manifest.setup.transcript_sha256 {
            digest(transcript, "setup transcript")?;
        }
        use ckb_types::{packed, prelude::*};
        let point = hex::decode(&manifest.deployment.code_out_point)?;
        let script = hex::decode(&manifest.deployment.application_script)?;
        ensure!(script.len() <= 4096, "ZK lifecycle Script exceeds byte budget");
        packed::OutPoint::from_slice(&point)?;
        let parsed_script = packed::Script::from_slice(&script)?;
        ensure!(matches!(parsed_script.hash_type().as_slice()[0], 0 | 1 | 2 | 4), "ZK lifecycle unsupported Script hash type");
        ensure!(hex::encode(point) == manifest.deployment.code_out_point, "noncanonical code OutPoint");
        ensure!(hex::encode(script) == manifest.deployment.application_script, "noncanonical application Script");
        Ok(Self { manifest, bytes_sha256: trusted_sha256.into() })
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Off-chain preflight only. Large PKs stay in the proving workflow and are
    /// not read by compilation or by change planning.
    pub fn check_runtime_artifacts(&self, vk: &[u8], verifier: &[u8], parent: &[u8], prover: &[u8]) -> Result<()> {
        ensure!(!vk.is_empty() && vk.len() <= 1_048_576, "ZK lifecycle VK byte budget");
        ensure!(!verifier.is_empty() && verifier.len() <= 4_194_304, "ZK lifecycle verifier byte budget");
        ensure!(!parent.is_empty() && parent.len() <= 4_194_304, "ZK lifecycle parent byte budget");
        ensure!(!prover.is_empty() && prover.len() <= 134_217_728, "ZK lifecycle prover byte budget");
        let m = &self.manifest;
        ensure!(sha256(vk) == m.verification_key_sha256, "ZK lifecycle VK substitution");
        ensure!(sha256(verifier) == m.verifier_sha256, "ZK lifecycle verifier substitution");
        ensure!(sha256(prover) == m.proving.executable_sha256, "ZK lifecycle prover substitution");
        ensure!(
            hex::encode(cellscript_zk_private_counter::hash(verifier)) == m.deployment.verifier_data_hash
                && hex::encode(cellscript_zk_private_counter::hash(parent)) == m.deployment.parent_data_hash,
            "ZK lifecycle deployed code substitution"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Change {
    Source,
    Proving,
    Statement,
    Codec,
    VerificationKey,
    CircuitRelation,
    Profile,
    Setup,
    Verifier,
    Deployment,
    Discovery,
    Package,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: String,
    pub old_manifest_sha256: String,
    pub new_manifest_sha256: String,
    pub package_plan_hash: Option<String>,
    pub changes: Vec<Change>,
    /// Only the exact same statement/transaction under the candidate contract.
    pub old_proof_reusable_for_identical_statement: bool,
    pub candidate_requires_reproving: bool,
    pub existing_cells_retain_old_rules: bool,
    pub migration: MigrationDisposition,
    pub discovery_authorizes_spend: bool,
    pub setup_trust_admitted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MigrationDisposition {
    NoRuntimeChange,
    ExistingAuthorizationRequired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LegacyDisposition {
    RetainExistingDeployment,
    RetrofitMigrationUnsupported,
}

impl Plan {
    /// The original counter lifecycle requires an identical successor Script.
    /// Creating a new instance never counts as migration of that legacy Cell.
    pub fn immutable_counter_disposition(&self) -> LegacyDisposition {
        if self.migration == MigrationDisposition::NoRuntimeChange {
            LegacyDisposition::RetainExistingDeployment
        } else {
            LegacyDisposition::RetrofitMigrationUnsupported
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixedPairMigration {
    pub schema: String,
    pub counter_script_hash: String,
    pub configuration_script_hash: String,
    pub authorizing_old_parent: String,
    pub destination_parent: String,
    pub configuration_transition: String,
    pub requires_fresh_old_key_proof: bool,
    pub preserves_owner_and_state_lineage: bool,
    pub chain_authorization_established: bool,
}

/// Compare exact, externally pinned manifests. This report intentionally cannot
/// invent an upgrade path: the existing deployed Script must authorize migration.
pub fn compare(old: &CheckedManifest, new: &CheckedManifest, package_plan: Option<&UpgradePlan>) -> Result<Plan> {
    let a = &old.manifest;
    let b = &new.manifest;
    ensure!(a.application == b.application, "ZK lifecycle application mismatch");
    ensure!(a.package.member == b.package.member, "ZK lifecycle package member mismatch");
    ensure!(
        a.deployment.genesis_hash == b.deployment.genesis_hash && a.deployment.chain_id == b.deployment.chain_id,
        "ZK lifecycle network mismatch"
    );
    ensure!(b.version >= a.version, "ZK lifecycle downgrade rejected");
    ensure!(b.status == Status::Active, "ZK lifecycle candidate is yanked");
    ensure!(a == b || b.version > a.version, "ZK lifecycle changed manifest must advance version");
    if let Some(plan) = package_plan {
        upgrade::validate_plan(plan).map_err(|e| anyhow::anyhow!("ZK lifecycle package plan: {e}"))?;
        let locks: Vec<_> = plan.locks.iter().filter(|lock| lock.member == a.package.member).collect();
        let [lock] = locks.as_slice() else {
            anyhow::bail!("ZK lifecycle package plan requires exactly one matching member");
        };
        ensure!(
            lock.old_lock_sha256 == format!("sha256:{}", a.package.lock_sha256)
                && lock.new_lock_sha256 == format!("sha256:{}", b.package.lock_sha256)
                && sha256(lock.old_lock_content.as_bytes()) == a.package.lock_sha256
                && sha256(lock.new_lock_content.as_bytes()) == b.package.lock_sha256,
            "ZK lifecycle package plan lock identity mismatch"
        );
    } else {
        ensure!(a.package.lock_sha256 == b.package.lock_sha256, "ZK lifecycle changed package lock requires upgrade plan");
    }
    let mut changes = Vec::new();
    for (changed, class) in [
        (a.circuit.source_sha256 != b.circuit.source_sha256, Change::Source),
        (a.proving != b.proving, Change::Proving),
        (a.circuit.statement_sha256 != b.circuit.statement_sha256, Change::Statement),
        (a.circuit.codec_sha256 != b.circuit.codec_sha256, Change::Codec),
        (a.verification_key_sha256 != b.verification_key_sha256, Change::VerificationKey),
        (a.circuit.relation_sha256 != b.circuit.relation_sha256, Change::CircuitRelation),
        (a.circuit.profile != b.circuit.profile, Change::Profile),
        (a.setup != b.setup, Change::Setup),
        (a.verifier_sha256 != b.verifier_sha256, Change::Verifier),
        (a.deployment != b.deployment, Change::Deployment),
        (a.status != b.status || a.version != b.version, Change::Discovery),
        (a.package != b.package, Change::Package),
    ] {
        if changed {
            changes.push(class);
        }
    }
    let runtime_changed = changes.iter().any(|change| {
        matches!(
            change,
            Change::Statement
                | Change::Codec
                | Change::VerificationKey
                | Change::CircuitRelation
                | Change::Profile
                | Change::Verifier
                | Change::Deployment
        )
    });
    Ok(Plan {
        schema: PLAN_SCHEMA.into(),
        old_manifest_sha256: old.bytes_sha256.clone(),
        new_manifest_sha256: new.bytes_sha256.clone(),
        package_plan_hash: package_plan.map(|plan| plan.plan_hash.clone()),
        changes,
        old_proof_reusable_for_identical_statement: !runtime_changed,
        candidate_requires_reproving: runtime_changed,
        existing_cells_retain_old_rules: true,
        migration: if runtime_changed {
            MigrationDisposition::ExistingAuthorizationRequired
        } else {
            MigrationDisposition::NoRuntimeChange
        },
        discovery_authorizes_spend: false,
        setup_trust_admitted: false,
    })
}

/// Recompute every decision against the currently trusted identities before use.
/// A changed/yanked discovery snapshot or package-plan receipt invalidates it.
pub fn validate_current(plan: &Plan, old: &CheckedManifest, new: &CheckedManifest, package_plan: Option<&UpgradePlan>) -> Result<()> {
    ensure!(*plan == compare(old, new, package_plan)?, "ZK lifecycle stale or substituted change plan");
    Ok(())
}
