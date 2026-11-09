// CLI workspace behavior; included in the original cli integration target.

#[test]
fn bundled_package_graph_exercises_alias_features_test_scope_and_ckb_environments() {
    let fixture = fixtures::examples();
    let root = fixture.path().join("package_graph");
    let manifest = std::fs::read_to_string(root.join("Cell.toml")).expect("package graph manifest");
    for needle in [
        "package = \"canonical_math\"",
        "version = \"^1.2.0\"",
        "optional = true",
        "[dev_dependencies.test_support]",
        "auditing = [\"dep:audit\"]",
        "full = [\"auditing\"]",
        "[environments.mainnet]",
        "[environments.testnet]",
        "[dependency_overrides.testnet.contracts]",
    ] {
        assert!(manifest.contains(needle), "package graph example should contain `{needle}`");
    }

    let lock_before = std::fs::read(root.join("Cell.lock")).expect("package graph example must carry a tracked lockfile");
    let lock_text = String::from_utf8(lock_before.clone()).unwrap();
    for needle in [
        "schema = \"cellscript-lock-v0.30-single-package-coordinate-v1\"",
        "compiler_requirement = \">=0.33.0-alpha.2\"",
        "[environments.mainnet.dependencies]",
        "[environments.mainnet.dev_dependencies]",
        "[environments.testnet.dependencies]",
        "[environments.testnet.dev_dependencies]",
        "network_contracts@1.0.0|path:deps/contracts-mainnet|compiler=3e3d302e33332e302d616c7068612e32|env=environment-independent:root=6d61696e6e6574",
        "network_contracts@2.0.0|path:deps/contracts-testnet|compiler=3e3d302e33332e302d616c7068612e32|env=environment-independent:root=746573746e6574",
        "chain=636b622d6d61696e6e6574:genesis=0x1111111111111111111111111111111111111111111111111111111111111111",
        "chain=636b622d746573746e6574:genesis=0x2222222222222222222222222222222222222222222222222222222222222222",
    ] {
        assert!(lock_text.contains(needle), "package graph lock should contain `{needle}`");
    }
    assert!(
        lock_text.contains(&format!("resolver_compiler_version = \"{}\"", cellscript::VERSION)),
        "package graph lock should name the active resolving compiler"
    );

    let missing_environment =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(&root).args(["check", "--frozen", "--offline"]).output().unwrap();
    assert!(!missing_environment.status.success());
    assert!(String::from_utf8_lossy(&missing_environment.stderr).contains("--environment"));

    for args in [
        vec!["check", "--frozen", "--offline", "--environment", "mainnet"],
        vec!["check", "--frozen", "--offline", "--environment", "testnet", "--features", "full"],
        vec!["test", "--no-run", "--frozen", "--offline", "--environment", "testnet", "--all-features"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(&root).args(&args).output().unwrap();
        assert!(output.status.success(), "cellc {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr));
    }
    assert_eq!(
        std::fs::read(root.join("Cell.lock")).unwrap(),
        lock_before,
        "frozen package-graph commands must not rewrite the tracked graph"
    );
}

#[test]
fn cellc_workspace_build_compiles_all_members() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Workspace root Cell.toml
    let workspace_toml = r#"[workspace]
members = ["pkg_a", "pkg_b"]
"#;
    std::fs::write(root.join("Cell.toml"), workspace_toml).unwrap();

    // Member pkg_a
    let pkg_a = root.join("pkg_a");
    std::fs::create_dir_all(pkg_a.join("src")).unwrap();
    std::fs::write(
        pkg_a.join("Cell.toml"),
        r#"[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "pkg_a"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        pkg_a.join("src").join("main.cell"),
        r#"module pkg_a;
action hello() -> u64 {

        let x: u64 = 42;
        return x;
}
"#,
    )
    .unwrap();

    // Member pkg_b
    let pkg_b = root.join("pkg_b");
    std::fs::create_dir_all(pkg_b.join("src")).unwrap();
    std::fs::write(
        pkg_b.join("Cell.toml"),
        r#"[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "pkg_b"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        pkg_b.join("src").join("main.cell"),
        r#"module pkg_b;
action world() -> u64 {

        let y: u64 = 99;
        return y;
}
"#,
    )
    .unwrap();

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").arg("--workspace").arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    let members = summary["results"].as_array().unwrap();
    assert_eq!(members.len(), 2);
    assert!(!root.join("Cell.lock").exists(), "virtual workspace roots must not receive a synthetic package lock");
    for package in [&pkg_a, &pkg_b] {
        let lock = cellscript::package::Lockfile::read_from_root(package).unwrap().unwrap();
        assert!(lock.package_build.is_some(), "successful member build must refresh package_build");
        assert!(lock.dependencies.values().all(|node| node.manifest_digest != "workspace-member-artifact"));
    }
}

#[test]
fn cellc_workspace_build_specific_member() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let workspace_toml = r#"[workspace]
members = ["alpha", "beta"]
"#;
    std::fs::write(root.join("Cell.toml"), workspace_toml).unwrap();

    // Member alpha
    let alpha = root.join("alpha");
    std::fs::create_dir_all(alpha.join("src")).unwrap();
    std::fs::write(
        alpha.join("Cell.toml"),
        r#"[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "alpha"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        alpha.join("src").join("main.cell"),
        r#"module alpha;
action run() -> u64 {  let x: u64 = 1; return x; }
"#,
    )
    .unwrap();

    // Member beta
    let beta = root.join("beta");
    std::fs::create_dir_all(beta.join("src")).unwrap();
    std::fs::write(
        beta.join("Cell.toml"),
        r#"[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "beta"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        beta.join("src").join("main.cell"),
        r#"module beta;
action run() -> u64 {  let y: u64 = 2; return y; }
"#,
    )
    .unwrap();

    // Build only the "alpha" member
    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("build")
        .arg("-p")
        .arg("alpha")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    let members = summary["results"].as_array().unwrap();
    assert_eq!(members.len(), 1);
    assert!(members[0]["member"].as_str().unwrap().contains("alpha"));
}

