use camino::Utf8Path;
use cellscript::{compile, compile_path_with_entry_action, validate_compile_metadata, BuilderAssumptionMetadata, CompileOptions};
use serde_json::json;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

mod common;

use common::cellc_command;

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

const SIMPLE_RESOURCE_CREATE: &str = r#"
module v016::simple_resource_identity

resource Token has store, create, consume, replace {
    amount: u64
}

action mint(amount: u64) -> output: Token
where
    create output = Token { amount }
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
    let evidence_payload = if assumption.kind == "spawn_target_cell_dep_binding" {
        let required = assumption.required_cell_deps.first().expect("spawn target assumption should name a required CellDep");
        let mut parts = required.split(':');
        let dep_source = parts.next().expect("dep source");
        let cell_dep_index = dep_source.strip_prefix("CellDep#").and_then(|value| value.parse::<usize>().ok()).unwrap();
        let mut payload = json!({
            "source": "unit-test-fixture",
            "checked": true,
            "dep_source": dep_source,
            "cell_dep_index": cell_dep_index
        });
        let object = payload.as_object_mut().expect("payload object");
        for part in parts {
            if let Some(value) = part.strip_prefix("name=") {
                object.insert("cell_dep_name".to_string(), json!(value));
            } else if let Some(value) = part.strip_prefix("dep_type=") {
                object.insert("dep_type".to_string(), json!(value));
            } else if let Some(value) = part.strip_prefix("tx_hash=") {
                object.insert("tx_hash".to_string(), json!(value));
            } else if let Some(value) = part.strip_prefix("out_index=") {
                object.insert("out_index".to_string(), json!(value.parse::<u32>().expect("out_index")));
            } else if let Some(value) = part.strip_prefix("hash_type=") {
                object.insert("hash_type".to_string(), json!(value));
            } else if let Some(value) = part.strip_prefix("data_hash=") {
                object.insert("data_hash".to_string(), json!(value));
            } else if let Some(value) = part.strip_prefix("type_id=") {
                object.insert("type_id".to_string(), json!(value));
            }
        }
        payload
    } else {
        json!({
            "source": "unit-test-fixture",
            "checked": true
        })
    };
    json!({
        "assumption_id": assumption.assumption_id,
        "kind": assumption.kind,
        "origin": assumption.origin,
        "feature": assumption.feature,
        "proof_plan_status": assumption.proof_plan_status,
        "evidence": evidence_payload
    })
}

fn write_manifest_bound_spawn_package(root: &Utf8Path) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "spawn_bound"
version = "0.1.0"
entry = "src/main.cell"

[build]
target_profile = "ckb"

[[deploy.ckb.cell_deps]]
name = "secp256k1_verifier"
out_point = "0x3333333333333333333333333333333333333333333333333333333333333333:1"
dep_type = "code"
hash_type = "data1"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/main.cell"),
        r#"
module spawn_bound::main

action delegate() -> u64
where
    return spawn("secp256k1_verifier")
"#,
    )
    .unwrap();
}

fn manifest_bound_spawn_tx(evidence: Vec<serde_json::Value>) -> serde_json::Value {
    json!({
        "inputs": [],
        "outputs": [],
        "cell_deps": [{
            "name": "secp256k1_verifier",
            "dep_type": "code",
            "tx_hash": "0x3333333333333333333333333333333333333333333333333333333333333333",
            "index": 1,
            "hash_type": "data1"
        }],
        "witnesses": [],
        "builder_assumption_evidence": evidence
    })
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

fn run_failure(mut command: Command) -> std::process::Output {
    let output = command.output().unwrap();
    assert!(!output.status.success(), "command must fail");
    output
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
fn cli_v0_16_compile_workflows_reject_metadata_only_proof_plan_gaps() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("metadata_only.cell");
    let bundle_dir = temp.path().join("audit-bundle");
    std::fs::write(&source, METADATA_ONLY_INVARIANT).unwrap();

    for command_name in ["explain-assumptions", "solve-tx", "profile"] {
        let mut command = cellc_command();
        command.arg(command_name).arg(&source).arg("--target-profile").arg("ckb").arg("--json");
        let output = run_failure(command);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("PP0150"), "unexpected {command_name} stderr: {stderr}");
    }

    let mut audit_bundle = cellc_command();
    audit_bundle.arg("audit-bundle").arg(&source).arg("--target-profile").arg("ckb").arg("--output").arg(&bundle_dir).arg("--json");
    let output = run_failure(audit_bundle);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("PP0150"), "unexpected audit-bundle stderr: {stderr}");
}

