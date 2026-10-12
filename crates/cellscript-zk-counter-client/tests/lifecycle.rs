use cellscript_zk_counter_client::lifecycle::*;
use cellscript_zk_private_counter::{hash, package::sha256};
use ckb_types::{bytes::Bytes, packed, prelude::*};

fn manifest() -> Manifest {
    Manifest {
        schema: MANIFEST_SCHEMA.into(),
        application: "private-counter".into(),
        version: 1,
        status: Status::Active,
        circuit: Circuit {
            source_sha256: sha256(b"source"),
            relation_sha256: sha256(b"relation"),
            statement_sha256: sha256(b"statement"),
            codec_sha256: sha256(b"codec"),
            profile: "exact-counter-v1".into(),
        },
        proving: Proving {
            key_sha256: sha256(b"pk"),
            executable_sha256: sha256(b"prover"),
            build_sha256: sha256(b"build"),
            toolchain: "1.97.1".into(),
        },
        verification_key_sha256: sha256(b"vk"),
        setup: Setup { manifest_sha256: sha256(b"setup"), transcript_sha256: None, trust: SetupTrust::PublicTestOnly },
        verifier_sha256: sha256(b"verifier"),
        deployment: Deployment {
            chain_id: "ckb-testnet".into(),
            genesis_hash: sha256(b"genesis"),
            verifier_data_hash: hex::encode(hash(b"verifier")),
            parent_data_hash: hex::encode(hash(b"parent")),
            code_out_point: hex::encode(packed::OutPoint::default().as_slice()),
            application_script: hex::encode(packed::Script::new_builder().args(Bytes::from(vec![1; 64]).pack()).build().as_slice()),
        },
        package: PackageBinding {
            member: ".".into(),
            lock_sha256: sha256(b"lock"),
            deployed_manifest_sha256: sha256(b"deployment record"),
        },
    }
}

fn checked(m: &Manifest) -> CheckedManifest {
    let bytes = serde_json::to_vec(m).unwrap();
    CheckedManifest::load(&bytes, &sha256(&bytes)).unwrap()
}

