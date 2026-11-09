// CLI scenarios behavior; included in the original cli integration target.

#[test]
fn cellc_test_subcommand_compiles_test_sources() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main;

action ping() -> u64 {

        1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("math.cell"),
        r#"
module demo::tests::math;

action adds() -> u64 {

        1 + 2
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Test compile complete"));
    assert!(stdout.contains("Compiled 1 test file(s)"));

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["status"], "ok");
    assert_eq!(stdout["test_files"], 1);
    assert_eq!(stdout["passed"], 1);
    assert_eq!(stdout["failed"], 0);
    assert_eq!(stdout["no_run"], true);
    assert_eq!(stdout["execution"], "disabled");
    let tests = stdout["tests"].as_array().unwrap();
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0]["status"], "passed");
    assert!(tests[0]["path"].as_str().unwrap().ends_with("tests/math.cell"));
}

#[test]
fn cellc_test_subcommand_supports_expected_compile_failures() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main;

action ping() -> u64 {

        1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("negative.cell"),
        r#"
// cellscript-test: expect-error: effect declarations have been removed
module demo::tests::negative;

#[effect(Pure)]
fn helper() -> u64 {
    return env::current_timepoint()
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Test compile complete"));
    assert!(stdout.contains("Compiled 1 test file(s)"));
}

#[test]
fn cellc_test_subcommand_rejects_missing_expected_error_text() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main;

action ping() -> u64 {

        1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("negative.cell"),
        r#"
// cellscript-test: expect-error: this text is intentionally absent
module demo::tests::negative;

#[effect(Pure)]
fn helper() -> u64 {
    return env::current_timepoint()
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("expected error text not found"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_test_subcommand_supports_target_directive() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main;

action ping() -> u64 {

        1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("elf.cell"),
        r#"
// cellscript-test: target: riscv64-elf
module demo::tests::elf;

action main() -> u64 {

        0
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Compiled 1 test file(s)"), "unexpected stdout: {}", stdout);
}

#[test]
fn cellc_test_subcommand_supports_policy_directives() {
    // A dynamic vector returned by a helper is not an admitted output value.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main;

action ping() -> u64 {

        1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("policy.cell"),
        r#"
// cellscript-test: deny-runtime-obligations
// cellscript-test: expect-error: create-output:Fingerprint
module demo::tests::policy;

resource Fingerprint {
    digest: Vec<u64>,
}

fn pass_digest(digest: Vec<u64>) -> Vec<u64> {
    return digest;
}

action issue(digest: Vec<u64>) -> Fingerprint {

        let dynamic_digest = pass_digest(digest);
        let token = create Fingerprint {
            digest: dynamic_digest,
        };
        return token;
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Compiled 1 test file(s)"), "unexpected stdout: {}", stdout);
}

#[test]
fn cellc_test_subcommand_supports_runtime_metadata_directives() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main;

action ping() -> u64 {

        1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("metadata.cell"),
        r#"
// cellscript-test: expect-not-standalone
// cellscript-test: expect-ckb-runtime
// cellscript-test: expect-runtime-feature: verify-output-cell
// cellscript-test: expect-no-runtime-feature: consume-expression
// cellscript-test: expect-verifier-obligation: create-output:Fingerprint
// cellscript-test: expect-no-verifier-obligation: not-present
module demo::tests::metadata;

resource Fingerprint {
    digest: Hash,
}

fn pass_digest(digest: Hash) -> Hash {
    return digest;
}

action issue(digest: Hash) -> Fingerprint {

        let dynamic_digest = pass_digest(digest);
        let token = create Fingerprint {
            digest: dynamic_digest,
        };
        return token;
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Compiled 1 test file(s)"), "unexpected stdout: {}", stdout);
}

#[test]
fn cellc_test_subcommand_rejects_missing_runtime_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main;

action ping() -> u64 {

        1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("metadata.cell"),
        r#"
// cellscript-test: expect-runtime-feature: not-present
module demo::tests::metadata;

action ping() -> u64 {

        1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("expected runtime metadata to contain 'not-present'"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_test_subcommand_supports_entrypoint_metadata_directives() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main;

action ping() -> u64 {

        1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("entries.cell"),
        r#"
// cellscript-test: expect-artifact-format: RISC-V assembly
// cellscript-test: expect-action: run
// cellscript-test: expect-function: helper
// cellscript-test: expect-no-action: helper
// cellscript-test: expect-no-lock: run
module demo::tests::entries;

fn helper(x: u64) -> u64 {
    x + 1
}

action run(x: u64) -> u64 {

        helper(x)
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Compiled 1 test file(s)"), "unexpected stdout: {}", stdout);
}

#[test]
fn cellc_test_subcommand_rejects_missing_entrypoint_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main;

action ping() -> u64 {

        1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("entries.cell"),
        r#"
// cellscript-test: expect-function: missing_helper
module demo::tests::entries;

action run(x: u64) -> u64 {

        x
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("expected function metadata to contain 'missing_helper'"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_test_subcommand_rejects_unknown_directives() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main;

action ping() -> u64 {

        1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("typo.cell"),
        r#"
// cellscript-test: expect-eror: typo should not be ignored
module demo::tests::typo;

action ping() -> u64 {

        1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown cellscript-test directive"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("expect-eror"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_test_subcommand_rejects_conflicting_expectations() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main;

action ping() -> u64 {

        1
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tests").join("conflict.cell"),
        r#"
// cellscript-test: expect-success
// cellscript-test: expect-fail
module demo::tests::conflict;

action ping() -> u64 {

        1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("test").arg("--no-run").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("conflicting cellscript-test directives"), "unexpected stderr: {}", stderr);
}

#[test]
fn bundled_scenario_basics_executes_positive_and_exact_negative_cases_on_both_backends() {
    let fixture = fixtures::examples();
    let root = fixture.path().join("scenario_basics");
    let lock_before = std::fs::read(root.join("Cell.lock")).expect("scenario example must carry a tracked lockfile");

    let graph_only_verify =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(&root).args(["package", "verify", "--json"]).output().unwrap();
    assert!(!graph_only_verify.status.success());
    assert!(
        String::from_utf8_lossy(&graph_only_verify.stdout).contains("Cell.lock has no [package.build]")
            || String::from_utf8_lossy(&graph_only_verify.stderr).contains("Cell.lock has no [package.build]")
    );

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(&root)
        .args(["test", "--frozen", "--offline", "--backend", "all", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "ok");
    assert_eq!(report["scenario_files"], 2);
    assert_eq!(report["scenario_runs"], 4);
    let scenarios = report["scenarios"].as_array().expect("scenario reports");
    assert!(scenarios.iter().any(|row| {
        row["scenario"] == "bundled-positive-entry"
            && row["backend"] == "simulator"
            && row["evidence_tier"] == "development-non-consensus"
    }));
    assert!(scenarios.iter().any(|row| {
        row["scenario"] == "bundled-positive-entry" && row["backend"] == "ckb-vm" && row["evidence_tier"] == "authoritative-runtime"
    }));
    let exact_negative = scenarios.iter().filter(|row| row["scenario"] == "bundled-exact-runtime-error").collect::<Vec<_>>();
    assert_eq!(exact_negative.len(), 2, "exact-negative scenario should run once per backend");
    assert!(exact_negative.iter().all(|row| {
        row["steps"][0]["status"] == "expected-runtime-error"
            && row["steps"][0]["runtime_error"]["code"] == 5
            && row["steps"][0]["runtime_error"]["name"] == "assertion-failed"
    }));

    let build = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(&root)
        .args(["build", "--frozen", "--offline", "--json"])
        .output()
        .unwrap();
    assert!(build.status.success(), "stderr: {}", String::from_utf8_lossy(&build.stderr));
    for file in ["main.elf", "main.elf.meta.json", "main.elf.lowering.json", "main.elf.sourcemap.json"] {
        assert!(root.join("build").join(file).is_file(), "verified-artifact example should emit {file}");
    }
    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(&root)
        .args(["verify-artifact", "build/main.elf", "--verify-sources", "--json"])
        .output()
        .unwrap();
    assert!(verify.status.success(), "stderr: {}", String::from_utf8_lossy(&verify.stderr));
    let verify_report: serde_json::Value = serde_json::from_slice(&verify.stdout).unwrap();
    assert_eq!(verify_report["status"], "ok");
    assert_eq!(verify_report["structural_verification"], "verified");
    assert_eq!(verify_report["sources_verified"], true);
    std::fs::remove_dir_all(root.join("build")).expect("remove generated bundled-example artifacts");

    assert_eq!(
        std::fs::read(root.join("Cell.lock")).unwrap(),
        lock_before,
        "frozen scenario execution must not rewrite the tracked graph"
    );
}
