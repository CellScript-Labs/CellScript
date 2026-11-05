// CLI artifacts behavior; included in the original cli integration target.

#[test]
fn cellc_audit_bundle_marks_cyclic_flow_type_as_root_required() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("pool.cell");
    let output_dir = dir.path().join("audit");
    std::fs::write(&input, cyclic_flow_template_layout_source()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("audit-bundle")
        .arg(&input)
        .args(["--output"])
        .arg(&output_dir)
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let bundle: serde_json::Value = serde_json::from_slice(&std::fs::read(output_dir.join("audit-bundle.json")).unwrap()).unwrap();
    assert_eq!(bundle["protocol_graph"]["schema"], "cellscript-protocol-graph-v0.22");
    assert_eq!(bundle["protocol_graph"]["cycle_detected"], true);
    let cyclic_layout = bundle["template_layouts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|layout| layout["type_name"] == "Pool")
        .expect("Pool template layout");
    assert_eq!(cyclic_layout["schema"], "cellscript-template-layout-v0.21");
    assert_eq!(cyclic_layout["cycle_policy"], "RootRequired", "unexpected layout: {cyclic_layout}");
    assert_eq!(cyclic_layout["state_machine_acyclic"], false);
    assert_eq!(cyclic_layout["consensus_checked"], false);
}

#[test]
fn cellc_audit_bundle_marks_acyclic_flow_type_as_path_only() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("grant.cell");
    let output_dir = dir.path().join("audit");
    std::fs::write(&input, acyclic_flow_template_layout_source()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("audit-bundle")
        .arg(&input)
        .args(["--output"])
        .arg(&output_dir)
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let bundle: serde_json::Value = serde_json::from_slice(&std::fs::read(output_dir.join("audit-bundle.json")).unwrap()).unwrap();
    let acyclic_layout = bundle["template_layouts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|layout| layout["type_name"] == "Grant")
        .expect("Grant template layout");
    assert_eq!(acyclic_layout["schema"], "cellscript-template-layout-v0.21");
    assert_eq!(acyclic_layout["cycle_policy"], "PathOnlyAllowed", "unexpected layout: {acyclic_layout}");
    assert_eq!(acyclic_layout["state_machine_acyclic"], true);
}

#[test]
fn cellc_audit_bundle_template_layout_hash_distinguishes_cyclic_vs_acyclic() {
    let dir = tempfile::tempdir().unwrap();
    let cyclic_input = dir.path().join("cyclic.cell");
    let acyclic_input = dir.path().join("acyclic.cell");
    std::fs::write(&cyclic_input, cyclic_flow_template_layout_source()).unwrap();
    std::fs::write(&acyclic_input, acyclic_flow_template_layout_source()).unwrap();

    let build_cyclic =
        Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&cyclic_input).arg("-o").arg(dir.path().join("cyclic.s")).status().unwrap();
    assert!(build_cyclic.success());
    let build_acyclic =
        Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&acyclic_input).arg("-o").arg(dir.path().join("acyclic.s")).status().unwrap();
    assert!(build_acyclic.success());

    let cyclic_meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("cyclic.s.meta.json")).unwrap()).unwrap();
    let acyclic_meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("acyclic.s.meta.json")).unwrap()).unwrap();

    let cyclic_hash =
        cyclic_meta["template_layouts"].as_array().unwrap().iter().find(|layout| layout["type_name"] == "Pool").expect("Pool layout")
            ["template_layout_hash"]
            .as_str()
            .unwrap();
    let acyclic_hash = acyclic_meta["template_layouts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|layout| layout["type_name"] == "Grant")
        .expect("Grant layout")["template_layout_hash"]
        .as_str()
        .unwrap();
    // The canonical hash input embeds the acyclic/cyclic marker and the cycle
    // policy, so the two layouts must produce distinct hashes.
    assert_eq!(cyclic_hash.len(), 64);
    assert_eq!(acyclic_hash.len(), 64);
    assert_ne!(cyclic_hash, acyclic_hash, "cyclic and acyclic layout hashes must differ");
}

#[test]
fn cellc_verify_artifact_rejects_template_layout_consensus_checked_true() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("pool.cell");
    let output = dir.path().join("pool.s");
    std::fs::write(&input, cyclic_flow_template_layout_source()).unwrap();
    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
    assert!(build.success());

    let metadata_path = dir.path().join("pool.s.meta.json");
    let mut metadata_json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&metadata_path).unwrap()).unwrap();
    // RC deferral: consensus-checked TemplateLayout commitments are not supported,
    // so externally-supplied metadata carrying consensus_checked=true must be rejected.
    metadata_json["template_layouts"][0]["consensus_checked"] = serde_json::json!(true);
    let tampered = dir.path().join("consensus-true.meta.json");
    std::fs::write(&tampered, serde_json::to_vec_pretty(&metadata_json).unwrap()).unwrap();

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(&output)
        .arg("--metadata")
        .arg(&tampered)
        .output()
        .unwrap();
    assert!(!verify.status.success(), "unexpected success: {}", String::from_utf8_lossy(&verify.stdout));
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(
        stderr.contains("cannot set consensus_checked=true until a backend verifier supports TemplateLayout commitments"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn cellc_verify_artifact_rejects_template_layout_unsupported_cycle_policy() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("pool.cell");
    let output = dir.path().join("pool.s");
    std::fs::write(&input, cyclic_flow_template_layout_source()).unwrap();
    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
    assert!(build.success());

    let metadata_path = dir.path().join("pool.s.meta.json");
    let mut metadata_json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&metadata_path).unwrap()).unwrap();
    metadata_json["template_layouts"][0]["cycle_policy"] = serde_json::json!("Provisional");
    let tampered = dir.path().join("bad-cycle-policy.meta.json");
    std::fs::write(&tampered, serde_json::to_vec_pretty(&metadata_json).unwrap()).unwrap();

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(&output)
        .arg("--metadata")
        .arg(&tampered)
        .output()
        .unwrap();
    assert!(!verify.status.success(), "unexpected success: {}", String::from_utf8_lossy(&verify.stdout));
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("unsupported cycle_policy"), "unexpected stderr: {stderr}");
}

#[test]
fn cellc_verify_artifact_rejects_template_layout_unsupported_layout() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("pool.cell");
    let output = dir.path().join("pool.s");
    std::fs::write(&input, cyclic_flow_template_layout_source()).unwrap();
    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
    assert!(build.success());

    let metadata_path = dir.path().join("pool.s.meta.json");
    let mut metadata_json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&metadata_path).unwrap()).unwrap();
    metadata_json["template_layouts"][0]["layout"] = serde_json::json!("MerkleRoot");
    let tampered = dir.path().join("bad-layout.meta.json");
    std::fs::write(&tampered, serde_json::to_vec_pretty(&metadata_json).unwrap()).unwrap();

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(&output)
        .arg("--metadata")
        .arg(&tampered)
        .output()
        .unwrap();
    assert!(!verify.status.success(), "unexpected success: {}", String::from_utf8_lossy(&verify.stdout));
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("unsupported layout"), "unexpected stderr: {stderr}");
}

#[test]
fn cellc_constraints_subcommand_surfaces_ckb_deployment_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"

[deploy.ckb]
hash_type = "data2"

