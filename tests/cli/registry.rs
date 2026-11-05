// CLI registry behavior; included in the original cli integration target.

#[test]
fn cellc_auth_help_hides_legacy_login_alias() {
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["auth", "--help"]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("capability"), "unexpected auth help: {stdout}");
    assert!(stdout.contains("reproducer"), "unexpected auth help: {stdout}");
    assert!(!stdout.contains("login"), "legacy auth login alias should be hidden from auth help: {stdout}");
}

#[test]
fn cellc_auth_login_outputs_capability_authorisation_payload() {
    let output = cellc_command()
        .arg("auth")
        .arg("capability")
        .arg("create")
        .arg("--principal-id")
        .arg("0xjoyidprincipal")
        .arg("--capability-pubkey")
        .arg("0xcapabilitypubkey")
        .arg("--scope")
        .arg("publish:cellscript/amm")
        .arg("--expires")
        .arg("90d")
        .arg("--json")
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["protocol"], "cellscript-registry-auth-v1");
    assert_eq!(payload["action"], "authorize_capability");
    assert_eq!(payload["registry_origin"], "https://api.registry.cellscript.dev");
    assert_eq!(payload["principal_type"], "joyid_ckb");
    assert_eq!(payload["principal_id"], "0xjoyidprincipal");
    assert_eq!(payload["capability_pubkey"], "0xcapabilitypubkey");
    assert_eq!(payload["requested_scopes"], serde_json::json!(["publish:cellscript/amm"]));
    assert!(payload["capability_expires_at"].as_str().is_some_and(|value| value.ends_with('Z')));
    assert!(payload["nonce"].as_str().is_some_and(|value| value.starts_with("0x")));
    assert!(payload["issued_at"].as_str().is_some());
    assert!(payload["expires_at"].as_str().is_some());
    assert!(payload["cli_version"].as_str().is_some());
}

#[test]
fn cellc_auth_capability_create_infers_only_the_exact_publish_scope() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("Cell.toml"),
        r#"[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "amm"
version = "0.1.0"
namespace = "cellscript"
"#,
    )
    .unwrap();

    let output = cellc_command()
        .args(["auth", "capability", "create"])
        .arg("--principal-id")
        .arg("0xjoyidprincipal")
        .arg("--capability-pubkey")
        .arg("0xcapabilitypubkey")
        .arg("--json")
        .current_dir(temp.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["requested_scopes"], serde_json::json!(["publish:cellscript/amm"]));
}

#[test]
fn cellc_auth_capability_create_rejects_unknown_or_duplicate_scopes() {
    for scopes in [vec!["admin:cellscript/amm"], vec!["publish:cellscript/amm", "publish:cellscript/amm"]] {
        let mut command = cellc_command();
        command
            .args(["auth", "capability", "create"])
            .arg("--principal-id")
            .arg("0xjoyidprincipal")
            .arg("--capability-pubkey")
            .arg("0xcapabilitypubkey")
            .arg("--json");
        for scope in scopes {
            command.arg("--scope").arg(scope);
        }
        let output = command.output().unwrap();
        assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
        let failure: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let message = failure["diagnostics"][0]["message"].as_str().unwrap_or_default();
        assert!(message.contains("capability scope"), "unexpected failure: {failure}");
    }
}

#[test]
fn cellc_auth_capability_create_requires_principal_id() {
    let output = cellc_command()
        .arg("auth")
        .arg("capability")
        .arg("create")
        .arg("--capability-pubkey")
        .arg("0xcapabilitypubkey")
        .arg("--scope")
        .arg("publish:cellscript/amm")
        .arg("--json")
        .output()
        .unwrap();

    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    assert!(output.stderr.is_empty(), "unexpected stderr: {}", String::from_utf8_lossy(&output.stderr));
    let failure: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(failure["status"], "failed");
    let message = failure["diagnostics"][0]["message"].as_str().unwrap_or_default();
    assert!(message.contains("principal id is required"), "unexpected failure: {failure}");
    assert!(message.contains("--principal-id"), "unexpected failure: {failure}");
}

