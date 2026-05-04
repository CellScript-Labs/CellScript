use cellscript::{compile, BuilderAssumptionMetadata, CompileOptions};
use serde_json::json;
use std::process::Command;
use tempfile::tempdir;

const IDENTITY_CREATE_UNIQUE: &str = r#"
module v016::identity

resource Badge has store, create, replace
    identity(field(badge_id))
{
    badge_id: [u8; 32]
    owner: Address
}

action issue_badge(badge_id: [u8; 32], owner: Address) -> Badge
where
    create_unique<Badge>(identity = field(badge_id)) {
        badge_id,
        owner
    } with_lock(owner)
"#;

const METADATA_ONLY_INVARIANT: &str = r#"
module v016::gap

invariant token_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_sum(group_outputs<Token>.amount) <= assert_sum(group_inputs<Token>.amount)
}

resource Token {
    amount: u64
}

action noop() -> u64
where
    0
"#;

fn evidence_for(assumption: &BuilderAssumptionMetadata) -> serde_json::Value {
    json!({
        "assumption_id": assumption.assumption_id,
        "kind": assumption.kind,
        "origin": assumption.origin,
        "feature": assumption.feature,
        "proof_plan_status": assumption.proof_plan_status,
        "evidence": {
            "source": "unit-test-fixture",
            "checked": true
        }
    })
}

#[test]
fn proof_plan_soundness_is_emitted_and_passes_for_checked_identity() {
    let result =
        compile(IDENTITY_CREATE_UNIQUE, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() })
            .unwrap();

    assert_eq!(result.metadata.runtime.proof_plan_soundness.status, "passed");
    assert!(result.metadata.runtime.proof_plan_soundness.checked_records > 0);
    assert!(
        result.metadata.runtime.builder_assumptions.iter().any(|assumption| assumption.kind == "create_unique_global_uniqueness"),
        "{:#?}",
        result.metadata.runtime.builder_assumptions
    );
}

#[test]
fn strict_0_16_rejects_metadata_only_proof_plan_gaps() {
    let err = compile(
        METADATA_ONLY_INVARIANT,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.16".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect_err("v0.16 strict mode must reject metadata-only ProofPlan gaps");

    assert!(err.message.contains("ProofPlan soundness check failed"), "unexpected error: {}", err.message);
    assert!(err.message.contains("PP0150"), "unexpected error: {}", err.message);
}

#[test]
fn proof_plan_soundness_rejects_local_runtime_mismatches() {
    let result =
        compile(IDENTITY_CREATE_UNIQUE, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() })
            .unwrap();
    let mut metadata = result.metadata.clone();
    let plan = metadata
        .actions
        .iter_mut()
        .flat_map(|action| action.proof_plan.iter_mut())
        .next()
        .expect("identity action should expose local ProofPlan records");
    plan.reads.push("witness".to_string());

    let report = cellscript::proof_plan::soundness::check_metadata(&metadata, false);
    assert_eq!(report.status, "failed", "{report:#?}");
    assert!(report.issues.iter().any(|issue| issue.code == "PP0403"), "{report:#?}");
}

#[test]
fn validate_tx_checks_builder_assumption_evidence() {
    let result =
        compile(IDENTITY_CREATE_UNIQUE, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() })
            .unwrap();
    let assumptions = &result.metadata.runtime.builder_assumptions;
    let assumption_id = assumptions
        .iter()
        .find(|assumption| assumption.kind == "create_unique_global_uniqueness")
        .expect("global uniqueness assumption")
        .assumption_id
        .clone();

    let missing_evidence = json!({
        "inputs": [{}],
        "outputs": [{}],
        "cell_deps": [],
        "witnesses": []
    });
    let report = cellscript::assumptions::validate_transaction_against_metadata(&result.metadata, &missing_evidence);
    assert_eq!(report.status, "failed");
    assert!(report.violations.iter().any(|violation| violation.assumption_id == assumption_id));

    let bare_evidence = assumptions.iter().map(|assumption| json!({"assumption_id": assumption.assumption_id})).collect::<Vec<_>>();
    let with_bare_evidence = json!({
        "inputs": [{}],
        "outputs": [{}],
        "cell_deps": [],
        "witnesses": [],
        "builder_assumption_evidence": bare_evidence
    });
    let report = cellscript::assumptions::validate_transaction_against_metadata(&result.metadata, &with_bare_evidence);
    assert_eq!(report.status, "failed");
    assert!(
        report
            .violations
            .iter()
            .any(|violation| violation.message.contains("proof_plan_status") || violation.message.contains("evidence")),
        "{report:#?}"
    );

    let evidence = assumptions.iter().map(evidence_for).collect::<Vec<_>>();
    let with_evidence = json!({
        "inputs": [{}],
        "outputs": [{}],
        "cell_deps": [],
        "witnesses": [],
        "builder_assumption_evidence": evidence
    });
    let report = cellscript::assumptions::validate_transaction_against_metadata(&result.metadata, &with_evidence);
    assert_eq!(report.status, "ok", "{:#?}", report);
}