#[test]
fn lifecycle_classifies_every_identity_without_authorizing_migration() {
    let old = manifest();
    let checked_old = checked(&old);
    type MutationCase = (Change, bool, fn(&mut Manifest));
    let mut vectors = Vec::new();
    let cases: Vec<MutationCase> = vec![
        (Change::Source, false, |m| m.circuit.source_sha256 = sha256(b"new source")),
        (Change::Proving, false, |m| m.proving.key_sha256 = sha256(b"new pk")),
        (Change::Proving, false, |m| m.proving.executable_sha256 = sha256(b"new prover")),
        (Change::Proving, false, |m| m.proving.build_sha256 = sha256(b"new build")),
        (Change::Proving, false, |m| m.proving.toolchain = "different toolchain".into()),
        (Change::Statement, true, |m| m.circuit.statement_sha256 = sha256(b"new statement")),
        (Change::Codec, true, |m| m.circuit.codec_sha256 = sha256(b"new codec")),
        (Change::VerificationKey, true, |m| m.verification_key_sha256 = sha256(b"new vk")),
        (Change::CircuitRelation, true, |m| m.circuit.relation_sha256 = sha256(b"new relation")),
        (Change::Profile, true, |m| m.circuit.profile = "different profile".into()),
        (Change::Setup, false, |m| m.setup.manifest_sha256 = sha256(b"new setup record")),
        (Change::Setup, false, |m| m.setup.transcript_sha256 = Some(sha256(b"transcript"))),
        (Change::Setup, false, |m| m.setup.trust = SetupTrust::Unreviewed),
        (Change::Verifier, true, |m| m.verifier_sha256 = sha256(b"new verifier")),
        (Change::Deployment, true, |m| m.deployment.parent_data_hash = sha256(b"new parent")),
        (Change::Deployment, true, |m| m.deployment.verifier_data_hash = sha256(b"new child")),
        (Change::Deployment, true, |m| {
            m.deployment.code_out_point = hex::encode(packed::OutPoint::new_builder().index(1u32).build().as_slice())
        }),
        (Change::Deployment, true, |m| m.deployment.application_script = hex::encode(packed::Script::default().as_slice())),
    ];
    for (class, reprove, mutate) in cases {
        let mut new = old.clone();
        new.version += 1;
        mutate(&mut new);
        let checked_new = checked(&new);
        let plan = compare(&checked_old, &checked_new, None).unwrap();
        vectors.push(serde_json::json!({"old_manifest_bytes":hex::encode(serde_json::to_vec(&old).unwrap()),
            "new_manifest_bytes":hex::encode(serde_json::to_vec(&new).unwrap()),"expected":plan}));
        assert_eq!(plan.changes, vec![class, Change::Discovery]);
        assert_eq!(plan.candidate_requires_reproving, reprove);
        assert_eq!(plan.old_proof_reusable_for_identical_statement, !reprove);
        assert!(plan.existing_cells_retain_old_rules);
        assert!(!plan.discovery_authorizes_spend && !plan.setup_trust_admitted);
        validate_current(&plan, &checked_old, &checked_new, None).unwrap();
        let mut forged = plan.clone();
        forged.discovery_authorizes_spend = true;
        assert!(validate_current(&forged, &checked_old, &checked_new, None).is_err());
        let mut stale = new.clone();
        stale.version += 1;
        assert!(validate_current(&plan, &checked_old, &checked(&stale), None).is_err());
        assert_eq!(
            serde_json::to_vec(&plan).unwrap(),
            serde_json::to_vec(&compare(&checked_old, &checked_new, None).unwrap()).unwrap()
        );
    }
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("zk-lifecycle-vectors.json"), serde_json::to_vec_pretty(&vectors).unwrap()).unwrap();
}

#[test]
fn lifecycle_rejects_downgrades_yanks_networks_and_unclassified_fields() {
    let old = manifest();
    let a = checked(&old);
    for mutate in [
        (|m: &mut Manifest| m.version = 0) as fn(&mut Manifest),
        |m| m.status = Status::Yanked,
        |m| m.deployment.genesis_hash = sha256(b"other network"),
        |m| m.application = "another app".into(),
        |m| m.circuit.codec_sha256 = sha256(b"same version changed codec"),
        |m| {
            m.version += 1;
            m.package.lock_sha256 = sha256(b"changed lock without plan");
        },
    ] {
        let mut new = old.clone();
        mutate(&mut new);
        assert!(compare(&a, &checked(&new), None).is_err());
    }
    let mut value = serde_json::to_value(&old).unwrap();
    value["circuit"]["unclassified"] = serde_json::json!("must reject");
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(CheckedManifest::load(&bytes, &sha256(&bytes)).is_err());
    let bytes = serde_json::to_vec(&old).unwrap();
    assert!(CheckedManifest::load(&bytes, &sha256(b"another manifest")).is_err());
    assert!(CheckedManifest::load(&vec![b' '; MAX_MANIFEST_BYTES + 1], &sha256(&bytes)).is_err());
    let mut value = serde_json::to_value(&old).unwrap();
    value["setup"].as_object_mut().unwrap().remove("transcript_sha256");
    let missing = serde_json::to_vec(&value).unwrap();
    assert!(CheckedManifest::load(&missing, &sha256(&missing)).is_err());
    let mut huge = old.clone();
    huge.version = u64::MAX;
    let huge = serde_json::to_vec(&huge).unwrap();
    assert!(CheckedManifest::load(&huge, &sha256(&huge)).is_err());
}