[[deploy.ckb.cell_deps]]
name = "secp256k1"
out_point = "0x1111111111111111111111111111111111111111111111111111111111111111:0"
dep_type = "dep_group"
hash_type = "type"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action main(value: u64) -> u64 {
    verification
        return value
}
"#,
    )
    .unwrap();

    let run = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("constraints")
        .arg("--target-profile")
        .arg("ckb")
        .arg("--entry-action")
        .arg("main")
        .output()
        .unwrap();

    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let constraints: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let ckb = &constraints["ckb"];
    assert_eq!(constraints["target_profile"], "ckb");
    assert_eq!(ckb["hash_type_policy"]["declared_hash_type"], "data2");
    assert_eq!(ckb["hash_type_policy"]["status"], "manifest-declared-builder-must-match");
    assert_eq!(ckb["dep_group_manifest"]["status"], "manifest-declares-dep-group-builder-must-expand-or-reference");
    let dep = &ckb["dep_group_manifest"]["declared_cell_deps"][0];
    assert_eq!(dep["name"], "secp256k1");
    assert_eq!(dep["dep_type"], "dep_group");
    assert_eq!(dep["tx_hash"], "0x1111111111111111111111111111111111111111111111111111111111111111");
    assert_eq!(dep["index"], 0);
    assert_eq!(dep["hash_type"], "type");
    assert_eq!(ckb["profile_abi_contract"]["witness_abi"], "ckb-molecule-witness-args-input-type-v2+cellscript-entry-witness-v1");
    assert_eq!(ckb["profile_abi_contract"]["lock_args_abi"], "ckb-script-args-typed-fixed-bytes");
    assert_eq!(ckb["profile_abi_contract"]["source_encoding"], "ckb-source-group-high-bit");
    assert_eq!(ckb["profile_abi_contract"]["cell_dep_abi"], "ckb-cell-dep-outpoint-and-dep-group");
    assert_eq!(ckb["profile_abi_contract"]["script_ref_abi"], "ckb-script-code-hash-hash-type-args");
    assert_eq!(ckb["profile_abi_contract"]["output_data_abi"], "ckb-outputs-and-outputs-data-index-aligned");
    assert_eq!(ckb["profile_abi_contract"]["capacity_floor_abi"], "ckb-output-capacity-floor-shannons");
    assert_eq!(ckb["profile_abi_contract"]["type_id_abi"], "ckb-type-id-v1");
    assert_eq!(ckb["capacity_evidence_contract"]["tx_size_measurement_required"], true);
}

#[test]
fn cellc_verify_artifact_accepts_matching_sidecar() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.cell");
    let output = dir.path().join("sample.s");
    let source = r#"
module test

action add(x: u64, y: u64) -> u64 {
    verification
        x + y
}
"#;
    std::fs::write(&input, source).unwrap();

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
    assert!(build.success());

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("verify-artifact").arg(&output).output().unwrap();

    assert!(verify.status.success(), "{}", String::from_utf8_lossy(&verify.stderr));
    let stdout = String::from_utf8_lossy(&verify.stdout);
    assert!(stdout.contains("Artifact verification succeeded"));
    assert!(stdout.contains("Metadata schema"));
    assert!(stdout.contains("Compiler"));
    assert!(stdout.contains("RISC-V assembly"));

    let verify_sources =
        Command::new(env!("CARGO_BIN_EXE_cellc")).arg("verify-artifact").arg(&output).arg("--verify-sources").output().unwrap();
    assert!(verify_sources.status.success(), "{}", String::from_utf8_lossy(&verify_sources.stderr));
    let stdout = String::from_utf8_lossy(&verify_sources.stdout);
    assert!(stdout.contains("Sources: verified 1 unit(s)"), "{}", stdout);
}

#[test]
fn cellc_verify_artifact_rejects_tampered_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.cell");
    let output = dir.path().join("sample.s");
    let source = r#"
module test

action add(x: u64, y: u64) -> u64 {
    verification
        x + y
}
"#;
    std::fs::write(&input, source).unwrap();

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
    assert!(build.success());
    std::fs::write(&output, b"tampered").unwrap();

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("verify-artifact").arg(&output).output().unwrap();

    assert!(!verify.status.success());
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("metadata artifact_hash") || stderr.contains("artifact_hash"), "{}", stderr);
}

#[test]
fn cellc_verify_artifact_rejects_tampered_source_when_requested() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.cell");
    let output = dir.path().join("sample.s");
    let source = r#"
module test

action add(x: u64, y: u64) -> u64 {
    verification
        x + y
}
"#;
    std::fs::write(&input, source).unwrap();

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
    assert!(build.success());
    std::fs::write(
        &input,
        r#"
module test

action add(x: u64, y: u64) -> u64 {
    verification
        x + y + 1
}
"#,
    )
    .unwrap();

    let verify =
        Command::new(env!("CARGO_BIN_EXE_cellc")).arg("verify-artifact").arg(&output).arg("--verify-sources").output().unwrap();

    assert!(!verify.status.success());
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("source unit") && stderr.contains("does not match metadata"), "{}", stderr);
}

#[test]
fn cellc_verify_artifact_rejects_metadata_schema_downgrade() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.cell");
    let output = dir.path().join("sample.s");
    let tampered_metadata = dir.path().join("schema-old.meta.json");
    let source = r#"
module test

action add(x: u64, y: u64) -> u64 {
    verification
        x + y
}
"#;
    std::fs::write(&input, source).unwrap();

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
    assert!(build.success());

    let metadata_path = dir.path().join("sample.s.meta.json");
    let mut metadata_json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&metadata_path).unwrap()).unwrap();
    let current_schema = metadata_json["metadata_schema_version"].as_u64().unwrap();
    metadata_json["metadata_schema_version"] = serde_json::json!(current_schema - 1);
    std::fs::write(&tampered_metadata, serde_json::to_vec_pretty(&metadata_json).unwrap()).unwrap();

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(&output)
        .arg("--metadata")
        .arg(&tampered_metadata)
        .output()
        .unwrap();

    assert!(!verify.status.success(), "unexpected success: {}", String::from_utf8_lossy(&verify.stdout));
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("unsupported metadata_schema_version"), "{}", stderr);
}

#[test]
fn cellc_verify_artifact_rejects_noncanonical_source_unit_hash() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.cell");
    let output = dir.path().join("sample.s");
    let tampered_metadata = dir.path().join("uppercase-source-hash.meta.json");
    let source = r#"
module test

action add(x: u64, y: u64) -> u64 {
    verification
        x + y
}
"#;
    std::fs::write(&input, source).unwrap();

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
    assert!(build.success());

    let metadata_path = dir.path().join("sample.s.meta.json");
    let mut metadata_json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&metadata_path).unwrap()).unwrap();
    let source_hash = metadata_json["source_units"][0]["hash"].as_str().unwrap().to_uppercase();
    metadata_json["source_units"][0]["hash"] = serde_json::json!(source_hash);
    std::fs::write(&tampered_metadata, serde_json::to_vec_pretty(&metadata_json).unwrap()).unwrap();

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(&output)
        .arg("--metadata")
        .arg(&tampered_metadata)
        .output()
        .unwrap();

    assert!(!verify.status.success(), "unexpected success: {}", String::from_utf8_lossy(&verify.stdout));
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("expected 64 lowercase hex characters"), "{}", stderr);
}