#[cfg(unix)]
#[test]
fn cellc_auth_reproducer_create_keeps_private_key_out_of_public_enrollment() {
    let temp = tempfile::tempdir().unwrap();
    let private_key_path = temp.path().join("builder-private.pkcs8.b64");
    let output = cellc_command()
        .args(["auth", "reproducer", "create"])
        .arg("--builder-id")
        .arg("independent-builder-a")
        .arg("--trust-domain")
        .arg("independent-org-a")
        .arg("--private-key-output")
        .arg(&private_key_path)
        .arg("--json")
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let enrollment: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(enrollment["schema"], "cellscript-reproducer-builder-enrollment-v1");
    assert_eq!(enrollment["builder_id"], "independent-builder-a");
    assert_eq!(enrollment["trust_domain"], "independent-org-a");
    assert_eq!(enrollment["policy_builder"]["builder_id"], "independent-builder-a");
    assert_eq!(enrollment["policy_builder"]["trust_domain"], "independent-org-a");
    assert_eq!(enrollment["private_key_storage"]["kind"], "pkcs8_base64_file");

    let public_key = enrollment["builder_public_key"].as_str().unwrap();
    assert!(public_key.starts_with("p256-spki:"));
    let spki = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(public_key.trim_start_matches("p256-spki:")).unwrap();
    assert_eq!(spki.len(), 91);
    let expected_key_id = format!("cap_{}", &hex::encode(Sha256::digest(public_key.as_bytes()))[..32]);
    assert_eq!(enrollment["builder_key_id"], expected_key_id);
    assert_eq!(enrollment["policy_builder"]["public_key"], public_key);

    let private_key_secret = std::fs::read_to_string(&private_key_path).unwrap();
    let private_key = base64::engine::general_purpose::STANDARD.decode(private_key_secret.trim()).unwrap();
    assert!(private_key.len() > 100);
    assert!(!String::from_utf8_lossy(&output.stdout).contains(private_key_secret.trim()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(std::fs::metadata(&private_key_path).unwrap().permissions().mode() & 0o777, 0o600);
    }

    let second = cellc_command()
        .args(["auth", "reproducer", "create"])
        .arg("--builder-id")
        .arg("independent-builder-a")
        .arg("--trust-domain")
        .arg("independent-org-a")
        .arg("--private-key-output")
        .arg(&private_key_path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!second.status.success(), "existing private-key file must not be overwritten");
    assert_eq!(std::fs::read_to_string(&private_key_path).unwrap(), private_key_secret);
}

#[cfg(not(unix))]
#[test]
fn cellc_auth_reproducer_create_rejects_private_key_file_without_unix_permissions() {
    let temp = tempfile::tempdir().unwrap();
    let private_key_path = temp.path().join("builder-private.pkcs8.b64");
    let output = cellc_command()
        .args(["auth", "reproducer", "create"])
        .arg("--builder-id")
        .arg("independent-builder-a")
        .arg("--trust-domain")
        .arg("independent-org-a")
        .arg("--private-key-output")
        .arg(&private_key_path)
        .arg("--json")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("requires Unix mode-0600 permission semantics"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!private_key_path.exists());
}

#[test]
fn cellc_publish_dry_run_validates_declared_non_cellscript_artifact() {
    let temp = tempfile::tempdir().unwrap();
    write_declared_artifact_fixture(temp.path());
    let output = cellc_command()
        .arg("publish")
        .arg("--artifact-manifest")
        .arg("Artifact.toml")
        .arg("--dry-run")
        .arg("--json")
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["status"], "valid");
    assert_eq!(result["coordinate"], "cellscript/rust-contract@1.0.0");
    assert_eq!(result["artifact"]["kind"], "deployable_contract");
    assert_eq!(result["artifact"]["profile"], "ckb_executable");
    assert_eq!(result["artifact"]["consumption_mode"], "deployment");
    assert_eq!(result["source_hash"].as_str().unwrap().len(), 64);
}