#[test]
fn cli_validate_and_trace_reject_non_strict_metadata_only_proof_plan_gaps() {
    let temp = tempdir().unwrap();
    let metadata_path = temp.path().join("metadata-only.meta.json");
    let tx_path = temp.path().join("tx.json");
    let result =
        compile(METADATA_ONLY_INVARIANT, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() })
            .expect("non-strict compile keeps metadata-only invariant as audit metadata");
    std::fs::write(&metadata_path, serde_json::to_vec_pretty(&result.metadata).unwrap()).unwrap();
    std::fs::write(&tx_path, serde_json::to_vec_pretty(&json!({"inputs": [], "outputs": []})).unwrap()).unwrap();

    let mut validate = cellc_command();
    validate.arg("validate-tx").arg("--against").arg(&metadata_path).arg(&tx_path).arg("--json");
    let validate_json = run_failure_json(validate);
    assert_eq!(validate_json["status"], "failed");
    assert_eq!(validate_json["proof_plan_soundness"]["status"], "failed");
    assert!(
        validate_json["proof_plan_soundness"]["issues"]
            .as_array()
            .is_some_and(|issues| issues.iter().any(|issue| issue["code"] == "PP0150")),
        "{validate_json:#?}"
    );

    let mut trace = cellc_command();
    trace.arg("trace-tx").arg("--against").arg(&metadata_path).arg(&tx_path).arg("--json");
    let trace_json = run_failure_json(trace);
    assert_eq!(trace_json["status"], "failed");
    assert_eq!(trace_json["schema"], "cellscript-tx-trace-v0.16");
    assert_eq!(trace_json["proof_plan_soundness"]["status"], "failed");
    assert!(
        trace_json["proof_plan_soundness"]["issues"]
            .as_array()
            .is_some_and(|issues| issues.iter().any(|issue| issue["code"] == "PP0150")),
        "{trace_json:#?}"
    );
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

    let scalar_evidence = assumptions
        .iter()
        .map(|assumption| {
            json!({
                "assumption_id": assumption.assumption_id,
                "kind": assumption.kind,
                "origin": assumption.origin,
                "feature": assumption.feature,
                "proof_plan_status": assumption.proof_plan_status,
                "evidence": true
            })
        })
        .collect::<Vec<_>>();
    let with_scalar_evidence = json!({
        "inputs": [{}],
        "outputs": [{}],
        "cell_deps": [],
        "witnesses": [],
        "builder_assumption_evidence": scalar_evidence
    });
    let report = cellscript::assumptions::validate_transaction_against_metadata(&result.metadata, &with_scalar_evidence);
    assert_eq!(report.status, "failed");
    assert!(report.violations.iter().any(|violation| violation.message.contains("structured evidence")), "{report:#?}");

    let implicit_map_evidence = assumptions
        .iter()
        .map(|assumption| {
            let mut evidence = evidence_for(assumption);
            evidence.as_object_mut().expect("evidence object").remove("assumption_id");
            (assumption.assumption_id.clone(), evidence)
        })
        .collect::<serde_json::Map<_, _>>();
    let with_implicit_map_evidence = json!({
        "inputs": [{}],
        "outputs": [{}],
        "cell_deps": [],
        "witnesses": [],
        "builder_assumption_evidence": implicit_map_evidence
    });
    let report = cellscript::assumptions::validate_transaction_against_metadata(&result.metadata, &with_implicit_map_evidence);
    assert_eq!(report.status, "failed");
    assert!(
        report.violations.iter().any(|violation| violation.message.contains("explicit assumption_id matching its map key")),
        "{report:#?}"
    );

    let map_evidence = assumptions
        .iter()
        .map(|assumption| (assumption.assumption_id.clone(), evidence_for(assumption)))
        .collect::<serde_json::Map<_, _>>();
    let with_map_evidence = json!({
        "inputs": [{}],
        "outputs": [{}],
        "cell_deps": [],
        "witnesses": [],
        "builder_assumption_evidence": map_evidence
    });
    let report = cellscript::assumptions::validate_transaction_against_metadata(&result.metadata, &with_map_evidence);
    assert_eq!(report.status, "ok", "{:#?}", report);

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
fn validate_tx_rejects_scoped_action_artifact_as_resource_type_identity() {
    let result =
        compile(IDENTITY_CREATE_UNIQUE, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() })
            .unwrap();
    let artifact_hash = result.metadata.artifact_hash.as_deref().expect("artifact hash");
    let resource_identities = &result.metadata.constraints.ckb.as_ref().expect("ckb constraints").resource_identities;
    assert!(
        resource_identities
            .iter()
            .any(|identity| identity.type_name == "Badge" && identity.status == "compiler-passive-identity-available"),
        "{resource_identities:#?}"
    );

    let evidence = result.metadata.runtime.builder_assumptions.iter().map(evidence_for).collect::<Vec<_>>();
    let tx = json!({
        "inputs": [{}],
        "outputs": [{
            "lock": {},
            "type": {
                "code_hash": format!("0x{artifact_hash}"),
                "hash_type": "data1",
                "args": "0x"
            }
        }],
        "cell_deps": [],
        "witnesses": [],
        "builder_assumption_evidence": evidence
    });
    let report = cellscript::assumptions::validate_transaction_against_metadata(&result.metadata, &tx);
    assert_eq!(report.status, "failed");
    assert!(
        report
            .violations
            .iter()
            .any(|violation| { violation.kind == "resource_identity" && violation.message.contains("active verifiers") }),
        "{report:#?}"
    );
}

#[test]
fn validate_tx_allows_scoped_action_artifact_on_mutated_output_type() {
    let result = compile_path_with_entry_action(
        Utf8Path::new("examples/amm_pool.cell"),
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
        "swap_a_for_b",
    )
    .unwrap();
    let artifact_hash = result.metadata.artifact_hash.as_deref().expect("artifact hash");
    let evidence = result.metadata.runtime.builder_assumptions.iter().map(evidence_for).collect::<Vec<_>>();
    let tx = json!({
        "inputs": [{}, {}],
        "outputs": [
            {
                "lock": {},
                "type": {
                    "code_hash": format!("0x{artifact_hash}"),
                    "hash_type": "data1",
                    "args": "0x"
                }
            },
            {
                "lock": {},
                "type": {
                    "code_hash": "0x1111111111111111111111111111111111111111111111111111111111111111",
                    "hash_type": "data1",
                    "args": "0x"
                }
            }
        ],
        "cell_deps": [],
        "witnesses": ["0x"],
        "builder_assumption_evidence": evidence
    });
    let report = cellscript::assumptions::validate_transaction_against_metadata(&result.metadata, &tx);
    assert!(!report.violations.iter().any(|violation| violation.assumption_id == "resource-identity-active-artifact"), "{report:#?}");
}

#[test]
fn validate_tx_rejects_scoped_action_artifact_on_created_output_type() {
    let result = compile_path_with_entry_action(
        Utf8Path::new("examples/amm_pool.cell"),
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
        "swap_a_for_b",
    )
    .unwrap();
    let artifact_hash = result.metadata.artifact_hash.as_deref().expect("artifact hash");
    let evidence = result.metadata.runtime.builder_assumptions.iter().map(evidence_for).collect::<Vec<_>>();
    let tx = json!({
        "inputs": [{}, {}],
        "outputs": [
            {
                "lock": {},
                "type": {
                    "code_hash": "0x1111111111111111111111111111111111111111111111111111111111111111",
                    "hash_type": "data1",
                    "args": "0x"
                }
            },
            {
                "lock": {},
                "type": {
                    "code_hash": format!("0x{artifact_hash}"),
                    "hash_type": "data1",
                    "args": "0x"
                }
            }
        ],
        "cell_deps": [],
        "witnesses": ["0x"],
        "builder_assumption_evidence": evidence
    });
    let report = cellscript::assumptions::validate_transaction_against_metadata(&result.metadata, &tx);
    assert!(report.violations.iter().any(|violation| violation.assumption_id == "resource-identity-active-artifact"), "{report:#?}");
}

#[test]
fn compile_metadata_rejects_tampered_resource_identity_contract() {
    let result =
        compile(SIMPLE_RESOURCE_CREATE, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() })
            .unwrap();
    let mut metadata = result.metadata.clone();
    metadata.constraints.ckb.as_mut().expect("ckb constraints").resource_identities.clear();
    let err = validate_compile_metadata(&metadata, result.artifact_format).unwrap_err();
    assert!(err.message.contains("resource_identities"), "unexpected error: {}", err.message);
}

