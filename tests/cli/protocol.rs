// CLI protocol behavior; included in the original cli integration target.

#[test]
fn cellc_audit_bundle_embeds_protocol_graph() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("pool.cell");
    let output_dir = dir.path().join("audit");
    std::fs::write(&input, protocol_graph_pool_source()).unwrap();

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
    assert_eq!(bundle["template_layouts"][0]["schema"], "cellscript-template-layout-v0.21");
    assert_eq!(bundle["template_layouts"][0]["type_name"], "Pool");
    assert_eq!(bundle["template_layouts"][0]["layout"], "Flat");
    assert_eq!(bundle["template_layouts"][0]["consensus_checked"], false);
}

#[test]
fn protocol_bundle_checks_three_independent_artifacts_and_hashes_canonical_composition() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let sources = [
        (
            "order",
            r#"
module protocol::order

resource SharedRecord has store, create {
    amount: u64
}

action settle() -> bool {
    verification
        true
}
"#,
        ),
        (
            "token",
            r#"
module protocol::token

resource SharedRecord has store, create {
    amount: u64
}

action transfer() -> SharedRecord {
    verification
        create SharedRecord { amount: 1 }
}
"#,
        ),
        (
            "auth",
            r#"
module protocol::auth

resource SharedRecord has store, create {
    amount: u64
}

lock authorize(witness approved: bool) -> bool {
    verification
        approved
}
"#,
        ),
    ];

    let mut artifact_hashes = std::collections::BTreeMap::new();
    let mut metadata_values = std::collections::BTreeMap::new();
    for (name, source) in sources {
        let source_path = root.join(format!("{name}.cell"));
        let artifact_path = root.join(format!("{name}.elf"));
        std::fs::write(&source_path, source).unwrap();
        let build = Command::new(env!("CARGO_BIN_EXE_cellc"))
            .arg(&source_path)
            .args(["--target", "riscv64-elf", "-o"])
            .arg(&artifact_path)
            .output()
            .unwrap();
        assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
        let metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join(format!("{name}.elf.meta.json"))).unwrap()).unwrap();
        artifact_hashes.insert(name, metadata["artifact_hash"].as_str().unwrap().to_string());
        metadata_values.insert(name, metadata);
    }
    for (name, action) in [("order", "settle"), ("token", "transfer")] {
        let generated = Command::new(env!("CARGO_BIN_EXE_cellc"))
            .args(["gen-builder", "--metadata"])
            .arg(root.join(format!("{name}.elf.meta.json")))
            .args(["--target", "typescript"])
            .args(["--action", action, "--output"])
            .arg(root.join(format!("{name}-builder")))
            .arg("--json")
            .output()
            .unwrap();
        assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
    }
    let shared_schema_hashes = metadata_values
        .values()
        .map(|metadata| {
            metadata["molecule_schema_manifest"]["entries"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["type_name"] == "SharedRecord")
                .and_then(|entry| entry["schema_hash"].as_str())
                .unwrap()
                .to_string()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(shared_schema_hashes.len(), 1, "all participants must compile the identical SharedRecord schema");
    let shared_schema_hash = shared_schema_hashes.into_iter().next().unwrap();

    let network = serde_json::json!({
        "chain_id": "ckb-testnet",
        "genesis_hash": format!("0x{}", "0".repeat(64)),
    });
    let artifact = |id: &str, entry_kind: &str, entry_name: &str, role: &str, dep_byte: &str| {
        let artifact_hash = artifact_hashes[id].clone();
        let mut artifact = serde_json::json!({
            "id": id,
            "package_coordinate": format!("example/{id}@1.0.0"),
            "lock_node_id": format!("{id}@1.0.0|path:{id}|env=default|features=default"),
            "entry": { "kind": entry_kind, "name": entry_name },
            "script_role": role,
            "files": {
                "artifact": format!("{id}.elf"),
                "metadata": format!("{id}.elf.meta.json"),
                "lowering_record": format!("{id}.elf.lowering.json"),
                "source_map": format!("{id}.elf.sourcemap.json"),
            },
            "deployment": {
                "network": network.clone(),
                "artifact_hash": artifact_hash,
                "script": {
                    "code_hash": format!("0x{}", artifact_hashes[id]),
                    "hash_type": "data2",
                    "args": "0x",
                },
                "code_cell_dep": {
                    "out_point": { "tx_hash": format!("0x{}", dep_byte.repeat(64)), "index": 0 },
                    "dep_type": "code",
                },
            },
        });
        if entry_kind == "action" {
            artifact["files"]["builder_manifest"] = serde_json::json!(format!("{id}-builder/cellscript-builder-manifest.json"));
        }
        artifact
    };
    let order = artifact("order", "action", "settle", "type", "1");
    let token = artifact("token", "action", "transfer", "type", "2");
    let auth = artifact("auth", "lock", "authorize", "lock", "3");
    let order_script = order.pointer("/deployment/script").unwrap().clone();
    let token_script = token.pointer("/deployment/script").unwrap().clone();
    let auth_script = auth.pointer("/deployment/script").unwrap().clone();
    let cell_deps = [&order, &token, &auth]
        .into_iter()
        .map(|artifact| artifact.pointer("/deployment/code_cell_dep").unwrap().clone())
        .collect::<Vec<_>>();
    let cell = |commitment_byte: &str, capacity: u64, lock: serde_json::Value, ty: serde_json::Value| {
        serde_json::json!({
            "cell_commitment": format!("0x{}", commitment_byte.repeat(64)),
            "capacity": capacity,
            "lock": lock,
            "type": ty,
            "data": "0x",
        })
    };
    let mut builder_assumption_evidence = serde_json::Map::new();
    for metadata in metadata_values.values() {
        let Some(assumptions) = metadata["runtime"]["builder_assumptions"].as_array() else {
            continue;
        };
        for assumption in assumptions {
            if assumption["kind"] != "capacity_policy" {
                continue;
            }
            let assumption_id = assumption["assumption_id"].as_str().unwrap();
            builder_assumption_evidence.insert(
                assumption_id.to_string(),
                serde_json::json!({
                    "assumption_id": assumption_id,
                    "kind": assumption["kind"].clone(),
                    "origin": assumption["origin"].clone(),
                    "feature": assumption["feature"].clone(),
                    "proof_plan_status": assumption["proof_plan_status"].clone(),
                    "evidence": {
                        "outputs": [{ "index": 0, "capacity": 80_000_000_000u64 }],
                        "occupied_capacity_shannons": 80_000_000_000u64,
                        "tx_size_bytes": 1,
                        "under_capacity_output_indexes": [],
                    },
                }),
            );
        }
    }
    assert!(!builder_assumption_evidence.is_empty(), "the creating action must expose a capacity-policy assumption");
    let mut input_cell = cell("4", 100_000_000_000, auth_script.clone(), order_script.clone());
    input_cell["out_point"] = serde_json::json!({ "tx_hash": format!("0x{}", "4".repeat(64)), "index": 0 });
    input_cell["since"] = serde_json::json!(0);
    let mut manifest = serde_json::json!({
        "schema": "cellscript-protocol-bundle-input-v1",
        "network": network,
        "artifacts": [order, token, auth],
        "transaction": {
            "version": 0,
            "inputs": [input_cell],
            "outputs": [cell("5", 80_000_000_000u64, auth_script.clone(), token_script.clone())],
            "witnesses": [{}],
            "cell_deps": cell_deps.clone(),
            "header_deps": [],
            "fee_policy_hash": format!("0x{}", "6".repeat(64)),
            "change_policy_hash": format!("0x{}", "7".repeat(64)),
            "builder_assumption_evidence": builder_assumption_evidence,
        },
        "roles": [
            {
                "artifact": "order", "name": "order-input", "location": "input", "index": 0,
                "ownership": "exclusive", "expected_type": order_script,
                "cell_commitment": format!("0x{}", "4".repeat(64)), "minimum_capacity": 100_000_000_000u64,
            },
            {
                "artifact": "auth", "name": "authorization", "location": "input", "index": 0,
                "ownership": "shared-read", "expected_lock": auth_script,
            },
            {
                "artifact": "token", "name": "token-output", "location": "output", "index": 0,
                "ownership": "exclusive", "expected_type": token_script,
                "resource_identity": "SharedRecord",
                "cell_commitment": format!("0x{}", "5".repeat(64)), "exact_capacity": 80_000_000_000u64,
            },
            {
                "artifact": "order", "name": "settlement-output", "location": "output", "index": 0,
                "ownership": "shared-read", "expected_type": token_script,
                "resource_identity": "SharedRecord",
            },
            {
                "artifact": "auth", "name": "settlement-output", "location": "output", "index": 0,
                "ownership": "shared-read", "expected_type": token_script,
                "resource_identity": "SharedRecord",
            },
        ],
        "closed_roles": [{
            "schema": "cellscript-protocol-closed-role-v1",
            "role_id": "settlement-record",
            "kind": "cell",
            "schema_identity": { "type_name": "SharedRecord", "schema_hash": shared_schema_hash },
            "provider": { "artifact": "token", "claim": "token-output" },
            "consumers": [
                { "artifact": "order", "claim": "settlement-output" },
                { "artifact": "auth", "claim": "settlement-output" },
            ],
            "correspondence": "exact-physical-binding",
        }],
        "cell_deps": [
            { "artifact": "order", "name": "order-code", "index": 0, "cell_dep": cell_deps[0].clone() },
            { "artifact": "token", "name": "token-code", "index": 1, "cell_dep": cell_deps[1].clone() },
            { "artifact": "auth", "name": "auth-code", "index": 2, "cell_dep": cell_deps[2].clone() },
        ],
    });
    let manifest_path = root.join("protocol-bundle.json");
    std::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();

    let first = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .args(["protocol", "bundle", "check"])
        .arg(&manifest_path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&first.stderr),
        String::from_utf8_lossy(&first.stdout)
    );
    let (materialized, materialization) = cellscript_ckb_adapter::materialize_protocol_bundle_report(&first.stdout).unwrap();
    assert_eq!(materialized.inputs().len(), 1);
    assert_eq!(materialized.outputs().len(), 1);
    assert_eq!(materialization.state, "MaterializedProtocolBundleTx");
    assert_eq!(materialization.transaction_serialization, "verified");
    assert_eq!(materialization.script_groups.len(), 3);
    assert_eq!(materialization.live_input_expectations.len(), 1);
    assert_eq!(materialization.code_cell_dep_expectations.len(), 3);
    assert_eq!(materialization.capacity_source, "bundle-skeleton-not-live-resolved");
    assert!(materialization
        .script_groups
        .iter()
        .all(|group| group.transaction_bytes_hash == materialization.serialized_transaction_hash));
    assert!(materialization.script_groups.iter().all(|group| group.execution == "not-executed"));
    assert_eq!(materialization.fee_shannons, 20_000_000_000);
    assert_eq!(materialization.ckb_vm_execution, "not-executed");
    let first: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(first["status"], "ok");
    assert_eq!(first["bundle"]["artifacts"].as_array().unwrap().len(), 3);
    assert_eq!(first["bundle"]["closed_roles"].as_array().unwrap().len(), 1);
    assert_eq!(first["bundle"]["closed_roles"][0]["locality"], "closed-foreign");
    assert_eq!(first["bundle"]["closed_roles"][0]["provider"]["artifact"], "token");
    assert_eq!(first["bundle"]["closed_roles"][0]["consumers"].as_array().unwrap().len(), 2);
    assert!(first["bundle"]["closed_roles"][0]["provider"]["interface_hash"].as_str().is_some());
    assert!(first["bundle"]["closed_roles"][0]["provider"]["deployment"]["script"]["code_hash"].as_str().is_some());
    let identities = first["bundle"]["artifacts"].as_array().unwrap();
    for identity in identities {
        let identity: cellscript::protocol_bundle::ProtocolArtifactIdentity = serde_json::from_value(identity.clone()).unwrap();
        assert_eq!(identity.runtime_abi_hash, identity.exact_handle_receipt.runtime_abi_hash);
        assert_eq!(identity.interface_hash, identity.exact_handle_receipt.interface_hash);
        assert_eq!(identity.typed_semantics_hash, identity.exact_handle_receipt.typed_semantics_hash);
        assert_eq!(identity.target_profile_hash, identity.exact_handle_receipt.target_profile_hash);
        assert_eq!(identity.exact_handle.encoded.len(), 2 + cellscript::script_handle::EXACT_SCRIPT_HANDLE_BYTES * 2);
        cellscript::script_handle::validate_exact_script_handle(&identity.exact_handle_receipt, &identity.exact_handle).unwrap();
        assert_eq!(
            identity.exact_handle_hash,
            cellscript::script_handle::exact_script_handle_value_hash(&identity.exact_handle).unwrap()
        );
    }
    assert_eq!(
        first["bundle"]["closed_roles"][0]["provider"]["exact_handle"],
        identities.iter().find(|identity| identity["id"] == "token").unwrap()["exact_handle"]
    );
    assert_eq!(
        first["bundle"]["closed_roles"][0]["provider"]["exact_handle_hash"],
        identities.iter().find(|identity| identity["id"] == "token").unwrap()["exact_handle_hash"]
    );
    assert_eq!(
        first["bundle"]["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|artifact| artifact.get("builder_manifest_hash").is_some())
            .count(),
        2
    );
    assert_eq!(first["evidence"]["structural_verification"], "verified");
    assert!(first["evidence"]["metadata_transaction_validation"]
        .as_object()
        .unwrap()
        .values()
        .all(|validation| validation["status"] == "ok"));
    assert_eq!(first["evidence"]["ckb_vm_execution"], "not-executed");
    assert_eq!(first["conflicts"], serde_json::json!([]));

    manifest["artifacts"].as_array_mut().unwrap().reverse();
    manifest["roles"].as_array_mut().unwrap().reverse();
    std::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    let second = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .args(["protocol", "bundle", "check"])
        .arg(&manifest_path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(second.status.success(), "{}", String::from_utf8_lossy(&second.stderr));
    let second: serde_json::Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(first["bundle_hash"], second["bundle_hash"]);

    let valid_closed_role_schema_hash = manifest["closed_roles"][0]["schema_identity"]["schema_hash"].clone();
    manifest["closed_roles"][0]["schema_identity"]["schema_hash"] = serde_json::json!("8".repeat(64));
    std::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    let closed_role_conflict_path = root.join("protocol-closed-role-conflicts.json");
    let closed_role_conflict = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .args(["protocol", "bundle", "check"])
        .arg(&manifest_path)
        .arg("--output")
        .arg(&closed_role_conflict_path)
        .output()
        .unwrap();
    assert!(!closed_role_conflict.status.success(), "an unknown participant schema must fail before signing");
    let closed_role_report: serde_json::Value = serde_json::from_slice(&std::fs::read(closed_role_conflict_path).unwrap()).unwrap();
    assert!(closed_role_report["conflicts"].as_array().unwrap().iter().any(|conflict| conflict["code"] == "PB213"));
    manifest["closed_roles"][0]["schema_identity"]["schema_hash"] = valid_closed_role_schema_hash;

    let valid_builder_evidence = manifest["transaction"]["builder_assumption_evidence"].clone();
    manifest["transaction"]["builder_assumption_evidence"] = serde_json::json!({});
    std::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    let validation_path = root.join("protocol-builder-validation.json");
    let validation_conflict = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .args(["protocol", "bundle", "check"])
        .arg(&manifest_path)
        .arg("--output")
        .arg(&validation_path)
        .output()
        .unwrap();
    assert!(!validation_conflict.status.success(), "missing builder evidence must fail before signing");
    let validation_report: serde_json::Value = serde_json::from_slice(&std::fs::read(validation_path).unwrap()).unwrap();
    assert!(validation_report["conflicts"].as_array().unwrap().iter().any(|conflict| conflict["code"] == "PB212"));
    assert_eq!(validation_report["evidence"]["metadata_transaction_validation"]["token"]["status"], "failed");
    manifest["transaction"]["builder_assumption_evidence"] = valid_builder_evidence;

    let builder_manifest_path = root.join("order-builder/cellscript-builder-manifest.json");
    let mut builder_manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(&builder_manifest_path).unwrap()).unwrap();
    let created_outputs = {
        let settle = builder_manifest["actions"].as_array_mut().unwrap().iter_mut().find(|action| action["name"] == "settle").unwrap();
        let created_outputs = settle["created_outputs"].clone();
        settle["created_outputs"] = serde_json::json!(created_outputs.as_u64().unwrap() + 1);
        created_outputs
    };
    std::fs::write(&builder_manifest_path, serde_json::to_vec_pretty(&builder_manifest).unwrap()).unwrap();
    std::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    let tampered_builder = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .args(["protocol", "bundle", "check"])
        .arg(&manifest_path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!tampered_builder.status.success(), "tampered builder projections must fail artifact admission");
    let diagnostics =
        format!("{}{}", String::from_utf8_lossy(&tampered_builder.stdout), String::from_utf8_lossy(&tampered_builder.stderr));
    assert!(diagnostics.contains("builder action 'settle' field 'created_outputs'"), "unexpected diagnostics: {diagnostics}");
    let settle = builder_manifest["actions"].as_array_mut().unwrap().iter_mut().find(|action| action["name"] == "settle").unwrap();
    settle["created_outputs"] = created_outputs;
    std::fs::write(&builder_manifest_path, serde_json::to_vec_pretty(&builder_manifest).unwrap()).unwrap();

    let authorization = manifest["roles"].as_array_mut().unwrap().iter_mut().find(|role| role["artifact"] == "auth").unwrap();
    authorization["ownership"] = serde_json::json!("exclusive");
    std::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    let conflict_path = root.join("protocol-conflicts.json");
    let conflict = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .args(["protocol", "bundle", "check"])
        .arg(&manifest_path)
        .arg("--output")
        .arg(&conflict_path)
        .output()
        .unwrap();
    assert!(!conflict.status.success(), "conflicting exclusive roles must fail before signing");
    let conflict_report: serde_json::Value = serde_json::from_slice(&std::fs::read(conflict_path).unwrap()).unwrap();
    assert_eq!(conflict_report["status"], "failed");
    assert!(conflict_report["conflicts"].as_array().unwrap().iter().any(|conflict| conflict["code"] == "PB200"));
    assert_eq!(conflict_report["evidence"]["structural_verification"], "not-provided");
}

#[test]
fn cellc_receipt_sign_and_verify_binds_artifact_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("pool.cell");
    let output = dir.path().join("pool.s");
    let receipt = dir.path().join("receipt.json");
    let signed_receipt = dir.path().join("receipt.signed.json");
    let tampered_receipt = dir.path().join("receipt.tampered.json");
    let key = dir.path().join("ed25519.pkcs8");
    let source = r#"
module test

resource Pool has store {
    reserve: u64
}

action swap(input: Pool) -> output: Pool {
    verification
        require output.reserve == input.reserve
}
"#;
    std::fs::write(&input, source).unwrap();

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
    assert!(build.success());
    let metadata_path = dir.path().join("pool.s.meta.json");
    let metadata: serde_json::Value = serde_json::from_slice(&std::fs::read(&metadata_path).unwrap()).unwrap();

    let receipt_output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("receipt")
        .arg(&input)
        .args(["--output"])
        .arg(&receipt)
        .arg("--json")
        .output()
        .unwrap();
    assert!(receipt_output.status.success(), "{}", String::from_utf8_lossy(&receipt_output.stderr));
    let receipt_json: serde_json::Value = serde_json::from_slice(&std::fs::read(&receipt).unwrap()).unwrap();
    assert_eq!(receipt_json["schema"], "cellscript-compile-receipt-v4");
    assert_eq!(receipt_json["artifact_hash"], metadata["artifact_hash"]);
    assert!(receipt_json["template_layout_hash"].as_str().is_some_and(|hash| hash.len() == 64));

    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    std::fs::write(&key, pkcs8.as_ref()).unwrap();

    let sign = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("sign-receipt")
        .arg(&receipt)
        .args(["--role", "publisher", "--key"])
        .arg(&key)
        .args(["--output"])
        .arg(&signed_receipt)
        .arg("--json")
        .output()
        .unwrap();
    assert!(sign.status.success(), "{}", String::from_utf8_lossy(&sign.stderr));
    let signed_json: serde_json::Value = serde_json::from_slice(&std::fs::read(&signed_receipt).unwrap()).unwrap();
    assert_eq!(signed_json["signatures"][0]["algorithm"], "ed25519");
    assert_eq!(signed_json["signatures"][0]["role"], "publisher");

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-receipt")
        .arg(&signed_receipt)
        .args(["--metadata"])
        .arg(&metadata_path)
        .args(["--artifact"])
        .arg(&output)
        .arg("--json")
        .output()
        .unwrap();
    assert!(verify.status.success(), "{}", String::from_utf8_lossy(&verify.stderr));
    let verify_json: serde_json::Value = serde_json::from_slice(&verify.stdout).unwrap();
    assert_eq!(verify_json["signatures_verified"], 1);
    assert_eq!(verify_json["unsigned_advisory"], false);

    let verify_artifact = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(&output)
        .args(["--receipt"])
        .arg(&signed_receipt)
        .arg("--json")
        .output()
        .unwrap();
    assert!(verify_artifact.status.success(), "{}", String::from_utf8_lossy(&verify_artifact.stderr));
    let verify_artifact_json: serde_json::Value = serde_json::from_slice(&verify_artifact.stdout).unwrap();
    assert_eq!(verify_artifact_json["receipt_verified"], true);
    assert_eq!(verify_artifact_json["receipt_signatures_verified"], 1);

    let mut tampered = signed_json;
    tampered["metadata_hash"] = serde_json::json!("00".repeat(32));
    std::fs::write(&tampered_receipt, serde_json::to_vec_pretty(&tampered).unwrap()).unwrap();
    let reject = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-receipt")
        .arg(&tampered_receipt)
        .args(["--metadata"])
        .arg(&metadata_path)
        .args(["--artifact"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(!reject.status.success(), "unexpected success: {}", String::from_utf8_lossy(&reject.stdout));
    let stderr = String::from_utf8_lossy(&reject.stderr);
    assert!(stderr.contains("metadata_hash"), "unexpected stderr: {stderr}");
}

#[test]
fn cellc_action_build_emits_builder_plan_json() {
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

    let parse = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("--json")
        .arg("--parse")
        .arg("src/main.cell")
        .output()
        .unwrap();
    assert!(parse.status.success(), "stderr: {}", String::from_utf8_lossy(&parse.stderr));
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&parse.stdout).unwrap()["status"], "ok");

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("action")
        .arg("build")
        .arg("--action")
        .arg("mint")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["status"], "ok");
    assert_eq!(plan["policy"], "cellscript-action-builder-plan-v1");
    assert_eq!(plan["headless"], true);
    assert_eq!(plan["ui_scope"], "none");
    assert_eq!(plan["action"], "mint");
    assert_eq!(plan["target_profile"], "ckb");
    assert!(plan["entry_witness_abi"]["required"].as_bool().unwrap());
    assert_eq!(plan["builder_requirements"]["created_outputs"].as_array().unwrap().len(), 1);
    assert_eq!(plan["builder_requirements"]["action_scan_selectors"], plan["action_scan_selectors"]);
    let scan_selectors = &plan["action_scan_selectors"];
    assert_eq!(scan_selectors["schema"], "cellscript-action-scan-selectors-v0.21");
    assert_eq!(scan_selectors["source"], "transaction_runtime_input_requirements");
    assert_eq!(scan_selectors["evidence_level"], "compile-only");
    assert_eq!(scan_selectors["status"], "compile-checked-runtime-selectors");
    assert_eq!(scan_selectors["runtime_required_selector_count"], 0);
    assert!(scan_selectors["selector_count"].as_u64().is_some_and(|count| count >= 1));
    assert!(scan_selectors["non_claims"].as_array().is_some_and(|items| items.iter().any(|item| item == "does-not-query-live-cells")));
    let create_selector = scan_selectors["selectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|selector| selector["feature"].as_str().is_some_and(|feature| feature.starts_with("create-output:Token")))
        .expect("create-output selector");
    assert_eq!(create_selector["ckb_source"], "Output");
    assert_eq!(create_selector["role"], "transaction-output");
    assert_eq!(create_selector["selector"]["kind"], "output-cell-selector");
    assert_eq!(create_selector["selector"]["source"], "Output");
    assert_eq!(create_selector["requirement_status"], "checked-runtime");
    assert_eq!(create_selector["scan_status"], "verifier-covered");
    assert_eq!(create_selector["resolution"]["adapter_action"], "materialize-and-preserve-verifier-covered-shape");
    assert!(plan["ckb"]["capacity_evidence_contract"]["required"].as_bool().unwrap());
    assert_eq!(plan["transaction_draft"]["format"], "cellscript-ccc-transaction-draft-v1");
    assert_eq!(plan["transaction_draft"]["state"], "ActionPlan");
    assert_eq!(plan["transaction_draft"]["ccc_compatible"], true);
    assert_eq!(plan["transaction_draft"]["can_submit"], false);
    assert_eq!(plan["transaction_draft"]["ckb_vm_execution"], false);
    assert_eq!(plan["transaction_draft"]["tx_pool_acceptance"], false);
    assert_eq!(plan["transaction_draft"]["requires_live_cell_resolution"], true);
    assert_eq!(plan["transaction_draft"]["requires_packed_materialization"], true);
    assert_eq!(plan["transaction_draft"]["packed_materialization"]["transaction"], "ckb_types::packed::Transaction");
    assert_eq!(plan["transaction_draft"]["packed_materialization"]["script"], "ckb_types::packed::Script");
    assert_eq!(plan["transaction_draft"]["packed_materialization"]["out_point"], "ckb_types::packed::OutPoint");
    assert_eq!(plan["transaction_draft"]["packed_materialization"]["realizer"], "cellscript-ckb-adapter via ckb-sdk-rust or CCC");
    assert!(plan["transaction_draft"]["required_evidence"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item == "tx_pool_acceptance")));
    assert_eq!(plan["adapter_contract"]["schema"], "cellscript-ckb-adapter-contract-v0.19");
    assert_eq!(plan["adapter_contract"]["compiler_core_dependency"], "no-ckb-sdk-rust");
    assert_eq!(plan["adapter_contract"]["compiler_output_state"], "ActionPlan");
    assert_eq!(plan["adapter_contract"]["adapter_output_state"], "ResolvedActionTx");
    assert_eq!(plan["adapter_contract"]["accepted_output_state"], "AcceptedActionTx");
    assert_eq!(plan["adapter_contract"]["must_not_infer_protocol_semantics_from_action_name"], true);
    assert_eq!(plan["adapter_contract"]["witness_policy"]["entry_payload_abi"], "cellscript-entry-witness-v1");
    assert_eq!(plan["adapter_contract"]["witness_policy"]["placement_abi"], "cellscript-witnessargs-input-type-v2");
    assert_eq!(plan["adapter_contract"]["witness_policy"]["default_action_payload_field"], "input_type");
    assert_eq!(plan["adapter_contract"]["witness_policy"]["runtime_source"], "group-input-0-then-group-output-0");
    assert_eq!(plan["adapter_contract"]["witness_policy"]["raw_v1_compatible"], false);
    assert_eq!(plan["adapter_contract"]["witness_policy"]["lock_signature_policy"], "explicit-adapter-owned-do-not-overwrite");
    assert!(plan["adapter_contract"]["resolved_tx_required_fields"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item == "outputs_data") && items.iter().any(|item| item == "lineage")));
    assert_eq!(plan["adapter_contract"]["acceptance_report_template"]["schema"], "cellscript-ckb-action-acceptance-report-v0.19");
    assert_eq!(plan["adapter_contract"]["acceptance_report_template"]["state"], "AcceptedActionTx");
    assert_eq!(plan["adapter_contract"]["acceptance_report_template"]["action_selector"], "mint");
    assert!(plan["adapter_contract"]["acceptance_report_template"]["metadata_hash"].as_str().is_some_and(|hash| hash.len() == 64));
    assert!(plan["adapter_contract"]["acceptance_report_template"]["known_limitations"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item.as_str().is_some_and(|text| text.contains("Template only")))));
    assert_eq!(plan["preview"]["format"], "cellscript-action-preview-v1");
    assert_eq!(plan["preview"]["action"], "mint");
    assert!(plan["preview"]["warnings"].as_array().is_some_and(|warnings| !warnings.is_empty()));
}