#[test]
fn cellc_publish_default_requires_capability_inputs_without_writing_registry_json() {
    let temp = tempfile::tempdir().unwrap();
    write_publish_fixture_package(temp.path());

    let output = cellc_command().arg("publish").current_dir(temp.path()).output().unwrap();

    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("wallet-authorised publishing key"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("cellc publish --authorise"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("--capability-key-id"), "unexpected stderr: {stderr}");
    assert!(!temp.path().join("registry.json").exists(), "default public publish must not silently write offline registry.json");
}

#[test]
fn cellc_publish_print_payload_outputs_signable_registry_publish_payload() {
    let temp = tempfile::tempdir().unwrap();
    write_publish_fixture_package(temp.path());

    let output = cellc_command()
        .arg("publish")
        .arg("--capability-key-id")
        .arg("cap_test")
        .arg("--print-payload")
        .arg("--json")
        .current_dir(temp.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["endpoint"], "https://api.registry.cellscript.dev/v1/artifacts/cellscript/demo/releases");
    assert_eq!(envelope["payload"]["protocol"], "cellscript-registry-publish-v1");
    assert_eq!(envelope["payload"]["action"], "publish");
    assert_eq!(envelope["payload"]["registry_origin"], "https://api.registry.cellscript.dev");
    assert_eq!(envelope["payload"]["namespace"], "cellscript");
    assert_eq!(envelope["payload"]["name"], "demo");
    assert_eq!(envelope["payload"]["version"], "1.2.3");
    assert_eq!(envelope["payload"]["capability_key_id"], "cap_test");
    assert_eq!(envelope["payload"]["artifact"]["kind"], "source_library");
    assert_eq!(envelope["payload"]["registry_entry"]["versions"][0]["verification_status"], "pending");
    let canonical_payload = envelope["canonical_payload"].as_str().expect("canonical payload");
    let canonical_json: serde_json::Value = serde_json::from_str(canonical_payload).unwrap();
    assert_eq!(canonical_json, envelope["payload"]);
    assert!(!temp.path().join("registry.json").exists(), "payload preview must not write offline registry.json");
}

#[test]
fn cellc_publish_profile_library_preserves_the_declared_artifact_kind() {
    let temp = tempfile::tempdir().unwrap();
    write_publish_fixture_package(temp.path());

    let output = cellc_command()
        .arg("publish")
        .arg("--artifact-kind")
        .arg("profile_library")
        .arg("--capability-key-id")
        .arg("cap_test")
        .arg("--print-payload")
        .arg("--json")
        .current_dir(temp.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["payload"]["artifact"]["kind"], "profile_library");
    assert_eq!(envelope["payload"]["artifact"]["profile"], "cellscript_source");
    assert_eq!(envelope["payload"]["registry_entry"]["artifact"]["kind"], "profile_library");
}

#[test]
fn cellc_publish_posts_signed_request_to_registry_api() {
    let temp = tempfile::tempdir().unwrap();
    write_publish_fixture_package(temp.path());

    let (api_url, request_rx) = start_mock_registry_api_capture_request(serde_json::json!({
        "request_id": "req_test",
        "verification_status": "pending",
        "deployment_status": "not_applicable",
        "availability_status": "active",
        "direct_url": "https://registry.cellscript.dev/artifacts/cellscript/demo/releases/1.2.3.json",
        "snapshot_hash": "sha256:test",
        "verification": "queued"
    }));

    let preview = cellc_command()
        .arg("publish")
        .arg("--api-url")
        .arg(&api_url)
        .arg("--capability-key-id")
        .arg("cap_test")
        .arg("--print-payload")
        .arg("--json")
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(preview.status.success(), "stderr: {}", String::from_utf8_lossy(&preview.stderr));
    let envelope: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    let payload_path = temp.path().join("publish-payload.json");
    std::fs::write(&payload_path, serde_json::to_vec_pretty(&envelope).unwrap()).unwrap();

    let output = cellc_command()
        .arg("publish")
        .arg("--api-url")
        .arg(&api_url)
        .arg("--payload")
        .arg(&payload_path)
        .arg("--capability-signature")
        .arg("0x1234")
        .arg("--json")
        .current_dir(temp.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["verification_status"], "pending");
    assert_eq!(response["deployment_status"], "not_applicable");
    let request = request_rx.recv_timeout(Duration::from_secs(5)).expect("registry API request");
    assert_eq!(request.path, "/v1/artifacts/cellscript/demo/releases");
    assert!(
        request
            .header("idempotency-key")
            .is_some_and(|value| value.starts_with("cellc-publish-") && value.len() > "cellc-publish-".len()),
        "missing or malformed Idempotency-Key header: {:?}",
        request.headers
    );
    let request = request.body;
    assert_eq!(request["payload"], envelope["payload"]);
    assert_eq!(request["capability_signature"]["algorithm"], "p256-sha256");
    assert_eq!(request["capability_signature"]["signature"], "0x1234");
    assert_eq!(request["source_snapshot"]["content_type"], "application/vnd.cellscript.source-snapshot+json");
    assert_eq!(request["source_snapshot"]["source_hash"], envelope["payload"]["source_hash"]);
    assert!(request["source_snapshot"]["content_base64"].as_str().is_some_and(|value| !value.is_empty()));
    assert!(request["source_snapshot"]["size_bytes"].as_u64().is_some_and(|value| value > 0));
    assert!(!temp.path().join("registry.json").exists(), "public publish must not write offline registry.json");
}

#[test]
fn cellc_publish_honors_explicit_idempotency_key() {
    let temp = tempfile::tempdir().unwrap();
    write_publish_fixture_package(temp.path());

    let (api_url, request_rx) = start_mock_registry_api_capture_request(serde_json::json!({
        "request_id": "req_test",
        "verification_status": "pending",
        "deployment_status": "not_applicable",
        "availability_status": "active",
        "direct_url": "https://registry.cellscript.dev/artifacts/cellscript/demo/releases/1.2.3.json",
        "snapshot_hash": "sha256:test",
        "verification": "queued"
    }));

    let preview = cellc_command()
        .arg("publish")
        .arg("--api-url")
        .arg(&api_url)
        .arg("--capability-key-id")
        .arg("cap_test")
        .arg("--print-payload")
        .arg("--json")
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(preview.status.success(), "stderr: {}", String::from_utf8_lossy(&preview.stderr));
    let envelope: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    let payload_path = temp.path().join("publish-payload.json");
    std::fs::write(&payload_path, serde_json::to_vec_pretty(&envelope).unwrap()).unwrap();

    let output = cellc_command()
        .arg("publish")
        .arg("--api-url")
        .arg(&api_url)
        .arg("--payload")
        .arg(&payload_path)
        .arg("--capability-signature")
        .arg("0x1234")
        .arg("--idempotency-key")
        .arg("ci-release-cellscript-demo-1.2.3")
        .current_dir(temp.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let request = request_rx.recv_timeout(Duration::from_secs(5)).expect("registry API request");
    assert_eq!(request.header("idempotency-key"), Some("ci-release-cellscript-demo-1.2.3"));
}

#[test]
fn cellc_publish_retries_transient_registry_error_with_same_idempotency_key() {
    let temp = tempfile::tempdir().unwrap();
    write_publish_fixture_package(temp.path());

    let (api_url, request_rx) = start_mock_registry_api_retry_then_success(serde_json::json!({
        "request_id": "req_retry",
        "verification_status": "pending",
        "deployment_status": "not_applicable",
        "availability_status": "active",
        "direct_url": "https://registry.cellscript.dev/artifacts/cellscript/demo/releases/1.2.3.json",
        "snapshot_hash": "sha256:test",
        "verification": "queued"
    }));

    let preview = cellc_command()
        .arg("publish")
        .arg("--api-url")
        .arg(&api_url)
        .arg("--capability-key-id")
        .arg("cap_test")
        .arg("--print-payload")
        .arg("--json")
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(preview.status.success(), "stderr: {}", String::from_utf8_lossy(&preview.stderr));
    let envelope: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    let payload_path = temp.path().join("publish-payload.json");
    std::fs::write(&payload_path, serde_json::to_vec_pretty(&envelope).unwrap()).unwrap();

    let output = cellc_command()
        .arg("publish")
        .arg("--api-url")
        .arg(&api_url)
        .arg("--payload")
        .arg(&payload_path)
        .arg("--capability-signature")
        .arg("0x1234")
        .arg("--idempotency-key")
        .arg("retry-cellscript-demo-1.2.3")
        .arg("--json")
        .current_dir(temp.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["request_id"], "req_retry");
    let first = request_rx.recv_timeout(Duration::from_secs(5)).expect("first registry API request");
    let second = request_rx.recv_timeout(Duration::from_secs(5)).expect("second registry API request");
    assert_eq!(first.header("idempotency-key"), Some("retry-cellscript-demo-1.2.3"));
    assert_eq!(second.header("idempotency-key"), Some("retry-cellscript-demo-1.2.3"));
    assert_eq!(first.body, second.body);
}

#[test]
fn cellc_auth_capability_submit_posts_ckb_wallet_signature_to_registry_api() {
    let temp = tempfile::tempdir().unwrap();
    let (api_url, request_rx) = start_mock_registry_api_expect_path(
        "/v1/capabilities",
        serde_json::json!({
            "request_id": "req_cap",
            "key_id": "cap_test",
            "status": "active"
        }),
    );
    let create = cellc_command()
        .arg("auth")
        .arg("capability")
        .arg("create")
        .arg("--registry-origin")
        .arg(&api_url)
        .arg("--principal-id")
        .arg(format!("0x{}", "11".repeat(32)))
        .arg("--principal-type")
        .arg("ckb_secp256k1")
        .arg("--capability-pubkey")
        .arg("p256-spki:test")
        .arg("--scope")
        .arg("publish:cellscript/demo")
        .arg("--json")
        .output()
        .unwrap();
    assert!(create.status.success(), "stderr: {}", String::from_utf8_lossy(&create.stderr));
    let payload: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let payload_path = temp.path().join("capability-payload.json");
    let signature_path = temp.path().join("wallet-signature.json");
    std::fs::write(&payload_path, serde_json::to_vec_pretty(&payload).unwrap()).unwrap();
    std::fs::write(
        &signature_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "scheme": "ckb_secp256k1",
            "challenge": serde_json::to_string(&payload).unwrap(),
            "signature": format!("0x{}", "22".repeat(65)),
            "public_key": format!("0x02{}", "33".repeat(32))
        }))
        .unwrap(),
    )
    .unwrap();

    let output = cellc_command()
        .arg("auth")
        .arg("capability")
        .arg("submit")
        .arg("--api-url")
        .arg(&api_url)
        .arg("--payload")
        .arg(&payload_path)
        .arg("--wallet-signature")
        .arg(&signature_path)
        .arg("--json")
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["status"], "active");
    let request = request_rx.recv_timeout(Duration::from_secs(5)).expect("capability request");
    assert_eq!(request["payload"], payload);
    assert_eq!(request["wallet_signature"]["scheme"], "ckb_secp256k1");
}