#[test]
fn validate_tx_requires_evidence_for_wildcard_structural_reads() {
    let assumption = BuilderAssumptionMetadata {
        assumption_id: "ba-structural-wildcard".to_string(),
        kind: "builder_evidence".to_string(),
        origin: "action:test".to_string(),
        feature: "input:Input#0".to_string(),
        proof_plan_status: "ckb-runtime".to_string(),
        required_inputs: vec!["input:*".to_string()],
        required_outputs: Vec::new(),
        required_cell_deps: Vec::new(),
        required_witness_fields: Vec::new(),
        capacity_policy: "none".to_string(),
        fee_policy: "builder-balances-fee-before-signing".to_string(),
        change_policy: "change-outputs-must-not-violate-proof-plan-shape".to_string(),
        signature_policy: "none".to_string(),
        failure_mode: "reject-before-signing".to_string(),
        detail: "unit-test structural wildcard".to_string(),
    };

    let shape_only = json!({
        "inputs": [{}],
        "outputs": [],
        "cell_deps": [],
        "witnesses": []
    });
    let report = cellscript::assumptions::validate_transaction_against_assumptions(&[assumption.clone()], &shape_only);
    assert_eq!(report.status, "failed");
    assert!(report.violations.iter().any(|violation| violation.message.contains("wildcard structural read bindings")), "{report:#?}");

    let with_evidence = json!({
        "inputs": [{}],
        "outputs": [],
        "cell_deps": [],
        "witnesses": [],
        "builder_assumption_evidence": [evidence_for(&assumption)]
    });
    let report = cellscript::assumptions::validate_transaction_against_assumptions(&[assumption], &with_evidence);
    assert_eq!(report.status, "ok", "{report:#?}");
}