#[test]
fn cellc_action_build_emits_runtime_required_scan_selectors() {
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
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Token has store {
    amount: u64,
}

action withdraw(token: Token, fee: u64) -> Token {
    verification
        let amount = token.amount
        let remaining = amount - fee
        consume token
        let out = create Token {
            amount: remaining
        }
        return out
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("action")
        .arg("build")
        .arg("--action")
        .arg("withdraw")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let scan_selectors = &plan["action_scan_selectors"];
    assert_eq!(scan_selectors["schema"], "cellscript-action-scan-selectors-v0.21");
    assert_eq!(scan_selectors["status"], "requires-runtime-resolution");
    assert_eq!(scan_selectors["runtime_required_selector_count"], 1);
    let selector = scan_selectors["selectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|selector| selector["feature"] == "resource-conservation:Token")
        .expect("Token resource-conservation selector");
    assert_eq!(selector["requirement_status"], "runtime-required");
    assert_eq!(selector["scan_status"], "requires-runtime-resolution");
    assert_eq!(selector["ckb_source"], "Transaction");
    assert_eq!(selector["selector"]["kind"], "transaction-selector");
    assert_eq!(selector["resolution"]["blocker_class"], "resource-conservation-proof-gap");
    assert_eq!(selector["resolution"]["adapter_action"], "resolve-or-reject-before-signing");
}

#[test]
fn cellc_action_build_emits_cellfabric_intent_envelope() {
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

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("action")
        .arg("build")
        .arg("--action")
        .arg("mint")
        .arg("--fabric-intent")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["schema"], "cellscript-cellfabric-intent-envelope-v0.20");
    assert_eq!(envelope["status"], "requires-runtime-binding");
    assert_eq!(envelope["bridge_boundary"]["kind"], "json-bridge");
    assert_eq!(envelope["bridge_boundary"]["cellscript_core_dependency"], "no-cell-fabric-rust-crate");
    assert_eq!(envelope["bridge_boundary"]["not_a_cellfabric_signed_intent"], true);
    assert_eq!(envelope["bridge_boundary"]["not_a_soft_confirmation"], true);
    assert_eq!(envelope["bridge_boundary"]["not_l1_finality"], true);

    let action_plan_hash = envelope["source"]["action_plan_hash"].as_str().expect("action plan hash");
    assert_eq!(action_plan_hash.len(), 64);
    assert_eq!(envelope["source"]["action"], "mint");
    assert_eq!(envelope["source"]["target_profile"], "ckb");
    assert_eq!(envelope["cellfabric_mapping"]["candidate_intent_action"], "App");
    assert_eq!(envelope["cellfabric_intent_template"]["domain"]["chain_id"], "ckb");
    assert_eq!(envelope["cellfabric_intent_template"]["action"]["kind"], "App");
    assert_eq!(envelope["cellfabric_intent_template"]["action"]["action"], "mint");
    assert_eq!(envelope["cellfabric_intent_template"]["action"]["payload_format"], "cellscript-action-plan-json-v1");
    assert_eq!(envelope["cellfabric_intent_template"]["action"]["payload_hash"], action_plan_hash);
    assert_eq!(envelope["cellfabric_intent_template"]["resources"]["status"], "template-only-runtime-outpoints-required");
    assert_eq!(envelope["cellfabric_intent_template"]["author"]["lock_script_hash"], serde_json::Value::Null);
    assert_eq!(envelope["cellfabric_intent_template"]["auth_mode"], "CoSignConcreteTx");
    assert!(envelope["resource_access_template"]["hard_conflicts"]["runtime_input_requirements"].as_array().is_some());
    assert!(envelope["resource_access_template"]["app_conflict_key_templates"].as_array().is_some());
    assert!(envelope["required_runtime_evidence"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item == "resolved_consumed_outpoints")
            && items.iter().any(|item| item == "l1_status_observation")));
    assert!(envelope["non_claims"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item.as_str().is_some_and(|text| text.contains("does not soft-confirm")))));
    assert_eq!(envelope["action_plan"]["policy"], "cellscript-action-builder-plan-v1");
    assert_eq!(envelope["action_plan"]["transaction_draft"]["state"], "ActionPlan");
}