#[test]
fn lifecycle_runtime_preflight_rejects_independent_artifact_substitutions() {
    let m = checked(&manifest());
    m.check_runtime_artifacts(b"vk", b"verifier", b"parent", b"prover").unwrap();
    for (vk, verifier, parent, prover) in [
        (&b"wrong"[..], &b"verifier"[..], &b"parent"[..], &b"prover"[..]),
        (b"vk", b"wrong", b"parent", b"prover"),
        (b"vk", b"verifier", b"wrong", b"prover"),
        (b"vk", b"verifier", b"parent", b"wrong"),
    ] {
        assert!(m.check_runtime_artifacts(vk, verifier, parent, prover).is_err());
    }
}

#[test]
fn lifecycle_binds_the_existing_package_upgrade_receipt_and_exact_lock_bytes() {
    use cellscript::package::{
        upgrade::{UpgradeLockTransaction, UpgradePlan},
        Lockfile,
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/amm_pool");
    let lock = Lockfile::read_from_root(&root).unwrap().unwrap();
    let old_content = std::fs::read_to_string(root.join("Cell.lock")).unwrap();
    let new_content = format!("{old_content}\n");
    let mut plan: UpgradePlan = serde_json::from_str(include_str!("../../../tests/fixtures/upgrade_plan_v1.json")).unwrap();
    plan.compiler_version = cellscript::VERSION.into();
    plan.locks.push(UpgradeLockTransaction {
        member: ".".into(),
        lock_path: "Cell.lock".into(),
        old_lock_sha256: format!("sha256:{}", sha256(old_content.as_bytes())),
        new_lock_sha256: format!("sha256:{}", sha256(new_content.as_bytes())),
        old_lock_content: old_content.clone(),
        new_lock_content: new_content.clone(),
        old_lock: lock.clone(),
        new_lock: lock,
        node_changes: vec![],
        edge_changes: vec![],
    });
    let seal = |p: &mut UpgradePlan| {
        let mut value = serde_json::to_value(&p).unwrap();
        value["plan_hash"] = serde_json::json!("");
        p.plan_hash = format!("sha256:{}", sha256(&serde_json::to_vec(&value).unwrap()));
    };
    seal(&mut plan);
    let mut a = manifest();
    a.package.lock_sha256 = sha256(old_content.as_bytes());
    let mut b = a.clone();
    b.version += 1;
    b.package.lock_sha256 = sha256(new_content.as_bytes());
    let (a, b) = (checked(&a), checked(&b));
    let report = compare(&a, &b, Some(&plan)).unwrap();
    assert_eq!(report.package_plan_hash.as_deref(), Some(plan.plan_hash.as_str()));
    assert!(report.changes.contains(&Change::Package));
    assert!(compare(&a, &b, None).is_err());
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    std::fs::write(
        target.join("zk-lifecycle-package-vector.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "old_manifest_bytes":hex::encode(serde_json::to_vec(a.manifest()).unwrap()),
            "new_manifest_bytes":hex::encode(serde_json::to_vec(b.manifest()).unwrap()),
            "package_plan":plan,"expected":report
        }))
        .unwrap(),
    )
    .unwrap();
    plan.locks[0].old_lock_content.push(' ');
    seal(&mut plan);
    assert!(compare(&a, &b, Some(&plan)).is_err(), "even a resealed plan cannot substitute pinned lock bytes");
}