#[test]
fn cli_explain_assumptions_and_validate_tx_are_machine_readable() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("identity.cell");
    std::fs::write(&source, IDENTITY_CREATE_UNIQUE).unwrap();

    let explain = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("explain-assumptions").arg(&source).arg("--json").output().unwrap();
    assert!(explain.status.success(), "stderr: {}", String::from_utf8_lossy(&explain.stderr));
    let explain_json: serde_json::Value = serde_json::from_slice(&explain.stdout).unwrap();
    assert_eq!(explain_json["status"], "ok");
    assert!(explain_json["assumption_count"].as_u64().unwrap() > 0);

    let result =
        compile(IDENTITY_CREATE_UNIQUE, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() })
            .unwrap();
    let evidence = result.metadata.runtime.builder_assumptions.iter().map(evidence_for).collect::<Vec<_>>();
    let metadata = temp.path().join("identity.meta.json");
    let tx = temp.path().join("tx.json");
    std::fs::write(&metadata, serde_json::to_vec_pretty(&result.metadata).unwrap()).unwrap();
    std::fs::write(
        &tx,
        serde_json::to_vec_pretty(&json!({
            "inputs": [{}],
            "outputs": [{}],
            "builder_assumption_evidence": evidence
        }))
        .unwrap(),
    )
    .unwrap();

    let validate = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("validate-tx")
        .arg("--against")
        .arg(&metadata)
        .arg(&tx)
        .arg("--json")
        .output()
        .unwrap();
    assert!(validate.status.success(), "stderr: {}", String::from_utf8_lossy(&validate.stderr));
    let validate_json: serde_json::Value = serde_json::from_slice(&validate.stdout).unwrap();
    assert_eq!(validate_json["status"], "ok");
}

#[test]
fn cli_verify_deploy_rejects_tampered_plan_integrity() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("identity.cell");
    let plan_path = temp.path().join("deploy.json");
    let bad_plan_path = temp.path().join("bad-deploy.json");
    std::fs::write(&source, IDENTITY_CREATE_UNIQUE).unwrap();

    let deploy = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("deploy-plan")
        .arg(&source)
        .arg("--target-profile")
        .arg("ckb")
        .arg("--output")
        .arg(&plan_path)
        .output()
        .unwrap();
    assert!(deploy.status.success(), "stderr: {}", String::from_utf8_lossy(&deploy.stderr));

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("verify-deploy").arg(&plan_path).arg("--json").output().unwrap();
    assert!(verify.status.success(), "stderr: {}", String::from_utf8_lossy(&verify.stderr));
    let verify_json: serde_json::Value = serde_json::from_slice(&verify.stdout).unwrap();
    assert_eq!(verify_json["status"], "ok");

    let mut plan: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&plan_path).unwrap()).unwrap();
    plan["artifact"]["hash"] = json!("not-a-canonical-hash");
    std::fs::write(&bad_plan_path, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();

    let verify_bad =
        Command::new(env!("CARGO_BIN_EXE_cellc")).arg("verify-deploy").arg(&bad_plan_path).arg("--json").output().unwrap();
    assert!(!verify_bad.status.success(), "tampered deploy plan must fail");
    let verify_bad_json: serde_json::Value = serde_json::from_slice(&verify_bad.stdout).unwrap();
    assert_eq!(verify_bad_json["status"], "failed");
    let violations = verify_bad_json["violations"].as_array().expect("violations");
    assert!(violations.iter().any(|violation| violation.as_str().is_some_and(|text| text.contains("artifact.hash"))));
}

#[test]
fn standard_ckb_compat_manifest_covers_required_suites() {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("compat/ckb_standard/manifest.json")).expect("compat manifest must parse");
    assert_eq!(manifest["schema"], "cellscript-ckb-standard-compat-v0.16");
    let suites = manifest["suites"].as_array().expect("suites array");
    let names = suites.iter().filter_map(|suite| suite["name"].as_str()).collect::<Vec<_>>();
    for expected in ["sudt", "xudt", "acp", "cheque", "omnilock", "nervosdao-since", "type-id"] {
        assert!(names.contains(&expected), "missing compat suite {expected}: {names:?}");
    }
    for suite in suites {
        assert!(suite["accepted_fixtures"].as_array().is_some_and(|fixtures| !fixtures.is_empty()), "{suite:#?}");
        assert!(suite["rejected_fixtures"].as_array().is_some_and(|fixtures| !fixtures.is_empty()), "{suite:#?}");
        assert_eq!(suite["script_reference_metadata"], "required");
        // Verify fixture files are declared
        assert!(suite.get("fixture_files").is_some(), "suite {:?} missing fixture_files", suite["name"]);
        let fixture_files = suite.get("fixture_files").unwrap().as_object().expect("fixture_files must be object");
        assert!(!fixture_files.is_empty(), "suite {:?} has empty fixture_files", suite["name"]);
    }
}