#[test]
fn cellc_generated_builder_without_schemas_imports_and_creates_a_plan() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("empty_schema.cell");
    std::fs::write(&source, "module empty_schema\naction ping() -> u64 { verification return 0 }\n").unwrap();
    let package = temp.path().join("sdk");
    let generated = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .args(["gen-builder", "--target", "typescript", "--target-profile", "ckb"])
        .arg(&source)
        .arg("--output")
        .arg(&package)
        .output()
        .unwrap();
    assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
    let smoke = package.join("smoke.mjs");
    std::fs::write(
        &smoke,
        "import assert from 'node:assert/strict';\n\
         import { protocolBundleArtifactIdentity, planPing } from './src/index.ts';\n\
         assert.deepEqual(protocolBundleArtifactIdentity.schemaContracts, []);\n\
         assert.equal(planPing({}).canSubmit, false);\n",
    )
    .unwrap();
    let imported = Command::new("node")
        .arg("--experimental-strip-types")
        .arg(&smoke)
        .output()
        .expect("generated-builder execution requires Node with TypeScript stripping, as do the ZK gate vectors");
    assert!(imported.status.success(), "generated SDK import failed: {}", String::from_utf8_lossy(&imported.stderr));
    let generated_tests =
        std::fs::read_to_string(package.join("test/builder.test.mjs")).unwrap().replace("../dist/index.js", "../src/index.ts");
    let test_path = package.join("test/schema-free.test.mjs");
    std::fs::write(&test_path, generated_tests).unwrap();
    let tested = Command::new("node").args(["--experimental-strip-types", "--test"]).arg(test_path).output().unwrap();
    assert!(
        tested.status.success(),
        "generated schema-free SDK tests failed: {}\n{}",
        String::from_utf8_lossy(&tested.stdout),
        String::from_utf8_lossy(&tested.stderr)
    );
}