#[test]
fn cellc_verify_artifact_enforces_policy_flags() {
    // A dynamic vector returned by a helper is not an admitted output value.
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.cell");
    let output = dir.path().join("sample.s");
    let source = r#"
module test

resource Fingerprint {
    digest: Vec<u64>,
}

fn pass_digest(digest: Vec<u64>) -> Vec<u64> {
    return digest
}

action issue(digest: Vec<u64>) -> Fingerprint {
    verification
        let dynamic_digest = pass_digest(digest)
        let token = create Fingerprint {
            digest: dynamic_digest
        }
        return token
}
"#;
    std::fs::write(&input, source).unwrap();

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
    assert!(build.success());

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("verify-artifact").arg(&output).arg("--production").output().unwrap();

    assert!(!verify.status.success(), "unexpected success: {}", String::from_utf8_lossy(&verify.stdout));
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("check policy failed"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("output-verification-incomplete"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("fail-closed"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_verify_artifact_enforces_expected_hashes() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.cell");
    let output = dir.path().join("sample.s");
    let source = r#"
module test

action add(x: u64, y: u64) -> u64 {
    verification
        x + y
}
"#;
    std::fs::write(&input, source).unwrap();

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
    assert!(build.success());

    let metadata_path = dir.path().join("sample.s.meta.json");
    let metadata_json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&metadata_path).unwrap()).unwrap();
    let artifact_hash = metadata_json["artifact_hash"].as_str().unwrap();
    let source_content_hash = metadata_json["source_content_hash"].as_str().unwrap();

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(&output)
        .arg("--expect-artifact-hash")
        .arg(artifact_hash)
        .arg("--expect-source-content-hash")
        .arg(source_content_hash)
        .output()
        .unwrap();
    assert!(verify.status.success(), "{}", String::from_utf8_lossy(&verify.stderr));
    let stdout = String::from_utf8_lossy(&verify.stdout);
    assert!(stdout.contains("Expected hashes: verified"), "{}", stdout);

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(&output)
        .arg("--json")
        .arg("--expect-artifact-hash")
        .arg(artifact_hash)
        .arg("--expect-source-content-hash")
        .arg(source_content_hash)
        .output()
        .unwrap();
    assert!(verify.status.success(), "{}", String::from_utf8_lossy(&verify.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&verify.stdout).unwrap();
    assert_eq!(stdout["status"], "ok");
    assert_eq!(stdout["artifact_hash"], artifact_hash);
    assert_eq!(stdout["source_content_hash"], source_content_hash);
    assert_eq!(stdout["expected_hashes_verified"], true);
    assert_eq!(stdout["policy_verified"], false);
    assert_eq!(stdout["sources_verified"], false);
    assert_eq!(stdout["runtime_required_verifier_obligations"], 0);
    assert_eq!(stdout["fail_closed_verifier_obligations"], 0);

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(&output)
        .arg("--expect-source-content-hash")
        .arg("00".repeat(32))
        .output()
        .unwrap();
    assert!(!verify.status.success(), "unexpected success: {}", String::from_utf8_lossy(&verify.stdout));
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("source_content_hash") && stderr.contains("does not match expected"), "{}", stderr);

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(&output)
        .arg("--expect-artifact-hash")
        .arg(artifact_hash.to_uppercase())
        .output()
        .unwrap();
    assert!(!verify.status.success(), "unexpected success: {}", String::from_utf8_lossy(&verify.stdout));
    let stderr = String::from_utf8_lossy(&verify.stderr);
    assert!(stderr.contains("lowercase CKB Blake2b hex digest"), "{}", stderr);
}

#[test]
fn cellc_build_resolves_artifact_api_dependency_and_writes_lockfile() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let dep_root = root.join("token");
    let app_root = root.join("app");

    std::fs::create_dir_all(dep_root.join("src")).unwrap();
    std::fs::write(
        dep_root.join("Cell.toml"),
        format!(r#"
[package]
cellscript_version = ">={}"
edition = "2027"
name = "token"
version = "0.3.0"
namespace = "cellscript"
"#, cellscript::VERSION),
    )
    .unwrap();
    std::fs::write(
        dep_root.join("src/token.cell"),
        r#"
module dep::token

resource Token has store, replace, relock, consume, burn {
    amount: u64
}
"#,
    )
    .unwrap();
    let source_hash = cellscript::package::registry::compute_source_hash(&dep_root).unwrap();
    let registry_entry = cellscript::package::registry::RegistryIndex {
        schema_version: cellscript::package::registry::RegistryIndex::CURRENT_SCHEMA_VERSION,
        name: "token".to_string(),
        namespace: "cellscript".to_string(),
        versions: vec![cellscript::package::registry::RegistryVersion {
            edition: cellscript::CURRENT_EDITION,
            compatibility_profile_hash: "test-compatibility-profile".to_string(),
            version: "0.3.0".to_string(),
            tag: "v0.3.0".to_string(),
            source_hash: source_hash.clone(),
            cellscript_version: "0.19.0".to_string(),
            dependencies: Default::default(),
            abi_index: None,
            schema_hash: None,
            license: None,
            released_at: None,
            status: cellscript::package::registry::RegistryEntryStatus::VerifiedBuild,
            yanked: false,
            yanked_at: None,
            yanked_reason: None,
            replaced_by: None,
            audit: None,
            compiler_requirement: format!(">={}", cellscript::VERSION),
        }],
    };

    let snapshot_file = |path: &str, content: &[u8]| {
        serde_json::json!({
            "path": path,
            "blake2b256": hex_lower(&cellscript::ckb_blake2b256(content)),
            "content_base64": base64::engine::general_purpose::STANDARD.encode(content),
        })
    };
    let manifest_bytes = std::fs::read(dep_root.join("Cell.toml")).unwrap();
    let source_bytes = std::fs::read(dep_root.join("src/token.cell")).unwrap();
    let snapshot_bytes = serde_json::to_vec(&serde_json::json!({
        "schema": "cellscript-source-snapshot-v1",
        "package": { "namespace": "cellscript", "name": "token", "version": "0.3.0" },
        "files": [
            snapshot_file("Cell.toml", &manifest_bytes),
            snapshot_file("src/token.cell", &source_bytes),
        ],
    }))
    .unwrap();
    let snapshot_digest = Sha256::digest(&snapshot_bytes);
    let snapshot_hash = format!("sha256:{}", hex_lower(&snapshot_digest));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let api_origin = format!("http://{address}");
    let snapshot_path = "/source-snapshots/cellscript/token/0.3.0/token.json";
    let api_body = serde_json::json!({
        "schema": "cellscript-registry-artifact",
        "namespace": "cellscript",
        "name": "token",
        "repository": "https://example.test/cellscript/token",
        "artifact": {
            "kind": "source_library",
            "profile": "cellscript_source",
            "consumption_mode": "dependency",
            "language": "cellscript"
        },
        "releases": [{
            "release": "0.3.0",
            "verification_status": "verified",
            "availability_status": "active",
            "registry_entry": registry_entry,
            "immutable_bundle": {
                "schema": "cellscript-registry-immutable-bundle",
                "url": format!("{api_origin}{snapshot_path}"),
                "snapshot_hash": snapshot_hash,
                "source_hash": source_hash,
                "size_bytes": snapshot_bytes.len(),
                "content_type": "application/vnd.cellscript.source-snapshot+json"
            }
        }]
    })
    .to_string();
    let served_snapshot = snapshot_bytes.clone();
    listener.set_nonblocking(true).unwrap();
    let stop_server = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let server_stop = std::sync::Arc::clone(&stop_server);
    let server = std::thread::spawn(move || {
        while !server_stop.load(std::sync::atomic::Ordering::Acquire) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    let (path, _) = read_http_request_path_and_body(&mut stream);
                    assert!(matches!(
                        path.as_str(),
                        "/v1/artifacts/cellscript/token" | "/source-snapshots/cellscript/token/0.3.0/token.json"
                    ));
                    let (body, content_type) = if path == snapshot_path {
                        (served_snapshot.as_slice(), "application/vnd.cellscript.source-snapshot+json")
                    } else {
                        (api_body.as_bytes(), "application/json")
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    stream.write_all(response.as_bytes()).unwrap();
                    stream.write_all(body).unwrap();
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => panic!("artifact API mock failed: {error}"),
            }
        }
    });

    std::fs::create_dir_all(app_root.join("src")).unwrap();
    std::fs::write(
        app_root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "0.1.0"
namespace = "cellscript"

[dependencies.token]
version = "0.3.0"
namespace = "cellscript"
"#,
    )
    .unwrap();
    std::fs::write(
        app_root.join("src/main.cell"),
        r#"
module app::main

use dep::token::Token

action pass_through(token: Token) -> Token {
    verification
        token
}
"#,
    )
    .unwrap();

    let lock = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("lock")
        .env(cellscript::package::registry::REGISTRY_API_URL_ENV, &api_origin)
        .current_dir(&app_root)
        .output()
        .unwrap();
    assert!(lock.status.success(), "stderr: {}", String::from_utf8_lossy(&lock.stderr));

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("build")
        .arg("--locked")
        .env(cellscript::package::registry::REGISTRY_API_URL_ENV, &api_origin)
        .current_dir(&app_root)
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let lockfile: cellscript::package::Lockfile =
        toml::from_str(&std::fs::read_to_string(app_root.join("Cell.lock")).unwrap()).unwrap();
    assert!(lockfile.package.source_hash.is_some());
    let build = lockfile.package_build.as_ref().expect("build identity");
    assert!(build.compiler_version.is_some());
    assert!(build.target_profile.is_some());
    assert!(build.artifact_hash.is_some());
    assert!(build.metadata_hash.is_some());
    assert!(build.schema_hash.is_some());
    assert!(build.abi_hash.is_some());
    assert!(build.constraints_hash.is_some());
    let token_node = lockfile.root.dependencies.get("token").expect("locked registry root edge");
    let token = lockfile.dependencies.get(token_node).expect("locked registry dependency");
    assert_eq!(token.source_hash.as_deref(), Some(source_hash.as_str()));

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("package")
        .arg("verify")
        .env(cellscript::package::registry::REGISTRY_API_URL_ENV, &api_origin)
        .current_dir(&app_root)
        .output()
        .unwrap();
    stop_server.store(true, std::sync::atomic::Ordering::Release);
    server.join().unwrap();
    assert!(verify.status.success(), "stderr: {}", String::from_utf8_lossy(&verify.stderr));
}