#[test]
fn cellc_workspace_check_all_members() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let workspace_toml = r#"[workspace]
members = ["lib_a"]
"#;
    std::fs::write(root.join("Cell.toml"), workspace_toml).unwrap();

    let lib_a = root.join("lib_a");
    std::fs::create_dir_all(lib_a.join("src")).unwrap();
    std::fs::write(
        lib_a.join("Cell.toml"),
        r#"[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "lib_a"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        lib_a.join("src").join("main.cell"),
        r#"module lib_a;
action compute() -> u64 {  let v: u64 = 7; return v; }
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("check")
        .arg("--workspace")
        .arg("--all-targets")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["results"][0]["checked_units"].as_array().unwrap().len(), 2);
}

#[test]
fn cellc_workspace_build_member_with_path_dependency_import() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    std::fs::write(
        root.join("Cell.toml"),
        r#"[workspace]
members = ["shared_types", "app"]
"#,
    )
    .unwrap();

    let shared = root.join("shared_types");
    std::fs::create_dir_all(shared.join("src")).unwrap();
    std::fs::write(
        shared.join("Cell.toml"),
        r#"[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "shared_types"
version = "0.1.0"
entry = "src/types.cell"
"#,
    )
    .unwrap();
    std::fs::write(
        shared.join("src").join("types.cell"),
        r#"module shared::types;

resource Token has store, replace, relock, consume, burn {
    amount: u64,
}


"#,
    )
    .unwrap();

    let app = root.join("app");
    std::fs::create_dir_all(app.join("src")).unwrap();
    std::fs::write(
        app.join("Cell.toml"),
        r#"[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "0.1.0"

[dependencies]
shared_types = { path = "../shared_types" }
"#,
    )
    .unwrap();
    std::fs::write(
        app.join("src").join("main.cell"),
        r#"module app::main;

use shared::types::Token;

action passthrough(token: Token) -> Token {

        token
}
"#,
    )
    .unwrap();

    lock_package(&app);
    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").arg("-p").arg("app").arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    let members = summary["results"].as_array().unwrap();
    assert_eq!(members.len(), 2, "package selection must include its transitive workspace-member closure");
    assert_eq!(members[0]["member_name"], "shared_types");
    assert_eq!(members[1]["member_name"], "app");
}

#[test]
fn cellc_workspace_exclude_removes_an_explicit_member() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("Cell.toml"), "[workspace]\nmembers = [\"included\", \"excluded\"]\nexclude = [\"excluded\"]\n").unwrap();
    write_workspace_member(root, "included", "included", &[]);
    write_workspace_member(root, "excluded", "excluded", &[]);

    let output = cellc_command().current_dir(root).args(["check", "--workspace", "--json"]).output().unwrap();
    assert!(output.status.success(), "stdout: {}", String::from_utf8_lossy(&output.stdout));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["results"].as_array().unwrap().len(), 1);
    assert_eq!(report["results"][0]["member_name"], "included");
}

