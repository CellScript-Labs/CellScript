#[path = "support/cost_toolchain.rs"]
mod cost_toolchain;

#[test]
fn gate_rejects_stale_or_missing_cost_reports() {
    let gate = include_str!("../scripts/cellscript_gate.sh");
    let functions = gate.split_once("prepare_cost_evidence() {").unwrap().1.split_once("run_ci_gate() {").unwrap().0;
    let root = tempfile::tempdir().unwrap();
    let report = root.path().join("report.json");
    let multi_report = root.path().join("multi-report.json");
    let run = |body: &str| {
        std::process::Command::new("bash")
            .arg("-c")
            // This checks the shell's freshness lifecycle. The native v2
            // consumer has separate malformed-measurement tests.
            .arg(format!("set -euo pipefail\nrun() {{ :; }}\nprepare_cost_evidence() {{{functions}\n{body}"))
            .env("CELLSCRIPT_COST_CORPUS_REPORT", &report)
            .env("CELLSCRIPT_MULTI_SCRIPT_COST_REPORT", &multi_report)
            .env("ROOT_DIR", root.path())
            .output()
            .expect("run cost report gate")
    };
    assert!(!run("check_cost_evidence").status.success(), "missing report must fail");
    std::fs::write(&report, "{\n  \"status\": \"passed\"\n}\n").unwrap();
    assert!(!run("check_cost_evidence").status.success(), "both report families are required");
    std::fs::write(&multi_report, "{\n  \"status\": \"passed\"\n}\n").unwrap();
    assert!(run("check_cost_evidence").status.success(), "fresh completed marker is accepted");
    // Pretend the test runner succeeds without running the corpus. Preflight
    // must invalidate the previous passing report before accepting new evidence.
    let skipped = run("run() { :; }\nprepare_cost_evidence\ncheck_cost_evidence");
    assert!(!skipped.status.success(), "stale passing evidence must fail when the corpus was skipped");
    assert!(String::from_utf8_lossy(&skipped.stderr).contains("cost corpus did not produce passing execution evidence"));
}

#[test]
fn cost_toolchain_is_available() {
    let strip = cost_toolchain::require_strip();
    eprintln!("cost toolchain: {}", strip.display());
}

#[test]
fn missing_target_or_strip_cannot_be_reported_as_executed_evidence() {
    let root = tempfile::tempdir().unwrap();
    let libdir = root.path().join("target-lib");
    let error = cost_toolchain::resolve_strip(root.path(), "missing-host", &libdir).unwrap_err();
    assert!(error.contains("missing riscv64imac-unknown-none-elf libcore"), "{error}");
    std::fs::create_dir(&libdir).unwrap();
    std::fs::write(libdir.join("libcore-fixture.rlib"), []).unwrap();
    let error = cost_toolchain::resolve_strip(root.path(), "missing-host", &libdir).unwrap_err();
    assert!(error.contains("llvm-tools-preview"), "{error}");
}

#[cfg(unix)]
#[test]
fn broken_strip_is_not_considered_available() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let libdir = root.path().join("target-lib");
    std::fs::create_dir(&libdir).unwrap();
    std::fs::write(libdir.join("libcore-fixture.rlib"), []).unwrap();
    let strip = root.path().join("lib/rustlib/test-host/bin/llvm-strip");
    std::fs::create_dir_all(strip.parent().unwrap()).unwrap();
    std::fs::write(&strip, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&strip, std::fs::Permissions::from_mode(0o755)).unwrap();
    let error = cost_toolchain::resolve_strip(root.path(), "test-host", &libdir).unwrap_err();
    assert!(error.contains("--version failed"), "{error}");
}