#[test]
fn cellc_auth_namespace_claim_posts_signed_capability_payload_to_registry_api() {
    let temp = tempfile::tempdir().unwrap();
    let (api_url, request_rx) = start_mock_registry_api_expect_path(
        "/v1/namespaces/claim",
        serde_json::json!({
            "request_id": "req_namespace",
            "namespace": "exampleorg",
            "status": "active"
        }),
    );
    let create = cellc_command()
        .arg("auth")
        .arg("capability")
        .arg("create")
        .arg("--registry-origin")
        .arg(&api_url)
        .arg("--principal-id")
        .arg("0x1111111111111111111111111111111111111111")
        .arg("--capability-pubkey")
        .arg("p256-spki:test")
        .arg("--scope")
        .arg("publish:exampleorg/demo")
        .arg("--json")
        .output()
        .unwrap();
    assert!(create.status.success(), "stderr: {}", String::from_utf8_lossy(&create.stderr));
    let payload: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    let payload_path = temp.path().join("capability-payload.json");
    let signature_path = temp.path().join("joyid-signature.json");
    std::fs::write(&payload_path, serde_json::to_vec_pretty(&payload).unwrap()).unwrap();
    std::fs::write(
        &signature_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "challenge": serde_json::to_string(&payload).unwrap(),
            "signature": "sig",
            "message": "message",
            "pubkey": "pubkey",
            "keyType": "main_key",
            "alg": -7
        }))
        .unwrap(),
    )
    .unwrap();

    let output = cellc_command()
        .arg("auth")
        .arg("namespace")
        .arg("claim")
        .arg("--api-url")
        .arg(&api_url)
        .arg("--namespace")
        .arg("exampleorg")
        .arg("--payload")
        .arg(&payload_path)
        .arg("--joyid-signature")
        .arg(&signature_path)
        .arg("--json")
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["status"], "active");
    let request = request_rx.recv_timeout(Duration::from_secs(5)).expect("namespace claim request");
    assert_eq!(request["namespace"], "exampleorg");
    assert_eq!(request["payload"], payload);
    assert_eq!(request["wallet_signature"]["signature"], "sig");
}

