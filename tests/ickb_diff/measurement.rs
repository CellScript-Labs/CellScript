//! Ordinary comparisons are read-only. Only the explicit ignored exporter can
//! acquire a thread-local capture and write a new candidate under target/.
use super::*;
use std::cell::RefCell;
use std::io::Write;

thread_local! {
    static CAPTURE: RefCell<Option<BTreeMap<String, Value>>> = const { RefCell::new(None) };
}

struct Capture;

impl Capture {
    fn begin() -> Self {
        CAPTURE.with(|slot| assert!(slot.borrow_mut().replace(BTreeMap::new()).is_none(), "nested measurement capture"));
        Self
    }

    fn finish(self) -> BTreeMap<String, Value> {
        CAPTURE.with(|slot| slot.borrow_mut().take().expect("active measurement capture"))
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        CAPTURE.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}

pub(super) fn assert_execution(scenario: &str, execution: &Value) {
    if CAPTURE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(measurements) = slot.as_mut() else { return false };
        if let Some(previous) = measurements.insert(scenario.to_owned(), execution.clone()) {
            assert_eq!(
                execution_with_dynamic_context_hashes(&previous),
                execution_with_dynamic_context_hashes(execution),
                "duplicate measurement differs: {scenario}"
            );
        }
        true
    }) {
        return;
    }
    assert_matches(&read_matrix(), scenario, execution);
}

fn assert_matches(matrix: &Value, scenario: &str, execution: &Value) {
    let row =
        matrix["rows"].as_array().expect("rows").iter().find(|row| row["scenario"].as_str() == Some(scenario)).unwrap_or_else(|| {
            panic!("missing matrix row for {scenario}; measured execution:\n{}", serde_json::to_string_pretty(execution).unwrap())
        });
    assert_eq!(row["evidence_level"], "DIFFERENTIAL_CKB_VM_EXECUTED", "{scenario}");
    assert_eq!(row["ckb_vm_execution"], true, "{scenario}");
    assert_eq!(row["original_ickb_executed"], true, "{scenario}");
    assert_eq!(row["full_differential"], true, "{scenario}");
    assert_eq!(
        execution_with_dynamic_context_hashes(&row["execution"]),
        execution_with_dynamic_context_hashes(execution),
        "{scenario} matrix execution object must match measured stable evidence"
    );
}

#[test]
#[ignore = "explicit maintenance: execute every differential case and export a new unapproved matrix candidate"]
fn export_candidate_matrix() {
    let capture = Capture::begin();
    run_all_measured_cases();
    let measurements = capture.finish();
    let mut matrix = read_matrix();
    let expected = matrix["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["ckb_vm_execution"] == true)
        .map(|row| row["scenario"].as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    let measured = measurements.keys().cloned().collect::<BTreeSet<_>>();
    assert!(
        expected.is_subset(&measured),
        "export omitted executed fixture rows: {:?}",
        expected.difference(&measured).collect::<Vec<_>>()
    );
    for (scenario, execution) in &measurements {
        let rows = matrix["rows"].as_array_mut().unwrap();
        if let Some(row) = rows.iter_mut().find(|row| row["scenario"].as_str() == Some(scenario)) {
            row["execution"] = execution.clone();
        } else {
            rows.push(matrix_row_from_execution(scenario, execution));
        }
    }
    refresh_matrix_artifact_evidence(&mut matrix);
    matrix["equivalence_status"] = json!("NOT_PROVEN");
    matrix["production_equivalence_claim"] = json!(false);
    let output_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/ickb-candidates");
    std::fs::create_dir_all(&output_root).unwrap();
    let directory = tempfile::Builder::new().prefix("measured-").tempdir_in(output_root).unwrap().keep();
    let path = directory.join("matrix.json");
    let mut output = std::fs::OpenOptions::new().create_new(true).write(true).open(&path).unwrap();
    serde_json::to_writer_pretty(&mut output, &matrix).unwrap();
    output.write_all(b"\n").unwrap();
    output.flush().unwrap();
    eprintln!("Measured {} differential rows; unapproved candidate: {}", measurements.len(), path.display());
}

#[test]
fn matrix_mismatch_stays_a_failure_and_never_rewrites_the_fixture() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("matrix.json");
    let matrix = json!({"rows": [matrix_row_from_execution("probe", &json!({"value": 1}))]});
    let original = serde_json::to_vec_pretty(&matrix).unwrap();
    std::fs::write(&path, &original).unwrap();
    for (scenario, execution) in [("probe", json!({"value": 2})), ("missing", json!({"value": 3}))] {
        let loaded = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(std::panic::catch_unwind(|| assert_matches(&loaded, scenario, &execution)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}

#[test]
fn capture_does_not_escape_its_owner_or_unwind_into_an_ordinary_comparison() {
    assert!(std::panic::catch_unwind(|| {
        let _capture = Capture::begin();
        assert_execution("probe", &json!({"value": 1}));
        panic!("cancelled export");
    })
    .is_err());
    CAPTURE.with(|slot| assert!(slot.borrow().is_none()));
    let capture = Capture::begin();
    assert_execution("probe", &json!({"value": 2}));
    assert_eq!(capture.finish()["probe"]["value"], 2);
    CAPTURE.with(|slot| assert!(slot.borrow().is_none()));
}

fn execution_with_dynamic_context_hashes(execution: &Value) -> Value {
    let mut normalized = execution.clone();
    if let Some(context_hashes) = normalized.get_mut("transaction_context_sha256").and_then(Value::as_object_mut) {
        context_hashes.insert("original".to_string(), json!("<ckb-testtool-context-hash>"));
        context_hashes.insert("cellscript".to_string(), json!("<ckb-testtool-context-hash>"));
    }
    normalized
}

fn matrix_row_from_execution(scenario: &str, execution: &Value) -> Value {
    let original_status = execution["original_ickb_status"].as_str().unwrap_or("fail");
    let cellscript_status = execution["cellscript_status"].as_str().unwrap_or("fail");
    let result = match (original_status, cellscript_status) {
        ("pass", "pass") => "differential-agree-pass",
        ("fail", "fail") => "differential-agree-fail",
        _ => "differential-mismatch",
    };
    json!({
        "scenario": scenario,
        "evidence_level": "DIFFERENTIAL_CKB_VM_EXECUTED",
        "ckb_vm_execution": true,
        "original_ickb_executed": true,
        "full_differential": true,
        "result": result,
        "original_ickb_expected": original_status,
        "cellscript_expected": cellscript_status,
        "failure_mode": execution.get("failure_mode").cloned().unwrap_or(Value::Null),
        "note": "Both original iCKB and CellScript execute the same normalized fixture; this row was added from measured CKB VM differential evidence.",
        "execution": execution
    })
}

fn refresh_matrix_artifact_evidence(matrix: &mut Value) {
    let artifacts: Vec<Value> = matrix["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["ckb_vm_execution"].as_bool() == Some(true))
        .filter_map(|row| row["execution"]["cellscript_artifact_sha256"].as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|artifact| Value::String(artifact.to_string()))
        .collect();
    matrix["equivalence_evidence"]["generated_cellscript_artifact_sha256"] = Value::Array(artifacts);
}
