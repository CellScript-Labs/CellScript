//! Consume the exact retained bytes from the focused CCC node replay. This
//! does not rerun a gate or substitute offline planning for node authorization.
use anyhow::{ensure, Result};
use cellscript_zk_counter_client::{
    hash32, lifecycle as life,
    migration::{Artifacts, CheckedDeployment, Manifest},
};
use cellscript_zk_private_counter::{self as counter, package};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

#[test]
#[ignore = "requires CELLSCRIPT_ZK_LIFECYCLE_NODE_EVIDENCE containing the completed CCC node replay and exact public artifacts"]
fn lifecycle_decisions_bind_the_continuous_confirmed_node_migration() -> Result<()> {
    let root = PathBuf::from(std::env::var_os("CELLSCRIPT_ZK_LIFECYCLE_NODE_EVIDENCE").expect("retained node evidence directory"));
    let read = |name: &str| fs::read(root.join(name));
    let provenance: Value = serde_json::from_slice(&read("replay.json")?)?;
    let node_bytes = read("node-evidence.json")?;
    let node: Value = serde_json::from_slice(&node_bytes)?;
    let ccc_bytes = read("ccc-migration.json")?;
    let ccc: Value = serde_json::from_slice(&ccc_bytes)?;
    ensure!(node["status"] == "passed" && ccc["status"] == "passed", "completed node evidence required");
    let row = node["rows"].as_array().unwrap().iter().find(|r| r["case"] == "CCC authorized migration").unwrap();
    ensure!(row["client"] == ccc, "node/CCC evidence differs");
    for flag in [
        "actual_successor_lineage",
        "new_key_self_installation_rejected",
        "old_key_after_migration_rejected",
        "stale_configuration_rejected",
        "second_migration_rejected",
        "raw_mutation_rejected",
    ] {
        ensure!(ccc[flag] == true, "node boundary missing: {flag}");
    }
    let manifest_bytes = read("migration-manifest.json")?;
    ensure!(ccc["manifest_data_hash"] == provenance["manifest_data_hash"], "node manifest commitment differs");
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)?;
    let lifecycle = read("lifecycle.elf")?;
    let guard = read("guard.elf")?;
    let child = read("child.elf")?;
    let keys = [read("key-0.bin")?, read("key-1.bin")?];
    let prover = read("prover")?;
    ensure!(package::sha256(&prover) == provenance["prover_sha256"], "actual node prover substitution");
    let deployment = CheckedDeployment::load(
        &manifest_bytes,
        hash32(provenance["manifest_data_hash"].as_str().unwrap(), "trusted node manifest")?,
        &manifest.chain_id,
        hash32(&manifest.genesis_hash, "node genesis")?,
        Artifacts { lifecycle: &lifecycle, config_guard: &guard, children: [&child; 2], keys: [&keys[0], &keys[1]] },
    )?;
    let mut manifests = Vec::new();
    let mut checked = Vec::new();
    let mut records = Vec::new();
    for i in 0..2 {
        let record = deployment.lifecycle_deployment_record(i, cellscript::package::DeploymentStatus::Active)?;
        let version = deployment.lifecycle_manifest(
            i,
            life::ManifestContext {
                version: i as u64,
                status: life::Status::Active,
                prover_executable: &prover,
                prover_build_record: &read("replay.json")?,
                package: life::PackageBinding {
                    member: ".".into(),
                    lock_sha256: package::sha256(include_bytes!("../Cargo.lock")),
                    deployed_manifest_sha256: life::deployment_record_digest(&record)?,
                },
            },
        )?;
        let bytes = serde_json::to_vec(&version)?;
        let m = life::CheckedManifest::load(&bytes, &package::sha256(&bytes))?;
        m.check_deployment_record(&record, hash32(&manifest.genesis_hash, "node genesis")?, life::DeploymentUse::Candidate)?;
        manifests.push(bytes);
        checked.push(m);
        records.push(record);
    }
    let plan = life::compare(&checked[0], &checked[1], None)?;
    let path = deployment.assess_lifecycle_migration(&plan, &checked[0], &checked[1], 0, None)?;
    ensure!(
        plan.candidate_requires_reproving && !plan.old_proof_reusable_for_identical_statement,
        "new VK/deployment must require a new proof"
    );
    let rows = ccc["rows"].as_array().unwrap();
    ensure!(rows.len() == 3, "complete continuous migration required");
    for (row, (name, selected)) in
        rows.iter().zip([("old-key-update", 0), ("old-key-authorized-migration", 0), ("new-key-successor-update", 1)])
    {
        ensure!(
            row["case"] == name && row["selected_version"] == selected && row["confirmed"] == true && row["fee_signature"] == true,
            "incorrect migration lineage stage: {name}"
        );
        ensure!(
            row["cycles"].as_u64().is_some_and(|n| n > 0 && n <= 250_000_000)
                && row["transaction_bytes"].as_u64().is_some_and(|n| n > 0 && n <= 16_384),
            "frozen node resource budget: {name}"
        );
    }
    ensure!(
        path.authorizing_old_parent == hex::encode(hash32(&manifest.versions[0].parent.data_hash, "old parent")?),
        "wrong authority"
    );
    ensure!(
        path.destination_parent == hex::encode(hash32(&manifest.versions[1].parent.data_hash, "new parent")?),
        "wrong destination"
    );
    fs::write(
        root.join("lifecycle-report.json"),
        serde_json::to_vec_pretty(&json!({
            "schema":"cellscript-zk-lifecycle-node-replay-v1","status":"passed", "node_provenance":provenance,
            "node_evidence_sha256":package::sha256(&node_bytes),"ccc_evidence_sha256":package::sha256(&ccc_bytes),
            "migration_manifest_data_hash":hex::encode(counter::hash(&manifest_bytes)),
            "old_manifest_bytes":hex::encode(&manifests[0]),"new_manifest_bytes":hex::encode(&manifests[1]),
            "plan":plan,"authorization_path":path,"deployed_records":records,"node_rows":rows,
            "runtime_contract_changed":false,"production_setup_admitted":false,
            "scope":"new off-chain decisions over exact checked artifacts from the separately identified continuous node execution"
        }))?,
    )?;
    Ok(())
}
