use serde_json::{json, Value};
use std::process::Command;
use tempfile::tempdir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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

#[cfg(unix)]
fn run_wrapper_with_fake_cellc(fake_cellc: &str, stale_report: Option<Value>) -> std::process::Output {
    let temp = tempdir().expect("tempdir should be available");
    let target = temp.path().join("target");
    std::fs::create_dir(&target).expect("target directory should be creatable");
    if let Some(report) = stale_report {
        std::fs::write(
            target.join("novaseal-devnet-stateful-acceptance.json"),
            serde_json::to_vec_pretty(&report).expect("report should serialize"),
        )
        .expect("stale report should be writable");
    }

    let fake = temp.path().join("fake-cellc");
    std::fs::write(&fake, fake_cellc).expect("fake cellc should be writable");
    let mut permissions = std::fs::metadata(&fake).expect("fake cellc metadata should be readable").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake, permissions).expect("fake cellc should be executable");

    Command::new("bash")
        .arg("scripts/novaseal_devnet_stateful_acceptance.sh")
        .arg("--repo-root")
        .arg(temp.path())
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("CELLC_BIN", fake)
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

fn fake_cellc_writes_report_and_exits(report: &Value, status: i32) -> String {
    format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
repo_root=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo-root)
      repo_root="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
mkdir -p "$repo_root/target"
cat > "$repo_root/target/novaseal-devnet-stateful-acceptance.json" <<'JSON'
{}
JSON
echo "NovaSeal V1 readiness requires external production evidence and endpoint acceptance" >&2
exit {}
"#,
        serde_json::to_string_pretty(report).expect("report should serialize"),
        status
    )
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
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("certifier_status=not_run"),
        "report-only mode should not imply that certification ran successfully"
    );
}

#[cfg(unix)]
#[test]
fn wrapper_allows_external_only_report_when_certifier_exits_for_production_boundary() {
    let fake_cellc = fake_cellc_writes_report_and_exits(&base_report(), 1);

    let output = run_wrapper_with_fake_cellc(&fake_cellc, None);

    assert!(
        output.status.success(),
        "wrapper should accept a fresh local-pass report even when certification exits for external production evidence\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("status=local_devnet_passed_external_endpoint_required"));
    assert!(stdout.contains("certifier_status=1"));
    assert!(
        String::from_utf8_lossy(&output.stderr).is_empty(),
        "certifier stderr should be suppressed when the fresh report proves local acceptance"
    );
}

#[cfg(unix)]
#[test]
fn wrapper_rejects_nonzero_certifier_when_no_fresh_report_is_written() {
    let fake_cellc = "#!/usr/bin/env bash\nexit 1\n";

    let output = run_wrapper_with_fake_cellc(fake_cellc, Some(base_report()));

    assert!(
        !output.status.success(),
        "wrapper must not reuse a stale report when certification fails before writing a fresh report\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing"), "failure should explain that the fresh report is missing");
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