#[test]
fn cellc_workspace_rejects_duplicate_names_and_canonical_paths() {
    for duplicate_paths in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_workspace_member(root, "one", "shared", &[]);
        if duplicate_paths {
            std::fs::write(root.join("Cell.toml"), "[workspace]\nmembers = [\"one\", \"./one\"]\n").unwrap();
        } else {
            write_workspace_member(root, "two", "shared", &[]);
            std::fs::write(root.join("Cell.toml"), "[workspace]\nmembers = [\"one\", \"two\"]\n").unwrap();
        }

        let output = cellc_command().current_dir(root).args(["check", "--workspace", "--json"]).output().unwrap();
        assert!(!output.status.success());
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["diagnostics"][0]["code"], "E2700");
        let message = report["diagnostics"][0]["message"].as_str().unwrap();
        assert!(message.contains(if duplicate_paths { "listed more than once" } else { "declared by more than one member" }));
    }
}

#[test]
fn cellc_workspace_builds_a_reversed_diamond_in_dependency_order() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("Cell.toml"), "[workspace]\nmembers = [\"app\", \"right\", \"left\", \"shared\"]\n").unwrap();
    write_workspace_member(root, "shared", "shared", &[]);
    let left = write_workspace_member(root, "left", "left", &[("shared", "../shared")]);
    let right = write_workspace_member(root, "right", "right", &[("shared", "../shared")]);
    let app = write_workspace_member(root, "app", "app", &[("left", "../left"), ("right", "../right")]);
    lock_package(&left);
    lock_package(&right);
    lock_package(&app);

    let output = cellc_command().current_dir(root).args(["build", "--workspace", "--json"]).output().unwrap();
    assert!(output.status.success(), "stdout: {}", String::from_utf8_lossy(&output.stdout));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let order = report["results"].as_array().unwrap().iter().map(|result| result["member_name"].as_str().unwrap()).collect::<Vec<_>>();
    assert_eq!(order, ["shared", "left", "right", "app"]);
    assert_eq!(report["member_edges"].as_array().unwrap().len(), 4);
    assert!(!root.join("Cell.lock").exists());
}

#[test]
fn cellc_workspace_unifies_the_same_member_across_different_relative_paths() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("Cell.toml"), "[workspace]\nmembers = [\"layers/left\", \"apps/deep/right\", \"shared\"]\n").unwrap();
    write_workspace_member(root, "shared", "shared", &[]);
    let left = write_workspace_member(root, "layers/left", "left", &[("shared", "../../shared")]);
    let right = write_workspace_member(root, "apps/deep/right", "right", &[("shared", "../../../shared")]);
    lock_package(&left);
    lock_package(&right);

    let graph = cellscript::package::workspace::resolve_workspace_graph(root, &Default::default()).unwrap();
    assert_eq!(graph.build_order, ["shared", "left", "right"]);
    let shared_nodes = graph.resolved_nodes.values().filter(|node| node.name == "shared").collect::<Vec<_>>();
    assert_eq!(shared_nodes.len(), 1);
    assert_eq!(shared_nodes[0].source_identity, "workspace:shared");
}

#[test]
fn cellc_workspace_rejects_a_virtual_root_lock() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("Cell.toml"), "[workspace]\nmembers = [\"member\"]\n").unwrap();
    write_workspace_member(root, "member", "member", &[]);
    std::fs::write(root.join("Cell.lock"), "synthetic workspace lock").unwrap();

    let output = cellc_command().current_dir(root).args(["check", "--workspace", "--json"]).output().unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["diagnostics"][0]["code"], "E2700");
    assert!(report["diagnostics"][0]["message"].as_str().unwrap().contains("must not contain Cell.lock"));
}

#[test]
fn cellc_workspace_rejects_member_cycles_before_lock_or_compilation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("Cell.toml"), "[workspace]\nmembers = [\"a\", \"b\"]\n").unwrap();
    write_workspace_member(root, "a", "a", &[("b", "../b")]);
    write_workspace_member(root, "b", "b", &[("a", "../a")]);

    let output = cellc_command().current_dir(root).args(["build", "--workspace", "--json"]).output().unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["diagnostics"][0]["code"], "E2700");
    assert!(report["diagnostics"][0]["message"].as_str().unwrap().contains("a -> b -> a"));
    assert!(!root.join("a/build").exists());
    assert!(!root.join("b/build").exists());
}

