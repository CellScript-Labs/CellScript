//! Exact proving/VK packages. An OS-random local setup is still a single-party
//! trust assumption, recorded explicitly by the admission command.
use super::*;
use std::{fs, path::Path};

const MAX_PK_BYTES: usize = 256 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SetupKind {
    PublicTestSeed,
    LocalSingleParty,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: String,
    pub circuit: CircuitIdentity,
    pub rust_toolchain: String,
    pub dependency_lock_sha256: String,
    pub proving_key_sha256: String,
    pub verification_key_sha256: String,
    pub verification_key_data_hash: String,
    pub setup_kind: SetupKind,
    pub setup_unix_seconds: u64,
    pub production_admitted: bool,
}
pub fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= limit as u64,
        "invalid or oversized artifact: {}",
        path.display()
    );
    let bytes = fs::read(path)?;
    ensure!(bytes.len() <= limit, "artifact changed size");
    Ok(bytes)
}
/// Reject hostile vector lengths before Arkworks can allocate from them.
fn check_pk_layout(bytes: &[u8]) -> Result<()> {
    ensure!(bytes.len() >= wire::VK_BYTES + 64, "truncated proving key");
    ensure!(bytes[224..232] == 16u64.to_le_bytes(), "wrong VK input count");
    let mut cursor = wire::VK_BYTES + 64;
    for width in [32, 32, 64, 32, 32] {
        let count_bytes = bytes.get(cursor..cursor + 8).ok_or_else(|| anyhow::anyhow!("truncated key vector"))?;
        let count = u64::from_le_bytes(count_bytes.try_into()?);
        cursor += 8;
        let remaining = (bytes.len() - cursor) / width;
        ensure!(count <= remaining as u64, "proving-key vector exceeds file bounds");
        cursor += count as usize * width;
    }
    ensure!(cursor == bytes.len(), "trailing proving-key data");
    Ok(())
}
pub fn write_package(path: &Path, pk: &ProvingKey<Bn254>, setup_kind: SetupKind) -> Result<Manifest> {
    ensure!(!path.exists(), "output package already exists");
    let pk_bytes = serialize(pk)?;
    let vk = serialize(&pk.vk)?;
    check_pk_layout(&pk_bytes)?;
    ensure!(vk.len() == wire::VK_BYTES, "wrong VK width");
    let manifest = Manifest {
        schema: "cellscript-counter-setup-v1".into(),
        circuit: circuit_identity()?,
        rust_toolchain: "1.97.1".into(),
        dependency_lock_sha256: sha256(include_bytes!("../Cargo.lock")),
        proving_key_sha256: sha256(&pk_bytes),
        verification_key_sha256: sha256(&vk),
        verification_key_data_hash: hex::encode(hash(&vk)),
        setup_kind,
        setup_unix_seconds: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(),
        production_admitted: false,
    };
    fs::create_dir_all(path)?;
    fs::write(path.join("proving-key.bin"), pk_bytes)?;
    fs::write(path.join("verification-key.bin"), vk)?;
    fs::write(path.join("setup.json"), serde_json::to_vec_pretty(&manifest)?)?;
    Ok(manifest)
}
pub fn load_package(path: &Path) -> Result<(Manifest, ProvingKey<Bn254>)> {
    let manifest: Manifest = serde_json::from_slice(&read_bounded(&path.join("setup.json"), 16384)?)?;
    ensure!(
        manifest.schema == "cellscript-counter-setup-v1" && !manifest.production_admitted,
        "unsupported or self-admitted setup manifest"
    );
    ensure!(manifest.circuit == circuit_identity()?, "circuit constraint identity mismatch");
    ensure!(
        manifest.rust_toolchain == "1.97.1" && manifest.dependency_lock_sha256 == sha256(include_bytes!("../Cargo.lock")),
        "toolchain/dependency identity mismatch"
    );
    let vk = read_bounded(&path.join("verification-key.bin"), wire::VK_BYTES)?;
    ensure!(vk.len() == wire::VK_BYTES && vk[224..232] == 16u64.to_le_bytes(), "wrong VK layout");
    ensure!(
        sha256(&vk) == manifest.verification_key_sha256 && hex::encode(hash(&vk)) == manifest.verification_key_data_hash,
        "VK identity mismatch"
    );
    let pk_bytes = read_bounded(&path.join("proving-key.bin"), MAX_PK_BYTES)?;
    ensure!(sha256(&pk_bytes) == manifest.proving_key_sha256, "proving-key identity mismatch");
    check_pk_layout(&pk_bytes)?;
    let pk: ProvingKey<Bn254> = decode_exact(&pk_bytes)?;
    ensure!(serialize(&pk.vk)? == vk, "proving key embeds a different VK");
    Ok((manifest, pk))
}