#[test]
fn cellc_gen_builder_typescript_emits_package_scaffold() {
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
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Token has store, replace, relock, consume {
    amount: u64,
}

action mint(amount: u64, owner: Address) -> Token {
    verification
        create Token { amount: amount }
}
"#,
    )
    .unwrap();

    let metadata_path = root.join("mint.meta.json");
    let metadata = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("metadata")
        .arg("--output")
        .arg(&metadata_path)
        .output()
        .unwrap();
    assert!(metadata.status.success(), "stderr: {}", String::from_utf8_lossy(&metadata.stderr));

    let output_dir = root.join("generated-builder");
    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("gen-builder")
        .arg("--target")
        .arg("typescript")
        .arg("--metadata")
        .arg(&metadata_path)
        .arg("--action")
        .arg("mint")
        .arg("--output")
        .arg(&output_dir)
        .arg("--package-name")
        .arg("@demo/token-builder")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["schema"], "cellscript-generated-builder-summary-v0.20");
    assert_eq!(summary["target"], "typescript");
    assert_eq!(summary["package_name"], "@demo/token-builder");
    assert_eq!(summary["action_count"], 1);
    assert_eq!(summary["actions"][0], "mint");
    assert!(summary["metadata_hash"].as_str().is_some_and(|hash| hash.len() == 64));
    assert_eq!(summary["cell_data_codec_abi"], "molecule");
    assert_eq!(summary["raw_cell_data_required"], false);
    assert_eq!(summary["protocol_bundle_api_schema"], "cellscript-protocol-bundle-v1");
    assert_eq!(summary["protocol_bundle_artifact_binding_schema"], "cellscript-protocol-bundle-artifact-binding-v1");
    assert_eq!(summary["exact_script_handle_receipt_schema"], "cellscript-exact-script-handle-receipt-v3");
    assert_eq!(summary["exact_script_handle_value_schema"], "cellscript-exact-script-handle-value-v1");
    assert_eq!(summary["exact_script_handle_encoding"], "CSHDLv1-fixed-202");
    assert_eq!(summary["protocol_bundle_closed_role_schema"], "cellscript-protocol-closed-role-v1");
    assert_eq!(summary["resumable_external_signing"], true);
    assert_eq!(summary["private_keys_in_generated_api"], false);

    let package_json: serde_json::Value = serde_json::from_slice(&std::fs::read(output_dir.join("package.json")).unwrap()).unwrap();
    assert_eq!(package_json["name"], "@demo/token-builder");
    assert_eq!(package_json["type"], "module");
    assert_eq!(package_json["scripts"]["build"], "tsc -p tsconfig.json");
    assert_eq!(package_json["scripts"]["test"], "npm run build && node --test test/*.test.mjs");

    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output_dir.join("cellscript-builder-manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["schema"], "cellscript-generated-action-builder-v1");
    assert_eq!(manifest["target"], "typescript");
    assert_eq!(manifest["actions"][0]["name"], "mint");
    assert_eq!(manifest["cell_data_codec_manifest"]["schema"], "cellscript-cell-data-codec-manifest-v1");
    assert_eq!(manifest["cell_data_codec_manifest"]["abi"], "molecule");
    assert_eq!(manifest["cell_data_codec_manifest"]["raw_bytes_required"], false);
    assert_eq!(
        manifest["cell_data_codec_manifest"]["molecule_schema_manifest_hash"],
        manifest["molecule_schema_manifest"]["manifest_hash"]
    );
    assert_eq!(manifest["runtime_contract"]["requires_live_cell_resolution"], true);
    assert_eq!(manifest["runtime_contract"]["requires_dry_run_before_submit"], true);
    assert_eq!(manifest["runtime_contract"]["requires_cell_data_codec_materialization"], true);
    assert_eq!(manifest["runtime_contract"]["requires_external_cell_data_codec_adapter"], false);
    assert_eq!(manifest["runtime_contract"]["cell_data_codec_abi"], "molecule");
    assert_eq!(manifest["runtime_contract"]["must_not_infer_protocol_semantics_from_action_name"], true);
    assert_eq!(manifest["runtime_contract"]["action_scan_selectors_schema"], "cellscript-action-scan-selectors-v0.21");
    assert_eq!(manifest["runtime_contract"]["action_scan_selector_source"], "transaction_runtime_input_requirements");
    assert_eq!(manifest["actions"][0]["action_scan_selectors"]["schema"], "cellscript-action-scan-selectors-v0.21");
    assert_eq!(manifest["actions"][0]["action_scan_selectors"]["source"], "transaction_runtime_input_requirements");
    assert_eq!(manifest["protocol_bundle_contract"]["schema"], "cellscript-protocol-bundle-v1");
    assert_eq!(manifest["protocol_bundle_contract"]["closed_role_schema"], "cellscript-protocol-closed-role-v1");
    assert_eq!(manifest["protocol_bundle_contract"]["exact_handle_receipt_schema"], "cellscript-exact-script-handle-receipt-v3");
    assert_eq!(manifest["protocol_bundle_contract"]["exact_handle_value_schema"], "cellscript-exact-script-handle-value-v1");
    assert_eq!(manifest["protocol_bundle_contract"]["exact_handle_encoding"], "CSHDLv1-fixed-202");
    assert_eq!(manifest["protocol_bundle_contract"]["exact_handle_hash_algorithm"], "ckb-blake2b-256");
    assert_eq!(manifest["protocol_bundle_contract"]["exact_handle_hash_personalization"], "ckb-default-hash");
    assert!(manifest["runtime_abi_hash"].as_str().is_some());
    assert!(manifest["target_profile_hash"].as_str().is_some());
    assert_eq!(manifest["protocol_bundle_contract"]["runtime_adapter"], "cellscript-ckb-adapter");
    assert_eq!(manifest["protocol_bundle_contract"]["private_keys"], "never-in-bundle-or-evidence");
    assert_eq!(manifest["protocol_bundle_contract"]["states"].as_array().unwrap().len(), 9);
    assert_eq!(manifest["protocol_bundle_contract"]["states"][8], "ConfirmedProtocolBundleTx");
    assert_eq!(
        manifest["actions"][0]["action_scan_selectors"]["selector_count"],
        manifest["actions"][0]["runtime_input_requirements"]
    );
    assert!(manifest["runtime_error_catalog"]
        .as_array()
        .is_some_and(|errors| errors.iter().any(|error| { error["code"] == 25 && error["name"] == "entry-witness-abi-invalid" })));

    let index_ts = std::fs::read_to_string(output_dir.join("src").join("index.ts")).unwrap();
    assert!(index_ts.contains("ACTION_SCAN_SELECTORS_SCHEMA"), "{index_ts}");
    assert!(index_ts.contains("actionScanSelectors"), "{index_ts}");
    assert!(index_ts.contains("scanSelectorEvidence"), "{index_ts}");
    assert!(index_ts.contains("assertScanSelectorEvidence"), "{index_ts}");
    assert!(index_ts.contains("duplicate selector_index"), "{index_ts}");
    assert!(index_ts.contains("seenEvidenceIndexes"), "{index_ts}");
    assert!(index_ts.contains("missing for selector"), "{index_ts}");
    assert!(index_ts.contains("unexpected for selector"), "{index_ts}");
    assert!(index_ts.contains("export interface MintParams"), "{index_ts}");
    assert!(index_ts.contains("amount: bigint | number | string;"), "{index_ts}");
    assert!(index_ts.contains("owner: HexString | Uint8Array;"), "{index_ts}");
    assert!(index_ts.contains("export function planMint"), "{index_ts}");
    assert!(index_ts.contains("createActionBuilder"), "{index_ts}");
    assert!(index_ts.contains("ActionBuilderResult"), "{index_ts}");
    assert!(index_ts.contains("submittedTxHashFromRuntime"), "{index_ts}");
    assert!(index_ts.contains("CellScript builder runtime missing dryRun adapter"), "{index_ts}");
    assert!(index_ts.contains("runtimeErrorCatalog"), "{index_ts}");
    assert!(index_ts.contains("explainCellScriptRuntimeError"), "{index_ts}");
    assert!(index_ts.contains("runtimeErrorContextForAction"), "{index_ts}");
    assert!(index_ts.contains("deployment record has no status"), "{index_ts}");
    assert!(index_ts.contains("deployment status is"), "{index_ts}");
    assert!(index_ts.contains("validateCellScriptDeploymentTrust"), "{index_ts}");
    assert!(index_ts.contains("publisher_signature required by trust policy"), "{index_ts}");
    assert!(index_ts.contains("live deployment evidence deployment_status"), "{index_ts}");
    assert!(index_ts.contains("canSubmit: false"), "{index_ts}");
    assert!(index_ts.contains("live_cell_availability"), "{index_ts}");
    assert!(index_ts.contains("export const cellDataCodecManifest"), "{index_ts}");
    assert!(index_ts.contains("cellDataCodecManifest,"), "{index_ts}");
    assert!(index_ts.contains("cell_data_codec_materialization"), "{index_ts}");
    assert!(index_ts.contains("export const metadata = {"), "{index_ts}");
    assert!(!index_ts.contains("import metadataJson"), "{index_ts}");
    assert!(index_ts.contains("PROTOCOL_BUNDLE_ARTIFACT_BINDING_SCHEMA"), "{index_ts}");
    assert!(index_ts.contains("bindProtocolBundleArtifact"), "{index_ts}");
    assert!(index_ts.contains("PROTOCOL_CLOSED_ROLE_SCHEMA"), "{index_ts}");
    assert!(index_ts.contains("bindClosedProtocolRole"), "{index_ts}");
    assert!(index_ts.contains("schemaContracts"), "{index_ts}");
    assert!(index_ts.contains("EXACT_SCRIPT_HANDLE_RECEIPT_SCHEMA"), "{index_ts}");
    assert!(index_ts.contains("bindCheckedExactScriptHandle"), "{index_ts}");
    assert!(index_ts.contains("exactScriptHandleFromCheckedBundle"), "{index_ts}");
    assert!(index_ts.contains("deploymentLineHandleFromCheckedBundle"), "{index_ts}");
    assert!(index_ts.contains("deploymentLineHandleEvidence"), "{index_ts}");
    assert!(index_ts.contains("CSHDLv1-fixed-202"), "{index_ts}");
    assert!(index_ts.contains("createProtocolBundleClient"), "{index_ts}");
    assert!(index_ts.contains("ProtocolBundleSigningRequest"), "{index_ts}");
    assert!(index_ts.contains("ProtocolBundleConfirmationPolicy"), "{index_ts}");
    assert!(index_ts.contains("ConfirmedProtocolBundleTx"), "{index_ts}");
    assert!(index_ts.contains("privateKeysIncluded: false"), "{index_ts}");

    let builder_test = std::fs::read_to_string(output_dir.join("test").join("builder.test.mjs")).unwrap();
    assert!(builder_test.contains("node:test"), "{builder_test}");
    assert!(builder_test.contains("plans all generated actions without submitting"), "{builder_test}");
    assert!(builder_test.contains("actionScanSelectors.schema"), "{builder_test}");
    assert!(builder_test.contains("selectorEvidenceForPlan"), "{builder_test}");
    assert!(builder_test.contains("scanSelectorEvidence.role mismatch"), "{builder_test}");
    assert!(builder_test.contains("scanSelectorEvidence.source missing"), "{builder_test}");
    assert!(builder_test.contains("duplicate selector_index"), "{builder_test}");
    assert!(builder_test.contains("delegates live-cell resolution and transaction build to runtime"), "{builder_test}");
    assert!(builder_test.contains("delegates dry-run and submit modes to runtime"), "{builder_test}");
    assert!(builder_test.contains("rejects missing runtime adapters and malformed runtime shapes"), "{builder_test}");
    assert!(builder_test.contains("maps runtime errors to action field context"), "{builder_test}");
    assert!(builder_test.contains("rejects mismatched lockfile identity"), "{builder_test}");
    assert!(builder_test.contains("rejects mismatched deployment identity"), "{builder_test}");
    assert!(builder_test.contains("trust policy requires a deployment record"), "{builder_test}");
    assert!(builder_test.contains("resumable ProtocolBundle runtime state machine without private keys"), "{builder_test}");
    assert!(builder_test.contains("shared-token"), "{builder_test}");
    assert!(builder_test.contains("bindCheckedExactScriptHandle"), "{builder_test}");

    let generated_metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output_dir.join("src").join("metadata.json")).unwrap()).unwrap();
    assert_eq!(generated_metadata["actions"][0]["name"], "mint");
    assert_eq!(generated_metadata["cell_data_codec_manifest"], manifest["cell_data_codec_manifest"]);
}