#[test]
fn cellc_check_all_targets_checks_asm_and_elf_without_writing_artifacts() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"

[build]
target = "riscv64-elf"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    verification
        1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--all-targets").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Check succeeded"), "unexpected stdout: {}", stdout);
    assert!(stdout.contains("riscv64-asm (RISC-V assembly)"), "unexpected stdout: {}", stdout);
    assert!(stdout.contains("riscv64-elf (RISC-V ELF)"), "unexpected stdout: {}", stdout);
    assert!(!root.join("build").join("main.s").exists());
    assert!(!root.join("build").join("main.elf").exists());
    assert!(!root.join("build").join("main.s.meta.json").exists());
    assert!(!root.join("build").join("main.elf.meta.json").exists());

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--all-targets").arg("--json").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["status"], "ok");
    assert_eq!(stdout["all_targets"], true);
    assert_eq!(stdout["policy_verified"], false);
    let checked_targets = stdout["checked_targets"].as_array().unwrap();
    assert_eq!(checked_targets.len(), 2);
    assert!(checked_targets.iter().all(|target| target["runtime_required_verifier_obligations"] == 0));
    assert!(checked_targets.iter().all(|target| target["fail_closed_verifier_obligations"] == 0));
    assert!(checked_targets.iter().all(|target| target["target_profile"] == "ckb"));
    assert!(checked_targets.iter().all(|target| target["compiled_target_profile"] == "ckb"));
    assert!(checked_targets.iter().all(|target| target["target_profile_policy_violations"].as_array().unwrap().is_empty()));
    assert!(checked_targets.iter().any(|target| target["requested_target"] == "riscv64-asm"));
    assert!(checked_targets.iter().any(|target| target["requested_target"] == "riscv64-elf"));
}

#[test]
fn cellc_build_accepts_pure_ckb_target_profile_without_vm_abi_trailer() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    verification
        1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("build")
        .arg("--target-profile")
        .arg("ckb")
        .arg("--target")
        .arg("riscv64-elf")
        .arg("--json")
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["target_profile"], "ckb");
    assert_eq!(stdout["artifact_format"], "RISC-V ELF");
    let artifact_path = stdout["artifact"].as_str().unwrap();
    let artifact = std::fs::read(artifact_path).unwrap();
    assert!(artifact.starts_with(b"\x7fELF"));
    assert!(!artifact.ends_with(b"CSABITR0\x01\x80\0\0\0\0\0\0"));

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(artifact_path)
        .arg("--expect-target-profile")
        .arg("ckb")
        .arg("--json")
        .output()
        .unwrap();
    assert!(verify.status.success(), "{}", String::from_utf8_lossy(&verify.stderr));
    let verify_stdout: serde_json::Value = serde_json::from_slice(&verify.stdout).unwrap();
    assert_eq!(verify_stdout["target_profile"], "ckb");
    assert_eq!(verify_stdout["expected_target_profile_verified"], true);

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(artifact_path)
        .arg("--expect-target-profile")
        .arg("unknown")
        .output()
        .unwrap();
    assert!(!verify.status.success(), "unexpected success: {}", String::from_utf8_lossy(&verify.stdout));
}

#[test]
fn cellc_build_uses_manifest_policy_before_writing_artifacts() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"

[policy]
deny_ckb_runtime = true
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Fingerprint {
    digest: Hash,
}

fn pass_digest(digest: Hash) -> Hash {
    return digest
}

action issue(digest: Hash) -> Fingerprint {
    verification
        let dynamic_digest = pass_digest(digest)
        let token = create Fingerprint {
            digest: dynamic_digest
        }
        return token
}
"#,
    )
    .unwrap();

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").arg("--target-profile").arg("ckb").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("check policy failed"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("CKB runtime features"), "unexpected stderr: {}", stderr);
    assert!(!root.join("build").join("main.s").exists());
    assert!(!root.join("build").join("main.s.meta.json").exists());
}

#[test]
fn cellc_schema_ack_creates_and_verifies_a_focused_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let old = temp.path().join("old");
    let new = temp.path().join("new");
    for root in [&old, &new] {
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
name = "schema-ack-demo"
version = "0.1.0"
edition = "2027"
entry = "src/main.cell"

[build]
target_profile = "ckb"
"#,
        )
        .unwrap();
    }
    std::fs::write(
        old.join("src/main.cell"),
        r#"
module schema_ack::token
resource Token has store, replace, relock { owner: Address, amount: u64 }
action transfer(input token: Token) -> next: Token {
    replace token -> next {
        data = same except { }
        lock = same
        capacity = same
        type_script = same
    }
}
"#,
    )
    .unwrap();
    let candidate = r#"
module schema_ack::token
resource Token has store, replace, relock { owner: Address, amount: u64, approval_nonce: u64 }
action transfer(input token: Token) -> next: Token {
    replace token -> next {
        data = same except { approval_nonce = 0 }
        lock = same
        capacity = same
        type_script = same
    }
}
"#;
    std::fs::write(new.join("src/main.cell"), candidate).unwrap();

    let selector_args = ["--action", "transfer", "--before", "token", "--after", "next"];
    let plan = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("schema-ack")
        .arg("--old")
        .arg(&old)
        .arg("--new")
        .arg(&new)
        .args(selector_args)
        .output()
        .unwrap();
    assert!(plan.status.success(), "stderr: {}", String::from_utf8_lossy(&plan.stderr));
    let plan: serde_json::Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(plan["schema"], "cellscript-schema-change-plan-v2");
    assert_eq!(plan["requires_acknowledgement"], true);
    assert_eq!(plan["blockers"].as_array().unwrap().len(), 0);

    let receipt_path = temp.path().join("schema-ack.json");
    let created = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("schema-ack")
        .arg("--old")
        .arg(&old)
        .arg("--new")
        .arg(&new)
        .args(selector_args)
        .args(["--acknowledge-by", "Arthur", "--rationale", "approval nonce resets on transfer", "--output"])
        .arg(&receipt_path)
        .output()
        .unwrap();
    assert!(created.status.success(), "stderr: {}", String::from_utf8_lossy(&created.stderr));
    let receipt: serde_json::Value = serde_json::from_slice(&std::fs::read(&receipt_path).unwrap()).unwrap();
    assert_eq!(receipt["schema"], "cellscript-schema-acknowledgement-v2");
    assert_eq!(receipt["reviewer"], "Arthur");

    let verified = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("schema-ack")
        .arg("--old")
        .arg(&old)
        .arg("--new")
        .arg(&new)
        .args(selector_args)
        .arg("--verify")
        .arg(&receipt_path)
        .output()
        .unwrap();
    assert!(verified.status.success(), "stderr: {}", String::from_utf8_lossy(&verified.stderr));
    let verified: serde_json::Value = serde_json::from_slice(&verified.stdout).unwrap();
    assert_eq!(verified["status"], "verified");

    std::fs::write(new.join("src/main.cell"), candidate.replace("approval_nonce = 0", "approval_nonce = 1")).unwrap();
    let stale = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("schema-ack")
        .arg("--old")
        .arg(&old)
        .arg("--new")
        .arg(&new)
        .args(selector_args)
        .arg("--verify")
        .arg(&receipt_path)
        .output()
        .unwrap();
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("stale"));
}

#[test]
fn cellc_check_denies_metadata_only_declared_invariant() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

invariant token_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_invariant(true, "token amount is conserved")
}

resource Token {
    amount: u64,
}

action run() -> u64 {
    verification
        return 0
}
"#,
    )
    .unwrap();

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--deny-runtime-obligations").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("runtime-required ProofPlan gaps"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("invariant:token_conservation"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("gap:metadata-only"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_check_production_rejects_metadata_only_executable_claim() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

invariant enforce_token_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_conserved(Token.amount, scope = group)
}

resource Token {
    amount: u64,
}