#[test]
fn lifecycle_deployment_bridge_rejects_rebound_wrong_code_coordinates_and_status() {
    use cellscript::package::{DeployedManifest, DEPLOYED_MANIFEST_SCHEMA};
    let mut m = manifest();
    let record: DeployedManifest = serde_json::from_value(serde_json::json!({
        "version":2,"schema":DEPLOYED_MANIFEST_SCHEMA,"package":{"name":"parent","version":"1","edition":"2026"},
        "deployments":[{"edition":"2026","network":"testnet","chain_id":"ckb-testnet",
            "tx_hash":format!("0x{}", "00".repeat(32)),"output_index":0,
            "code_hash":m.deployment.parent_data_hash,"data_hash":m.deployment.parent_data_hash,
            "hash_type":"data2","dep_type":"code","out_point":format!("0x{}:0", "00".repeat(32)),
            "compatibility_profile_hash":sha256(b"profile"),"status":"active"}]
    }))
    .unwrap();
    m.package.deployed_manifest_sha256 = deployment_record_digest(&record).unwrap();
    let genesis: [u8; 32] = hex::decode(&m.deployment.genesis_hash).unwrap().try_into().unwrap();
    checked(&m).check_deployment_record(&record, genesis, DeploymentUse::Candidate).unwrap();
    let mut maximum = record.clone();
    for i in 1..64 {
        let mut other = record.deployments[0].clone();
        other.chain_id = format!("other-network-{i}");
        maximum.deployments.push(other);
    }
    let mut maximum_manifest = m.clone();
    maximum_manifest.package.deployed_manifest_sha256 = deployment_record_digest(&maximum).unwrap();
    checked(&maximum_manifest).check_deployment_record(&maximum, genesis, DeploymentUse::Candidate).unwrap();
    maximum.deployments.push(record.deployments[0].clone());
    assert!(deployment_record_digest(&maximum).is_err());
    let mut oversized = record.clone();
    oversized.package.source_hash = Some("x".repeat(262_144));
    assert!(deployment_record_digest(&oversized).is_err());
    assert!(checked(&m).check_deployment_record(&record, [0; 32], DeploymentUse::Candidate).is_err());
    let mut vectors = Vec::new();
    for (name, mutate) in [
        ("accepted", (|_: &mut DeployedManifest| {}) as fn(&mut DeployedManifest)),
        ("data", |r| r.deployments[0].data_hash = sha256(b"other")),
        ("code", |r| r.deployments[0].code_hash = sha256(b"other")),
        ("type", |r| r.deployments[0].hash_type = "type".into()),
        ("group", |r| r.deployments[0].dep_type = "dep_group".into()),
        ("point", |r| r.deployments[0].out_point = format!("0x{}:1", "00".repeat(32))),
        ("index", |r| r.deployments[0].output_index = 1),
        ("network", |r| r.deployments[0].chain_id = "ckb-mainnet".into()),
        ("ambiguous", |r| r.deployments.push(r.deployments[0].clone())),
        ("missing", |r| r.deployments.clear()),
        ("revoked", |r| r.deployments[0].status = Some(cellscript::package::DeploymentStatus::Revoked)),
        ("candidate", |r| r.deployments[0].status = Some(cellscript::package::DeploymentStatus::Candidate)),
        ("artifact", |r| r.deployments[0].artifact_hash = Some(sha256(b"other"))),
    ] {
        let mut changed = record.clone();
        mutate(&mut changed);
        if name != "accepted" {
            assert!(checked(&m).check_deployment_record(&changed, genesis, DeploymentUse::Candidate).is_err());
        }
        let mut rebound = m.clone();
        rebound.package.deployed_manifest_sha256 = deployment_record_digest(&changed).unwrap();
        let candidate_ok = checked(&rebound).check_deployment_record(&changed, genesis, DeploymentUse::Candidate).is_ok();
        assert_eq!(candidate_ok, name == "accepted", "{name}");
        let existing_ok = checked(&rebound).check_deployment_record(&changed, genesis, DeploymentUse::ExistingSpend).is_ok();
        assert_eq!(existing_ok, matches!(name, "accepted" | "revoked" | "candidate"), "{name}");
        vectors.push(serde_json::json!({"case":name,"manifest_bytes":hex::encode(serde_json::to_vec(&rebound).unwrap()),
            "record":changed,"genesis":m.deployment.genesis_hash,"candidate_ok":candidate_ok,"existing_ok":existing_ok}));
    }
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    std::fs::write(target.join("zk-lifecycle-deployment-vectors.json"), serde_json::to_vec_pretty(&vectors).unwrap()).unwrap();
}
