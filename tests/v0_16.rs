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

const TWO_ACTION_IDENTITY_CREATE_UNIQUE: &str = r#"
module v016::two_identity

resource Badge has store, create, replace
    identity(field(badge_id))
{
    badge_id: [u8; 32]
    owner: Address
}

action issue_badge_a(badge_id: [u8; 32], owner: Address) -> Badge
where
    create_unique<Badge>(identity = field(badge_id)) {
        badge_id,
        owner
    } with_lock(owner)

action issue_badge_b(badge_id: [u8; 32], owner: Address) -> Badge
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

fn cellc_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cellc"))
}

fn run_success_json(mut command: Command) -> serde_json::Value {
    let output = command.output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

fn run_failure_json(mut command: Command) -> serde_json::Value {
    let output = command.output().unwrap();
    assert!(!output.status.success(), "command must fail");
    serde_json::from_slice(&output.stdout).unwrap()
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
fn proof_plan_soundness_rejects_scoped_duplicate_obligation_deletion() {
    let result = compile(
        TWO_ACTION_IDENTITY_CREATE_UNIQUE,
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap();
    let mut metadata = result.metadata.clone();
    assert!(metadata.runtime.proof_plan.iter().any(|plan| plan.origin == "action:issue_badge_b"));
    assert!(metadata.runtime.verifier_obligations.iter().any(|obligation| obligation.scope == "action:issue_badge_b"));

    metadata.runtime.proof_plan.retain(|plan| plan.origin != "action:issue_badge_b");
    for action in &mut metadata.actions {
        if action.name == "issue_badge_b" {
            action.proof_plan.clear();
        }
    }

    let report = cellscript::proof_plan::soundness::check_metadata(&metadata, false);
    assert_eq!(report.status, "failed", "{report:#?}");
    assert!(report.issues.iter().any(|issue| issue.code == "PP0002" && issue.origin == "action:issue_badge_b"), "{report:#?}");
}

#[test]
fn proof_plan_soundness_rejects_group_cardinality_drift_after_optimization() {
    let result = compile(
        IDENTITY_CREATE_UNIQUE,
        CompileOptions { target_profile: Some("ckb".to_string()), opt_level: 1, ..CompileOptions::default() },
    )
    .unwrap();
    assert_eq!(result.metadata.runtime.proof_plan_soundness.status, "passed");

    let mut metadata = result.metadata.clone();
    let plan = metadata
        .actions
        .iter_mut()
        .flat_map(|action| action.proof_plan.iter_mut())
        .next()
        .expect("identity action should expose local ProofPlan records");
    plan.group_cardinality = "stale optimizer cardinality".to_string();

    let report = cellscript::proof_plan::soundness::check_metadata(&metadata, false);
    assert_eq!(report.status, "failed", "{report:#?}");
    assert!(report.issues.iter().any(|issue| issue.code == "PP0403" && issue.message.contains("group cardinality")), "{report:#?}");
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

    let mut explain = cellc_command();
    explain.arg("explain-assumptions").arg(&source).arg("--json");
    let explain_json = run_success_json(explain);
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

    let mut validate = cellc_command();
    validate.arg("validate-tx").arg("--against").arg(&metadata).arg(&tx).arg("--json");
    let validate_json = run_success_json(validate);
    assert_eq!(validate_json["status"], "ok");
}

#[test]
fn cli_verify_deploy_rejects_tampered_plan_integrity() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("identity.cell");
    let plan_path = temp.path().join("deploy.json");
    let bad_plan_path = temp.path().join("bad-deploy.json");
    std::fs::write(&source, IDENTITY_CREATE_UNIQUE).unwrap();

    let mut deploy = cellc_command();
    let deploy =
        deploy.arg("deploy-plan").arg(&source).arg("--target-profile").arg("ckb").arg("--output").arg(&plan_path).output().unwrap();
    assert!(deploy.status.success(), "stderr: {}", String::from_utf8_lossy(&deploy.stderr));

    let mut verify = cellc_command();
    verify.arg("verify-deploy").arg(&plan_path).arg("--json");
    let verify_json = run_success_json(verify);
    assert_eq!(verify_json["status"], "ok");

    let mut plan: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&plan_path).unwrap()).unwrap();
    assert!(plan["metadata_schema_version"].as_u64().is_some_and(|version| version > 0), "{plan:#?}");
    plan["artifact"]["hash"] = json!("not-a-canonical-hash");
    plan["metadata_schema_version"] = json!(0);
    std::fs::write(&bad_plan_path, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();

    let mut verify_bad = cellc_command();
    verify_bad.arg("verify-deploy").arg(&bad_plan_path).arg("--json");
    let verify_bad_json = run_failure_json(verify_bad);
    assert_eq!(verify_bad_json["status"], "failed");
    let violations = verify_bad_json["violations"].as_array().expect("violations");
    assert!(violations.iter().any(|violation| violation.as_str().is_some_and(|text| text.contains("artifact.hash"))));
    assert!(violations.iter().any(|violation| violation.as_str().is_some_and(|text| text.contains("metadata_schema_version"))));
}

#[test]
fn cli_v0_16_tooling_outputs_are_machine_readable_and_schema_bound() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("identity.cell");
    let metadata_path = temp.path().join("identity.meta.json");
    let old_metadata_path = temp.path().join("old.meta.json");
    let new_metadata_path = temp.path().join("new.meta.json");
    let tx_path = temp.path().join("tx.json");
    let old_deploy_path = temp.path().join("old.deploy.json");
    let new_deploy_path = temp.path().join("new.deploy.json");
    let bundle_dir = temp.path().join("audit-bundle");
    std::fs::write(&source, IDENTITY_CREATE_UNIQUE).unwrap();

    let result =
        compile(IDENTITY_CREATE_UNIQUE, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() })
            .unwrap();
    let evidence = result.metadata.runtime.builder_assumptions.iter().map(evidence_for).collect::<Vec<_>>();
    std::fs::write(&metadata_path, serde_json::to_vec_pretty(&result.metadata).unwrap()).unwrap();
    std::fs::write(&old_metadata_path, serde_json::to_vec_pretty(&result.metadata).unwrap()).unwrap();
    let mut changed_metadata = result.metadata.clone();
    changed_metadata.runtime.proof_plan[0].coverage.push("unit-test-extra-coverage".to_string());
    std::fs::write(&new_metadata_path, serde_json::to_vec_pretty(&changed_metadata).unwrap()).unwrap();
    std::fs::write(
        &tx_path,
        serde_json::to_vec_pretty(&json!({
            "inputs": [{}],
            "outputs": [{}],
            "builder_assumption_evidence": evidence
        }))
        .unwrap(),
    )
    .unwrap();

    let mut solve = cellc_command();
    solve.arg("solve-tx").arg(&source).arg("--target-profile").arg("ckb").arg("--json");
    let solve_json = run_success_json(solve);
    assert_eq!(solve_json["status"], "template");
    assert!(solve_json["transaction_plan"]["builder_assumption_evidence_requirements"]
        .as_array()
        .is_some_and(|requirements| !requirements.is_empty()));
    assert!(solve_json["limitations"].as_array().is_some_and(|limitations| !limitations.is_empty()));

    let mut profile = cellc_command();
    profile.arg("profile").arg(&source).arg("--target-profile").arg("ckb").arg("--json");
    let profile_json = run_success_json(profile);
    assert_eq!(profile_json["schema"], "cellscript-profile-v0.16");
    let proof_records = profile_json["proof_plan_records"].as_array().expect("profile proof_plan_records");
    assert!(!proof_records.is_empty(), "{profile_json:#?}");
    assert!(proof_records.iter().all(|record| record["feature"].as_str().is_some()), "{profile_json:#?}");

    let mut lock_deps = cellc_command();
    lock_deps.arg("lock-deps").arg(&source).arg("--target-profile").arg("ckb").arg("--json");
    let lock_deps_json = run_success_json(lock_deps);
    assert_eq!(lock_deps_json["schema"], "cellscript-dependency-lock-v0.16");

    let mut proof_diff = cellc_command();
    proof_diff.arg("proof-diff").arg(&old_metadata_path).arg(&new_metadata_path).arg("--json");
    let proof_diff_json = run_success_json(proof_diff);
    assert_eq!(proof_diff_json["schema"], "cellscript-proof-diff-v0.16");
    assert!(proof_diff_json["changed"].as_array().is_some_and(|changed| !changed.is_empty()), "{proof_diff_json:#?}");
    let changed_records = proof_diff_json["changed_records"].as_array().expect("changed_records");
    assert!(
        changed_records.iter().any(|record| {
            record["fields"].as_array().is_some_and(|fields| fields.iter().any(|field| field["field"] == "coverage"))
        }),
        "{proof_diff_json:#?}"
    );

    let mut trace = cellc_command();
    trace.arg("trace-tx").arg("--against").arg(&metadata_path).arg(&tx_path).arg("--json");
    let trace_json = run_success_json(trace);
    assert_eq!(trace_json["schema"], "cellscript-tx-trace-v0.16");
    assert_eq!(trace_json["status"], "ok");
    assert!(trace_json["steps"].as_array().is_some_and(|steps| !steps.is_empty()), "{trace_json:#?}");

    let mut audit_bundle = cellc_command();
    audit_bundle.arg("audit-bundle").arg(&source).arg("--target-profile").arg("ckb").arg("--output").arg(&bundle_dir).arg("--json");
    let audit_bundle_json = run_success_json(audit_bundle);
    assert_eq!(audit_bundle_json["status"], "ok");
    assert!(bundle_dir.join("audit-bundle.json").exists());
    assert!(bundle_dir.join("index.html").exists());

    let mut deploy_old = cellc_command();
    let deploy_old = deploy_old
        .arg("deploy-plan")
        .arg(&source)
        .arg("--target-profile")
        .arg("ckb")
        .arg("--output")
        .arg(&old_deploy_path)
        .output()
        .unwrap();
    assert!(deploy_old.status.success(), "stderr: {}", String::from_utf8_lossy(&deploy_old.stderr));
    let mut deploy_plan: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&old_deploy_path).unwrap()).unwrap();
    let schema_version = deploy_plan["metadata_schema_version"].as_u64().expect("metadata_schema_version");
    deploy_plan["metadata_schema_version"] = json!(schema_version + 1);
    std::fs::write(&new_deploy_path, serde_json::to_vec_pretty(&deploy_plan).unwrap()).unwrap();

    let mut diff_deploy = cellc_command();
    diff_deploy.arg("diff-deploy").arg(&old_deploy_path).arg(&new_deploy_path).arg("--json");
    let diff_deploy_json = run_success_json(diff_deploy);
    assert_eq!(diff_deploy_json["schema"], "cellscript-deploy-diff-v0.16");
    let changed = diff_deploy_json["changed"].as_array().expect("changed");
    assert!(changed.iter().any(|entry| entry["path"] == "/metadata_schema_version"), "{diff_deploy_json:#?}");
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
            assert!(fixture["script_group"].is_object(), "fixture {} missing script_group", fixture_name);
            if fixture["suite"] == "acp" {
                assert_eq!(fixture["script_group"]["kind"], "lock", "ACP fixture {} must model ACP as a lock script", fixture_name);
                assert_eq!(
                    fixture["metadata_expectation"]["proof_plan"]["trigger"], "lock_group",
                    "ACP fixture {} must use lock_group ProofPlan trigger",
                    fixture_name
                );
            }
            assert!(
                fixture["script_group"]["positive"].as_array().is_some_and(|cases| !cases.is_empty()),
                "fixture {} missing ScriptGroup positive matrix",
                fixture_name
            );
            assert!(
                fixture["script_group"]["negative"].as_array().is_some_and(|cases| !cases.is_empty()),
                "fixture {} missing ScriptGroup negative matrix",
                fixture_name
            );
            let group_inputs = fixture["script_group"]["group_inputs"].as_array().expect("script_group.group_inputs");
            let group_outputs = fixture["script_group"]["group_outputs"].as_array().expect("script_group.group_outputs");
            assert!(!group_inputs.is_empty() || !group_outputs.is_empty(), "fixture {} has empty ScriptGroup", fixture_name);
            assert!(fixture["outputs_data_matrix"].is_object(), "fixture {} missing outputs_data_matrix", fixture_name);
            assert!(
                fixture["outputs_data_matrix"]["positive"].as_array().is_some_and(|cases| !cases.is_empty()),
                "fixture {} missing outputs_data positive matrix",
                fixture_name
            );
            assert!(
                fixture["outputs_data_matrix"]["negative"].as_array().is_some_and(|cases| !cases.is_empty()),
                "fixture {} missing outputs_data negative matrix",
                fixture_name
            );
            assert!(fixture["expected_behavior"].is_object(), "fixture {} missing expected_behavior", fixture_name);
            assert!(fixture["script_args_layout"].is_object(), "fixture {} missing script_args_layout", fixture_name);
            assert!(fixture["witness_layout"].is_object(), "fixture {} missing witness_layout", fixture_name);
            assert!(fixture["molecule_data_layout"].is_object(), "fixture {} missing molecule_data_layout", fixture_name);
            assert!(fixture["metadata_expectation"].is_object(), "fixture {} missing metadata_expectation", fixture_name);
            let reads = fixture["metadata_expectation"]["proof_plan"]["reads"].as_array().expect("proof_plan.reads");
            if reads.iter().any(|read| read.as_str() == Some("group_input")) {
                assert!(!group_inputs.is_empty(), "fixture {} reads group_input without ScriptGroup inputs", fixture_name);
            }
            if reads.iter().any(|read| read.as_str() == Some("group_output")) {
                assert!(!group_outputs.is_empty(), "fixture {} reads group_output without ScriptGroup outputs", fixture_name);
            }
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
    let acp = modules.iter().find(|module| module.name == "std::acp").expect("std::acp module");
    assert_eq!(acp.script_type, "lock");
    assert_eq!(acp.proof_plan_trigger, "lock_group");
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