#[test]
fn cellc_auth_capability_revoke_generates_payload_and_posts_revocation() {
    let temp = tempfile::tempdir().unwrap();
    let key_id = format!("cap_{}", "a".repeat(32));
    let expected_path = format!("/v1/capabilities/{}/revoke", key_id);
    let (api_url, request_rx) = start_mock_registry_api_expect_path(
        &expected_path,
        serde_json::json!({
            "request_id": "req_revoke",
            "key_id": key_id,
            "status": "revoked",
            "revoked_at": "2026-06-23T12:00:00Z"
        }),
    );
    let create = cellc_command()
        .arg("auth")
        .arg("capability")
        .arg("revoke")
        .arg("--registry-origin")
        .arg(&api_url)
        .arg("--principal-id")
        .arg("0x1111111111111111111111111111111111111111")
        .arg("--capability-key-id")
        .arg(&key_id)
        .arg("--json")
        .output()
        .unwrap();
    assert!(create.status.success(), "stderr: {}", String::from_utf8_lossy(&create.stderr));
    let payload: serde_json::Value = serde_json::from_slice(&create.stdout).unwrap();
    assert_eq!(payload["action"], "revoke_capability");
    assert_eq!(payload["capability_key_id"], key_id);

    let payload_path = temp.path().join("revoke-payload.json");
    let signature_path = temp.path().join("joyid-revoke-signature.json");
    std::fs::write(&payload_path, serde_json::to_vec_pretty(&payload).unwrap()).unwrap();
    std::fs::write(
        &signature_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "challenge": serde_json::to_string(&payload).unwrap(),
            "signature": "sig",
            "message": "message",
            "pubkey": "pubkey",
            "keyType": "main_key",
            "alg": -7
        }))
        .unwrap(),
    )
    .unwrap();

    let output = cellc_command()
        .arg("auth")
        .arg("capability")
        .arg("revoke")
        .arg("--api-url")
        .arg(&api_url)
        .arg("--payload")
        .arg(&payload_path)
        .arg("--joyid-signature")
        .arg(&signature_path)
        .arg("--reason")
        .arg("rotated")
        .arg("--json")
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["status"], "revoked");
    let request = request_rx.recv_timeout(Duration::from_secs(5)).expect("revoke request");
    assert_eq!(request["payload"], payload);
    assert_eq!(request["reason"], "rotated");
}

#[test]
fn cellc_publish_offline_writes_source_published_registry_fixture() {
    let temp = tempfile::tempdir().unwrap();
    write_publish_fixture_package(temp.path());

    let output = cellc_command().arg("publish").arg("--offline").current_dir(temp.path()).output().unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Published offline registry fixture"), "unexpected stdout: {stdout}");

    let index = cellscript::package::registry::RegistryIndex::read_from_repo(temp.path()).unwrap();
    assert_eq!(index.namespace, "cellscript");
    assert_eq!(index.name, "demo");
    let version = index.versions.iter().find(|version| version.version == "1.2.3").expect("published version");
    assert_eq!(version.status, cellscript::package::registry::RegistryEntryStatus::SourcePublished);
}

#[test]
fn cellc_registry_edit_yanks_existing_version() {
    let temp = tempfile::tempdir().unwrap();
    cellscript::package::registry::RegistryIndex::append_version(
        temp.path(),
        "token",
        "cellscript",
        cellscript::package::registry::RegistryVersion {
            edition: cellscript::CURRENT_EDITION,
            compatibility_profile_hash: "test-compatibility-profile".to_string(),
            version: "1.0.0".to_string(),
            tag: "v1.0.0".to_string(),
            source_hash: "abc123".to_string(),
            cellscript_version: "0.20.0".to_string(),
            dependencies: Default::default(),
            abi_index: None,
            schema_hash: None,
            license: Some("MIT".to_string()),
            released_at: None,
            status: cellscript::package::registry::RegistryEntryStatus::VerifiedBuild,
            yanked: false,
            yanked_at: None,
            yanked_reason: None,
            replaced_by: None,
            audit: None,
            compiler_requirement: format!(">={}", cellscript::VERSION),
        },
    )
    .unwrap();

    let output = cellc_command()
        .arg("registry")
        .arg("edit")
        .arg("--yank")
        .arg("1.0.0")
        .arg("--reason")
        .arg("security advisory")
        .arg("--replaced-by")
        .arg("1.0.1")
        .arg("--yanked-at")
        .arg("2026-06-23T00:00:00Z")
        .current_dir(temp.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let index = cellscript::package::registry::RegistryIndex::read_from_repo(temp.path()).unwrap();
    let version = index.versions.iter().find(|version| version.version == "1.0.0").unwrap();
    assert!(version.yanked);
    assert_eq!(version.yanked_at.as_deref(), Some("2026-06-23T00:00:00Z"));
    assert_eq!(version.yanked_reason.as_deref(), Some("security advisory"));
    assert_eq!(version.replaced_by.as_deref(), Some("1.0.1"));
}

#[test]
fn cellc_registry_verify_json_fails_closed_for_missing_deployment_ref() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let mut lockfile = cellscript::package::Lockfile::new();
    lockfile.package = cellscript::package::LockfilePackageInfo {
        edition: cellscript::CURRENT_EDITION,
        name: "token".to_string(),
        version: "1.0.0".to_string(),
        namespace: Some("cellscript".to_string()),
        source_hash: Some("source_hash".to_string()),
        compiler_source_hash: None,
        compiler_requirement: format!(">={}", cellscript::VERSION),
        resolver_compiler_version: cellscript::VERSION.to_string(),
    };
    lockfile.package_build = Some(cellscript::package::LockedBuildInfo {
        edition: cellscript::CURRENT_EDITION,
        compatibility_profile_hash: "test-compatibility-profile".to_string(),
        compiler_version: Some("0.19.0".to_string()),
        target_profile: Some("ckb".to_string()),
        artifact_hash: Some("artifact_hash".to_string()),
        metadata_hash: Some("metadata_hash".to_string()),
        schema_hash: Some("schema_hash".to_string()),
        cell_data_codec_manifest_hash: Some("codec_manifest_hash".to_string()),
        abi_hash: Some("abi_hash".to_string()),
        constraints_hash: Some("constraints_hash".to_string()),
    });
    lockfile.write_to_root(root).unwrap();

    let deployed = cellscript::package::DeployedManifest {
        version: cellscript::package::DeployedManifest::CURRENT_VERSION,
        schema: cellscript::package::DEPLOYED_MANIFEST_SCHEMA.to_string(),
        package: cellscript::package::DeployedPackageInfo {
            edition: cellscript::CURRENT_EDITION,
            name: "token".to_string(),
            version: "1.0.0".to_string(),
            source_hash: Some("source_hash".to_string()),
        },
        build: Some(cellscript::package::DeployedBuildInfo {
            edition: cellscript::CURRENT_EDITION,
            compatibility_profile_hash: "test-compatibility-profile".to_string(),
            compiler_version: Some("0.19.0".to_string()),
            artifact_hash: Some("artifact_hash".to_string()),
            metadata_hash: Some("metadata_hash".to_string()),
            schema_hash: Some("schema_hash".to_string()),
            cell_data_codec_manifest_hash: Some("codec_manifest_hash".to_string()),
            abi_hash: Some("abi_hash".to_string()),
            constraints_hash: Some("constraints_hash".to_string()),
        }),
        deployments: vec![cellscript::package::DeploymentRecord {
            edition: cellscript::CURRENT_EDITION,
            compatibility_profile_hash: "test-compatibility-profile".to_string(),
            network: "aggron4".to_string(),
            chain_id: "ckb-testnet".to_string(),
            tx_hash: "0xaaaa".to_string(),
            output_index: 0,
            code_hash: "0xbbbb".to_string(),
            hash_type: "data1".to_string(),
            dep_type: "code".to_string(),
            data_hash: "0xcccc".to_string(),
            out_point: "0xaaaa:0".to_string(),
            artifact_hash: None,
            metadata_hash: None,
            schema_hash: None,
            cell_data_codec_manifest_hash: None,
            abi_hash: None,
            constraints_hash: None,
            compiler_version: None,
            type_id: None,
            script_role: None,
            status: None,
            upgrade_lineage: None,
            audit_report_hash: None,
            publisher_signature: None,
            cell_deps: vec![],
        }],
    };
    deployed.write_to_root(root).unwrap();

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).arg("registry").arg("verify").arg("--json").current_dir(root).output().unwrap();

    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "failed");
    assert!(report["violations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|violation| violation.as_str().unwrap_or_default().contains("missing from Cell.lock")));
}

#[test]
fn cellc_registry_verify_accepts_upgrade_lineage_pointing_elsewhere() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    // Lineage points at a pruned prior deployment's out_point (not self, not empty).
    write_offline_fixture_with_lineage(root, Some("0xaaaa:0"));

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).arg("registry").arg("verify").arg("--json").current_dir(root).output().unwrap();

    assert!(
        output.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "ok", "violations: {:?}", report["violations"]);
    assert!(report["violations"].as_array().unwrap().is_empty());
}