action run() -> u64 {
    verification
        return 0
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--production").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unresolved consensus ProofPlan enforcement"), "{stderr}");
    assert!(stderr.contains("gap:metadata-only"), "{stderr}");
    assert!(stderr.contains("invariant:enforce_token_conservation"), "{stderr}");
}

#[test]
fn cellc_metadata_subcommand_emits_lowering_runtime_json() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

shared Config {
    threshold: u64
}

resource Token has store, replace, relock, consume, burn {
    amount: u64
}

action update(amount: u64) -> u64 {
    verification
        let cfg = read_ref<Config>()
        let token = create Token { amount: amount }
        consume token
        return cfg.threshold
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("metadata").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"lowering\""));
    assert!(stdout.contains("\"runtime\""));
    assert!(stdout.contains("\"fail_closed_runtime_features\""));
    assert!(stdout.contains("\"verifier_obligations\""));
    assert!(stdout.contains("\"source\": \"Input\""));
    assert!(stdout.contains("\"source\": \"CellDep\""));
    assert!(stdout.contains("\"source\": \"Output\""));
    assert!(stdout.contains("\"elf_compatible\": true"));
    assert!(stdout.contains("\"ckb_runtime_required\": true"));
    assert!(stdout.contains("read-cell-dep"));
    assert!(stdout.contains("verify-output-cell"));
    assert!(!stdout.contains("schema-field-access"));
}

#[test]
fn cellc_metadata_reports_multiple_compile_diagnostics() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action bad() -> bool {
    verification
        let first: u64 = true
        let second: bool = 1
        return true
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("metadata").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("2 diagnostics"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("expected U64, found Bool"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("expected Bool, found U64"), "unexpected stderr: {stderr}");
}

#[test]
fn cellc_expand_reports_native_pool_and_metadata_only_audit() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("Cell.toml"), "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"native-pool-preview\"\nversion = \"0.1.0\"\n")
        .unwrap();
    std::fs::write(
        root.join("src/main.cell"),
        r#"
module native_pool_preview
resource Token has store, create, consume { owner: Address, amount: u64 }
type_script TokenPool on type_group<Token> {
    action merge(
        input left: Token from group_input[0],
        input right: Token from group_input[1],
        witness recipient: Address from group_witness.input_type,
        output merged: Token from group_output[0],
    ) {
        require left.amount > 0
        audit settlement_policy { expected_evidence = external_policy(recipient) }
        pool value_flow {
            inputs { left, right }
            outputs { merged }
            data { owner { merged = recipient } amount = conserve }
            identity = pooled
            type_script = same
            lock { merged = exact(recipient) }
            capacity = builder_computed
            cardinality = declared
        }
    }
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).args(["--json", "expand"]).output().unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let foundation: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        foundation["dispositions"].as_array().unwrap().iter().filter(|disposition| disposition["input"]["kind"] == "pooled").count(),
        2
    );
    assert!(foundation["dispositions"].as_array().unwrap().iter().any(|disposition| disposition["output"]["kind"] == "pool-result"));
    let audit = foundation["claims"].as_array().unwrap().iter().find(|claim| claim["category"] == "audit").expect("audit claim");
    assert_eq!(audit["enforcement"], "metadata-only");
    assert_eq!(audit["on_chain_checked"], false);
    assert!(audit["execution"].is_null());
}

#[test]
fn cellc_check_xudt_conserved_runtime_helper_required_gap_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_xudt_package(root, xudt_conserved_source_without_helper_call());

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let metadata: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("build").join("main.s.meta.json")).unwrap()).unwrap();
    let proof_plan = metadata["runtime"]["proof_plan"].as_array().expect("proof_plan array");
    let aggregate =
        proof_plan.iter().find(|plan| plan["category"] == "aggregate-invariant").expect("aggregate-invariant ProofPlan record");

    // The invariant is recognised as runtime-helper-backed but the action does not
    // call the helper, so it must surface as a runtime-helper-required gap, not as
    // checked or as a pure metadata-only gap.
    assert_eq!(aggregate["status"], "runtime-required", "unexpected metadata: {:?}", aggregate);
    assert_eq!(aggregate["codegen_coverage_status"], "gap:runtime-helper-required", "unexpected metadata: {:?}", aggregate);
    assert_eq!(aggregate["on_chain_checked"], false);
    assert!(
        aggregate["builder_assumptions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|assumption| assumption == "declared(runtime-helper-required:xudt::require_group_amount_conserved)"),
        "missing required helper assumption: {:?}",
        aggregate["builder_assumptions"]
    );
}

#[test]
fn cellc_atomic_swap_full_lifecycle_build_check_audit_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"

[build]
target_profile = "ckb"
"#,
    )
    .unwrap();
    std::fs::write(root.join("src").join("main.cell"), atomic_swap_inline_source()).unwrap();

    // 1. check succeeds with the linear (acyclic) state machine.
    let check = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").output().unwrap();
    assert!(check.status.success(), "check failed: {}", String::from_utf8_lossy(&check.stderr));

    // 2. build produces an artifact + metadata.
    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").output().unwrap();
    assert!(build.status.success(), "build failed: {}", String::from_utf8_lossy(&build.stderr));
    let metadata: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("build").join("main.s.meta.json")).unwrap()).unwrap();
    let swap_layout = metadata["template_layouts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|layout| layout["type_name"] == "SwapLock")
        .expect("SwapLock template layout");
    assert_eq!(swap_layout["schema"], "cellscript-template-layout-v0.21");
    // The flow is linear (Pending -> Claimed/Refunded), so the layout must be PathOnlyAllowed.
    assert_eq!(swap_layout["cycle_policy"], "PathOnlyAllowed");
    assert_eq!(swap_layout["state_machine_acyclic"], true);

    // 3. explain graph reports the action transitions + template-layout-aware protocol view.
    let graph = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).args(["explain", "graph"]).arg("--json").output().unwrap();
    assert!(graph.status.success(), "{}", String::from_utf8_lossy(&graph.stderr));
    let graph_json: serde_json::Value = serde_json::from_slice(&graph.stdout).unwrap();
    assert_eq!(graph_json["schema"], "cellscript-protocol-graph-v0.22");
    assert_eq!(graph_json["consensus_checked"], false);
    assert!(graph_json["vertices"]
        .as_array()
        .unwrap()
        .iter()
        .any(|vertex| { vertex["id"] == "SwapLock:Pending" && vertex["initial"] == true && vertex["terminal"] == false }));
    assert!(graph_json["vertices"]
        .as_array()
        .unwrap()
        .iter()
        .any(|vertex| { vertex["id"] == "SwapLock:Claimed" && vertex["initial"] == false && vertex["terminal"] == true }));
    assert!(
        graph_json["edges"].as_array().unwrap().iter().any(|edge| {
            edge["action_name"] == "claim_with_preimage"
                && edge["source_vertex"] == "SwapLock:Pending"
                && edge["target_vertex"] == "SwapLock:Claimed"
                && edge["derivation"] == "state-transition"
        }),
        "expected claim_with_preimage state-transition edge: {graph_json}"
    );

    // 4. audit-bundle embeds both protocol_graph and template_layouts with the v0.21 schemas.
    let audit = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("audit-bundle")
        .args(["--output"])
        .arg(root.join("audit"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(audit.status.success(), "{}", String::from_utf8_lossy(&audit.stderr));
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("audit").join("audit-bundle.json")).unwrap()).unwrap();
    assert_eq!(bundle["protocol_graph"]["schema"], "cellscript-protocol-graph-v0.22");
    assert!(bundle["template_layouts"].as_array().unwrap().iter().any(|layout| {
        layout["schema"] == "cellscript-template-layout-v0.21"
            && layout["type_name"] == "SwapLock"
            && layout["consensus_checked"] == false
    }));

    // 5. action build surfaces verifier-covered runtime selectors for the consume/create actions.
    let action_build = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("action")
        .arg("build")
        .arg("--action")
        .arg("claim_with_preimage")
        .arg("--json")
        .output()
        .unwrap();
    assert!(action_build.status.success(), "{}", String::from_utf8_lossy(&action_build.stderr));
    let plan: serde_json::Value = serde_json::from_slice(&action_build.stdout).unwrap();
    let scan_selectors = &plan["action_scan_selectors"];
    assert_eq!(scan_selectors["schema"], "cellscript-action-scan-selectors-v0.21");
    assert_eq!(scan_selectors["status"], "compile-checked-runtime-selectors");
    assert_eq!(scan_selectors["runtime_required_selector_count"], 0);
    assert!(
        scan_selectors["checked_runtime_selector_count"].as_u64().unwrap() >= 1,
        "claim_with_preimage should declare at least one checked runtime selector: {scan_selectors}"
    );
}

