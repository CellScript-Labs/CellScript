use serde_json::{json, Value};
use std::process::Command;
use tempfile::tempdir;

fn run_wrapper(report: Value) -> std::process::Output {
    let temp = tempdir().expect("tempdir should be available");
    let target = temp.path().join("target");
    std::fs::create_dir(&target).expect("target directory should be creatable");
    std::fs::write(
        target.join("novaseal-devnet-stateful-acceptance.json"),
        serde_json::to_vec_pretty(&report).expect("report should serialize"),
    )
    .expect("report should be writable");

    Command::new("bash")
        .arg("scripts/novaseal_devnet_stateful_acceptance.sh")
        .arg("--report-only")
        .arg("--repo-root")
        .arg(temp.path())
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("acceptance wrapper should execute")
}

fn base_report() -> Value {
    json!({
        "status": "local_devnet_passed_external_endpoint_required",
        "live_devnet_rpc_executed": true,
        "local_blocker_count": 0,
        "acceptance_blocker_count": 1,
        "blocker_count": 1,
        "external_endpoint_coverage": {
            "status": "external_required"
        }
    })
}

#[test]
fn wrapper_allows_external_only_blocker_after_local_devnet_passes() {
    let output = run_wrapper(base_report());

    assert!(
        output.status.success(),
        "wrapper should accept local devnet readiness with external-only blocker\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("external_endpoint_status=external_required"),
        "wrapper should print the external endpoint status"
    );
}

#[test]
fn wrapper_rejects_external_required_status_with_local_blockers() {
    let mut report = base_report();
    report["local_blocker_count"] = json!(1);

    let output = run_wrapper(report);

    assert!(
        !output.status.success(),
        "wrapper must still reject local blockers\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn wrapper_rejects_passed_status_with_any_remaining_blocker() {
    let mut report = base_report();
    report["status"] = json!("passed");
    report["external_endpoint_coverage"]["status"] = json!("passed");

    let output = run_wrapper(report);

    assert!(
        !output.status.success(),
        "fully passed status must still require blocker_count=0\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