#[test]
fn cellc_gen_builder_preserves_typed_temporal_scalar_domains() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "temporal-builder"
version = "0.1.0"

[build]
target_profile = "ckb"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module temporal_builder::main

action schedule(
    epoch: EpochNumber,
    duration: EpochDuration,
    block: BlockNumber,
    length: EpochLength,
    timestamp: TimestampMillis,
    encoded: EncodedSince,
    decoded: DecodedSince,
    absolute_block: AbsoluteBlockSince,
    absolute_epoch: AbsoluteEpochSince,
    absolute_timestamp: AbsoluteTimestampSince,
    relative_block: RelativeBlockSince,
    relative_epoch: RelativeEpochSince,
    relative_timestamp: RelativeTimestampSince,
) -> bool {
    verification
        return epoch == epoch
            && duration == duration
            && block == block
            && length == length
            && timestamp == timestamp
            && ckb::since_to_raw(encoded) == ckb::since_to_raw(decoded)
            && ckb::since_to_raw(absolute_block) == ckb::since_to_raw(relative_block)
            && ckb::since_to_raw(absolute_epoch) == ckb::since_to_raw(relative_epoch)
            && ckb::since_to_raw(absolute_timestamp) == ckb::since_to_raw(relative_timestamp)
}
"#,
    )
    .unwrap();

    let metadata_path = root.join("temporal.meta.json");
    let metadata_output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("metadata")
        .arg("--output")
        .arg(&metadata_path)
        .output()
        .unwrap();
    assert!(metadata_output.status.success(), "stderr: {}", String::from_utf8_lossy(&metadata_output.stderr));

    let metadata: serde_json::Value = serde_json::from_slice(&std::fs::read(&metadata_path).unwrap()).unwrap();
    let params = metadata["actions"][0]["params"].as_array().unwrap();
    assert_eq!(params.len(), 13);
    assert!(params.iter().all(|param| param["schema_pointer_abi"] == false && param["schema_length_abi"] == false));

    let output_dir = root.join("generated-builder");
    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("gen-builder")
        .arg("--target")
        .arg("typescript")
        .arg("--metadata")
        .arg(&metadata_path)
        .arg("--output")
        .arg(&output_dir)
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output_dir.join("cellscript-builder-manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["runtime_contract"]["temporal_interface"]["schema"], "cellscript-ckb-temporal-interface-v1");
    assert_eq!(manifest["runtime_contract"]["temporal_interface"]["since_abi"], "ckb-since-rfc0017-typed-v1");

    let index_ts = std::fs::read_to_string(output_dir.join("src").join("index.ts")).unwrap();
    assert!(index_ts.contains("export type CellScriptTemporalDomain ="), "{index_ts}");
    assert!(index_ts.contains("export function temporalValue"), "{index_ts}");
    assert!(index_ts.contains("temporalContract: typeof temporalContract;"), "{index_ts}");
    assert!(index_ts.contains("epoch: EpochNumber;"), "{index_ts}");
    assert!(index_ts.contains("duration: EpochDuration;"), "{index_ts}");
    assert!(index_ts.contains("encoded: EncodedSince;"), "{index_ts}");
    assert!(index_ts.contains("absolute_epoch: AbsoluteEpochSince;"), "{index_ts}");
    assert!(index_ts.contains("relative_timestamp: RelativeTimestampSince;"), "{index_ts}");
}