#[test]
fn cellc_multi_phase_dao_flow_lifecycle_build_check_audit_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"

[build]
target_profile = "ckb"
"#,
    )
    .unwrap();
    // A linear Proposal state machine: Draft -> Active -> {Executed, Defeated}.
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::dao

shared DaoConfig has store, create, read_ref {
    admin: Address,
    quorum_bps: u64,
}

receipt Proposal has store, create, consume {
    proposal_id: Hash,
    state: u8,
    proposer: Address,
    for_votes: u64,
    against_votes: u64,
    end_timepoint: u64,
}

receipt ExecutionRecord has create {
    proposal_id: Hash,
    executed_at: u64,
    for_votes: u64,
    against_votes: u64,
}

flow Proposal.state {
    Draft -> Active;
    Active -> Executed;
    Active -> Defeated;
}

action propose(proposer: Address, proposal_id: Hash, end_timepoint: u64) -> proposal: Proposal {
    verification
        create proposal = Proposal { proposal_id, state: Draft, proposer, for_votes: 0, against_votes: 0, end_timepoint } with_lock(proposer)

}

action activate_proposal(proposal_before: Proposal) -> proposal_after: Proposal {
    transition proposal_before.state: Draft -> proposal_after.state: Active
    verification
        let now = env::current_timepoint()
        require proposal_before.state == Proposal::Draft, "not draft"
        require now < proposal_before.end_timepoint, "activation window closed"
        preserve proposal_after from proposal_before {
            proposal_id
            proposer
            for_votes
            against_votes
            end_timepoint
        }
        consume proposal_before
        create proposal_after = Proposal { proposal_id: proposal_before.proposal_id, state: Proposal::Active, proposer: proposal_before.proposer, for_votes: proposal_before.for_votes, against_votes: proposal_before.against_votes, end_timepoint: proposal_before.end_timepoint } with_lock(proposal_before.proposer)

}

action execute_proposal(proposal_before: Proposal, read config: DaoConfig) -> (proposal_after: Proposal, record: ExecutionRecord) {
    transition proposal_before.state: Active -> proposal_after.state: Executed
    verification
        let now = env::current_timepoint()
        require proposal_before.state == Proposal::Active, "not active"
        require now >= proposal_before.end_timepoint, "voting not closed"
        require proposal_before.for_votes > proposal_before.against_votes, "not enough for-votes"
        consume proposal_before
        create proposal_after = Proposal { proposal_id: proposal_before.proposal_id, state: Proposal::Executed, proposer: proposal_before.proposer, for_votes: proposal_before.for_votes, against_votes: proposal_before.against_votes, end_timepoint: proposal_before.end_timepoint } with_lock(proposal_before.proposer)
        create record = ExecutionRecord { proposal_id: proposal_before.proposal_id, executed_at: now, for_votes: proposal_before.for_votes, against_votes: proposal_before.against_votes }

}
"#,
    )
    .unwrap();

    let check = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").output().unwrap();
    assert!(check.status.success(), "check failed: {}", String::from_utf8_lossy(&check.stderr));

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").output().unwrap();
    assert!(build.status.success(), "build failed: {}", String::from_utf8_lossy(&build.stderr));
    let metadata: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("build").join("main.s.meta.json")).unwrap()).unwrap();
    let proposal_layout = metadata["template_layouts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|layout| layout["type_name"] == "Proposal")
        .expect("Proposal template layout");
    assert_eq!(proposal_layout["schema"], "cellscript-template-layout-v0.21");
    // Linear flow => PathOnlyAllowed + acyclic.
    assert_eq!(proposal_layout["cycle_policy"], "PathOnlyAllowed");
    assert_eq!(proposal_layout["state_machine_acyclic"], true);

    let audit = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("audit-bundle")
        .args(["--output"])
        .arg(root.join("audit"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(audit.status.success(), "{}", String::from_utf8_lossy(&audit.stderr));
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("audit").join("audit-bundle.json")).unwrap()).unwrap();
    assert_eq!(bundle["protocol_graph"]["schema"], "cellscript-protocol-graph-v0.22");
    // The audit bundle must carry the Active -> Executed and Active -> Defeated edges.
    let edges = bundle["protocol_graph"]["edges"].as_array().unwrap();
    assert!(
        edges.iter().any(|edge| { edge["action_name"] == "execute_proposal" && edge["derivation"] == "state-transition" }),
        "expected execute_proposal state-transition edge: {bundle}"
    );

    // The DAO exposes three entry actions, so action build for each must succeed and
    // surface scan selectors under the v0.21 schema.
    for action in ["propose", "activate_proposal", "execute_proposal"] {
        let action_build = Command::new(env!("CARGO_BIN_EXE_cellc"))
            .current_dir(root)
            .arg("action")
            .arg("build")
            .arg("--action")
            .arg(action)
            .arg("--json")
            .output()
            .unwrap();
        assert!(action_build.status.success(), "{action} build failed: {}", String::from_utf8_lossy(&action_build.stderr));
        let plan: serde_json::Value = serde_json::from_slice(&action_build.stdout).unwrap();
        assert_eq!(plan["action_scan_selectors"]["schema"], "cellscript-action-scan-selectors-v0.21", "{action} selector schema");
    }
}

#[test]
fn cellc_entry_witness_subcommand_emits_parameterized_witness_json() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action main(amount: u64) -> u64 {
    verification
        return amount
}
"#,
    )
    .unwrap();

    let output_path = root.join("witness.bin");
    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("entry-witness")
        .arg("--action")
        .arg("main")
        .arg("--arg")
        .arg("77")
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["status"], "ok");
    assert_eq!(stdout["abi"], "cellscript-entry-witness-v1");
    assert_eq!(stdout["placement_abi"], "cellscript-witnessargs-input-type-v2");
    assert_eq!(stdout["witness_args_field"], "input_type");
    assert_eq!(stdout["witness_source"], "group-input-0-then-group-output-0");
    assert_eq!(stdout["raw_v1_compatible"], false);
    assert_eq!(stdout["entry_kind"], "action");
    assert_eq!(stdout["entry"], "main");
    assert_eq!(stdout["witness_hex"], "43534152477631004d00000000000000");
    assert_eq!(stdout["witness_size_bytes"], 16);
    assert_eq!(stdout["payload_params"][0], "amount");
    assert_eq!(stdout["payload_args"], 1);

    let mut expected = b"CSARGv1\0".to_vec();
    expected.extend_from_slice(&77u64.to_le_bytes());
    assert_eq!(std::fs::read(output_path).unwrap(), expected);
}