#[test]
fn strict_0_16_rejects_unbound_spawn_target_cell_dep() {
    let err = compile(
        r#"
module v016::spawn_unbound

action delegate() -> u64
where
    return spawn("secp256k1_verifier")
"#,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.16".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap_err();

    assert!(err.message.contains("PP0150"), "unexpected error: {}", err.message);
    assert!(err.message.contains("spawn-target:CellDep#0@0x"), "unexpected error: {}", err.message);
}

#[test]
fn strict_0_16_accepts_manifest_bound_spawn_target_cell_dep() {
    let dir = tempdir().unwrap();
    let root = Utf8Path::from_path(dir.path()).unwrap();
    write_manifest_bound_spawn_package(root);

    let result = cellscript::compile_path(
        root,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.16".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap();
    let spawn_plan =
        result.metadata.runtime.proof_plan.iter().find(|plan| plan.category == "spawn-target").expect("spawn target ProofPlan record");
    assert!(spawn_plan.feature.starts_with("spawn-target:CellDep#0@0x"), "{spawn_plan:#?}");
    assert_eq!(spawn_plan.status, "builder-required");
    assert_eq!(spawn_plan.codegen_coverage_status, "builder-required");
    assert!(spawn_plan.detail.contains("secp256k1_verifier"), "{spawn_plan:#?}");
    assert!(spawn_plan.builder_assumptions.iter().all(|assumption| !assumption.contains("runtime-required")));

    let ckb = result.metadata.constraints.ckb.as_ref().expect("ckb constraints");
    assert!(ckb.script_references.iter().any(|reference| {
        reference.purpose == "spawn-target"
            && reference.name == "secp256k1_verifier"
            && reference.dep_source == "CellDep#0"
            && reference.status == "builder-required-manifest-bound-cell-dep"
    }));

    let spawn_assumption = result
        .metadata
        .runtime
        .builder_assumptions
        .iter()
        .find(|assumption| assumption.kind == "spawn_target_cell_dep_binding")
        .expect("spawn target builder assumption");
    assert_eq!(spawn_assumption.proof_plan_status, "builder-required");
    assert_eq!(
        spawn_assumption.required_cell_deps,
        vec![
            "CellDep#0:name=secp256k1_verifier:dep_type=code:tx_hash=0x3333333333333333333333333333333333333333333333333333333333333333:out_index=1:hash_type=data1"
        ],
        "{spawn_assumption:#?}"
    );

    let evidence = result.metadata.runtime.builder_assumptions.iter().map(evidence_for).collect::<Vec<_>>();
    let no_cell_dep = json!({
        "inputs": [],
        "outputs": [],
        "cell_deps": [],
        "witnesses": [],
        "builder_assumption_evidence": evidence
    });
    let report = cellscript::assumptions::validate_transaction_against_metadata(&result.metadata, &no_cell_dep);
    assert_eq!(report.status, "failed");
    assert!(report.violations.iter().any(|violation| violation.kind == "spawn_target_cell_dep_binding"), "{report:#?}");

    let wrong_spawn_evidence = result
        .metadata
        .runtime
        .builder_assumptions
        .iter()
        .map(|assumption| {
            if assumption.kind == "spawn_target_cell_dep_binding" {
                json!({
                    "assumption_id": assumption.assumption_id,
                    "kind": assumption.kind,
                    "origin": assumption.origin,
                    "feature": assumption.feature,
                    "proof_plan_status": assumption.proof_plan_status,
                    "evidence": {
                        "source": "unit-test-fixture",
                        "checked": true,
                        "dep_source": "CellDep#0",
                        "cell_dep_index": 0,
                        "cell_dep_name": "wrong_verifier",
                        "dep_type": "code"
                    }
                })
            } else {
                evidence_for(assumption)
            }
        })
        .collect::<Vec<_>>();
    let wrong_identity = json!({
        "inputs": [],
        "outputs": [],
        "cell_deps": [{
            "name": "secp256k1_verifier",
            "dep_type": "code",
            "tx_hash": "0x3333333333333333333333333333333333333333333333333333333333333333",
            "index": 1,
            "hash_type": "data1"
        }],
        "witnesses": [],
        "builder_assumption_evidence": wrong_spawn_evidence
    });
    let report = cellscript::assumptions::validate_transaction_against_metadata(&result.metadata, &wrong_identity);
    assert_eq!(report.status, "failed");
    assert!(
        report
            .violations
            .iter()
            .any(|violation| violation.kind == "spawn_target_cell_dep_binding" && violation.message.contains("cell_dep_name")),
        "{report:#?}"
    );

    let wrong_spawn_index_evidence = result
        .metadata
        .runtime
        .builder_assumptions
        .iter()
        .map(|assumption| {
            if assumption.kind == "spawn_target_cell_dep_binding" {
                json!({
                    "assumption_id": assumption.assumption_id,
                    "kind": assumption.kind,
                    "origin": assumption.origin,
                    "feature": assumption.feature,
                    "proof_plan_status": assumption.proof_plan_status,
                    "evidence": {
                        "source": "unit-test-fixture",
                        "checked": true,
                        "dep_source": "CellDep#0",
                        "cell_dep_index": 1,
                        "cell_dep_name": "secp256k1_verifier",
                        "dep_type": "code",
                        "tx_hash": "0x3333333333333333333333333333333333333333333333333333333333333333",
                        "out_index": 1,
                        "hash_type": "data1"
                    }
                })
            } else {
                evidence_for(assumption)
            }
        })
        .collect::<Vec<_>>();
    let wrong_evidence_locator = json!({
        "inputs": [],
        "outputs": [],
        "cell_deps": [{
            "name": "secp256k1_verifier",
            "dep_type": "code",
            "tx_hash": "0x3333333333333333333333333333333333333333333333333333333333333333",
            "index": 1,
            "hash_type": "data1"
        }],
        "witnesses": [],
        "builder_assumption_evidence": wrong_spawn_index_evidence
    });
    let report = cellscript::assumptions::validate_transaction_against_metadata(&result.metadata, &wrong_evidence_locator);
    assert_eq!(report.status, "failed");
    assert!(
        report
            .violations
            .iter()
            .any(|violation| violation.kind == "spawn_target_cell_dep_binding" && violation.message.contains("cell_dep_index 0")),
        "{report:#?}"
    );

    let evidence = result.metadata.runtime.builder_assumptions.iter().map(evidence_for).collect::<Vec<_>>();
    let wrong_tx_cell_dep = json!({
        "inputs": [],
        "outputs": [],
        "cell_deps": [{
            "name": "secp256k1_verifier",
            "dep_type": "code",
            "tx_hash": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "index": 1,
            "hash_type": "data1"
        }],
        "witnesses": [],
        "builder_assumption_evidence": evidence
    });
    let report = cellscript::assumptions::validate_transaction_against_metadata(&result.metadata, &wrong_tx_cell_dep);
    assert_eq!(report.status, "failed");
    assert!(
        report
            .violations
            .iter()
            .any(|violation| violation.kind == "spawn_target_cell_dep_binding" && violation.message.contains("tx_hash")),
        "{report:#?}"
    );

    let evidence = result.metadata.runtime.builder_assumptions.iter().map(evidence_for).collect::<Vec<_>>();
    let with_cell_dep_and_evidence = json!({
        "inputs": [],
        "outputs": [],
        "cell_deps": [{
            "name": "secp256k1_verifier",
            "dep_type": "code",
            "tx_hash": "0x3333333333333333333333333333333333333333333333333333333333333333",
            "index": 1,
            "hash_type": "data1"
        }],
        "witnesses": [],
        "builder_assumption_evidence": evidence
    });
    let report = cellscript::assumptions::validate_transaction_against_metadata(&result.metadata, &with_cell_dep_and_evidence);
    assert_eq!(report.status, "ok", "{report:#?}");

    let mut solve = cellc_command();
    solve.arg("solve-tx").arg(root.as_std_path()).arg("--target-profile").arg("ckb").arg("--json");
    let solve_json = run_success_json(solve);
    let requirements = solve_json["transaction_plan"]["builder_assumption_evidence_requirements"]
        .as_array()
        .expect("builder assumption evidence requirements");
    assert!(
        requirements.iter().any(|requirement| {
            requirement["kind"] == "spawn_target_cell_dep_binding"
                && requirement["evidence_schema"]["required_cell_deps"]
                    .as_array()
                    .is_some_and(|deps| deps.iter().any(|dep| dep.as_str().is_some_and(|dep| dep.contains("tx_hash=0x333333"))))
        }),
        "{solve_json:#?}"
    );
    let cell_deps = solve_json["transaction_plan"]["cell_deps"].as_array().expect("solver cell_deps");
    assert!(
        cell_deps.iter().any(|dep| {
            dep["name"] == "secp256k1_verifier"
                && dep["dep_type"] == "code"
                && dep["tx_hash"] == "0x3333333333333333333333333333333333333333333333333333333333333333"
                && dep["index"] == 1
                && dep["hash_type"] == "data1"
        }),
        "{solve_json:#?}"
    );
}

#[test]
fn strict_0_16_rejects_spawn_target_manifest_binding_outside_cell_dep_zero() {
    let dir = tempdir().unwrap();
    let root = Utf8Path::from_path(dir.path()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "spawn_bound_second"
version = "0.1.0"
entry = "src/main.cell"

[build]
target_profile = "ckb"

[[deploy.ckb.cell_deps]]
name = "other_verifier"
out_point = "0x1111111111111111111111111111111111111111111111111111111111111111:0"
dep_type = "code"
hash_type = "data1"

[[deploy.ckb.cell_deps]]
name = "secp256k1_verifier"
out_point = "0x2222222222222222222222222222222222222222222222222222222222222222:0"
dep_type = "code"
hash_type = "data1"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/main.cell"),
        r#"
module spawn_bound_second::main

action delegate() -> u64
where
    return spawn("secp256k1_verifier")
"#,
    )
    .unwrap();

    let err = cellscript::compile_path(
        root,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.16".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap_err();
    assert!(err.message.contains("PP0150"), "unexpected error: {}", err.message);
    assert!(err.message.contains("spawn-target:CellDep#0@0x"), "unexpected error: {}", err.message);
}

#[test]
fn strict_0_16_rejects_spawn_target_manifest_dep_group_binding() {
    let dir = tempdir().unwrap();
    let root = Utf8Path::from_path(dir.path()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "spawn_bound_dep_group"
version = "0.1.0"
entry = "src/main.cell"

[build]
target_profile = "ckb"

[[deploy.ckb.cell_deps]]
name = "secp256k1_verifier"
out_point = "0x3333333333333333333333333333333333333333333333333333333333333333:0"
dep_type = "dep_group"
hash_type = "data1"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/main.cell"),
        r#"
module spawn_bound_dep_group::main

action delegate() -> u64
where
    return spawn("secp256k1_verifier")
"#,
    )
    .unwrap();

    let err = cellscript::compile_path(
        root,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.16".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap_err();
    assert!(err.message.contains("PP0150"), "unexpected error: {}", err.message);
    assert!(err.message.contains("spawn-target:CellDep#0@0x"), "unexpected error: {}", err.message);
}

#[test]
fn strict_0_16_rejects_checked_partial_proof_plan_gaps() {
    let err = compile(
        r#"
module v016::partial_state_gap

resource Ticket has store, create, consume, replace, burn, relock, read_ref {
    state: u8
    note: String
}

flow Ticket.state {
    Created -> Active;
}

action activate(ticket: Ticket, note: String) -> output: Ticket
    transition ticket.state: Created -> output.state: Active
where
    consume ticket
    create output = Ticket {
        state: Ticket::Active,
        note: note
    }
"#,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.16".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect_err("strict v0.16 must reject checked-partial ProofPlan gaps");

    assert!(err.message.contains("PP0151"), "unexpected error: {}", err.message);
    assert!(err.message.contains("partial verifier coverage gaps"), "unexpected error: {}", err.message);
}

#[test]
fn cli_explain_assumptions_and_validate_tx_are_machine_readable() {
    let temp = tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    write_manifest_bound_spawn_package(root);

    let mut explain = cellc_command();
    explain.arg("explain-assumptions").arg(root.as_std_path()).arg("--json");
    let explain_json = run_success_json(explain);
    assert_eq!(explain_json["status"], "ok");
    assert!(explain_json["assumption_count"].as_u64().unwrap() > 0);

    let result = cellscript::compile_path(
        root,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.16".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap();
    let evidence = result.metadata.runtime.builder_assumptions.iter().map(evidence_for).collect::<Vec<_>>();
    let metadata = temp.path().join("spawn.meta.json");
    let tx = temp.path().join("tx.json");
    std::fs::write(&metadata, serde_json::to_vec_pretty(&result.metadata).unwrap()).unwrap();
    std::fs::write(&tx, serde_json::to_vec_pretty(&manifest_bound_spawn_tx(evidence)).unwrap()).unwrap();

    let mut validate = cellc_command();
    validate.arg("validate-tx").arg("--against").arg(&metadata).arg(&tx).arg("--json");
    let validate_json = run_success_json(validate);
    assert_eq!(validate_json["status"], "ok");
}

#[test]
fn cli_solve_tx_exposes_resource_identity_contracts() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("identity.cell");
    std::fs::write(&source, SIMPLE_RESOURCE_CREATE).unwrap();

    let mut solve = cellc_command();
    solve.arg("solve-tx").arg(&source).arg("--target-profile").arg("ckb").arg("--json");
    let solve_json = run_success_json(solve);
    let resource_identities = solve_json["transaction_plan"]["resource_identities"].as_array().expect("resource identity contracts");
    assert_eq!(
        solve_json["transaction_plan"]["fixture_identity_policy"]["always_success"],
        "fixture-only; may be used in harness and negative tests, never as a production resource identity",
        "{solve_json:#?}"
    );
    assert!(
        resource_identities.iter().any(|identity| {
            identity["type_name"] == "Token"
                && identity["status"] == "compiler-passive-identity-available"
                && identity["action_artifact_policy"]
                    .as_str()
                    .is_some_and(|policy| policy.contains("forbidden-as-passive-type-identity"))
        }),
        "{solve_json:#?}"
    );
}

#[test]
fn cli_explain_assumptions_and_solve_tx_can_scope_entry_action() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/amm_pool.cell");

    let mut unscoped = cellc_command();
    unscoped.arg("explain-assumptions").arg(&source).arg("--target-profile").arg("ckb").arg("--json");
    let unscoped_json = run_success_json(unscoped);
    let unscoped_count = unscoped_json["assumption_count"].as_u64().expect("unscoped assumption count");

    let mut explain = cellc_command();
    explain
        .arg("explain-assumptions")
        .arg(&source)
        .arg("--target-profile")
        .arg("ckb")
        .arg("--entry-action")
        .arg("swap_a_for_b")
        .arg("--json");
    let explain_json = run_success_json(explain);
    assert_eq!(explain_json["status"], "ok", "{explain_json:#?}");
    assert_eq!(explain_json["selected_entrypoint"]["kind"], "action", "{explain_json:#?}");
    assert_eq!(explain_json["selected_entrypoint"]["name"], "swap_a_for_b", "{explain_json:#?}");
    let scoped_count = explain_json["assumption_count"].as_u64().expect("scoped assumption count");
    assert!(scoped_count > 0 && scoped_count < unscoped_count, "{explain_json:#?}");

    let mut solve = cellc_command();
    solve.arg("solve-tx").arg(&source).arg("--target-profile").arg("ckb").arg("--entry-action").arg("swap_a_for_b").arg("--json");
    let solve_json = run_success_json(solve);
    assert_eq!(solve_json["status"], "template", "{solve_json:#?}");
    assert_eq!(solve_json["submit_ready"], false, "{solve_json:#?}");
    assert!(solve_json["missing_builder_steps"]
        .as_array()
        .is_some_and(|steps| steps.iter().any(|step| step.as_str() == Some("ckb_dry_run"))));
    let requirements =
        solve_json["transaction_plan"]["builder_assumption_evidence_requirements"].as_array().expect("builder evidence requirements");
    assert!(
        requirements.iter().any(|requirement| {
            requirement["kind"] == "builder_evidence"
                && requirement["feature"] == "input:Input#0"
                && requirement["evidence_schema"]["required_inputs"]
                    .as_array()
                    .is_some_and(|inputs| inputs.iter().any(|input| input.as_str() == Some("input:*")))
        }),
        "{solve_json:#?}"
    );
}

#[test]
fn cli_entry_witness_json_exposes_script_group_placement() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/amm_pool.cell");
    let mut entry_witness = cellc_command();
    entry_witness
        .arg("entry-witness")
        .arg(&source)
        .arg("--target-profile")
        .arg("ckb")
        .arg("--action")
        .arg("swap_a_for_b")
        .arg("--arg")
        .arg("49000")
        .arg("--arg")
        .arg("0x0101010101010101010101010101010101010101010101010101010101010101")
        .arg("--json");
    let witness_json = run_success_json(entry_witness);
    assert_eq!(witness_json["status"], "ok", "{witness_json:#?}");
    assert_eq!(witness_json["placement"]["kind"], "raw_script_group_witness", "{witness_json:#?}");
    assert_eq!(witness_json["placement"]["source"], "GroupInput", "{witness_json:#?}");
    assert_eq!(witness_json["placement"]["group_index"], 0, "{witness_json:#?}");
    assert_eq!(witness_json["placement"]["fallback_source"], "GroupOutput", "{witness_json:#?}");
}

#[test]
fn cli_resource_identity_generates_plan_and_validate_tx_checks_it() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("identity.cell");
    let artifact = temp.path().join("resource-identity.elf");
    let plan_path = temp.path().join("resource-identities.json");
    let metadata_path = temp.path().join("mint.meta.json");
    let tx_path = temp.path().join("mint.tx.json");
    let bad_tx_path = temp.path().join("bad-mint.tx.json");
    std::fs::write(&source, SIMPLE_RESOURCE_CREATE).unwrap();

    let mut resource_identity = cellc_command();
    resource_identity
        .arg("resource-identity")
        .arg(&source)
        .arg("--target-profile")
        .arg("ckb")
        .arg("--output")
        .arg(&artifact)
        .arg("--plan-output")
        .arg(&plan_path)
        .arg("--identity")
        .arg("Token:output=test-token");
    let plan_json = run_success_json(resource_identity);
    assert_eq!(plan_json["status"], "ok", "{plan_json:#?}");
    assert!(artifact.exists());
    assert!(plan_path.exists());
    let token = plan_json["resource_identities"]
        .as_array()
        .expect("resource identities")
        .iter()
        .find(|entry| entry["type_name"] == "Token")
        .expect("Token identity");
    assert_eq!(token["script"]["hash_type"], "data1");
    assert!(token["script"]["code_hash"].as_str().is_some_and(|hash| hash.starts_with("0x") && hash.len() == 66));
    assert!(token["script"]["args"].as_str().is_some_and(|args| args.starts_with("0x") && args.len() == 66));
    assert!(token["script_hash"].as_str().is_some_and(|hash| hash.starts_with("0x") && hash.len() == 66));
    let output_script = token["create_scripts"]
        .as_array()
        .expect("create scripts")
        .iter()
        .find(|entry| entry["binding"] == "output")
        .expect("output binding script")["script"]
        .clone();

    let result =
        compile(SIMPLE_RESOURCE_CREATE, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() })
            .unwrap();
    let evidence = result.metadata.runtime.builder_assumptions.iter().map(evidence_for).collect::<Vec<_>>();
    let tx = json!({
        "inputs": [],
        "outputs": [{
            "lock": {},
            "type": output_script
        }],
        "cell_deps": [],
        "witnesses": [],
        "builder_assumption_evidence": evidence
    });
    std::fs::write(&metadata_path, serde_json::to_vec_pretty(&result.metadata).unwrap()).unwrap();
    std::fs::write(&tx_path, serde_json::to_vec_pretty(&tx).unwrap()).unwrap();

    let mut validate = cellc_command();
    validate
        .arg("validate-tx")
        .arg("--against")
        .arg(&metadata_path)
        .arg("--resource-identities")
        .arg(&plan_path)
        .arg(&tx_path)
        .arg("--json");
    let validate_json = run_success_json(validate);
    assert_eq!(validate_json["status"], "ok", "{validate_json:#?}");

    let mut bad_tx = tx;
    bad_tx["outputs"][0]["type"]["args"] = json!("0x0000000000000000000000000000000000000000000000000000000000000000");
    std::fs::write(&bad_tx_path, serde_json::to_vec_pretty(&bad_tx).unwrap()).unwrap();
    let mut validate_bad = cellc_command();
    validate_bad
        .arg("validate-tx")
        .arg("--against")
        .arg(&metadata_path)
        .arg("--resource-identities")
        .arg(&plan_path)
        .arg(&bad_tx_path)
        .arg("--json");
    let validate_bad_json = run_failure_json(validate_bad);
    assert_eq!(validate_bad_json["status"], "failed", "{validate_bad_json:#?}");
    assert!(validate_bad_json["validation"]["violations"].as_array().is_some_and(|violations| {
        violations.iter().any(|violation| {
            violation["kind"] == "resource_identity" && violation["message"].as_str().is_some_and(|m| m.contains("args"))
        })
    }));
}

#[test]
fn cli_builder_manifest_and_builder_check_validate_resource_identity_flow() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("identity.cell");
    let artifact = temp.path().join("resource-identity.elf");
    let plan_path = temp.path().join("resource-identities.json");
    let manifest_path = temp.path().join("mint.builder.json");
    let tx_path = temp.path().join("mint.tx.json");
    std::fs::write(&source, SIMPLE_RESOURCE_CREATE).unwrap();

    let mut resource_identity = cellc_command();
    resource_identity
        .arg("resource-identity")
        .arg(&source)
        .arg("--target-profile")
        .arg("ckb")
        .arg("--output")
        .arg(&artifact)
        .arg("--plan-output")
        .arg(&plan_path)
        .arg("--identity")
        .arg("Token:output=test-token")
        .arg("--json");
    let plan_json = run_success_json(resource_identity);
    let output_script = plan_json["resource_identities"]
        .as_array()
        .expect("resource identities")
        .iter()
        .find(|entry| entry["type_name"] == "Token")
        .and_then(|entry| entry["create_scripts"].as_array())
        .and_then(|scripts| scripts.iter().find(|script| script["binding"] == "output"))
        .and_then(|script| script.get("script"))
        .cloned()
        .expect("output script");

    let mut manifest = cellc_command();
    manifest
        .arg("builder")
        .arg("manifest")
        .arg(&source)
        .arg("--target-profile")
        .arg("ckb")
        .arg("--entry-action")
        .arg("mint")
        .arg("--resource-identities")
        .arg(&plan_path)
        .arg("--output")
        .arg(&manifest_path);
    let manifest_json = run_success_json(manifest);
    assert_eq!(manifest_json["status"], "ok", "{manifest_json:#?}");
    let manifest_value: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    assert_eq!(manifest_value["schema"], "cellscript-builder-manifest-v0.16.2", "{manifest_value:#?}");
    assert_eq!(manifest_value["submit_ready"], false, "{manifest_value:#?}");
    assert_eq!(manifest_value["entry_witness"]["placement"]["source"], "GroupInput", "{manifest_value:#?}");
    assert_eq!(manifest_value["transaction_template"]["submit_ready"], false, "{manifest_value:#?}");
    assert!(
        !manifest_value["transaction_template"]["transaction_plan"]["builder_assumption_evidence_template"]
            .as_object()
            .unwrap()
            .is_empty(),
        "{manifest_value:#?}"
    );

    let metadata: cellscript::CompileMetadata = serde_json::from_value(manifest_value["metadata"].clone()).unwrap();
    let evidence = metadata.runtime.builder_assumptions.iter().map(evidence_for).collect::<Vec<_>>();
    let tx = json!({
        "inputs": [],
        "outputs": [{
            "lock": {},
            "type": output_script
        }],
        "cell_deps": [],
        "witnesses": [],
        "builder_assumption_evidence": evidence
    });
    std::fs::write(&tx_path, serde_json::to_vec_pretty(&tx).unwrap()).unwrap();

    let mut builder_check = cellc_command();
    builder_check.arg("builder").arg("check").arg("--manifest").arg(&manifest_path).arg("--tx").arg(&tx_path).arg("--production");
    let check_json = run_success_json(builder_check);
    assert_eq!(check_json["schema"], "cellscript-builder-check-v0.16.2", "{check_json:#?}");
    assert_eq!(check_json["status"], "ok", "{check_json:#?}");
    assert_eq!(check_json["pre_sign_ready"], true, "{check_json:#?}");
    assert_eq!(check_json["submit_ready"], false, "{check_json:#?}");
    assert!(check_json["builder_assumption_evidence_template"].is_null(), "{check_json:#?}");
    assert!(
        !check_json["missing_submit_steps"].as_array().unwrap().iter().any(|step| step == "builder_assumption_evidence"),
        "{check_json:#?}"
    );

    let mut builder_check_human = cellc_command();
    let human_output = builder_check_human
        .arg("builder")
        .arg("check")
        .arg("--manifest")
        .arg(&manifest_path)
        .arg("--tx")
        .arg(&tx_path)
        .arg("--production")
        .arg("--human")
        .output()
        .unwrap();
    assert!(human_output.status.success(), "stderr: {}", String::from_utf8_lossy(&human_output.stderr));
    let human_stdout = String::from_utf8_lossy(&human_output.stdout);
    assert!(human_stdout.contains("Builder check: ok"), "{human_stdout}");
    assert!(serde_json::from_slice::<serde_json::Value>(&human_output.stdout).is_err(), "{human_stdout}");
}