#[test]
fn cellc_workspace_rejects_stale_member_locks_before_compilation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("Cell.toml"), "[workspace]\nmembers = [\"base\", \"app\"]\n").unwrap();
    let base = write_workspace_member(root, "base", "base", &[]);
    let app = write_workspace_member(root, "app", "app", &[("base", "../base")]);
    lock_package(&app);
    let manifest = std::fs::read_to_string(base.join("Cell.toml")).unwrap().replace("version = \"1.0.0\"", "version = \"1.0.1\"");
    std::fs::write(base.join("Cell.toml"), manifest).unwrap();

    let output = cellc_command().current_dir(root).args(["check", "--workspace", "--json"]).output().unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["diagnostics"][0]["message"].as_str().unwrap().contains("manifest digest mismatch"));
    assert!(!app.join("build").exists());
}

#[test]
fn cellc_workspace_propagates_dependency_failure_without_building_dependents() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("Cell.toml"), "[workspace]\nmembers = [\"app\", \"base\"]\n").unwrap();
    let base = write_workspace_member(root, "base", "base", &[]);
    let app = write_workspace_member(root, "app", "app", &[("base", "../base")]);
    std::fs::write(base.join("src/main.cell"), "invalid CellScript source").unwrap();
    lock_package(&app);

    let output = cellc_command().current_dir(root).args(["build", "--workspace", "--json"]).output().unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = report["results"].as_array().unwrap();
    assert_eq!(results[0]["member_name"], "base");
    assert_eq!(results[0]["status"], "failed");
    assert_eq!(results[1]["member_name"], "app");
    assert_eq!(results[1]["status"], "blocked");
    assert!(!app.join("build").exists());
}

#[test]
fn cellc_resolve_graph_is_deterministic_read_only_and_reproduces_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let dependency = write_workspace_member(root, "dependency", "dependency", &[]);
    let app = write_workspace_member(root, "app", "app", &[("dependency", "../dependency")]);
    lock_package(&app);
    let lock_before = std::fs::read(app.join("Cell.lock")).unwrap();

    let query = || cellc_command().current_dir(&app).args(["resolve-graph", "--offline", "--json"]).output().unwrap();
    let first = query();
    let second = query();
    assert!(first.status.success(), "stdout: {}", String::from_utf8_lossy(&first.stdout));
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(std::fs::read(app.join("Cell.lock")).unwrap(), lock_before);
    assert!(!app.join("build").exists());
    assert!(!app.join(".cell").exists());
    assert!(!dependency.join("build").exists());

    let graph: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(graph["schema"], "cellscript-resolve-graph-v1");
    assert_eq!(graph["schema_version"], 1);
    assert_eq!(graph["lockfiles"][0]["content"].as_str().unwrap().as_bytes(), lock_before);
    assert_eq!(graph["edges"][0]["alias"], "dependency");
    assert_eq!(graph["edges"][0]["provenance"], "locked-runtime-root-edge");
}

#[test]
fn cellc_build_plan_and_build_share_the_same_unit_identity() {
    let dir = tempfile::tempdir().unwrap();
    let package = write_workspace_member(dir.path(), "app", "app", &[]);
    let plan_output = cellc_command().current_dir(&package).args(["build-plan", "--offline", "--json"]).output().unwrap();
    assert!(plan_output.status.success(), "stdout: {}", String::from_utf8_lossy(&plan_output.stdout));
    let plan: serde_json::Value = serde_json::from_slice(&plan_output.stdout).unwrap();
    assert_eq!(plan["units"][0]["cache"]["status"], "missing");

    let build_output = cellc_command().current_dir(&package).args(["build", "--offline", "--json"]).output().unwrap();
    assert!(build_output.status.success(), "stdout: {}", String::from_utf8_lossy(&build_output.stdout));
    let build: serde_json::Value = serde_json::from_slice(&build_output.stdout).unwrap();
    assert_eq!(build["build_plan_schema"], "cellscript-build-plan-v2");
    assert_eq!(build["resolve_graph_digest"], plan["resolve_graph_digest"]);
    assert_eq!(build["build_unit_id"], plan["units"][0]["id"]);

    let cached_output = cellc_command().current_dir(&package).args(["build-plan", "--offline", "--json"]).output().unwrap();
    let cached: serde_json::Value = serde_json::from_slice(&cached_output.stdout).unwrap();
    assert_ne!(cached["resolve_graph_digest"], plan["resolve_graph_digest"]);
    assert_eq!(cached["resolve_resolution_digest"], plan["resolve_resolution_digest"]);
    assert_eq!(cached["units"][0]["id"], plan["units"][0]["id"]);
    assert_eq!(cached["units"][0]["cache"]["status"], "up-to-date");
    assert_eq!(cached["units"][0]["cache"]["rebuild_reason"], "cache-entry-valid");

    let verify_output = cellc_command().current_dir(&package).args(["package", "verify", "--json"]).output().unwrap();
    assert!(verify_output.status.success(), "stdout: {}", String::from_utf8_lossy(&verify_output.stdout));
    let verification: serde_json::Value = serde_json::from_slice(&verify_output.stdout).unwrap();
    assert_eq!(verification["resolve_graphs"][0]["schema"], "cellscript-resolve-graph-v1");
}