#[test]
fn cellc_registry_verify_rejects_upgrade_lineage_pointing_at_itself() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    // Current deployment's lineage points at its own out_point.
    write_offline_fixture_with_lineage(root, Some("0xbbbb:0"));

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).arg("registry").arg("verify").arg("--json").current_dir(root).output().unwrap();

    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "failed");
    assert!(
        report["violations"].as_array().unwrap().iter().any(|v| v.as_str().unwrap_or_default().contains("own out_point")),
        "violations: {:?}",
        report["violations"]
    );
}

#[test]
fn cellc_registry_verify_live_accepts_matching_rpc_cell() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let data_hash = "0x1111111111111111111111111111111111111111111111111111111111111111";
    write_live_registry_fixture(root, data_hash);
    let rpc_url = start_mock_ckb_rpc(vec![
        ("get_blockchain_info", serde_json::json!({ "chain": "ckb_testnet" })),
        ("get_live_cell", live_cell_rpc_result("live", data_hash)),
    ]);

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("registry")
        .arg("verify")
        .arg("--live")
        .arg("--rpc-url")
        .arg(&rpc_url)
        .arg("--network")
        .arg("aggron4")
        .arg("--json")
        .current_dir(root)
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "ok");
    assert_eq!(report["live"]["enabled"], true);
    assert_eq!(report["live"]["checked"], 1);
    assert_eq!(report["live"]["evidence"][0]["status"], "live-verified");
    assert_eq!(report["live"]["evidence"][0]["rpc_data_hash"], data_hash);
    assert!(report["violations"].as_array().unwrap().is_empty());
}

#[test]
fn cellc_registry_verify_live_accepts_type_hash_and_type_id() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let data_hash = "0x3333333333333333333333333333333333333333333333333333333333333333";
    let type_code_hash = "0x4444444444444444444444444444444444444444444444444444444444444444";
    let type_id = "0x5555555555555555555555555555555555555555555555555555555555555555";
    let script_hash = ckb_script_hash_for_test(type_code_hash, "data1", type_id);
    write_live_registry_fixture_with(root, data_hash, &script_hash, "type", Some(type_id));
    let rpc_url = start_mock_ckb_rpc(vec![
        ("get_blockchain_info", serde_json::json!({ "chain": "ckb-testnet" })),
        (
            "get_live_cell",
            live_cell_rpc_result_with_type(
                "live",
                data_hash,
                serde_json::json!({
                    "code_hash": type_code_hash,
                    "hash_type": "data1",
                    "args": type_id
                }),
            ),
        ),
    ]);

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("registry")
        .arg("verify")
        .arg("--live")
        .arg("--rpc-url")
        .arg(&rpc_url)
        .arg("--json")
        .current_dir(root)
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "ok");
    assert_eq!(report["live"]["evidence"][0]["rpc_code_hash"], script_hash);
    assert!(report["violations"].as_array().unwrap().is_empty());
}

