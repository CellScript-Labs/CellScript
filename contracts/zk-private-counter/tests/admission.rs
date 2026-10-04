use ark_std::rand::rngs::OsRng;
use cellscript_zk_private_counter::{self as counter, package};
use serde_json::json;
#[test]
fn admission_requires_exact_candidate_completed_cases_and_explicit_setup_trust() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("candidate");
    let (pk, _) = counter::setup(&mut OsRng).unwrap();
    let manifest = package::write_package(&dir, &pk, package::SetupKind::LocalSingleParty).unwrap();
    // Minimal synthetic report for parser mutations, never exported as execution evidence.
    let mut rows = vec![];
    for name in ["create", "update-0-1", "update-1-2"] {
        rows.push(json!({"case":name,"cycles":110_000_000,"commit":{"status":"committed"}}));
    }
    rows.push(json!({"case":"successor-replay","error_code":79}));
    let section = json!({"status":"passed","verification_key_data_hash":manifest.verification_key_data_hash,"circuit_sha256":manifest.circuit.r1cs_sha256,"rows":rows});
    let report = json!({"schema":"cellscript-counter-application-evidence-v1","status":"passed","setup_kind":"local-single-party",
        "verification_key_data_hash":manifest.verification_key_data_hash,"circuit_sha256":manifest.circuit.r1cs_sha256,
        "scheduler":section,"node_acceptance":section,"reproducibility":{"status":"passed"}});
    let path = temp.path().join("synthetic-parser-fixture.json");
    let write = |report: &serde_json::Value| std::fs::write(&path, serde_json::to_vec(report).unwrap()).unwrap();
    write(&report);
    assert!(package::verify_admission(&dir, &path, false).is_err());
    let admitted = package::verify_admission(&dir, &path, true).unwrap();
    assert_eq!(admitted["production_admitted"], true);
    assert_eq!(admitted["public_network_deployment"], false);
    assert_eq!(admitted["mpc_ceremony"], false);
    for (pointer, value) in [
        ("/verification_key_data_hash", json!("other")),
        ("/node_acceptance/rows/1/cycles", json!(250_000_000)),
        ("/node_acceptance/rows/1/commit/status", json!("pending")),
        ("/scheduler/rows/3/error_code", json!(0)),
        ("/reproducibility/status", json!("missing")),
        ("/setup_kind", json!("public-test-seed")),
    ] {
        let mut changed = report.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        write(&changed);
        assert!(package::verify_admission(&dir, &path, true).is_err(), "{pointer}");
    }
    write(&report);
    let mut manifest = manifest;
    manifest.setup_kind = package::SetupKind::PublicTestSeed;
    std::fs::write(dir.join("setup.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(package::verify_admission(&dir, &path, true).is_err());
}