#[test]
fn cellc_gen_builder_preserves_dynamic_runtime_index_bounds() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "dynamic-index-builder"
version = "0.1.0"

[build]
target_profile = "ckb"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module dynamic_index_builder::main

resource Token has store { amount: u64 }

action inspect(source_index: u64) -> u64 {
    let input = ckb::input<Token>(source_index)
    return input.capacity
}
"#,
    )
    .unwrap();

    let metadata_path = root.join("inspect.meta.json");
    let metadata_output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("metadata")
        .arg("--output")
        .arg(&metadata_path)
        .output()
        .unwrap();
    assert!(metadata_output.status.success(), "stderr: {}", String::from_utf8_lossy(&metadata_output.stderr));

    let metadata: serde_json::Value = serde_json::from_slice(&std::fs::read(&metadata_path).unwrap()).unwrap();
    assert_eq!(metadata["metadata_schema_version"], cellscript::METADATA_SCHEMA_VERSION);
    assert_eq!(metadata["runtime"]["ckb_runtime_access_provenance_contract"], "cellscript-ckb-runtime-access-provenance-v1");
    let dynamic_access = metadata["actions"][0]["ckb_runtime_accesses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|access| access["operation"] == "source-input")
        .expect("source-input access");
    assert_eq!(dynamic_access["index"], 0);
    assert_eq!(dynamic_access["provenance"]["source"]["resolved_source"], "Input");
    assert_eq!(dynamic_access["provenance"]["index"]["kind"], "dynamic");
    assert_eq!(dynamic_access["provenance"]["index"]["binding"], "source_index");
    assert_eq!(dynamic_access["provenance"]["index"]["max_inclusive"], u64::from(u32::MAX));

    let output_dir = root.join("generated-builder");
    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("gen-builder")
        .arg("--target")
        .arg("typescript")
        .arg("--metadata")
        .arg(&metadata_path)
        .arg("--output")
        .arg(&output_dir)
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output_dir.join("cellscript-builder-manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["runtime_contract"]["runtime_access_provenance"], "cellscript-ckb-runtime-access-provenance-v1");
    assert_eq!(manifest["actions"][0]["runtime_accesses"][0]["provenance"]["contract"], "cellscript-ckb-runtime-access-provenance-v1");

    let index_ts = std::fs::read_to_string(output_dir.join("src").join("index.ts")).unwrap();
    assert!(index_ts.contains("export const runtimeAccessProvenanceContract"), "{index_ts}");
    assert!(index_ts.contains("assertRuntimeAccessParams"), "{index_ts}");
    assert!(index_ts.contains("exceeds max_inclusive"), "{index_ts}");
    assert!(index_ts.contains("4294967295"), "{index_ts}");
    let builder_test = std::fs::read_to_string(output_dir.join("test").join("builder.test.mjs")).unwrap();
    assert!(builder_test.contains("dynamicIndexCases"), "{builder_test}");
    assert!(builder_test.contains("rejects dynamic source indexes above their declared bound"), "{builder_test}");
}

#[test]
fn cellc_gen_builder_emits_executable_bounded_output_plan_contract() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "bounded-output-builder"
version = "0.1.0"

[build]
target_profile = "ckb"
"#,
    )
    .unwrap();
    std::fs::write(root.join("src/main.cell"), include_str!("../fixtures/bounded_output_plan.cell")).unwrap();

    let metadata_path = root.join("verify.meta.json");
    let metadata_output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .args(["metadata", "--output"])
        .arg(&metadata_path)
        .output()
        .unwrap();
    assert!(metadata_output.status.success(), "stderr: {}", String::from_utf8_lossy(&metadata_output.stderr));

    let output_dir = root.join("generated-builder");
    let generated = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .args(["gen-builder", "--target", "typescript", "--metadata"])
        .arg(&metadata_path)
        .args(["--action", "verify", "--output"])
        .arg(&output_dir)
        .arg("--json")
        .output()
        .unwrap();
    assert!(generated.status.success(), "stderr: {}", String::from_utf8_lossy(&generated.stderr));

    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output_dir.join("cellscript-builder-manifest.json")).unwrap()).unwrap();
    let contract = &manifest["actions"][0]["bounded_output_plans"][0];
    assert_eq!(contract["schema"], "cellscript-bounded-output-plan-contract");
    assert_eq!(contract["binding"], "plans");
    assert_eq!(contract["max_elements"], 3);
    assert_eq!(contract["ordering"], "plan-index-equals-group-output-index");
    assert_eq!(contract["identity_policy"], "fresh-output-outpoint-plus-type-group-ordinal");

    let index_ts = std::fs::read_to_string(output_dir.join("src/index.ts")).unwrap();
    assert!(index_ts.contains("encodeBoundedOutputPlanV1"), "{index_ts}");
    assert!(index_ts.contains("materializeBoundedOutputPlanV1"), "{index_ts}");
    assert!(index_ts.contains("boundedOutputPlans"), "{index_ts}");
    assert!(index_ts.contains("ProtocolBundleBoundedOutputPlanEvidence"), "{index_ts}");
    assert_eq!(
        manifest["protocol_bundle_contract"]["bounded_output_plan_evidence_schema"],
        "cellscript-bounded-output-plan-evidence-v1"
    );
    let builder_test = std::fs::read_to_string(output_dir.join("test/builder.test.mjs")).unwrap();
    assert!(builder_test.contains("encodes and materializes bounded output plans with exact order and length"), "{builder_test}");
    assert!(builder_test.contains("wrongMagic"), "{builder_test}");
    assert!(builder_test.contains("trailing"), "{builder_test}");
}

#[test]
fn cellc_gen_builder_preserves_bounded_witness_owner_contract() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "bounded-witness-builder"
version = "0.1.0"

[build]
target_profile = "ckb"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module bounded_witness_builder::main