#[test]
fn cli_validate_tx_production_rejects_fixture_resource_identity() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("identity.cell");
    let metadata_path = temp.path().join("mint.meta.json");
    let tx_path = temp.path().join("mint.fixture.tx.json");
    std::fs::write(&source, SIMPLE_RESOURCE_CREATE).unwrap();

    let result =
        compile(SIMPLE_RESOURCE_CREATE, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() })
            .unwrap();
    let evidence = result.metadata.runtime.builder_assumptions.iter().map(evidence_for).collect::<Vec<_>>();
    let tx = json!({
        "inputs": [],
        "outputs": [{
            "lock": {},
            "type": {
                "code_hash": "0x28e83a1277d48add8e72fadaa9248559e1b632bab2bd60b27955ebc4c03800a5",
                "hash_type": "data",
                "args": "0x"
            }
        }],
        "cell_deps": [],
        "witnesses": [],
        "builder_assumption_evidence": evidence
    });
    std::fs::write(&metadata_path, serde_json::to_vec_pretty(&result.metadata).unwrap()).unwrap();
    std::fs::write(&tx_path, serde_json::to_vec_pretty(&tx).unwrap()).unwrap();

    let mut validate_fixture = cellc_command();
    validate_fixture.arg("validate-tx").arg("--against").arg(&metadata_path).arg(&tx_path).arg("--json");
    let validate_fixture_json = run_success_json(validate_fixture);
    assert_eq!(validate_fixture_json["status"], "ok", "{validate_fixture_json:#?}");

    let mut validate_production = cellc_command();
    validate_production.arg("validate-tx").arg("--against").arg(&metadata_path).arg(&tx_path).arg("--production").arg("--json");
    let validate_production_json = run_failure_json(validate_production);
    assert_eq!(validate_production_json["status"], "failed", "{validate_production_json:#?}");
    assert_eq!(validate_production_json["production"], true, "{validate_production_json:#?}");
    assert!(validate_production_json["validation"]["violations"].as_array().is_some_and(|violations| {
        violations.iter().any(|violation| {
            violation["kind"] == "resource_identity"
                && violation["message"].as_str().is_some_and(|message| message.contains("always_success_fixture_only"))
        })
    }));
}