pub const LOCAL_SETUP_TRUST: &str = "single-party-local-setup;operator-and-host-trusted;erasure-not-independently-proven";

/// Admit this exact application bundle under the explicitly selected local
/// setup trust model. This is an engineering release gate, not an MPC ceremony,
/// an independent security audit, or a public-network deployment receipt.
pub fn verify_admission(package: &Path, evidence: &Path, trust_local_setup: bool) -> Result<serde_json::Value> {
    let (manifest, _) = load_package(package)?;
    ensure!(manifest.setup_kind == SetupKind::LocalSingleParty, "public-seed setup cannot be admitted");
    ensure!(trust_local_setup, "select --trust-local-setup only when the setup operator and host are trusted");
    let bytes = read_bounded(evidence, 16 * 1024 * 1024)?;
    let report: serde_json::Value = serde_json::from_slice(&bytes)?;
    ensure!(
        report["schema"] == "cellscript-counter-application-evidence-v1" && report["status"] == "passed",
        "application evidence must pass"
    );
    ensure!(
        report["verification_key_data_hash"] == manifest.verification_key_data_hash
            && report["circuit_sha256"] == manifest.circuit.r1cs_sha256,
        "evidence belongs to another circuit/VK"
    );
    ensure!(report["setup_kind"] == "local-single-party", "evidence must use the local candidate key");
    for name in ["scheduler", "node_acceptance", "reproducibility"] {
        ensure!(report[name]["status"] == "passed", "{name}: passing evidence missing");
    }
    for name in ["scheduler", "node_acceptance"] {
        ensure!(
            report[name]["verification_key_data_hash"] == manifest.verification_key_data_hash
                && report[name]["circuit_sha256"] == manifest.circuit.r1cs_sha256,
            "{name}: circuit/VK mismatch"
        );
        let rows = report[name]["rows"].as_array().ok_or_else(|| anyhow::anyhow!("{name}: cases missing"))?;
        for required in ["create", "update-0-1", "update-1-2"] {
            let row = rows.iter().find(|row| row["case"] == required).ok_or_else(|| anyhow::anyhow!("{name}: {required} missing"))?;
            if required != "create" {
                ensure!(
                    row["cycles"].as_u64().is_some_and(|cycles| cycles > 0 && cycles < 250_000_000),
                    "{name}: cycle budget failure"
                );
            }
            if name == "node_acceptance" {
                ensure!(row["commit"]["status"] == "committed", "node transaction did not commit");
            }
        }
        ensure!(
            rows.iter().any(|row| row["case"] == "successor-replay" && row["error_code"] == 79
                || row["label"] == "successor-replay" && row["matched_expected"] == true && row["expected"]["error_code"] == 79),
            "{name}: replay rejection missing"
        );
    }
    Ok(serde_json::json!({"schema":"cellscript-counter-admission-v1", "production_admitted":true,
        "scope":"private-counter-v1 engineering admission under local single-party setup trust",
        "verification_key_data_hash":manifest.verification_key_data_hash, "circuit_sha256":manifest.circuit.r1cs_sha256,
        "proving_key_sha256":manifest.proving_key_sha256,"setup_manifest_sha256":sha256(&read_bounded(&package.join("setup.json"),16384)?),
        "evidence_sha256":sha256(&bytes), "trust_model":LOCAL_SETUP_TRUST,
        "independent_review":false, "mpc_ceremony":false, "public_network_deployment":false}))
}