action inspect() -> u64 {
    verification
        let witness_args = witness::args(0)
        let lock = witness::bounded_lock(witness_args, 64)
        require lock.size <= 64
        return 0
}
"#,
    )
    .unwrap();

    let metadata_path = root.join("inspect.meta.json");
    let metadata_output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("metadata")
        .arg("--output")
        .arg(&metadata_path)
        .output()
        .unwrap();
    assert!(metadata_output.status.success(), "stderr: {}", String::from_utf8_lossy(&metadata_output.stderr));

    let metadata: serde_json::Value = serde_json::from_slice(&std::fs::read(&metadata_path).unwrap()).unwrap();
    assert_eq!(metadata["metadata_schema_version"], cellscript::METADATA_SCHEMA_VERSION);
    let handle = metadata["runtime"]["transaction_view_handles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|handle| handle["handle_type"] == "WitnessBytesView<lock,64>")
        .expect("bounded lock handle");
    assert_eq!(handle["witness_owner"], "lock");
    assert_eq!(handle["max_bytes"], 64);
    assert_eq!(handle["provenance"]["range"]["length"]["max_inclusive"], 64);

    let output_dir = root.join("generated-builder");
    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("gen-builder")
        .arg("--target")
        .arg("typescript")
        .arg("--metadata")
        .arg(&metadata_path)
        .arg("--output")
        .arg(&output_dir)
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output_dir.join("cellscript-builder-manifest.json")).unwrap()).unwrap();
    let generated_handle = manifest["transaction_view_handles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|handle| handle["handle_type"] == "WitnessBytesView<lock,64>")
        .expect("generated bounded lock handle");
    assert_eq!(generated_handle["witness_owner"], "lock");
    assert_eq!(generated_handle["max_bytes"], 64);
    assert!(manifest["actions"][0]["runtime_accesses"]
        .as_array()
        .unwrap()
        .iter()
        .any(|access| access["operation"] == "witness-bounded-lock-size"));

    let index_ts = std::fs::read_to_string(output_dir.join("src").join("index.ts")).unwrap();
    assert!(index_ts.contains("export const transactionViewHandles"), "{index_ts}");
    assert!(index_ts.contains("WitnessBytesView<lock,64>"), "{index_ts}");
    assert!(index_ts.contains("\"witness_owner\": \"lock\""), "{index_ts}");
    assert!(index_ts.contains("\"max_bytes\": 64"), "{index_ts}");
}

#[test]
fn cellc_gen_builder_preserves_zero_lock_signing_message_domain() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "sighash-zero-lock-builder"
version = "0.1.0"

[build]
target_profile = "ckb"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module sighash_zero_lock_builder::main

action inspect() -> u64 {
    verification
        let digest = env::sighash_all_zero_lock(4, 8, 4, 4096)
        let expected = witness::lock_exact32(source::input(0))
        require Hash::from_sighash_all(digest).0 == expected
        return 0
}
"#,
    )
    .unwrap();

    let metadata_path = root.join("inspect.meta.json");
    let metadata_output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("metadata")
        .arg("--output")
        .arg(&metadata_path)
        .output()
        .unwrap();
    assert!(metadata_output.status.success(), "stderr: {}", String::from_utf8_lossy(&metadata_output.stderr));

    let output_dir = root.join("generated-builder");
    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("gen-builder")
        .arg("--target")
        .arg("typescript")
        .arg("--metadata")
        .arg(&metadata_path)
        .arg("--output")
        .arg(&output_dir)
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output_dir.join("cellscript-builder-manifest.json")).unwrap()).unwrap();
    let domain = &manifest["signing_message_domains"][0];
    assert_eq!(domain["contract"], "cellscript-ckb-sighash-all-zero-lock-v1");
    assert_eq!(domain["max_group_inputs"], 4);
    assert_eq!(domain["max_inputs"], 8);
    assert_eq!(domain["max_extra_witnesses"], 4);
    assert_eq!(domain["max_witness_bytes"], 4096);
    assert_eq!(manifest["runtime_contract"]["requires_pre_signing_witness_placement"], true);

    let index_ts = std::fs::read_to_string(output_dir.join("src").join("index.ts")).unwrap();
    assert!(index_ts.contains("export const signingMessageDomains"), "{index_ts}");
    assert!(index_ts.contains("cellscript-ckb-sighash-all-zero-lock-v1"), "{index_ts}");
    assert!(index_ts.contains("signingMessageDomains: typeof signingMessageDomains"), "{index_ts}");
}

#[test]
fn cellc_gen_builder_typescript_declares_raw_cell_data_codec_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "raw-codec-demo"
version = "0.1.0"

[build]
target_profile = "ckb"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module raw_codec_demo::main

action inspect() -> u64 {
    verification
        let input = source::group_input(0)
        let quantity = ckb::cell_data_u32_le(input, 0)
        let amount = ckb::cell_data_u64_le(input, 4)
        if quantity != 7 {
            return 90
        }
        return amount
}
"#,
    )
    .unwrap();

    let metadata_path = root.join("inspect.meta.json");
    let metadata = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("metadata")
        .arg("--output")
        .arg(&metadata_path)
        .output()
        .unwrap();
    assert!(metadata.status.success(), "stderr: {}", String::from_utf8_lossy(&metadata.stderr));

    let output_dir = root.join("generated-builder");
    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("gen-builder")
        .arg("--target")
        .arg("typescript")
        .arg("--metadata")
        .arg(&metadata_path)
        .arg("--output")
        .arg(&output_dir)
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["cell_data_codec_abi"], "molecule+raw-bytes-v1");
    assert_eq!(summary["raw_cell_data_required"], true);

    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output_dir.join("cellscript-builder-manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["cell_data_codec_manifest"]["abi"], "molecule+raw-bytes-v1");
    assert_eq!(manifest["cell_data_codec_manifest"]["raw_bytes_required"], true);
    assert!(manifest["cell_data_codec_manifest"]["raw_runtime_accesses"]
        .as_array()
        .is_some_and(|accesses| accesses.iter().any(|access| access["binding"] == "ckb::cell_data_u32_le")
            && accesses.iter().any(|access| access["binding"] == "ckb::cell_data_u64_le")));
    assert_eq!(manifest["runtime_contract"]["requires_external_cell_data_codec_adapter"], true);
    assert_eq!(manifest["runtime_contract"]["cell_data_codec_abi"], "molecule+raw-bytes-v1");

    let index_ts = std::fs::read_to_string(output_dir.join("src").join("index.ts")).unwrap();
    assert!(index_ts.contains("export const cellDataCodecManifest"), "{index_ts}");
    assert!(index_ts.contains("molecule+raw-bytes-v1"), "{index_ts}");
    assert!(index_ts.contains("cell_data_codec_materialization"), "{index_ts}");
}