#[test]
fn cellc_entry_witness_subcommand_encodes_bundled_token_amm_bootstrap_payloads() {
    let fixture = fixtures::examples();
    let examples = fixture.path();
    let launch = examples.join("launch.cell");
    let token = examples.join("token.cell");
    let amm_pool = examples.join("amm_pool.cell");
    let address = "0x1111111111111111111111111111111111111111111111111111111111111111";
    let distribution = format!("0x{}", "22".repeat(160));

    let launch_output = cellc_command()
        .arg("entry-witness")
        .arg(&launch)
        .arg("--target-profile")
        .arg("ckb")
        .arg("--action")
        .arg("launch_token")
        .arg("--arg")
        .arg("0x4c41554e43483031")
        .arg("--arg")
        .arg("10000")
        .arg("--arg")
        .arg("1000")
        .arg("--arg")
        .arg("500")
        .arg("--arg")
        .arg("30")
        .arg("--arg")
        .arg(address)
        .arg("--arg")
        .arg(&distribution)
        .arg("--json")
        .output()
        .unwrap();
    assert!(launch_output.status.success(), "stderr: {}", String::from_utf8_lossy(&launch_output.stderr));
    let launch_stdout: serde_json::Value = serde_json::from_slice(&launch_output.stdout).unwrap();
    assert_eq!(launch_stdout["status"], "ok");
    assert_eq!(launch_stdout["entry"], "launch_token");
    assert_eq!(launch_stdout["payload_args"], 7);
    assert_eq!(launch_stdout["witness_size_bytes"], 234);
    assert_eq!(launch_stdout["payload_params"][0], "symbol");
    assert_eq!(launch_stdout["payload_params"][4], "fee_rate_bps");
    assert_eq!(launch_stdout["payload_params"][6], "distribution");

    let token_output = cellc_command()
        .arg("entry-witness")
        .arg(&token)
        .arg("--target-profile")
        .arg("ckb")
        .arg("--action")
        .arg("mint_with_authority")
        .arg("--arg")
        .arg(address)
        .arg("--arg")
        .arg("25")
        .arg("--json")
        .output()
        .unwrap();
    assert!(token_output.status.success(), "stderr: {}", String::from_utf8_lossy(&token_output.stderr));
    let token_stdout: serde_json::Value = serde_json::from_slice(&token_output.stdout).unwrap();
    assert_eq!(token_stdout["status"], "ok");
    assert_eq!(token_stdout["entry"], "mint_with_authority");
    assert_eq!(token_stdout["payload_params"][0], "to");
    assert_eq!(token_stdout["payload_params"][1], "amount");
    assert_eq!(token_stdout["witness_size_bytes"], 48);

    let seed_output = cellc_command()
        .arg("entry-witness")
        .arg(&amm_pool)
        .arg("--target-profile")
        .arg("ckb")
        .arg("--action")
        .arg("seed_pool")
        .arg("--arg")
        .arg("30")
        .arg("--arg")
        .arg(address)
        .arg("--json")
        .output()
        .unwrap();
    assert!(seed_output.status.success(), "stderr: {}", String::from_utf8_lossy(&seed_output.stderr));
    let seed_stdout: serde_json::Value = serde_json::from_slice(&seed_output.stdout).unwrap();
    assert_eq!(seed_stdout["status"], "ok");
    assert_eq!(seed_stdout["entry"], "seed_pool");
    assert_eq!(seed_stdout["payload_params"][0], "fee_rate_bps");
    assert_eq!(seed_stdout["payload_params"][1], "provider");
    assert_eq!(seed_stdout["witness_size_bytes"], 42);

    let swap_output = cellc_command()
        .arg("entry-witness")
        .arg(&amm_pool)
        .arg("--target-profile")
        .arg("ckb")
        .arg("--action")
        .arg("swap_a_for_b")
        .arg("--arg")
        .arg("2")
        .arg("--arg")
        .arg(address)
        .arg("--json")
        .output()
        .unwrap();
    assert!(swap_output.status.success(), "stderr: {}", String::from_utf8_lossy(&swap_output.stderr));
    let swap_stdout: serde_json::Value = serde_json::from_slice(&swap_output.stdout).unwrap();
    assert_eq!(swap_stdout["status"], "ok");
    assert_eq!(swap_stdout["entry"], "swap_a_for_b");
    assert_eq!(swap_stdout["payload_params"][0], "min_output");
    assert_eq!(swap_stdout["payload_params"][1], "to");
    assert_eq!(swap_stdout["witness_size_bytes"], 48);
}

#[test]
fn cellc_abi_subcommand_explains_entry_witness_layout() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

struct Snapshot {
    amount: u64,
}

action main(snapshot: Snapshot, amount: u64) -> u64 {
    verification
        return amount
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("abi").arg("--action").arg("main").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["status"], "ok");
    assert_eq!(stdout["abi"], "cellscript-entry-witness-v1");
    assert_eq!(stdout["entry_kind"], "action");
    assert_eq!(stdout["entry"], "main");
    assert_eq!(stdout["payload_params"][0], "snapshot");
    assert_eq!(stdout["payload_params"][1], "amount");
    assert_eq!(stdout["layout"]["abi_slots_used"], 3);
    assert_eq!(stdout["layout"]["min_witness_bytes"], 20);
    assert_eq!(stdout["params"][0]["name"], "snapshot");
    assert_eq!(stdout["params"][0]["abi_kind"], "schema-pointer");
    assert_eq!(stdout["params"][0]["witness_bytes"], 4);
    assert_eq!(stdout["params"][0]["slot_start"], 0);
    assert_eq!(stdout["params"][0]["slot_end"], 1);
    assert_eq!(stdout["params"][1]["name"], "amount");
    assert_eq!(stdout["params"][1]["abi_kind"], "scalar");
    assert_eq!(stdout["params"][1]["witness_bytes"], 8);
}

#[test]
fn cellc_entry_witness_subcommand_encodes_schema_backed_params() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

struct Snapshot {
    amount: u64,
}

action main(snapshot: Snapshot, amount: u64) -> u64 {
    verification
        return amount
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("entry-witness")
        .arg("--action")
        .arg("main")
        .arg("--arg")
        .arg("0500000000000000")
        .arg("--arg")
        .arg("5")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["witness_hex"], "43534152477631000800000005000000000000000500000000000000");
    assert_eq!(stdout["payload_params"][0], "snapshot");
    assert_eq!(stdout["payload_params"][1], "amount");
}

#[test]
fn cellc_entry_witness_subcommand_rejects_wrong_width_fixed_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action owned(owner: Address) -> u64 {
    verification
        return 0
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("entry-witness")
        .arg("--action")
        .arg("owned")
        .arg("--arg")
        .arg("0x010203")
        .arg("--json")
        .output()
        .unwrap();
    assert!(!output.status.success(), "stdout: {}", String::from_utf8_lossy(&output.stdout));
    assert!(output.stderr.is_empty(), "unexpected stderr: {}", String::from_utf8_lossy(&output.stderr));
    let failure: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(failure["diagnostics"][0]["message"]
        .as_str()
        .is_some_and(|message| message.contains("parameter 'owner' expects 32 byte(s), got 3")));
}

#[test]
fn cellc_inspection_rejects_unknown_schema_versions() {
    let dir = tempfile::tempdir().unwrap();
    let package = write_workspace_member(dir.path(), "app", "app", &[]);
    for command in ["resolve-graph", "build-plan"] {
        let output = cellc_command().current_dir(&package).args([command, "--schema-version", "3", "--json"]).output().unwrap();
        assert!(!output.status.success());
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(report["diagnostics"][0]["message"].as_str().unwrap().contains("schema version 3"));
    }
}

#[test]
fn cellc_update_plan_rejects_unknown_schema_and_precise_path_substitution() {
    let dir = tempfile::tempdir().unwrap();
    let dependency = write_workspace_member(dir.path(), "dependency", "dependency", &[]);
    let app = write_workspace_member(dir.path(), "app", "app", &[("dependency", "../dependency")]);
    lock_package(&app);

    let schema = cellc_command().current_dir(&app).args(["update-plan", "--schema-version", "999", "--json"]).output().unwrap();
    assert!(!schema.status.success());
    let report: serde_json::Value = serde_json::from_slice(&schema.stdout).unwrap();
    assert_eq!(report["diagnostics"][0]["code"], "E2800");

    let precise = cellc_command()
        .current_dir(&app)
        .args(["update-plan", "--package", "dependency", "--precise", "1.0.0", "--offline", "--json"])
        .output()
        .unwrap();
    assert!(!precise.status.success());
    let report: serde_json::Value = serde_json::from_slice(&precise.stdout).unwrap();
    assert!(report["diagnostics"][0]["message"].as_str().unwrap().contains("immutable path or git source"));
    assert!(dependency.join("Cell.toml").exists());
}