#[test]
fn cellc_resolve_graph_records_feature_environment_and_test_selection() {
    let fixture = fixtures::examples();
    let package = fixture.path().join("package_graph");
    let output = cellc_command()
        .current_dir(&package)
        .args(["resolve-graph", "--scope", "test", "--features", "auditing", "--environment", "testnet", "--offline", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "stdout: {}", String::from_utf8_lossy(&output.stdout));
    let graph: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(graph["selection"]["scope"], "test");
    assert_eq!(graph["selection"]["environment"], "testnet");
    assert_eq!(graph["selection"]["effective_features"], serde_json::json!(["auditing", "default"]));
    let edges = graph["edges"].as_array().unwrap();
    assert!(edges.iter().any(|edge| edge["alias"] == "test_support" && edge["dependency_kind"] == "test"));
    assert!(graph["nodes"].as_object().unwrap().keys().all(|node| node.starts_with("package-node:")));
    assert!(graph["nodes"]
        .as_object()
        .unwrap()
        .values()
        .all(|node| node["environment_identity"].as_str().unwrap().contains("chain=636b622d746573746e6574")));
    assert_eq!(graph["lockfiles"][0]["content"], std::fs::read_to_string(package.join("Cell.lock")).unwrap());
}

#[test]
fn cellc_build_plan_workspace_units_follow_the_member_graph() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("Cell.toml"), "[workspace]\nmembers = [\"app\", \"right\", \"left\", \"shared\"]\n").unwrap();
    write_workspace_member(root, "shared", "shared", &[]);
    let left = write_workspace_member(root, "left", "left", &[("shared", "../shared")]);
    let right = write_workspace_member(root, "right", "right", &[("shared", "../shared")]);
    let app = write_workspace_member(root, "app", "app", &[("left", "../left"), ("right", "../right")]);
    lock_package(&left);
    lock_package(&right);
    lock_package(&app);

    let output = cellc_command().current_dir(root).args(["build-plan", "-p", "app", "--offline", "--json"]).output().unwrap();
    assert!(output.status.success(), "stdout: {}", String::from_utf8_lossy(&output.stdout));
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let units = plan["units"].as_array().unwrap();
    assert_eq!(
        units.iter().map(|unit| unit["package_name"].as_str().unwrap()).collect::<Vec<_>>(),
        ["shared", "left", "right", "app"]
    );
    assert!(units[0]["direct_dependencies"].as_array().unwrap().is_empty());
    assert_eq!(units[1]["direct_dependencies"], serde_json::json!([units[0]["id"].clone()]));
    assert_eq!(units[2]["direct_dependencies"], serde_json::json!([units[0]["id"].clone()]));
    assert_eq!(units[3]["direct_dependencies"], serde_json::json!([units[1]["id"].clone(), units[2]["id"].clone()]));
}

#[test]
fn cellc_update_plan_includes_package_workspace_root_and_members() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"root_package\"\nversion = \"1.0.0\"\n\n[workspace]\nmembers = [\"member\"]\n",
    )
    .unwrap();
    std::fs::write(root.join("src/main.cell"), "module root_package;\n\naction run() -> u64 {\n    \n        1\n}\n")
        .unwrap();
    let member = write_workspace_member(root, "member", "member", &[]);
    lock_package(root);
    lock_package(&member);

    let output = cellc_command().current_dir(root).args(["update-plan", "--offline", "--json"]).output().unwrap();
    assert!(output.status.success(), "stdout: {}", String::from_utf8_lossy(&output.stdout));
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let members = plan["locks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|transaction| transaction["member"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(members, std::collections::BTreeSet::from(["member", "root_package"]));
    assert_eq!(plan["reverse_dependents"].as_array().unwrap().len(), 2);
    assert!(!root.join(".cell").exists());
    assert!(!member.join(".cell").exists());
}