#[test]
fn standard_ckb_compat_fixture_files_parse_and_have_required_fields() {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("compat/ckb_standard/manifest.json")).expect("compat manifest must parse");
    let suites = manifest["suites"].as_array().expect("suites array");
    for suite in suites {
        let fixture_files = suite.get("fixture_files").unwrap().as_object().expect("fixture_files");
        for (fixture_name, file_name) in fixture_files {
            let file_name_str = file_name.as_str().expect("file name string");
            let path = format!("tests/compat/ckb_standard/{}", file_name_str);
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("fixture file {} for '{}' not found: {}", path, fixture_name, e));
            let fixture: serde_json::Value = serde_json::from_str(&content)
                .unwrap_or_else(|e| panic!("fixture file {} for '{}' does not parse as JSON: {}", path, fixture_name, e));
            assert_eq!(fixture["schema"], "cellscript-ckb-fixture-v0.16", "fixture {} schema mismatch", fixture_name);
            assert!(fixture["status"].as_str().is_some(), "fixture {} missing status", fixture_name);
            assert!(fixture["transaction_shape"].is_object(), "fixture {} missing transaction_shape", fixture_name);
            assert!(fixture["expected_behavior"].is_object(), "fixture {} missing expected_behavior", fixture_name);
            assert!(fixture["script_args_layout"].is_object(), "fixture {} missing script_args_layout", fixture_name);
            assert!(fixture["witness_layout"].is_object(), "fixture {} missing witness_layout", fixture_name);
            assert!(fixture["molecule_data_layout"].is_object(), "fixture {} missing molecule_data_layout", fixture_name);
            assert!(fixture["metadata_expectation"].is_object(), "fixture {} missing metadata_expectation", fixture_name);
            assert!(fixture["cycle_report"].is_object(), "fixture {} missing cycle_report", fixture_name);
            assert!(fixture["capacity_report"].is_object(), "fixture {} missing capacity_report", fixture_name);
        }
    }
}

#[test]
fn ckb_stdlib_protocol_modules_exist_and_cover_required_suites() {
    let modules = cellscript::stdlib::ckb_protocols::ckb_stdlib_modules();
    let names = modules.iter().map(|m| m.name.as_str()).collect::<Vec<_>>();
    for expected in ["std::sudt", "std::xudt", "std::type_id", "std::htlc", "std::cheque", "std::acp"] {
        assert!(names.contains(&expected), "missing stdlib module {expected}: {names:?}");
    }
    for module in &modules {
        assert!(!module.proof_plan_trigger.is_empty(), "module {} missing proof_plan_trigger", module.name);
        assert!(!module.proof_plan_scope.is_empty(), "module {} missing proof_plan_scope", module.name);
        assert!(!module.proof_plan_reads.is_empty(), "module {} missing proof_plan_reads", module.name);
        assert!(!module.compatibility_fixture.is_empty(), "module {} missing compatibility_fixture", module.name);
        assert_eq!(module.stability, "schema-stub", "module {} must not be marked stable before implementation coverage", module.name);
    }
}

#[test]
fn ckb_stdlib_protocol_functions_cover_core_operations() {
    let functions = cellscript::stdlib::ckb_protocols::ckb_stdlib_functions();
    let names = functions.iter().map(|f| f.name.as_str()).collect::<Vec<_>>();
    // Verify at least the core protocol functions are present
    assert!(names.contains(&"sudt_transfer"), "missing sudt_transfer: {names:?}");
    assert!(names.contains(&"sudt_mint"), "missing sudt_mint: {names:?}");
    assert!(names.contains(&"xudt_transfer"), "missing xudt_transfer: {names:?}");
    assert!(names.contains(&"type_id_create"), "missing type_id_create: {names:?}");
    assert!(names.contains(&"htlc_claim_with_preimage"), "missing htlc_claim_with_preimage: {names:?}");
    assert!(names.contains(&"cheque_claim"), "missing cheque_claim: {names:?}");
    assert!(names.contains(&"acp_deposit"), "missing acp_deposit: {names:?}");
    // Each function must declare ProofPlan metadata
    for function in &functions {
        assert!(!function.proof_plan_trigger.is_empty(), "function {} missing trigger", function.name);
        assert!(!function.proof_plan_scope.is_empty(), "function {} missing scope", function.name);
        assert!(!function.proof_plan_reads.is_empty(), "function {} missing reads", function.name);
    }
}
