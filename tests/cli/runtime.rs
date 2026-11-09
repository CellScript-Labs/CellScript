// CLI runtime behavior; included in the original cli integration target.

#[cfg(not(feature = "vm-runner"))]
#[test]
fn cellc_run_subcommand_without_vm_runner_degrades_gracefully() {
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("run").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("simulate") || stderr.contains("experimental") || stderr.contains("Cell.toml") || stderr.contains("compile")
    );
}

#[test]
fn cellc_run_simulate_json_reports_steps_and_null_cycles() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(root.join("src/main.cell"), "module demo::main;\naction main() -> u64 {\n    \n        0\n}\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).args(["run", "--simulate", "--json"]).output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["mode"], "simulate");
    assert_eq!(summary["artifact_format"], "RISC-V ELF");
    assert_eq!(summary["entry"]["kind"], "action");
    assert_eq!(summary["entry"]["name"], "main");
    assert!(summary["steps"].as_u64().is_some());
    assert!(summary["cycles"].is_null());
    assert!(summary["has_cell_operations"].is_boolean());
    assert!(summary["result"].is_string());
    assert!(summary["trace"].is_array());
}

#[cfg(feature = "vm-runner")]
#[test]
fn cellc_run_subcommand_executes_pure_elf_package() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
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

action main() -> u64 {

        0
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).args(["run", "--json"]).output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["mode"], "ckb-vm");
    assert_eq!(summary["artifact_format"], "RISC-V ELF");
    assert_eq!(summary["entry"]["kind"], "action");
    assert_eq!(summary["entry"]["name"], "main");
    assert!(summary["cycles"].as_u64().is_some());
    assert!(summary["steps"].is_null());
    assert!(summary["has_cell_operations"].is_null());
    assert!(summary["result"].is_null());
    assert!(summary["trace"].is_array());
}

#[cfg(feature = "vm-runner")]
#[test]
fn cellc_run_subcommand_rejects_parameterized_schema_elf() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
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

struct Snapshot {
    amount: u64,
}

action main(snapshot: Snapshot) -> u64 {

        snapshot.amount
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("run").output().unwrap();
    assert!(!output.status.success(), "stdout: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no-argument pure ELF entrypoints"), "stderr: {}", stderr);
    assert!(stderr.contains("action main"), "stderr: {}", stderr);
}

#[cfg(feature = "vm-runner")]
#[test]
fn cellc_run_subcommand_rejects_ckb_runtime_elf() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
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

shared Config {
    threshold: u64,
}

action main() -> u64 {

        let cfg = read_ref::<Config>();
        cfg.threshold
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("run").output().unwrap();
    assert!(!output.status.success(), "stdout: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot provide CKB transaction/syscall context"), "stderr: {}", stderr);
    assert!(stderr.contains("read-cell-dep"), "stderr: {}", stderr);
}
