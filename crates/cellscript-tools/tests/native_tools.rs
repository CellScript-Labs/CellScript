use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("CellScript repository root must exist")
}

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cellscript-tools"))
        .args(["--root", root.to_str().expect("UTF-8 repository path")])
        .args(args)
        .current_dir(root)
        .output()
        .expect("cellscript-tools must run")
}

struct TestDir(PathBuf);

impl TestDir {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock must follow Unix epoch").as_nanos();
        let path = std::env::temp_dir().join(format!("cellscript-tools-rust-test-{label}-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).expect("test directory must be creatable");
        Self(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        if self.0.parent() == Some(std::env::temp_dir().as_path())
            && self.0.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with("cellscript-tools-rust-test-"))
        {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn repository_policy_commands_pass_without_an_interpreter() {
    let root = repo_root();
    for command in ["check-skill-pack", "validate-tooling-release", "check-source-policy"] {
        let output = run(&root, &[command]);
        assert!(
            output.status.success(),
            "{command} failed:\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn fixture_generators_emit_complete_rust_reports() {
    let root = repo_root();
    let temp = TestDir::new("fixtures");
    let operator = temp.0.join("operator.json");
    let service = temp.0.join("service.json");
    let operator_output = run(&root, &["profile-operator-fixtures", "--output", operator.to_str().unwrap()]);
    assert!(operator_output.status.success(), "operator generator failed: {}", String::from_utf8_lossy(&operator_output.stderr));
    let service_output = run(
        &root,
        &["service-builder-fixtures", "--operator-fixtures", operator.to_str().unwrap(), "--output", service.to_str().unwrap()],
    );
    assert!(service_output.status.success(), "service generator failed: {}", String::from_utf8_lossy(&service_output.stderr));
    let operator_json: serde_json::Value = serde_json::from_slice(&fs::read(operator).unwrap()).unwrap();
    let service_json: serde_json::Value = serde_json::from_slice(&fs::read(service).unwrap()).unwrap();
    assert_eq!(operator_json["status"], "passed");
    assert_eq!(service_json["status"], "passed");
    assert!(service_json["cases"].as_array().is_some_and(|cases| !cases.is_empty()));
}

#[test]
fn novaseal_summary_preserves_shell_contract() {
    let root = repo_root();
    let temp = TestDir::new("summary");
    let report = temp.0.join("report.json");
    fs::write(
        &report,
        r#"{"status":"local_devnet_passed_external_endpoint_required","live_devnet_rpc_executed":true,"local_blocker_count":0,"acceptance_blocker_count":1,"blocker_count":1,"external_endpoint_coverage":{"status":"external_required"}}"#,
    )
    .unwrap();
    let output = run(&root, &["novaseal-acceptance-summary", report.to_str().unwrap()]);
    assert!(output.status.success(), "summary failed: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "local_devnet_passed_external_endpoint_required\ttrue\t0\t1\t1\texternal_required\n"
    );
}