#[test]
fn cli_validate_tx_resource_identity_plan_allows_mutated_scoped_action_output() {
    let temp = tempdir().unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/amm_pool.cell");
    let artifact = temp.path().join("resource-identity.elf");
    let plan_path = temp.path().join("resource-identities.json");
    let metadata_path = temp.path().join("swap.meta.json");
    let tx_path = temp.path().join("swap.tx.json");

    let mut resource_identity = cellc_command();
    resource_identity
        .arg("resource-identity")
        .arg(&source)
        .arg("--target-profile")
        .arg("ckb")
        .arg("--type")
        .arg("Token")
        .arg("--output")
        .arg(&artifact)
        .arg("--plan-output")
        .arg(&plan_path)
        .arg("--identity")
        .arg("Token:token_out=test-token-out")
        .arg("--json");
    let plan_json = run_success_json(resource_identity);
    let token_out_script = plan_json["resource_identities"]
        .as_array()
        .expect("resource identities")
        .iter()
        .find(|entry| entry["type_name"] == "Token")
        .and_then(|entry| entry["create_scripts"].as_array())
        .and_then(|scripts| scripts.iter().find(|script| script["binding"] == "token_out"))
        .and_then(|script| script.get("script"))
        .cloned()
        .expect("Token token_out script");

    let result = compile_path_with_entry_action(
        Utf8Path::from_path(&source).expect("utf8 source path"),
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
        "swap_a_for_b",
    )
    .unwrap();
    let artifact_hash = result.metadata.artifact_hash.as_deref().expect("artifact hash");
    let evidence = result.metadata.runtime.builder_assumptions.iter().map(evidence_for).collect::<Vec<_>>();
    let tx = json!({
        "inputs": [{}, {}],
        "outputs": [
            {
                "lock": {},
                "type": {
                    "code_hash": format!("0x{artifact_hash}"),
                    "hash_type": "data1",
                    "args": "0x"
                }
            },
            {
                "lock": {},
                "type": token_out_script
            }
        ],
        "cell_deps": [],
        "witnesses": ["0x"],
        "builder_assumption_evidence": evidence
    });
    std::fs::write(&metadata_path, serde_json::to_vec_pretty(&result.metadata).unwrap()).unwrap();
    std::fs::write(&tx_path, serde_json::to_vec_pretty(&tx).unwrap()).unwrap();

    let mut validate = cellc_command();
    validate
        .arg("validate-tx")
        .arg("--against")
        .arg(&metadata_path)
        .arg("--resource-identities")
        .arg(&plan_path)
        .arg(&tx_path)
        .arg("--json");
    let validate_json = run_success_json(validate);
    assert_eq!(validate_json["status"], "ok", "{validate_json:#?}");
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
    plan["target_profile"] = json!(null);
    plan.as_object_mut().unwrap().remove("code_cell_manifest");
    plan["dep_group_manifest"] = json!("not a CKB deployment manifest");
    plan["script_references"] = json!("not script references");
    plan["builder_assumptions"] = json!("not an assumption array");
    std::fs::write(&bad_plan_path, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();

    let mut verify_bad = cellc_command();
    verify_bad.arg("verify-deploy").arg(&bad_plan_path).arg("--json");
    let verify_bad_json = run_failure_json(verify_bad);
    assert_eq!(verify_bad_json["status"], "failed");
    let violations = verify_bad_json["violations"].as_array().expect("violations");
    assert!(violations.iter().any(|violation| violation.as_str().is_some_and(|text| text.contains("artifact.hash"))));
    assert!(violations.iter().any(|violation| violation.as_str().is_some_and(|text| text.contains("metadata_schema_version"))));
    assert!(violations.iter().any(|violation| violation.as_str().is_some_and(|text| text.contains("target_profile.name"))));
    assert!(violations.iter().any(|violation| violation.as_str().is_some_and(|text| text.contains("code_cell_manifest"))));
    assert!(violations.iter().any(|violation| violation.as_str().is_some_and(|text| text.contains("dep_group_manifest"))));
    assert!(violations.iter().any(|violation| violation.as_str().is_some_and(|text| text.contains("script_references"))));
    assert!(violations.iter().any(|violation| violation.as_str().is_some_and(|text| text.contains("builder_assumptions"))));
}

#[test]
fn cli_v0_16_tooling_outputs_are_machine_readable_and_schema_bound() {
    let temp = tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    write_manifest_bound_spawn_package(root);
    let metadata_path = temp.path().join("spawn.meta.json");
    let old_metadata_path = temp.path().join("old.meta.json");
    let new_metadata_path = temp.path().join("new.meta.json");
    let tx_path = temp.path().join("tx.json");
    let old_deploy_path = temp.path().join("old.deploy.json");
    let new_deploy_path = temp.path().join("new.deploy.json");
    let bundle_dir = temp.path().join("audit-bundle");

    let result = cellscript::compile_path(
        root,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.16".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap();
    let evidence = result.metadata.runtime.builder_assumptions.iter().map(evidence_for).collect::<Vec<_>>();
    std::fs::write(&metadata_path, serde_json::to_vec_pretty(&result.metadata).unwrap()).unwrap();
    std::fs::write(&old_metadata_path, serde_json::to_vec_pretty(&result.metadata).unwrap()).unwrap();
    let mut changed_metadata = result.metadata.clone();
    changed_metadata.runtime.proof_plan[0].coverage.push("unit-test-extra-coverage".to_string());
    std::fs::write(&new_metadata_path, serde_json::to_vec_pretty(&changed_metadata).unwrap()).unwrap();
    std::fs::write(&tx_path, serde_json::to_vec_pretty(&manifest_bound_spawn_tx(evidence)).unwrap()).unwrap();

    let mut solve = cellc_command();
    solve.arg("solve-tx").arg(root.as_std_path()).arg("--target-profile").arg("ckb").arg("--json");
    let solve_json = run_success_json(solve);
    assert_eq!(solve_json["status"], "template");
    assert!(solve_json["transaction_plan"]["builder_assumption_evidence_requirements"]
        .as_array()
        .is_some_and(|requirements| !requirements.is_empty()));
    assert!(solve_json["limitations"].as_array().is_some_and(|limitations| !limitations.is_empty()));

    let mut profile = cellc_command();
    profile.arg("profile").arg(root.as_std_path()).arg("--target-profile").arg("ckb").arg("--json");
    let profile_json = run_success_json(profile);
    assert_eq!(profile_json["schema"], "cellscript-profile-v0.16");
    let proof_records = profile_json["proof_plan_records"].as_array().expect("profile proof_plan_records");
    assert!(!proof_records.is_empty(), "{profile_json:#?}");
    assert!(proof_records.iter().all(|record| record["feature"].as_str().is_some()), "{profile_json:#?}");

    let mut lock_deps = cellc_command();
    lock_deps.arg("lock-deps").arg(root.as_std_path()).arg("--target-profile").arg("ckb").arg("--json");
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
    audit_bundle
        .arg("audit-bundle")
        .arg(root.as_std_path())
        .arg("--target-profile")
        .arg("ckb")
        .arg("--output")
        .arg(&bundle_dir)
        .arg("--json");
    let audit_bundle_json = run_success_json(audit_bundle);
    assert_eq!(audit_bundle_json["status"], "ok");
    assert!(bundle_dir.join("audit-bundle.json").exists());
    assert!(bundle_dir.join("index.html").exists());

    let mut deploy_old = cellc_command();
    let deploy_old = deploy_old
        .arg("deploy-plan")
        .arg(root.as_std_path())
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
    deploy_plan["builder_assumptions"][0]["detail"] = json!("unit-test-tampered-builder-assumption-detail");
    std::fs::write(&new_deploy_path, serde_json::to_vec_pretty(&deploy_plan).unwrap()).unwrap();

    let mut diff_deploy = cellc_command();
    diff_deploy.arg("diff-deploy").arg(&old_deploy_path).arg(&new_deploy_path).arg("--json");
    let diff_deploy_json = run_success_json(diff_deploy);
    assert_eq!(diff_deploy_json["schema"], "cellscript-deploy-diff-v0.16");
    let changed = diff_deploy_json["changed"].as_array().expect("changed");
    assert!(changed.iter().any(|entry| entry["path"] == "/metadata_schema_version"), "{diff_deploy_json:#?}");
    assert!(changed.iter().any(|entry| entry["path"] == "/builder_assumptions/0/detail"), "{diff_deploy_json:#?}");
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
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/compat/ckb_standard").join(file_name_str);
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("fixture file {} for '{}' not found: {}", path.display(), fixture_name, e));
            let fixture: serde_json::Value = serde_json::from_str(&content)
                .unwrap_or_else(|e| panic!("fixture file {} for '{}' does not parse as JSON: {}", path.display(), fixture_name, e));
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
