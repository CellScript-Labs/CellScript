// CLI cache behavior; included in the original cli integration target.

#[test]
fn cellc_source_contract_builds_and_plans_keep_separate_cache_entries() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(root.join("Cell.toml"), "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"source_contract_cache\"\nversion = \"0.1.0\"\n")
        .unwrap();
    std::fs::write(root.join("src/main.cell"), "module source_contract_cache\npublic struct Value has copy, drop, store, fixed, serializable, non_linear { amount: u64 }\npublic fn unused(value: u64) -> u64 { value }\npublic action verify(witness value: Value) { verification require value.amount == 1 }\n").unwrap();
    let mut elf = None;
    let mut unit_ids = std::collections::BTreeMap::new();
    let mut cache_keys = std::collections::BTreeMap::new();
    // Switching evidence mode refreshes Cell.lock build metadata. The cache
    // correctly binds that lock snapshot and requires a fresh switched build.
    for (source_contracts, cache_hit) in [(false, false), (false, true), (true, false), (true, true), (false, false), (false, true)] {
        let mut command = cellc_command();
        command.current_dir(root).args(["build", "--target", "riscv64-elf", "--release", "--json"]);
        if source_contracts {
            command.arg("--source-contracts");
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(summary["cache_hit"], cache_hit, "source_contracts={source_contracts}");
        let artifact = root.join(summary["artifact"].as_str().unwrap());
        let metadata = std::fs::read(root.join(summary["metadata"].as_str().unwrap())).unwrap();
        let parsed: cellscript::CompileMetadata = serde_json::from_slice(&metadata).unwrap();
        assert_eq!(parsed.typed_semantics.nominal_declarations.is_some(), source_contracts);
        assert!(parsed.public_interface.callables.iter().any(|callable| callable.name == "unused"));
        let bytes = std::fs::read(&artifact).unwrap();
        if let Some(previous) = &elf {
            assert_eq!(previous, &bytes);
        } else {
            elf = Some(bytes.clone());
        }
        let artifact = camino::Utf8Path::from_path(&artifact).unwrap();
        let inspection = cellscript_artifact_checker::interface::inspect_bundle(
            &bytes,
            &metadata,
            &std::fs::read(cellscript::lowering_record_output_path_from_artifact(artifact)).unwrap(),
            &std::fs::read(cellscript::source_map_output_path_from_artifact(artifact)).unwrap(),
            &cellscript_artifact_checker::CheckerBudgets::default(),
        )
        .unwrap();
        assert_eq!(inspection.project_module_contract().is_ok(), source_contracts);
        let mut command = cellc_command();
        command.current_dir(root).args(["build-plan", "--target", "riscv64-elf", "--release", "--json"]);
        if source_contracts {
            command.arg("--source-contracts");
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(plan["selection"]["source_contracts"].as_bool().unwrap_or(false), source_contracts);
        let id = plan["units"][0]["id"].clone();
        assert_eq!(summary["build_unit_id"], id);
        assert_eq!(plan["units"][0]["cache"]["status"], "up-to-date");
        let cache_key = plan["units"][0]["cache"]["cache_key"].clone();
        if let Some(previous) = unit_ids.insert(source_contracts, id.clone()) {
            assert_eq!(previous, id);
        }
        if let Some(previous) = cache_keys.insert(source_contracts, cache_key.clone()) {
            assert_eq!(previous, cache_key);
        }
    }
    assert_ne!(unit_ids[&false], unit_ids[&true]);
    assert_ne!(cache_keys[&false], cache_keys[&true]);
}

#[test]
fn cellc_incremental_cache_hit_on_second_build() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Set up a minimal package
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2026"
name = "cache_test"
version = "0.1.0"
"#,
    )
    .unwrap();

    let source = r#"module cache_test
action compute() -> u64 {
    verification
        let x: u64 = 123
        return x
}
"#;
    std::fs::write(root.join("src").join("main.cell"), source).unwrap();

    // First build
    let output1 = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").arg("--json").output().unwrap();
    assert!(output1.status.success(), "stderr: {}", String::from_utf8_lossy(&output1.stderr));
    let summary1: serde_json::Value = serde_json::from_slice(&output1.stdout).unwrap();
    assert_eq!(summary1["status"], "ok");
    // First build should not be a cache hit
    assert_eq!(summary1["cache_hit"], false);

    // Second build (same source, same options)
    let output2 = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").arg("--json").output().unwrap();
    assert!(output2.status.success(), "stderr: {}", String::from_utf8_lossy(&output2.stderr));
    let summary2: serde_json::Value = serde_json::from_slice(&output2.stdout).unwrap();
    assert_eq!(summary2["status"], "ok");
    // Second build should be a cache hit
    assert_eq!(summary2["cache_hit"], true);
}