#[test]
fn cellc_build_writes_lockfile_deployment_ref_from_deployed_toml() {
    // `cellc build` is the canonical place where Cell.lock gets refreshed.
    // When a Deployed.toml is present, build must bridge its deployment
    // records into the lockfile so that `cellc registry verify` does not
    // always fail with "deployment for network 'X' is missing from Cell.lock".
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"

[dependencies]
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Token has store, replace, relock, consume {
    amount: u64,
}

action mint(amount: u64) -> Token {
    verification
        create Token { amount: amount }
}
"#,
    )
    .unwrap();

    // First build without Deployed.toml to capture the locked build identity.
    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").output().unwrap();
    assert!(build.status.success(), "stderr: {}", String::from_utf8_lossy(&build.stderr));

    let lockfile: cellscript::package::Lockfile = toml::from_str(&std::fs::read_to_string(root.join("Cell.lock")).unwrap()).unwrap();
    assert!(lockfile.package_build.is_some(), "Cell.lock must carry build identity");
    assert!(lockfile.deployment.is_empty(), "no deployment section when Deployed.toml is absent");

    // Now write a Deployed.toml that matches the locked build identity and
    // build again. The lockfile should now carry a [deployment.devnet] entry.
    let artifact_hash = lockfile.package_build.as_ref().unwrap().artifact_hash.as_deref().unwrap();
    let metadata_hash = lockfile.package_build.as_ref().unwrap().metadata_hash.as_deref().unwrap();
    let schema_hash = lockfile.package_build.as_ref().unwrap().schema_hash.as_deref().unwrap();
    let cell_data_codec_manifest_hash = lockfile.package_build.as_ref().unwrap().cell_data_codec_manifest_hash.as_deref().unwrap();
    let abi_hash = lockfile.package_build.as_ref().unwrap().abi_hash.as_deref().unwrap();
    let constraints_hash = lockfile.package_build.as_ref().unwrap().constraints_hash.as_deref().unwrap();
    let compatibility_profile_hash = lockfile.package_build.as_ref().unwrap().compatibility_profile_hash.as_str();
    let source_hash = lockfile.package.source_hash.as_deref().unwrap();
    let compiler_version = lockfile.package_build.as_ref().unwrap().compiler_version.as_deref().unwrap();
    let deployed = format!(
        r#"version = 2
schema = "cellscript-deployed-v2"

[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
source_hash = "{source_hash}"

[build]
edition = "2027"
compatibility_profile_hash = "{compatibility_profile_hash}"
compiler_version = "{compiler_version}"
artifact_hash = "{artifact_hash}"
metadata_hash = "{metadata_hash}"
schema_hash = "{schema_hash}"
cell_data_codec_manifest_hash = "{cell_data_codec_manifest_hash}"
abi_hash = "{abi_hash}"
constraints_hash = "{constraints_hash}"

[[deployments]]
edition = "2027"
compatibility_profile_hash = "{compatibility_profile_hash}"
name = "demo-mock"
status = "active"
network = "devnet"
chain_id = "ckb-devnet"
tx_hash = "0x0000000000000000000000000000000000000000000000000000000000000001"
output_index = 0
code_hash = "{artifact_hash}"
data_hash = "{artifact_hash}"
hash_type = "data1"
dep_type = "code"
out_point = "0x0000000000000000000000000000000000000000000000000000000000000001:0"
artifact_hash = "{artifact_hash}"
metadata_hash = "{metadata_hash}"
schema_hash = "{schema_hash}"
cell_data_codec_manifest_hash = "{cell_data_codec_manifest_hash}"
abi_hash = "{abi_hash}"
constraints_hash = "{constraints_hash}"
compiler_version = "{compiler_version}"
"#
    );
    std::fs::write(root.join("Deployed.toml"), deployed).unwrap();

    let build2 = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").output().unwrap();
    assert!(build2.status.success(), "stderr: {}", String::from_utf8_lossy(&build2.stderr));

    let lockfile2: cellscript::package::Lockfile = toml::from_str(&std::fs::read_to_string(root.join("Cell.lock")).unwrap()).unwrap();
    let devnet = lockfile2
        .deployment
        .get("devnet")
        .expect("Cell.lock must carry a [deployment.devnet] entry after build bridges Deployed.toml");
    assert_eq!(devnet.record, "0x0000000000000000000000000000000000000000000000000000000000000001:0");
    assert_eq!(devnet.code_hash.as_deref(), Some(artifact_hash));
    assert_eq!(devnet.data_hash.as_deref(), Some(artifact_hash));
    assert_eq!(devnet.out_point.as_deref(), Some("0x0000000000000000000000000000000000000000000000000000000000000001:0"));
    assert!(devnet.record_hash.is_some(), "record_hash must be computed for build-identity-matching deployment");

    // Finally, registry verify on this clean fixture must succeed.
    let verify =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("registry").arg("verify").arg("--json").output().unwrap();
    assert!(
        verify.status.success(),
        "registry verify must pass after build bridges Deployed.toml: stderr: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&verify.stdout).unwrap();
    assert_eq!(report["status"], "ok");
    assert_eq!(report["violations"].as_array().map(|a| a.len()).unwrap_or(0), 0);
}

#[test]
fn cellc_build_omits_lockfile_deployment_when_artifact_hash_mismatches() {
    // When the Deployed.toml artifact_hash disagrees with the locked build
    // identity, the deployment ref must be written with hash fields left None
    // so that `registry verify` reports a deterministic build-identity mismatch
    // violation rather than silently agreeing.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"

[dependencies]
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Token has store, replace, relock, consume {
    amount: u64,
}

action mint(amount: u64) -> Token {
    verification
        create Token { amount: amount }
}
"#,
    )
    .unwrap();

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").output().unwrap();
    assert!(build.status.success(), "stderr: {}", String::from_utf8_lossy(&build.stderr));

    // Deployed.toml with a wrong artifact_hash. The record field still points
    // at the out_point, but the code/out_point/data/record_hash fields must
    // be left None so the verifier can surface the build-identity mismatch.
    let deployed = r#"version = 2
schema = "cellscript-deployed-v2"

[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
source_hash = "fake"

[build]
edition = "2027"
compatibility_profile_hash = "mismatched-profile"
compiler_version = "0.17.0"
artifact_hash = "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
metadata_hash = "0x00"
schema_hash = "0x00"
abi_hash = "0x00"
constraints_hash = "0x00"

[[deployments]]
edition = "2027"
compatibility_profile_hash = "mismatched-profile"
name = "demo-mock"
status = "active"
network = "devnet"
chain_id = "ckb-devnet"
tx_hash = "0x0000000000000000000000000000000000000000000000000000000000000001"
output_index = 0
code_hash = "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
data_hash = "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
hash_type = "data1"
dep_type = "code"
out_point = "0x0000000000000000000000000000000000000000000000000000000000000001:0"
artifact_hash = "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
"#;
    std::fs::write(root.join("Deployed.toml"), deployed).unwrap();

    let build2 = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").output().unwrap();
    assert!(build2.status.success(), "stderr: {}", String::from_utf8_lossy(&build2.stderr));

    let lockfile: cellscript::package::Lockfile = toml::from_str(&std::fs::read_to_string(root.join("Cell.lock")).unwrap()).unwrap();
    let devnet =
        lockfile.deployment.get("devnet").expect("Cell.lock must still record a deployment ref even when build identity mismatches");
    assert_eq!(devnet.record, "0x0000000000000000000000000000000000000000000000000000000000000001:0");
    assert!(devnet.code_hash.is_none());
    assert!(devnet.out_point.is_none());
    assert!(devnet.data_hash.is_none());
    assert!(devnet.record_hash.is_none());

    let verify =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("registry").arg("verify").arg("--json").output().unwrap();
    let report: serde_json::Value = serde_json::from_slice(&verify.stdout).unwrap();
    assert_eq!(report["status"], "failed");
    let violations = report["violations"].as_array().unwrap();
    // The ref carries no code_hash/out_point/data_hash/record_hash because
    // the build identity did not match, so the verifier must surface at least
    // one of the deterministic "no <field>" violations from the lockfile ref.
    assert!(
        violations.iter().any(|v| {
            let s = v.as_str().unwrap_or("");
            s.contains("has no code_hash")
                || s.contains("has no out_point")
                || s.contains("has no data_hash")
                || s.contains("has no record_hash")
        }),
        "expected a 'has no <hash>' violation from the mismatched ref, got: {violations:?}"
    );
    // Additionally, the top-level build-identity comparison must surface the
    // artifact_hash disagreement.
    assert!(
        violations.iter().any(|v| v.as_str().unwrap_or("").contains("artifact_hash mismatch")),
        "expected artifact_hash mismatch violation, got: {violations:?}"
    );
}