#[test]
fn cellc_gen_builder_lockfile_identity_fails_closed() {
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
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Token has store, replace, relock, consume {
    amount: u64,
}

action mint(amount: u64, owner: Address) -> Token {
    verification
        create Token { amount: amount }
}
"#,
    )
    .unwrap();

    let metadata_path = root.join("mint.meta.json");
    let metadata_output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("metadata")
        .arg("--output")
        .arg(&metadata_path)
        .output()
        .unwrap();
    assert!(metadata_output.status.success(), "stderr: {}", String::from_utf8_lossy(&metadata_output.stderr));

    let metadata: cellscript::CompileMetadata = serde_json::from_slice(&std::fs::read(&metadata_path).unwrap()).unwrap();
    let build_info = locked_build_from_metadata_for_test(&metadata);
    let deployment_network = "aggron4";
    let deployment_code_hash = "0x1111111111111111111111111111111111111111111111111111111111111111";
    let deployment_out_point = "0xaaaa:0";
    let package_source_hash = "package-registry-source-hash".to_string();
    let mut lockfile = cellscript::package::Lockfile {
        version: cellscript::package::Lockfile::CURRENT_VERSION,
        schema: cellscript::package::Lockfile::CURRENT_SCHEMA.to_string(),
        resolver_model: cellscript::package::Lockfile::CURRENT_RESOLVER_MODEL.to_string(),
        package: cellscript::package::LockfilePackageInfo {
            edition: cellscript::CURRENT_EDITION,
            name: "demo".to_string(),
            version: "0.1.0".to_string(),
            namespace: None,
            source_hash: Some(package_source_hash.clone()),
            compiler_source_hash: metadata.source_hash.clone(),
            compiler_requirement: format!(">={}", cellscript::VERSION),
            resolver_compiler_version: cellscript::VERSION.to_string(),
        },
        root: Default::default(),
        dependencies: Default::default(),
        environments: Default::default(),
        package_build: Some(build_info.clone()),
        deployment: Default::default(),
    };
    lockfile.deployment.insert(
        deployment_network.to_string(),
        cellscript::package::LockfileDeploymentRef {
            record: deployment_out_point.to_string(),
            record_hash: None,
            code_hash: Some(deployment_code_hash.to_string()),
            out_point: Some(deployment_out_point.to_string()),
            data_hash: Some(deployment_code_hash.to_string()),
        },
    );
    let lockfile_path = root.join("Cell.lock");
    std::fs::write(&lockfile_path, toml::to_string_pretty(&lockfile).unwrap()).unwrap();

    let deployed = cellscript::package::DeployedManifest {
        version: cellscript::package::DeployedManifest::CURRENT_VERSION,
        schema: cellscript::package::DEPLOYED_MANIFEST_SCHEMA.to_string(),
        package: cellscript::package::DeployedPackageInfo {
            edition: cellscript::CURRENT_EDITION,
            name: "demo".to_string(),
            version: "0.1.0".to_string(),
            source_hash: Some(package_source_hash.clone()),
        },
        build: Some(cellscript::package::DeployedBuildInfo {
            edition: cellscript::CURRENT_EDITION,
            compatibility_profile_hash: build_info.compatibility_profile_hash.clone(),
            compiler_version: build_info.compiler_version.clone(),
            artifact_hash: build_info.artifact_hash.clone(),
            metadata_hash: build_info.metadata_hash.clone(),
            schema_hash: build_info.schema_hash.clone(),
            cell_data_codec_manifest_hash: build_info.cell_data_codec_manifest_hash.clone(),
            abi_hash: build_info.abi_hash.clone(),
            constraints_hash: build_info.constraints_hash.clone(),
        }),
        deployments: vec![cellscript::package::DeploymentRecord {
            edition: cellscript::CURRENT_EDITION,
            compatibility_profile_hash: build_info.compatibility_profile_hash.clone(),
            network: deployment_network.to_string(),
            chain_id: "ckb-testnet".to_string(),
            tx_hash: "0xaaaa".to_string(),
            output_index: 0,
            code_hash: deployment_code_hash.to_string(),
            hash_type: "data1".to_string(),
            dep_type: "code".to_string(),
            data_hash: deployment_code_hash.to_string(),
            out_point: deployment_out_point.to_string(),
            artifact_hash: build_info.artifact_hash.clone(),
            metadata_hash: build_info.metadata_hash.clone(),
            schema_hash: build_info.schema_hash.clone(),
            cell_data_codec_manifest_hash: build_info.cell_data_codec_manifest_hash.clone(),
            abi_hash: build_info.abi_hash.clone(),
            constraints_hash: build_info.constraints_hash.clone(),
            compiler_version: build_info.compiler_version.clone(),
            type_id: None,
            script_role: Some(cellscript::package::ScriptRole::Type),
            status: Some(cellscript::package::DeploymentStatus::Active),
            upgrade_lineage: None,
            audit_report_hash: None,
            publisher_signature: None,
            cell_deps: vec![],
        }],
    };
    let deployed_path = root.join("Deployed.toml");
    std::fs::write(&deployed_path, toml::to_string_pretty(&deployed).unwrap()).unwrap();

    let output_dir = root.join("locked-builder");
    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("gen-builder")
        .arg("--target")
        .arg("typescript")
        .arg("--metadata")
        .arg(&metadata_path)
        .arg("--lockfile")
        .arg(&lockfile_path)
        .arg("--deployed")
        .arg(&deployed_path)
        .arg("--deployment-network")
        .arg(deployment_network)
        .arg("--action")
        .arg("mint")
        .arg("--output")
        .arg(&output_dir)
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["lockfile_verified"], true);
    assert_eq!(summary["deployment_verified"], true);

    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output_dir.join("cellscript-builder-manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["locked_identity"]["schema"], "cellscript-builder-locked-identity-v0.20");
    assert_eq!(manifest["deployment_identity"]["schema"], "cellscript-builder-deployment-identity-v0.20");
    assert_eq!(manifest["deployment_identity"]["deployments"][0]["network"], deployment_network);
    assert_eq!(manifest["locked_identity"]["package"]["source_hash"], package_source_hash);
    assert_eq!(manifest["locked_identity"]["package"]["compiler_source_hash"], metadata.source_hash.as_deref().unwrap());
    assert_eq!(manifest["locked_identity"]["build"]["metadata_hash"], build_info.metadata_hash.as_deref().unwrap());

    let index_ts = std::fs::read_to_string(output_dir.join("src").join("index.ts")).unwrap();
    assert!(index_ts.contains("validateCellScriptLockfile"), "{index_ts}");
    assert!(index_ts.contains("single-package-coordinate-v1"), "{index_ts}");
    assert!(index_ts.contains("validateCellScriptDeployment"), "{index_ts}");
    assert!(index_ts.contains("assertCellScriptLockfile(options.lockfile)"), "{index_ts}");
    assert!(
        index_ts.contains(
            "assertCellScriptDeployment(options.lockfile, options.deployment, options.liveDeploymentEvidence, options.trustPolicy)"
        ),
        "{index_ts}"
    );

    let mut bad_lockfile = lockfile;
    bad_lockfile.package_build.as_mut().unwrap().metadata_hash = Some("bad_metadata_hash".to_string());
    let bad_lockfile_path = root.join("Bad.lock");
    std::fs::write(&bad_lockfile_path, toml::to_string_pretty(&bad_lockfile).unwrap()).unwrap();

    let rejected = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("gen-builder")
        .arg("--target")
        .arg("typescript")
        .arg("--metadata")
        .arg(&metadata_path)
        .arg("--lockfile")
        .arg(&bad_lockfile_path)
        .arg("--action")
        .arg("mint")
        .arg("--output")
        .arg(root.join("bad-builder"))
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(stderr.contains("generated builder identity verification failed"), "{stderr}");
    assert!(stderr.contains("metadata_hash mismatch"), "{stderr}");

    let mut bad_codec_lockfile = bad_lockfile;
    bad_codec_lockfile.package_build.as_mut().unwrap().metadata_hash = build_info.metadata_hash.clone();
    bad_codec_lockfile.package_build.as_mut().unwrap().cell_data_codec_manifest_hash = Some("bad_codec_manifest_hash".to_string());
    let bad_codec_lockfile_path = root.join("BadCodec.lock");
    std::fs::write(&bad_codec_lockfile_path, toml::to_string_pretty(&bad_codec_lockfile).unwrap()).unwrap();

    let codec_rejected = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("gen-builder")
        .arg("--target")
        .arg("typescript")
        .arg("--metadata")
        .arg(&metadata_path)
        .arg("--lockfile")
        .arg(&bad_codec_lockfile_path)
        .arg("--action")
        .arg("mint")
        .arg("--output")
        .arg(root.join("bad-codec-builder"))
        .output()
        .unwrap();
    assert!(!codec_rejected.status.success());
    let codec_stderr = String::from_utf8_lossy(&codec_rejected.stderr);
    assert!(codec_stderr.contains("cell_data_codec_manifest_hash mismatch"), "{codec_stderr}");

    let mut bad_deployed = deployed.clone();
    bad_deployed.deployments[0].code_hash = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
    let bad_deployed_path = root.join("BadDeployed.toml");
    std::fs::write(&bad_deployed_path, toml::to_string_pretty(&bad_deployed).unwrap()).unwrap();

    let rejected_deployment = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("gen-builder")
        .arg("--target")
        .arg("typescript")
        .arg("--metadata")
        .arg(&metadata_path)
        .arg("--lockfile")
        .arg(&lockfile_path)
        .arg("--deployed")
        .arg(&bad_deployed_path)
        .arg("--deployment-network")
        .arg(deployment_network)
        .arg("--action")
        .arg("mint")
        .arg("--output")
        .arg(root.join("bad-deployment-builder"))
        .output()
        .unwrap();
    assert!(!rejected_deployment.status.success());
    let stderr = String::from_utf8_lossy(&rejected_deployment.stderr);
    assert!(stderr.contains("generated builder deployment identity verification failed"), "{stderr}");
    assert!(stderr.contains("code_hash mismatch"), "{stderr}");

    let mut missing_status_deployed = deployed.clone();
    missing_status_deployed.deployments[0].status = None;
    let missing_status_deployed_path = root.join("MissingStatusDeployed.toml");
    std::fs::write(&missing_status_deployed_path, toml::to_string_pretty(&missing_status_deployed).unwrap()).unwrap();

    let rejected_missing_status = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("gen-builder")
        .arg("--target")
        .arg("typescript")
        .arg("--metadata")
        .arg(&metadata_path)
        .arg("--lockfile")
        .arg(&lockfile_path)
        .arg("--deployed")
        .arg(&missing_status_deployed_path)
        .arg("--deployment-network")
        .arg(deployment_network)
        .arg("--action")
        .arg("mint")
        .arg("--output")
        .arg(root.join("missing-status-deployment-builder"))
        .output()
        .unwrap();
    assert!(!rejected_missing_status.status.success());
    let stderr = String::from_utf8_lossy(&rejected_missing_status.stderr);
    assert!(stderr.contains("generated builder deployment identity verification failed"), "{stderr}");
    assert!(stderr.contains("has no status"), "{stderr}");

    let mut deprecated_deployed = deployed;
    deprecated_deployed.deployments[0].status = Some(cellscript::package::DeploymentStatus::Deprecated);
    let deprecated_deployed_path = root.join("DeprecatedDeployed.toml");
    std::fs::write(&deprecated_deployed_path, toml::to_string_pretty(&deprecated_deployed).unwrap()).unwrap();

    let rejected_deprecated = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("gen-builder")
        .arg("--target")
        .arg("typescript")
        .arg("--metadata")
        .arg(&metadata_path)
        .arg("--lockfile")
        .arg(&lockfile_path)
        .arg("--deployed")
        .arg(&deprecated_deployed_path)
        .arg("--deployment-network")
        .arg(deployment_network)
        .arg("--action")
        .arg("mint")
        .arg("--output")
        .arg(root.join("deprecated-deployment-builder"))
        .output()
        .unwrap();
    assert!(!rejected_deprecated.status.success());
    let stderr = String::from_utf8_lossy(&rejected_deprecated.stderr);
    assert!(stderr.contains("generated builder deployment identity verification failed"), "{stderr}");
    assert!(stderr.contains("not active"), "{stderr}");
}
