use cellscript::{compile, CompileOptions};
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

action issue_badge(badge_id: [u8; 32], owner: Address) -> Badge {
    create_unique<Badge>(identity = field(badge_id)) {
        badge_id,
        owner
    } with_lock(owner)
}
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

action noop() -> u64 {
    0
}
"#;

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

    let evidence = assumptions.iter().map(|assumption| json!({"assumption_id": assumption.assumption_id})).collect::<Vec<_>>();
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
    let evidence = result
        .metadata
        .runtime
        .builder_assumptions
        .iter()
        .map(|assumption| json!({"assumption_id": assumption.assumption_id}))
        .collect::<Vec<_>>();
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
    }
}