#[test]
fn cellc_incremental_cache_invalidated_on_source_change() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Set up a minimal package
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2026"
name = "inval_test"
version = "0.1.0"
"#,
    )
    .unwrap();

    let source_v1 = r#"module inval_test
action compute() -> u64 {
    verification
        let x: u64 = 1
        return x
}
"#;
    std::fs::write(root.join("src").join("main.cell"), source_v1).unwrap();

    // Build v1
    let output1 = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").arg("--json").output().unwrap();
    assert!(output1.status.success(), "stderr: {}", String::from_utf8_lossy(&output1.stderr));

    // Modify source
    let source_v2 = r#"module inval_test
action compute() -> u64 {
    verification
        let x: u64 = 2
        return x
}
"#;
    std::fs::write(root.join("src").join("main.cell"), source_v2).unwrap();

    // Build v2 - should NOT be a cache hit since source changed
    let output2 = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").arg("--json").output().unwrap();
    assert!(output2.status.success(), "stderr: {}", String::from_utf8_lossy(&output2.stderr));
    let summary2: serde_json::Value = serde_json::from_slice(&output2.stdout).unwrap();
    assert_eq!(summary2["cache_hit"], false);
}

#[test]
fn cellc_clean_cache_flag_removes_incremental_cache() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Set up a minimal package
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2026"
name = "clean_test"
version = "0.1.0"
"#,
    )
    .unwrap();

    let source = r#"module clean_test
action compute() -> u64 {
    verification
        let x: u64 = 55
        return x
}
"#;
    std::fs::write(root.join("src").join("main.cell"), source).unwrap();

    // Build to populate incremental cache
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    // Verify cache directory was created
    let cache_dir = root.join(".cell").join("build").join("cache");
    assert!(cache_dir.exists(), "incremental cache directory should exist after build");
    let nested_cache = root.join("examples/nested/.cell/build/cache");
    std::fs::create_dir_all(&nested_cache).unwrap();
    std::fs::write(nested_cache.join("generated"), "cache").unwrap();

    // Clean with --cache flag
    let clean_output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("clean").arg("--cache").arg("--json").output().unwrap();
    assert!(clean_output.status.success(), "stderr: {}", String::from_utf8_lossy(&clean_output.stderr));

    // Verify cache directory was removed
    assert!(!cache_dir.exists(), "incremental cache directory should be removed after clean --cache");
    assert!(!nested_cache.exists(), "clean --cache should remove nested workspace caches");

    // Verify next build is NOT a cache hit
    let output2 = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").arg("--json").output().unwrap();
    assert!(output2.status.success(), "stderr: {}", String::from_utf8_lossy(&output2.stderr));
    let summary2: serde_json::Value = serde_json::from_slice(&output2.stdout).unwrap();
    assert_eq!(summary2["cache_hit"], false, "build after clean --cache should not be a cache hit");
}

#[cfg(unix)]
#[test]
fn cellc_clean_cache_refuses_symlinked_managed_components() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(root.join(".cell")).unwrap();
    std::fs::create_dir_all(outside.join("cache/victim")).unwrap();
    std::fs::write(outside.join("cache/victim/evidence"), "preserve").unwrap();
    symlink(&outside, root.join(".cell/build")).unwrap();

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(&root).arg("clean").arg("--cache").arg("--json").output().unwrap();

    assert!(!output.status.success(), "clean --cache must reject an intermediate symlink");
    let diagnostic = format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(diagnostic.contains("symlink or non-directory component"), "unexpected diagnostic: {diagnostic}");
    assert!(outside.join("cache/victim/evidence").is_file(), "clean must not escape the workspace");
}

#[test]
fn cellc_entry_action_bypasses_incremental_cache() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Set up a minimal package
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2026"
name = "entry_bypass"
version = "0.1.0"
"#,
    )
    .unwrap();

    let source = r#"module entry_bypass
action compute() -> u64 {
    verification
        let x: u64 = 10
        return x
}
"#;
    std::fs::write(root.join("src").join("main.cell"), source).unwrap();

    // First build (default entry scope)
    let output1 = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").arg("--json").output().unwrap();
    assert!(output1.status.success(), "stderr: {}", String::from_utf8_lossy(&output1.stderr));

    // Build with --entry-action: should bypass cache and produce a fresh compile
    let output2 = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("build")
        .arg("--entry-action")
        .arg("compute")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output2.status.success(), "stderr: {}", String::from_utf8_lossy(&output2.stderr));
    let summary2: serde_json::Value = serde_json::from_slice(&output2.stdout).unwrap();
    assert_eq!(summary2["cache_hit"], false, "--entry-action should bypass incremental cache");
}