#[test]
fn cellc_registry_verify_live_rejects_dead_rpc_cell() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let data_hash = "0x2222222222222222222222222222222222222222222222222222222222222222";
    write_live_registry_fixture(root, data_hash);
    let rpc_url = start_mock_ckb_rpc(vec![
        ("get_blockchain_info", serde_json::json!({ "chain": "ckb-testnet" })),
        ("get_live_cell", live_cell_rpc_result("dead", data_hash)),
    ]);

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("registry")
        .arg("verify")
        .arg("--live")
        .arg("--rpc-url")
        .arg(&rpc_url)
        .arg("--json")
        .current_dir(root)
        .output()
        .unwrap();

    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "failed");
    assert_eq!(report["live"]["evidence"][0]["rpc_status"], "dead");
    assert!(report["violations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|violation| violation.as_str().unwrap_or_default().contains("is not live")));
}

#[test]
fn cellc_registry_verify_live_rejects_deprecated_deployment_status() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let data_hash = "0x6666666666666666666666666666666666666666666666666666666666666666";
    write_live_registry_fixture(root, data_hash);
    let mut deployed = cellscript::package::DeployedManifest::read_from_root(root).unwrap().unwrap();
    deployed.deployments[0].status = Some(cellscript::package::DeploymentStatus::Deprecated);
    deployed.write_to_root(root).unwrap();
    let rpc_url = start_mock_ckb_rpc(vec![
        ("get_blockchain_info", serde_json::json!({ "chain": "ckb-testnet" })),
        ("get_live_cell", live_cell_rpc_result("live", data_hash)),
    ]);

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("registry")
        .arg("verify")
        .arg("--live")
        .arg("--rpc-url")
        .arg(&rpc_url)
        .arg("--json")
        .current_dir(root)
        .output()
        .unwrap();

    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "failed");
    assert_eq!(report["live"]["evidence"][0]["status"], "failed");
    assert_eq!(report["live"]["evidence"][0]["deployment_status"], "deprecated");
    assert!(report["live"]["evidence"][0]["violations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|violation| violation.as_str().unwrap_or_default().contains("not active")));
}

#[test]
fn cellc_registry_verify_live_rejects_missing_deployment_status() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let data_hash = "0x7777777777777777777777777777777777777777777777777777777777777777";
    write_live_registry_fixture(root, data_hash);
    let mut deployed = cellscript::package::DeployedManifest::read_from_root(root).unwrap().unwrap();
    deployed.deployments[0].status = None;
    deployed.write_to_root(root).unwrap();
    let rpc_url = start_mock_ckb_rpc(vec![
        ("get_blockchain_info", serde_json::json!({ "chain": "ckb-testnet" })),
        ("get_live_cell", live_cell_rpc_result("live", data_hash)),
    ]);

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("registry")
        .arg("verify")
        .arg("--live")
        .arg("--rpc-url")
        .arg(&rpc_url)
        .arg("--json")
        .current_dir(root)
        .output()
        .unwrap();

    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "failed");
    assert_eq!(report["live"]["evidence"][0]["status"], "failed");
    assert!(report["live"]["evidence"][0]["deployment_status"].is_null());
    assert!(report["live"]["evidence"][0]["violations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|violation| violation.as_str().unwrap_or_default().contains("has no status")));
}

#[test]
fn cellc_registry_verify_requires_trust_metadata_when_requested() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let data_hash = "0x8888888888888888888888888888888888888888888888888888888888888888";
    write_live_registry_fixture(root, data_hash);

    let rejected = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("registry")
        .arg("verify")
        .arg("--require-publisher-signature")
        .arg("--require-audit-report")
        .arg("--json")
        .current_dir(root)
        .output()
        .unwrap();

    assert!(!rejected.status.success(), "unexpected success: {}", String::from_utf8_lossy(&rejected.stdout));
    let report: serde_json::Value = serde_json::from_slice(&rejected.stdout).unwrap();
    assert_eq!(report["status"], "failed");
    assert_eq!(report["trust"]["enabled"], true);
    assert_eq!(report["trust"]["verification_boundary"], "metadata-presence-only");
    assert_eq!(report["trust"]["evidence"][0]["publisher_signature_status"], "missing");
    assert_eq!(report["trust"]["evidence"][0]["audit_report_hash_status"], "missing");
    assert!(report["violations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|violation| violation.as_str().unwrap_or_default().contains("publisher_signature")));

    let mut deployed = cellscript::package::DeployedManifest::read_from_root(root).unwrap().unwrap();
    deployed.deployments[0].publisher_signature = Some("sig:fixture".to_string());
    deployed.deployments[0].audit_report_hash = Some("0xabc".to_string());
    deployed.write_to_root(root).unwrap();

    let accepted = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("registry")
        .arg("verify")
        .arg("--require-publisher-signature")
        .arg("--require-audit-report")
        .arg("--json")
        .current_dir(root)
        .output()
        .unwrap();

    assert!(accepted.status.success(), "stderr: {}", String::from_utf8_lossy(&accepted.stderr));
    let report: serde_json::Value = serde_json::from_slice(&accepted.stdout).unwrap();
    assert_eq!(report["status"], "ok");
    assert_eq!(report["trust"]["evidence"][0]["status"], "policy-satisfied");
    assert_eq!(report["trust"]["evidence"][0]["publisher_signature_status"], "present-unverified");
    assert_eq!(report["trust"]["evidence"][0]["audit_report_hash_status"], "present");
    assert!(report["violations"].as_array().unwrap().is_empty());
}

#[test]
fn cellc_install_path_updates_lockfile_and_remove_prunes_it() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let dep_root = root.join("math");
    let util_root = root.join("util");

    std::fs::create_dir_all(dep_root.join("src")).unwrap();
    std::fs::create_dir_all(util_root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.cell"), "module demo;\n").unwrap();
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
        dep_root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "math"
version = "0.2.0"
entry = "src/lib.cell"

[dependencies.util]
version = "0.1.0"
path = "../util"
"#,
    )
    .unwrap();
    std::fs::write(dep_root.join("src/lib.cell"), "module math;\n").unwrap();
    std::fs::write(
        util_root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "util"
version = "0.1.0"
entry = "src/lib.cell"
"#,
    )
    .unwrap();
    std::fs::write(util_root.join("src/lib.cell"), "module util;\n").unwrap();

    let install = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("install")
        .arg("math")
        .arg("--path")
        .arg("math")
        .output()
        .unwrap();
    assert!(install.status.success(), "stderr: {}", String::from_utf8_lossy(&install.stderr));

    let manifest: toml::Value = std::fs::read_to_string(root.join("Cell.toml")).unwrap().parse().unwrap();
    assert_eq!(manifest["dependencies"]["math"]["path"].as_str().unwrap(), "math");

    let lockfile: cellscript::package::Lockfile = toml::from_str(&std::fs::read_to_string(root.join("Cell.lock")).unwrap()).unwrap();
    let math_node = lockfile.root.dependencies.get("math").expect("math root edge should be locked");
    let locked = lockfile.dependencies.get(math_node).expect("math should be locked");
    assert_eq!(locked.version, "0.2.0");
    assert!(matches!(&locked.source, cellscript::package::LockedSource::Path { path } if path == "math"));
    let util_node = locked.dependencies.get("util").expect("math should have a util edge");
    let util = lockfile.dependencies.get(util_node).expect("transitive util should be locked");
    assert_eq!(util.version, "0.1.0");

    let lock_before_update = std::fs::read(root.join("Cell.lock")).unwrap();
    let plan_path = root.join("upgrade-plan.json");
    let update = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("update")
        .arg("--output")
        .arg(&plan_path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(update.status.success(), "stderr: {}", String::from_utf8_lossy(&update.stderr));
    assert_eq!(std::fs::read(root.join("Cell.lock")).unwrap(), lock_before_update);
    let plan: serde_json::Value = serde_json::from_slice(&std::fs::read(&plan_path).unwrap()).unwrap();
    assert_eq!(plan["apply_status"], "ready");
    assert_eq!(plan["locks"][0]["new_lock"]["dependencies"].as_object().unwrap().len(), 2);
    let apply = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("update")
        .arg("--apply-plan")
        .arg(&plan_path)
        .output()
        .unwrap();
    assert!(apply.status.success(), "stderr: {}", String::from_utf8_lossy(&apply.stderr));

    let remove = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("remove").arg("math").output().unwrap();
    assert!(remove.status.success(), "stderr: {}", String::from_utf8_lossy(&remove.stderr));

    let pruned: cellscript::package::Lockfile = toml::from_str(&std::fs::read_to_string(root.join("Cell.lock")).unwrap()).unwrap();
    assert!(pruned.root.dependencies.is_empty());
    assert!(pruned.dependencies.is_empty());
}

#[test]
fn cellc_install_rejects_self_path_dependency() {
    // `cellc install --path <self_root>` used to write a `[dependencies.""]` row
    // that turned every subsequent `cellc build` into a circular-dep failure.
    // The cellc install surface must now refuse the self-reference fail-closed.
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

    let install = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("install").arg("--path").arg(".").output().unwrap();

    assert!(!install.status.success(), "self path install must be rejected");
    let stderr = String::from_utf8_lossy(&install.stderr);
    assert!(
        stderr.contains("refusing to add self-dependency") || stderr.contains("current package root"),
        "expected self-dep refusal, got: {stderr}"
    );

    // Cell.toml must not have gained a dependencies row.
    let manifest_text = std::fs::read_to_string(root.join("Cell.toml")).unwrap();
    let manifest: toml::Value = manifest_text.parse().unwrap();
    let deps = manifest.get("dependencies").and_then(|d| d.as_table()).map(|t| t.len()).unwrap_or(0);
    assert_eq!(deps, 0, "no dependency row should be written for a self path install");
}

#[test]
fn cellc_install_rejects_self_name_dependency() {
    // `cellc install demo --path <somewhere>` where the package's own name is
    // 'demo' must be rejected: a package cannot list itself as a dependency.
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

    // Even when the path points somewhere else, an explicit self-name dependency
    // is a logical circular dep and must be rejected.
    let install = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("install")
        .arg("demo")
        .arg("--path")
        .arg("./src")
        .output()
        .unwrap();

    assert!(!install.status.success(), "self name install must be rejected");
    let stderr = String::from_utf8_lossy(&install.stderr);
    assert!(
        stderr.contains("refusing to add self-dependency") && stderr.contains("cannot depend on itself"),
        "expected self-name refusal, got: {stderr}"
    );
}