/// Compare complete ELF bytes from independent Cargo target directories.
pub fn reproducibility(first: &Path, second: &Path) -> Result<serde_json::Value> {
    let mut artifacts = serde_json::Map::new();
    for name in ["cellscript-zk-transition-verifier", "cellscript-counter-lifecycle"] {
        let a = read_bounded(&first.join(name), 1024 * 1024)?;
        let b = read_bounded(&second.join(name), 1024 * 1024)?;
        ensure!(!a.is_empty() && a == b, "{name}: independent builds differ");
        artifacts.insert(name.into(), serde_json::json!({"sha256":sha256(&a),"data_hash":hex::encode(hash(&a)),"bytes":a.len()}));
    }
    Ok(
        serde_json::json!({"schema":"cellscript-counter-reproducibility-v1","status":"passed","scope":"same pinned sources/toolchain; independent Cargo target directories","artifacts":artifacts}),
    )
}

/// Assemble exact local evidence. The admission command additionally checks
/// completed cases and requires an explicit choice of setup trust model.
pub fn evidence_bundle(package: &Path, scheduler: &Path, node: &Path, repro: &Path) -> Result<serde_json::Value> {
    let (manifest, _) = load_package(package)?;
    let mut reports = serde_json::Map::new();
    let mut hashes = serde_json::Map::new();
    for (name, path, schema) in [
        ("scheduler", scheduler, "cellscript-counter-application-evidence-v1"),
        ("node_acceptance", node, "cellscript-counter-node-evidence-v1"),
        ("reproducibility", repro, "cellscript-counter-reproducibility-v1"),
    ] {
        let bytes = read_bounded(path, 16 * 1024 * 1024)?;
        let report: serde_json::Value = serde_json::from_slice(&bytes)?;
        ensure!(report["schema"] == schema && report["status"] == "passed", "{name}: not passing evidence");
        if name != "reproducibility" {
            ensure!(
                report["circuit_sha256"] == manifest.circuit.r1cs_sha256
                    && report["verification_key_data_hash"] == manifest.verification_key_data_hash,
                "{name}: another circuit/VK"
            );
        }
        hashes.insert(name.into(), serde_json::json!(sha256(&bytes)));
        reports.insert(name.into(), report);
    }
    for (artifact, field) in
        [("cellscript-zk-transition-verifier", "child_data_hash"), ("cellscript-counter-lifecycle", "lifecycle_data_hash")]
    {
        let expected = &reports["reproducibility"]["artifacts"][artifact]["data_hash"];
        ensure!(expected.is_string() && &reports["scheduler"][field] == expected, "scheduler ELF differs from reproduced artifact");
        let name = if field == "child_data_hash" { "zk-child" } else { "counter-lifecycle" };
        let deployed = reports["node_acceptance"]["deployments"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("node deployments missing"))?
            .iter()
            .find(|v| v["name"] == name)
            .ok_or_else(|| anyhow::anyhow!("node artifact missing"))?;
        ensure!(deployed["data_hash"] == format!("0x{}", expected.as_str().unwrap()), "node ELF differs from reproduced artifact");
    }
    Ok(
        serde_json::json!({"schema":"cellscript-counter-application-evidence-v1","status":"passed","circuit_sha256":manifest.circuit.r1cs_sha256,
        "verification_key_data_hash":manifest.verification_key_data_hash,"setup_kind":manifest.setup_kind,"production_admitted":false,
        "scheduler":reports["scheduler"],"node_acceptance":reports["node_acceptance"],"reproducibility":reports["reproducibility"],"report_sha256":hashes}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn attacker_controlled_key_lengths_fail_before_deserialization() {
        let mut bytes = vec![0; wire::VK_BYTES + 64 + 40];
        bytes[224..232].copy_from_slice(&16u64.to_le_bytes());
        assert!(check_pk_layout(&bytes).is_ok());
        bytes[wire::VK_BYTES + 64..wire::VK_BYTES + 72].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(check_pk_layout(&bytes).is_err());
        assert!(check_pk_layout(&bytes[..300]).is_err());
    }
}
