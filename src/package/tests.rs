use super::*;
use tempfile::tempdir;

#[test]
fn test_manifest_serialization() {
    let manifest = PackageManifest {
        package: PackageInfo {
            name: "test".to_string(),
            version: "0.1.0".to_string(),
            edition: CURRENT_EDITION,
            namespace: None,
            authors: vec!["Test Author".to_string()],
            description: "Test package".to_string(),
            license: "MIT".to_string(),
            repository: String::new(),
            homepage: String::new(),
            documentation: String::new(),
            keywords: vec!["test".to_string()],
            categories: vec!["test".to_string()],
            cellscript_version: "*".to_string(),
            entry: "src/main.cell".to_string(),
            source_roots: vec![],
            include: vec![],
            exclude: vec![],
        },
        workspace: None,
        artifacts: Vec::new(),
        dependencies: HashMap::new(),
        dev_dependencies: HashMap::new(),
        features: BTreeMap::new(),
        environments: BTreeMap::new(),
        dependency_overrides: BTreeMap::new(),
        resolvers: BTreeMap::new(),
        build: BuildConfig::default(),
        policy: PolicyConfig::default(),
        deploy: DeployConfig::default(),
        metadata: HashMap::new(),
    };

    let toml_str = toml::to_string(&manifest).unwrap();
    assert!(toml_str.contains("name = \"test\""));
    assert!(toml_str.contains("version = \"0.1.0\""));
    assert!(toml_str.contains("edition = \"2027\""));
    let parsed: PackageManifest = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.package.edition, CURRENT_EDITION);
    assert!(parsed.artifacts.is_empty());
    assert!(!toml_str.contains("artifacts"));
}

#[test]
fn policy_artifact_manifest_roundtrip_preserves_explicit_tags_and_common_order() {
    let dir = tempdir().unwrap();
    let manager = PackageManager::new(dir.path());
    manager.init("policy_fixture").unwrap();
    let mut manifest = manager.read_manifest().unwrap();
    let declaration: ArtifactDeclaration = toml::from_str(
        r#"
name = "token-policy"
context = { kind = "type-group" }
bindings = "group-relative"
coverage = "fixed-group"
layout = "Token"
dispatch = "policy-witness-v1"
actions = [{ tag = 40, action = "burn" }, { tag = 10, action = "mint" }]
common_checks = ["authenticate", "audit"]
"#,
    )
    .unwrap();
    manifest.artifacts.push(declaration.clone());
    manager.write_manifest(&manifest).unwrap();
    let loaded = manager.read_manifest().unwrap();
    assert_eq!(loaded.artifacts, vec![declaration]);
    let encoded = std::fs::read_to_string(dir.path().join("Cell.toml")).unwrap();
    assert!(encoded.contains("[[artifacts]]"));
    assert_eq!(loaded.artifacts[0].actions[0].tag, 40);
    assert_eq!(loaded.artifacts[0].common_checks, ["authenticate", "audit"]);
}

#[test]
fn policy_artifact_manifest_rejects_duplicate_names_tags_and_unknown_contract_fields() {
    let dir = tempdir().unwrap();
    let manager = PackageManager::new(dir.path());
    manager.init("policy_fixture").unwrap();
    let base = std::fs::read_to_string(dir.path().join("Cell.toml")).unwrap();
    let valid = r#"
[[artifacts]]
name = "token-policy"
context = { kind = "type-group" }
bindings = "group-relative"
coverage = "fixed-group"
layout = "Token"
dispatch = "policy-witness-v1"
actions = [{ tag = 10, action = "mint" }, { tag = 20, action = "burn" }]
"#;
    for (suffix, expected) in [
        (format!("{valid}{valid}"), "declared more than once"),
        (valid.replace("tag = 20", "tag = 10"), "repeats numeric tag"),
        (format!("{valid}fallback = true\n"), "unknown field"),
        (valid.replace("type-group", "lock-group"), "Lock artifacts require single-entry dispatch"),
        (valid.replace("policy-witness-v1", "policy-witness-v2"), "unknown variant"),
    ] {
        std::fs::write(dir.path().join("Cell.toml"), format!("{base}{suffix}")).unwrap();
        let error = manager.read_manifest().unwrap_err();
        assert!(error.message.contains(expected), "{expected}: {}", error.message);
    }
    let mut manifest: PackageManifest = toml::from_str(&format!("{base}{valid}")).unwrap();
    manifest.artifacts.push(manifest.artifacts[0].clone());
    assert!(manager.write_manifest(&manifest).unwrap_err().message.contains("declared more than once"));
    assert!(manager.validate_manifest_package_contract(&manifest).unwrap_err().message.contains("declared more than once"));
}

#[test]
fn package_manifest_requires_a_known_explicit_edition() {
    let missing = toml::from_str::<PackageManifest>(
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap_err();
    assert!(missing.to_string().contains("missing field `edition`"));

    let unsupported = toml::from_str::<PackageManifest>(
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "unsupported"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap_err();
    let message = unsupported.to_string();
    assert!(message.contains("2027"));

    let preview = toml::from_str::<PackageManifest>(
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    assert_eq!(preview.package.edition, crate::CURRENT_EDITION);
}

#[test]
fn explicit_repin_preserves_current_pins_and_rejects_unknown_lock_contracts() {
    let root = tempdir().unwrap();
    let mut lockfile = Lockfile::new();
    lockfile.package.name = "current_pins".into();
    lockfile.root.manifest_digest = "source-digest".into();
    let source = toml::to_string_pretty(&lockfile).unwrap();
    std::fs::write(root.path().join("Cell.lock"), &source).unwrap();
    let current = Lockfile::read_for_explicit_repin(root.path()).unwrap();
    assert_eq!(current.package.edition, CURRENT_EDITION);
    assert_eq!(current.package.name, "current_pins");
    assert_eq!(current.root.manifest_digest, "source-digest");
    assert_eq!(std::fs::read_to_string(root.path().join("Cell.lock")).unwrap(), source);

    for invalid in [
        source.replace("edition = \"2027\"", "edition = \"2099\""),
        source.replace(Lockfile::CURRENT_SCHEMA, "unknown-lock-schema"),
        "not valid TOML = [".to_string(),
    ] {
        std::fs::write(root.path().join("Cell.lock"), invalid).unwrap();
        assert!(Lockfile::read_from_root(root.path()).is_err());
        assert!(Lockfile::read_for_explicit_repin(root.path()).is_err());
    }
}

#[test]
fn package_manager_init_writes_current_edition() {
    let temp = tempdir().unwrap();
    PackageManager::new(temp.path()).init("demo").unwrap();
    let source = std::fs::read_to_string(temp.path().join("Cell.toml")).unwrap();
    assert!(source.contains("edition = \"2027\""));
    assert!(source.contains(&format!("cellscript_version = \">={}\"", crate::VERSION)));
    let manifest: PackageManifest = toml::from_str(&source).unwrap();
    assert_eq!(manifest.package.edition, CURRENT_EDITION);
}

#[test]
fn compiler_requirement_is_semver_checked_and_legacy_bare_versions_are_minimums() {
    assert!(compiler_requirement_matches("*", "0.1.0").unwrap());
    assert!(compiler_requirement_matches("0.16", "0.26.0").unwrap());
    assert!(!compiler_requirement_matches("0.30", "0.29.9").unwrap());
    assert!(compiler_requirement_matches("0.30", "0.30.0").unwrap());
    assert!(!compiler_requirement_matches("^0.30", "0.31.0").unwrap());
    assert!(compiler_requirement_matches(">=0.26.0-alpha.1", "0.26.0").unwrap());
    assert!(compiler_requirement_matches("=0.30.0-alpha.1", "0.30.0-alpha.1").unwrap());
    assert!(!compiler_requirement_matches("=0.30.0-alpha.1", "0.30.0-alpha.2").unwrap());
    assert!(parse_compiler_requirement("").unwrap_err().message.contains("must not be empty"));
    assert!(parse_compiler_requirement("not-semver").unwrap_err().message.contains("invalid package cellscript_version"));

    let incompatible: PackageManifest = toml::from_str(
        r#"
[package]
edition = "2027"
name = "future"
version = "1.0.0"
cellscript_version = ">=999.0.0"
"#,
    )
    .unwrap();
    let error = validate_package_compiler_requirement(&incompatible.package).unwrap_err();
    assert!(error.message.contains("future@1.0.0"), "{}", error.message);
    assert!(error.message.contains(crate::VERSION), "{}", error.message);
}

#[test]
fn transitive_path_package_rejects_incompatible_compiler_before_source_loading() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join("deps/middle/src")).unwrap();
    std::fs::create_dir_all(root.join("deps/future/src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "1.0.0"

[dependencies.middle]
path = "deps/middle"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("deps/middle/Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "middle"
version = "1.0.0"

[dependencies.future]
path = "../future"
"#,
    )
    .unwrap();
    std::fs::write(root.join("deps/middle/src/lib.cell"), "module middle;\n").unwrap();
    std::fs::write(
        root.join("deps/future/Cell.toml"),
        r#"
[package]
edition = "2027"
name = "future"
version = "1.0.0"
cellscript_version = ">=999.0.0"
"#,
    )
    .unwrap();
    std::fs::write(root.join("deps/future/src/lib.cell"), "this is deliberately invalid CellScript").unwrap();

    let error = PackageManager::new(root).resolve_dependencies().unwrap_err();
    assert_eq!(error.code.as_deref(), Some("E2600"));
    let incompatible = &error.details.as_ref().unwrap()["incompatible_packages"][0];
    assert_eq!(incompatible["package"], "future");
    assert_eq!(incompatible["package_version"], "1.0.0");
    assert_eq!(incompatible["incoming_edge"]["from_package"], "middle");
    assert_eq!(incompatible["incoming_edge"]["alias"], "future");
    assert!(incompatible["message"].as_str().unwrap().contains("before loading source"));
    assert!(!incompatible["message"].as_str().unwrap().contains("parse"));
}

#[test]
fn dependency_preflight_reports_every_incompatible_package_and_incoming_edge() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    for package in ["future_a", "future_b"] {
        let package_root = root.join("deps").join(package);
        std::fs::create_dir_all(package_root.join("src")).unwrap();
        std::fs::write(
            package_root.join("Cell.toml"),
            format!(
                r#"
[package]
edition = "2027"
name = "{package}"
version = "1.0.0"
cellscript_version = ">=999.0.0"
"#
            ),
        )
        .unwrap();
        std::fs::write(package_root.join("src/lib.cell"), "this source is intentionally invalid").unwrap();
    }
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "1.0.0"

[dependencies.first]
package = "future_a"
path = "deps/future_a"

[dependencies.second]
package = "future_b"
path = "deps/future_b"
"#,
    )
    .unwrap();

    let mut manager = PackageManager::new(root);
    let error = manager.resolve_dependencies().unwrap_err();

    assert_eq!(error.code.as_deref(), Some("E2600"));
    let incompatible = error.details.as_ref().unwrap()["incompatible_packages"].as_array().unwrap();
    assert_eq!(incompatible.len(), 2, "{}", error.message);
    assert_eq!(incompatible.iter().map(|entry| entry["package"].as_str().unwrap()).collect::<Vec<_>>(), ["future_a", "future_b"]);
    assert_eq!(incompatible[0]["incoming_edge"]["alias"], "first");
    assert_eq!(incompatible[1]["incoming_edge"]["alias"], "second");
    assert!(manager.get_resolved().is_empty());
}

#[test]
fn aliases_reuse_one_package_coordinate_but_distinct_paths_fail_closed() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write_path_package(root, "deps/shared", "shared", "1.0.0");
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "1.0.0"

[dependencies.first]
package = "shared"
path = "deps/shared"

[dependencies.second]
package = "shared"
path = "deps/shared"
"#,
    )
    .unwrap();
    let mut manager = PackageManager::new(root);
    manager.resolve_dependencies().unwrap();
    assert_eq!(manager.root_dependencies()["first"], manager.root_dependencies()["second"]);
    assert_eq!(manager.get_resolved().values().filter(|package| package.name == "shared").count(), 1);

    write_path_package(root, "deps/substitute", "shared", "1.0.0");
    let manifest = std::fs::read_to_string(root.join("Cell.toml")).unwrap().replace(
        "[dependencies.second]\npackage = \"shared\"\npath = \"deps/shared\"",
        "[dependencies.second]\npackage = \"shared\"\npath = \"deps/substitute\"",
    );
    std::fs::write(root.join("Cell.toml"), manifest).unwrap();
    let error = manager.resolve_dependencies().unwrap_err();
    assert_eq!(error.code.as_deref(), Some("E2601"));
    assert_eq!(error.details.as_ref().unwrap()["conflict_kind"], "source");
    assert_eq!(error.details.as_ref().unwrap()["incoming_edges"].as_array().unwrap().len(), 2);
    assert!(manager.get_resolved().is_empty());
}

#[test]
fn locked_resolution_rejects_duplicate_coordinate_nodes_and_discards_the_partial_graph() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    for (path, version) in [("deps/first", "1.0.0"), ("deps/second", "2.0.0")] {
        write_path_package(root, path, "shared", version);
        let manifest_path = root.join(path).join("Cell.toml");
        let manifest = std::fs::read_to_string(&manifest_path)
            .unwrap()
            .replace(&format!("version = \"{version}\""), &format!("version = \"{version}\"\nnamespace = \"cellscript\""));
        std::fs::write(manifest_path, manifest).unwrap();
    }
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "1.0.0"

[dependencies.first]
package = "shared"
path = "deps/first"

[dependencies.second]
package = "shared"
path = "deps/second"
"#,
    )
    .unwrap();

    let mut lockfile = Lockfile::new();
    lockfile.package = LockfilePackageInfo {
        edition: CURRENT_EDITION,
        name: "app".to_string(),
        version: "1.0.0".to_string(),
        namespace: None,
        source_hash: Some(registry::compute_source_hash(root).unwrap()),
        compiler_source_hash: None,
        compiler_requirement: ">=0.33.0-alpha.1".to_string(),
        resolver_compiler_version: crate::VERSION.to_string(),
    };
    lockfile.root.manifest_digest = compute_manifest_digest(root).unwrap();
    for (alias, path, version) in [("first", "deps/first", "1.0.0"), ("second", "deps/second", "2.0.0")] {
        let package_root = root.join(path);
        let manifest = std::fs::read(package_root.join("Cell.toml")).unwrap();
        let package = ResolvedPackage {
            node_id: String::new(),
            name: "shared".to_string(),
            version: version.to_string(),
            path: package_root.clone(),
            source: PackageSource::Local(PathBuf::from(path)),
            dependencies: BTreeMap::new(),
            namespace: Some("cellscript".to_string()),
            source_hash: Some(registry::compute_source_hash(&package_root).unwrap()),
            manifest_digest: manifest_digest(&manifest),
            compiler_requirement: ">=0.33.0-alpha.1".to_string(),
        };
        let node_id = package_node_id(&package, &ResolutionOptions::default(), None);
        lockfile.root.dependencies.insert(alias.to_string(), node_id.clone());
        lockfile.dependencies.insert(
            node_id,
            LockedDependency {
                name: package.name,
                namespace: package.namespace,
                version: package.version,
                source: LockedSource::Path { path: path.to_string() },
                source_hash: package.source_hash,
                manifest_digest: package.manifest_digest,
                compiler_requirement: package.compiler_requirement,
                resolver_compiler_version: crate::VERSION.to_string(),
                dependencies: BTreeMap::new(),
                build: None,
            },
        );
    }
    lockfile.write_to_root(root).unwrap();

    let mut manager = PackageManager::new(root);
    let error = manager.resolve_locked_dependencies(&ResolutionOptions::default()).unwrap_err();
    assert_eq!(error.code.as_deref(), Some("E2601"));
    assert_eq!(error.details.as_ref().unwrap()["conflict_kind"], "source");
    assert_eq!(error.details.as_ref().unwrap()["locked"], true);
    assert!(manager.get_resolved().is_empty());
    assert!(manager.root_dependencies().is_empty());
}

#[test]
fn package_coordinates_keep_namespaces_distinct() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    for (path, namespace) in [("deps/alpha", "alpha"), ("deps/beta", "beta")] {
        write_path_package(root, path, "shared", "1.0.0");
        let manifest_path = root.join(path).join("Cell.toml");
        let manifest = std::fs::read_to_string(&manifest_path)
            .unwrap()
            .replace("version = \"1.0.0\"", &format!("version = \"1.0.0\"\nnamespace = \"{namespace}\""));
        std::fs::write(manifest_path, manifest).unwrap();
    }
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "1.0.0"

[dependencies.alpha]
package = "shared"
path = "deps/alpha"

[dependencies.beta]
package = "shared"
path = "deps/beta"
"#,
    )
    .unwrap();
    let mut manager = PackageManager::new(root);
    manager.resolve_dependencies().unwrap();
    assert_ne!(manager.root_dependencies()["alpha"], manager.root_dependencies()["beta"]);
    assert_eq!(manager.get_resolved().values().filter(|package| package.name == "shared").count(), 2);
}

#[test]
fn path_and_git_cannot_substitute_for_a_registry_coordinate_implicitly() {
    let temp = tempdir().unwrap();
    let registry_package = ResolvedPackage {
        node_id: String::new(),
        name: "token".to_string(),
        version: "1.0.0".to_string(),
        path: temp.path().join("registry"),
        source: PackageSource::Registry {
            registry: "https://registry.example".to_string(),
            url: "https://registry.example/token.snapshot".to_string(),
            revision: format!("sha256:{}", "11".repeat(32)),
            namespace: "cellscript".to_string(),
            version: "1.0.0".to_string(),
        },
        dependencies: BTreeMap::new(),
        namespace: Some("cellscript".to_string()),
        source_hash: Some("registry-hash".to_string()),
        manifest_digest: "registry-manifest".to_string(),
        compiler_requirement: "*".to_string(),
    };
    let dependency = Dependency::Simple("1.0.0".to_string());
    for alternate_source in [
        PackageSource::Local(PathBuf::from("deps/token")),
        PackageSource::Git { url: "https://git.example/token".to_string(), revision: "22".repeat(20) },
    ] {
        let alternate = ResolvedPackage {
            source: alternate_source,
            path: temp.path().join("alternate"),
            source_hash: Some("alternate-hash".to_string()),
            manifest_digest: "alternate-manifest".to_string(),
            ..registry_package.clone()
        };
        let mut manager = PackageManager::new(temp.path());
        let registry_node = package_node_id(&registry_package, &ResolutionOptions::default(), None);
        manager
            .register_package_instance(
                &registry_package,
                &registry_node,
                &ResolutionOptions::default(),
                None,
                "app",
                "published",
                &dependency,
                false,
            )
            .unwrap();
        let alternate_node = package_node_id(&alternate, &ResolutionOptions::default(), None);
        let error = manager
            .register_package_instance(
                &alternate,
                &alternate_node,
                &ResolutionOptions::default(),
                None,
                "app",
                "override",
                &dependency,
                false,
            )
            .unwrap_err();
        assert_eq!(error.code.as_deref(), Some("E2601"));
        assert_eq!(error.details.as_ref().unwrap()["conflict_kind"], "source");
        assert_eq!(error.details.as_ref().unwrap()["incoming_edges"].as_array().unwrap().len(), 2);
    }
}

#[test]
fn feature_variants_for_one_coordinate_fail_before_source_loading() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    for (path, name) in [("deps/left", "left"), ("deps/right", "right"), ("deps/shared", "shared")] {
        write_path_package(root, path, name, "1.0.0");
    }
    let shared_manifest = root.join("deps/shared/Cell.toml");
    let manifest = format!("{}\n[features]\nleft = []\nright = []\n", std::fs::read_to_string(&shared_manifest).unwrap());
    std::fs::write(shared_manifest, manifest).unwrap();
    for (parent, feature) in [("left", "left"), ("right", "right")] {
        let parent_manifest = root.join("deps").join(parent).join("Cell.toml");
        let manifest = format!(
            "{}\n[dependencies.shared]\npath = \"../shared\"\ndefault_features = false\nfeatures = [\"{feature}\"]\n",
            std::fs::read_to_string(&parent_manifest).unwrap()
        );
        std::fs::write(parent_manifest, manifest).unwrap();
    }
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "1.0.0"

[dependencies.left]
path = "deps/left"

[dependencies.right]
path = "deps/right"
"#,
    )
    .unwrap();

    let mut manager = PackageManager::new(root);
    let error = manager.resolve_dependencies().unwrap_err();
    assert_eq!(error.code.as_deref(), Some("E2601"));
    assert_eq!(error.details.as_ref().unwrap()["coordinate"]["name"], "shared");
    assert_eq!(error.details.as_ref().unwrap()["conflict_kind"], "feature");
    assert_eq!(error.details.as_ref().unwrap()["incoming_edges"][0]["features"], serde_json::json!(["left"]));
    assert_eq!(error.details.as_ref().unwrap()["incoming_edges"][1]["features"], serde_json::json!(["right"]));
    assert!(manager.get_resolved().is_empty());
}

#[test]
fn lockfile_binds_root_and_transitive_compiler_requirements() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write_path_package(root, "deps/math", "math", "1.2.3");
    let dependency_manifest = root.join("deps/math/Cell.toml");
    let dependency_source = std::fs::read_to_string(&dependency_manifest)
        .unwrap()
        .replace("cellscript_version = \">=0.33.0-alpha.1\"", "cellscript_version = \">=0.16, >=0.33.0-alpha.1\"");
    std::fs::write(&dependency_manifest, dependency_source).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
edition = "2027"
name = "app"
version = "1.0.0"
cellscript_version = ">=0.20, >=0.33.0-alpha.1"

[dependencies.math]
path = "deps/math"
version = "1.2.3"
"#,
    )
    .unwrap();

    write_test_lock(root, &ResolutionOptions::default());
    let mut lockfile = Lockfile::read_from_root(root).unwrap().unwrap();
    assert_eq!(lockfile.package.compiler_requirement, ">=0.20, >=0.33.0-alpha.1");
    assert_eq!(lockfile.package.resolver_compiler_version, crate::VERSION);
    let node = lockfile.root.dependencies.get("math").unwrap().clone();
    assert!(node.contains("compiler=3e3d302e3136"), "{node}");
    let dependency = lockfile.dependencies.get(&node).unwrap();
    assert_eq!(dependency.compiler_requirement, ">=0.16, >=0.33.0-alpha.1");
    assert_eq!(dependency.resolver_compiler_version, crate::VERSION);

    lockfile.dependencies.get_mut(&node).unwrap().compiler_requirement = "*".to_string();
    lockfile.write_to_root(root).unwrap();
    let error = PackageManager::new(root).resolve_locked_dependencies(&ResolutionOptions::default()).unwrap_err();
    assert!(
        error.message.contains("compiler requirement '*' does not match Cell.toml '>=0.16, >=0.33.0-alpha.1'"),
        "{}",
        error.message
    );
}

#[test]
fn locked_resolution_revalidates_active_compiler_without_selecting_a_replacement() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write_path_package(root, "deps/math", "math", "1.2.3");
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "1.0.0"

[dependencies.math]
path = "deps/math"
version = "1.2.3"
"#,
    )
    .unwrap();
    write_test_lock(root, &ResolutionOptions::default());

    let dependency_manifest = root.join("deps/math/Cell.toml");
    let future_manifest = std::fs::read_to_string(&dependency_manifest)
        .unwrap()
        .replace("cellscript_version = \">=0.33.0-alpha.1\"", "cellscript_version = \">=999.0.0\"");
    std::fs::write(&dependency_manifest, future_manifest).unwrap();
    std::fs::write(root.join("deps/math/src/lib.cell"), "this source must not be parsed").unwrap();

    let manager = PackageManager::new(root);
    let resolved = manager.resolve_from_path("math", "deps/math").unwrap();
    let future_node = package_node_id(&resolved, &ResolutionOptions::default(), None);
    let mut lockfile = Lockfile::read_from_root(root).unwrap().unwrap();
    let old_node = lockfile.root.dependencies.get("math").unwrap().clone();
    let mut locked = lockfile.dependencies.remove(&old_node).unwrap();
    locked.compiler_requirement = resolved.compiler_requirement.clone();
    locked.manifest_digest = resolved.manifest_digest.clone();
    locked.source_hash = resolved.source_hash.clone();
    lockfile.dependencies.insert(future_node.clone(), locked);
    lockfile.root.dependencies.insert("math".to_string(), future_node.clone());
    lockfile.write_to_root(root).unwrap();

    let error = PackageManager::new(root).resolve_locked_dependencies(&ResolutionOptions::default()).unwrap_err();
    assert_eq!(error.code.as_deref(), Some("E2600"));
    assert_eq!(error.details.as_ref().unwrap()["compiler_requirement"], ">=999.0.0");
    assert_eq!(error.details.as_ref().unwrap()["phase"], "manifest-before-source");
    assert_eq!(error.details.as_ref().unwrap()["incoming_edge"]["alias"], "math");
    assert!(error.message.contains("before loading source"), "{}", error.message);
    assert_eq!(lockfile.root.dependencies["math"], future_node);
}

#[test]
fn test_dependency_graph() {
    let mut graph = DependencyGraph::new();
    graph.add_node("A".to_string());
    graph.add_node("B".to_string());
    graph.add_node("C".to_string());
    graph.add_edge("A".to_string(), "B".to_string());
    graph.add_edge("B".to_string(), "C".to_string());

    assert!(graph.find_cycle().is_none());

    graph.add_edge("C".to_string(), "A".to_string());
    assert!(graph.find_cycle().is_some());
}

fn locked_path(name: &str, version: &str, path: &str, dependencies: BTreeMap<String, String>) -> LockedDependency {
    LockedDependency {
        name: name.to_string(),
        namespace: None,
        version: version.to_string(),
        source: LockedSource::Path { path: path.to_string() },
        source_hash: Some(format!("hash-{name}")),
        manifest_digest: format!("manifest-{name}"),
        dependencies,
        build: None,
        compiler_requirement: "*".to_string(),
        resolver_compiler_version: crate::VERSION.to_string(),
    }
}

fn resolved_path(name: &str, version: &str, path: &str, dependencies: BTreeMap<String, String>) -> ResolvedPackage {
    ResolvedPackage {
        node_id: name.to_string(),
        name: name.to_string(),
        version: version.to_string(),
        path: PathBuf::from(path),
        source: PackageSource::Local(PathBuf::from(path)),
        dependencies,
        namespace: None,
        source_hash: Some(format!("hash-{name}")),
        manifest_digest: format!("manifest-{name}"),
        compiler_requirement: "*".to_string(),
    }
}

#[test]
fn test_version_compatibility() {
    assert!(version::satisfies("1.2.3", &VersionReq::Compatible("1.0.0".to_string())));
    assert!(version::satisfies("1.5.0", &VersionReq::Compatible("1.2.3".to_string())));
    assert!(!version::satisfies("1.1.9", &VersionReq::Compatible("1.2.3".to_string())));
    assert!(!version::satisfies("2.0.0", &VersionReq::Compatible("1.0.0".to_string())));
    assert!(!version::satisfies("0.2.0", &VersionReq::Compatible("0.1.0".to_string())));
    assert!(version::satisfies("0.1.5", &VersionReq::Compatible("0.1.0".to_string())));
    assert!(!version::satisfies("0.1.0-alpha.1", &VersionReq::Compatible("0.1.0".to_string())));
    assert!(version::satisfies("0.1.0-alpha.2", &VersionReq::Compatible("0.1.0-alpha.1".to_string())));
    assert!(version::satisfies("1.2.3+build.7", &VersionReq::Exact("1.2.3".to_string())));
    assert!(version::satisfies("1.2.3", &VersionReq::Range(">=1.0.0, <2.0.0".to_string())));
    assert!(!version::satisfies("2.0.0", &VersionReq::Range(">=1.0.0, <2.0.0".to_string())));
    assert!(!version::satisfies("1.2.3", &VersionReq::Range(">=1.3.0".to_string())));
    assert!(!version::satisfies("1.bad", &VersionReq::Compatible("1.0.0".to_string())));
    assert!(!version::satisfies("1.2.3", &VersionReq::Compatible("1.bad".to_string())));
    assert!(!version::satisfies("1.bad", &VersionReq::Range(">=1.0.0".to_string())));
    assert!(version::parse_version_req("^1.bad").is_err());
}

#[test]
fn package_manager_resolves_local_path_dependencies() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join("deps/math/src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "0.1.0"

[dependencies.math]
version = "0.1.0"
path = "deps/math"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("deps/math/Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "math"
version = "0.1.0"
"#,
    )
    .unwrap();

    let mut manager = PackageManager::new(root);
    manager.resolve_dependencies().unwrap();

    let math_id = manager.root_dependencies().get("math").expect("root math edge");
    let math = manager.get_resolved().get(math_id).expect("path dependency should resolve");
    assert_eq!(math.name, "math");
    assert_eq!(math.version, "0.1.0");
    assert!(matches!(math.source, PackageSource::Local(_)));
    // Temporary-directory prefixes can be symlinks (for example /var on
    // macOS); dependency resolution returns canonical source paths.
    assert_eq!(manager.get_source_paths(), vec![root.join("deps/math/src").canonicalize().unwrap()]);
}

#[test]
fn package_manager_allows_path_dependency_without_version() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join("deps/math/src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "0.1.0"

[dependencies.math]
path = "deps/math"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("deps/math/Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "math"
version = "0.2.0"
"#,
    )
    .unwrap();

    let mut manager = PackageManager::new(root);
    manager.resolve_dependencies().unwrap();

    let math_id = manager.root_dependencies().get("math").expect("root math edge");
    let math = manager.get_resolved().get(math_id).expect("path dependency should resolve");
    assert_eq!(math.version, "0.2.0");
}

#[test]
fn package_manager_resolves_transitive_local_path_dependencies() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join("deps/math/src")).unwrap();
    std::fs::create_dir_all(root.join("deps/util/src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "0.1.0"

[dependencies.math]
version = "0.1.0"
path = "deps/math"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("deps/math/Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "math"
version = "0.1.0"

[dependencies.util]
version = "0.1.0"
path = "../util"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("deps/util/Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "util"
version = "0.1.0"
"#,
    )
    .unwrap();

    let mut manager = PackageManager::new(root);
    manager.resolve_dependencies().unwrap();

    let math_id = manager.root_dependencies().get("math").expect("root math edge");
    let math = manager.get_resolved().get(math_id).expect("math node");
    let util_id = math.dependencies.get("util").expect("math util edge");
    assert!(manager.get_resolved().contains_key(util_id));
}

#[test]
fn package_manager_rejects_transitive_path_dependency_cycles() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join("deps/a/src")).unwrap();
    std::fs::create_dir_all(root.join("deps/b/src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "0.1.0"

[dependencies.a]
path = "deps/a"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("deps/a/Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "a"
version = "0.1.0"

[dependencies.b]
path = "../b"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("deps/b/Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "b"
version = "0.1.0"

[dependencies.a]
path = "../a"
"#,
    )
    .unwrap();

    let mut manager = PackageManager::new(root);
    let error = manager.resolve_dependencies().unwrap_err();

    assert!(error.message.contains("Circular dependency detected"), "{}", error.message);
    assert!(error.message.contains("a -> b -> a"), "{}", error.message);
}

fn write_test_lock(root: &Path, options: &ResolutionOptions) {
    let mut manager = PackageManager::new(root);
    let manifest = manager.read_manifest().unwrap();
    manager.resolve_dependencies_with_options(options).unwrap();
    let mut lockfile = Lockfile::new();
    lockfile.package = LockfilePackageInfo {
        edition: manifest.package.edition,
        name: manifest.package.name.clone(),
        version: manifest.package.version.clone(),
        namespace: manifest.package.namespace.clone(),
        source_hash: Some(registry::compute_source_hash(root).unwrap()),
        compiler_source_hash: None,
        compiler_requirement: manifest.package.cellscript_version.clone(),
        resolver_compiler_version: crate::VERSION.to_string(),
    };
    lockfile.replace_with_resolution(&manager, &manifest, options).unwrap();
    lockfile.write_to_root(root).unwrap();
}

fn write_path_package(root: &Path, relative: &str, name: &str, version: &str) {
    let package = root.join(relative);
    std::fs::create_dir_all(package.join("src")).unwrap();
    std::fs::write(
        package.join("Cell.toml"),
        format!(
            "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"{name}\"\nversion = \"{version}\"\n"
        ),
    )
    .unwrap();
    std::fs::write(package.join("src/lib.cell"), format!("module {name};\n")).unwrap();
}

#[test]
fn locked_resolution_requires_explicit_lock_and_detects_source_drift() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write_path_package(root, "deps/math", "math", "1.2.3");
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "0.1.0"

[dependencies.math]
path = "deps/math"
version = "^1.2.0"
"#,
    )
    .unwrap();

    let mut manager = PackageManager::new(root);
    let missing = manager.resolve_locked_dependencies(&ResolutionOptions::default()).unwrap_err();
    assert!(missing.message.contains("Cell.lock is missing"), "{}", missing.message);

    write_test_lock(root, &ResolutionOptions::default());
    let mut manager = PackageManager::new(root);
    manager.resolve_locked_dependencies(&ResolutionOptions::default()).unwrap();
    std::fs::write(root.join("deps/math/src/lib.cell"), "module math;\n// changed\n").unwrap();
    let mut manager = PackageManager::new(root);
    let drift = manager.resolve_locked_dependencies(&ResolutionOptions::default()).unwrap_err();
    assert!(drift.message.contains("source hash mismatch"), "{}", drift.message);
}

#[cfg(unix)]
#[test]
fn external_resolver_is_bounded_normalized_and_absent_from_locked_builds() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let root = temp.path();
    let dependency_repo = root.join("resolver-package");
    write_path_package(root, "resolver-package", "resolved_math", "1.2.3");
    for arguments in [
        vec!["init", "-q"],
        vec!["config", "user.email", "tests@cellscript.dev"],
        vec!["config", "user.name", "CellScript Tests"],
        vec!["config", "commit.gpgSign", "false"],
        vec!["add", "."],
        vec!["commit", "-q", "-m", "initial"],
    ] {
        let status = std::process::Command::new("git").args(arguments).current_dir(&dependency_repo).status().unwrap();
        assert!(status.success());
    }
    let revision = PackageManager::git_revision(&dependency_repo).unwrap();
    let response = serde_json::json!({
        "schema": EXTERNAL_RESOLVER_RESPONSE_SCHEMA,
        "dependency": {
            "package": "resolved_math",
            "version": "1.2.3",
            "git": dependency_repo.to_string_lossy(),
            "rev": revision,
        }
    });
    let resolver_path = root.join("resolver.sh");
    // Consume the request before exiting so the fixture cannot race the
    // parent's stdin writes and fail nondeterministically with BrokenPipe.
    std::fs::write(&resolver_path, format!("#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{}'\n", response)).unwrap();
    let mut permissions = std::fs::metadata(&resolver_path).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&resolver_path, permissions).unwrap();
    let resolver_digest = sha256_file(&resolver_path).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        format!(
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "0.1.0"

[resolvers.local]
command = "{}"
sha256 = "sha256:{}"

[dependencies.math]
package = "resolved_math"
version = "^1.2.0"
resolver = "local"
"#,
            resolver_path.display(),
            resolver_digest
        ),
    )
    .unwrap();

    write_test_lock(root, &ResolutionOptions::default());
    let lockfile = Lockfile::read_from_root(root).unwrap().unwrap();
    let target = lockfile.root.dependencies.get("math").unwrap();
    assert!(matches!(lockfile.dependencies[target].source, LockedSource::Git { .. }));

    std::fs::remove_file(&resolver_path).unwrap();
    let mut locked = PackageManager::new(root);
    locked.resolve_locked_dependencies(&ResolutionOptions::default()).unwrap();
    assert_eq!(locked.get_resolved()[target].version, "1.2.3");
}

#[test]
fn moving_git_branch_changes_only_after_explicit_repin() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let dependency_repo = root.join("moving-package");
    write_path_package(root, "moving-package", "moving_math", "1.2.3");
    for arguments in [
        vec!["init", "-q", "--initial-branch=main"],
        vec!["config", "user.email", "tests@cellscript.dev"],
        vec!["config", "user.name", "CellScript Tests"],
        vec!["config", "commit.gpgSign", "false"],
        vec!["add", "."],
        vec!["commit", "-q", "-m", "first"],
    ] {
        let status = std::process::Command::new("git").args(arguments).current_dir(&dependency_repo).status().unwrap();
        assert!(status.success());
    }
    std::fs::write(
        root.join("Cell.toml"),
        format!(
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "0.1.0"

[dependencies.math]
package = "moving_math"
version = "^1.2.0"
git = "{}"
branch = "main"
"#,
            dependency_repo.display()
        ),
    )
    .unwrap();

    write_test_lock(root, &ResolutionOptions::default());
    let first_lock = Lockfile::read_from_root(root).unwrap().unwrap();
    let first_target = first_lock.root.dependencies.get("math").unwrap();
    let first_revision = match &first_lock.dependencies[first_target].source {
        LockedSource::Git { revision, .. } => revision.clone(),
        source => panic!("expected Git source, got {source:?}"),
    };

    std::fs::write(dependency_repo.join("src/lib.cell"), "module moving_math;\n// second commit\n").unwrap();
    for arguments in [vec!["add", "."], vec!["commit", "-q", "-m", "second"]] {
        let status = std::process::Command::new("git").args(arguments).current_dir(&dependency_repo).status().unwrap();
        assert!(status.success());
    }
    let second_revision = PackageManager::git_revision(&dependency_repo).unwrap();
    assert_ne!(first_revision, second_revision);

    let mut locked = PackageManager::new(root);
    locked.resolve_locked_dependencies(&ResolutionOptions::default()).unwrap();
    assert!(locked.get_resolved().contains_key(first_target));

    write_test_lock(root, &ResolutionOptions::default());
    let repinned = Lockfile::read_from_root(root).unwrap().unwrap();
    let repinned_target = repinned.root.dependencies.get("math").unwrap();
    let repinned_revision = match &repinned.dependencies[repinned_target].source {
        LockedSource::Git { revision, .. } => revision,
        source => panic!("expected Git source, got {source:?}"),
    };
    assert_eq!(repinned_revision, &second_revision);
    assert_ne!(repinned_revision, &first_revision);
}

#[test]
fn git_package_rejects_incompatible_compiler_before_source_loading() {
    if std::process::Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let temp = tempdir().unwrap();
    let root = temp.path();
    let dependency_repo = root.join("future-git");
    write_path_package(root, "future-git", "future_git", "1.0.0");
    let manifest_path = dependency_repo.join("Cell.toml");
    let manifest = std::fs::read_to_string(&manifest_path)
        .unwrap()
        .replace("cellscript_version = \">=0.33.0-alpha.1\"", "cellscript_version = \">=999.0.0\"");
    std::fs::write(&manifest_path, manifest).unwrap();
    std::fs::write(dependency_repo.join("src/lib.cell"), "this source must not be parsed").unwrap();
    for arguments in [
        vec!["init", "-q"],
        vec!["config", "user.email", "tests@cellscript.dev"],
        vec!["config", "user.name", "CellScript Tests"],
        vec!["config", "commit.gpgSign", "false"],
        vec!["add", "."],
        vec!["commit", "-q", "-m", "future compiler package"],
    ] {
        let status = std::process::Command::new("git").args(arguments).current_dir(&dependency_repo).status().unwrap();
        assert!(status.success());
    }
    std::fs::write(
        root.join("Cell.toml"),
        format!(
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "1.0.0"

[dependencies.future]
package = "future_git"
git = "{}"
"#,
            dependency_repo.display()
        ),
    )
    .unwrap();

    let error = PackageManager::new(root).resolve_dependencies().unwrap_err();

    assert_eq!(error.code.as_deref(), Some("E2600"));
    let incompatible = &error.details.as_ref().unwrap()["incompatible_packages"][0];
    assert_eq!(incompatible["package"], "future_git");
    assert_eq!(incompatible["incoming_edge"]["from_package"], "app");
    assert_eq!(incompatible["incoming_edge"]["alias"], "future");
    assert!(incompatible["message"].as_str().unwrap().contains("before loading source"));
}

#[test]
fn optional_features_and_dev_dependencies_select_locked_subgraphs() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write_path_package(root, "deps/base", "base", "1.0.0");
    write_path_package(root, "deps/extra", "extra", "1.0.0");
    write_path_package(root, "deps/test-kit", "test-kit", "1.0.0");
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "0.1.0"

[dependencies.base]
path = "deps/base"

[dependencies.extra]
path = "deps/extra"
optional = true

[dev_dependencies.test]
package = "test-kit"
path = "deps/test-kit"

[features]
default = []
extended = ["dep:extra"]
"#,
    )
    .unwrap();
    write_test_lock(root, &ResolutionOptions { scope: DependencyScope::Test, all_features: true, ..ResolutionOptions::default() });

    let mut runtime = PackageManager::new(root);
    runtime.resolve_locked_dependencies(&ResolutionOptions::default()).unwrap();
    assert_eq!(runtime.root_dependencies().keys().cloned().collect::<Vec<_>>(), vec!["base"]);

    let mut extended = PackageManager::new(root);
    extended
        .resolve_locked_dependencies(&ResolutionOptions {
            features: BTreeSet::from(["extended".to_string()]),
            ..ResolutionOptions::default()
        })
        .unwrap();
    assert_eq!(extended.root_dependencies().keys().cloned().collect::<Vec<_>>(), vec!["base", "extra"]);

    let mut tests = PackageManager::new(root);
    tests.resolve_locked_dependencies(&ResolutionOptions { scope: DependencyScope::Test, ..ResolutionOptions::default() }).unwrap();
    assert_eq!(tests.root_dependencies().keys().cloned().collect::<Vec<_>>(), vec!["base", "test"]);
    let test_node = tests.root_dependencies().get("test").unwrap();
    assert_eq!(tests.get_resolved()[test_node].name, "test-kit");
}

#[test]
fn environment_overrides_bind_chain_identity_and_dependency_graph() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write_path_package(root, "deps/mainnet", "contracts", "1.0.0");
    write_path_package(root, "deps/testnet", "contracts", "2.0.0");
    std::fs::write(
        root.join("Cell.toml"),
        format!(
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "0.1.0"

[dependencies.contracts]
path = "deps/mainnet"

[environments.mainnet]
chain_id = "ckb-mainnet"
genesis_hash = "0x{}"

[environments.testnet]
chain_id = "ckb-testnet"
genesis_hash = "0x{}"

[dependency_overrides.testnet.contracts]
path = "deps/testnet"
"#,
            "11".repeat(32),
            "22".repeat(32)
        ),
    )
    .unwrap();

    let manifest = PackageManager::new(root).read_manifest().unwrap();
    let mut lockfile = Lockfile::new();
    lockfile.package.edition = CURRENT_EDITION;
    for environment in manifest.environments.keys() {
        let options = ResolutionOptions {
            environment: Some(environment.clone()),
            scope: DependencyScope::Test,
            all_features: true,
            ..ResolutionOptions::default()
        };
        let mut manager = PackageManager::new(root);
        manager.resolve_dependencies_with_options(&options).unwrap();
        lockfile.merge_resolution(&manager, &manifest, &options).unwrap();
    }
    lockfile.write_to_root(root).unwrap();

    let mut mainnet = PackageManager::new(root);
    mainnet
        .resolve_locked_dependencies(&ResolutionOptions { environment: Some("mainnet".to_string()), ..ResolutionOptions::default() })
        .unwrap();
    let mainnet_node = mainnet.root_dependencies().get("contracts").unwrap();
    assert_eq!(mainnet.get_resolved()[mainnet_node].version, "1.0.0");

    let mut testnet = PackageManager::new(root);
    testnet
        .resolve_locked_dependencies(&ResolutionOptions { environment: Some("testnet".to_string()), ..ResolutionOptions::default() })
        .unwrap();
    let testnet_node = testnet.root_dependencies().get("contracts").unwrap();
    assert_eq!(testnet.get_resolved()[testnet_node].version, "2.0.0");

    let missing = PackageManager::new(root).resolve_locked_dependencies(&ResolutionOptions::default()).unwrap_err();
    assert!(missing.message.contains("--environment"), "{}", missing.message);
}

#[test]
fn transitive_environment_selection_uses_chain_identity_across_local_names_and_a_diamond() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    for (relative, name) in [("deps/left", "left"), ("deps/right", "right"), ("deps/common", "common")] {
        write_path_package(root, relative, name, "1.0.0");
    }
    let genesis = "11".repeat(32);
    std::fs::write(
            root.join("deps/common/Cell.toml"),
            format!(
                "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"common\"\nversion = \"1.0.0\"\n\n[environments.live]\nchain_id = \"ckb\"\ngenesis_hash = \"0x{genesis}\"\n"
            ),
        )
        .unwrap();
    for (relative, name, local_environment) in [("deps/left", "left", "production"), ("deps/right", "right", "ckb-main")] {
        std::fs::write(
                root.join(relative).join("Cell.toml"),
                format!(
                    "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"{name}\"\nversion = \"1.0.0\"\n\n[dependencies.common]\npath = \"../common\"\n\n[environments.{local_environment}]\nchain_id = \"ckb\"\ngenesis_hash = \"0x{genesis}\"\n"
                ),
            )
            .unwrap();
    }
    std::fs::write(
            root.join("Cell.toml"),
            format!(
                "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies.left]\npath = \"deps/left\"\nuse_environment = \"production\"\n\n[dependencies.right]\npath = \"deps/right\"\n\n[environments.mainnet]\nchain_id = \"ckb\"\ngenesis_hash = \"0x{genesis}\"\n"
            ),
        )
        .unwrap();

    let options = ResolutionOptions { environment: Some("mainnet".to_string()), ..ResolutionOptions::default() };
    let mut manager = PackageManager::new(root);
    manager.resolve_dependencies_with_options(&options).unwrap();
    let left = manager.root_dependencies()["left"].clone();
    let right = manager.root_dependencies()["right"].clone();
    let left_common = manager.get_resolved()[&left].dependencies["common"].clone();
    let right_common = manager.get_resolved()[&right].dependencies["common"].clone();
    assert_eq!(left_common, right_common);
    assert!(left.contains("explicit-local-name"), "{left}");
    assert!(right.contains("inherit-by-chain-identity"), "{right}");
    assert!(left_common.contains("inherit-by-chain-identity"), "{left_common}");

    let manifest = manager.read_manifest().unwrap();
    let mut lockfile = Lockfile::new();
    lockfile.package.edition = CURRENT_EDITION;
    lockfile.replace_with_resolution(&manager, &manifest, &options).unwrap();
    lockfile.write_to_root(root).unwrap();
    let mut locked = PackageManager::new(root);
    locked.resolve_locked_dependencies(&options).unwrap();
    assert_eq!(locked.get_resolved()[&left].dependencies["common"], left_common);
    assert_eq!(locked.get_resolved()[&right].dependencies["common"], right_common);
}

#[test]
fn transitive_environment_mismatch_and_ambiguity_fail_before_overrides_apply() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    for (relative, name) in [("deps/child", "child"), ("deps/base", "base"), ("deps/alternate", "base")] {
        write_path_package(root, relative, name, "1.0.0");
    }
    std::fs::write(
            root.join("deps/child/Cell.toml"),
            format!(
                "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"child\"\nversion = \"1.0.0\"\n\n[dependencies.base]\npath = \"../base\"\n\n[environments.mainnet]\nchain_id = \"ckb\"\ngenesis_hash = \"0x{}\"\n\n[dependency_overrides.mainnet.base]\npath = \"../alternate\"\n",
                "22".repeat(32)
            ),
        )
        .unwrap();
    std::fs::write(
            root.join("Cell.toml"),
            format!(
                "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies.child]\npath = \"deps/child\"\n\n[environments.mainnet]\nchain_id = \"ckb\"\ngenesis_hash = \"0x{}\"\n",
                "11".repeat(32)
            ),
        )
        .unwrap();
    let options = ResolutionOptions { environment: Some("mainnet".to_string()), ..ResolutionOptions::default() };
    let mismatch = PackageManager::new(root).resolve_dependencies_with_options(&options).unwrap_err();
    assert!(mismatch.message.contains("no environment matches root chain identity"), "{}", mismatch.message);

    std::fs::write(
            root.join("deps/child/Cell.toml"),
            format!(
                "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"child\"\nversion = \"1.0.0\"\n\n[dependencies.base]\npath = \"../base\"\n\n[environments.mainnet]\nchain_id = \"ckb-fork\"\ngenesis_hash = \"0x{}\"\n\n[dependency_overrides.mainnet.base]\npath = \"../alternate\"\n",
                "11".repeat(32)
            ),
        )
        .unwrap();
    let wrong_chain = PackageManager::new(root).resolve_dependencies_with_options(&options).unwrap_err();
    assert!(wrong_chain.message.contains("no environment matches root chain identity"), "{}", wrong_chain.message);

    std::fs::write(
            root.join("deps/child/Cell.toml"),
            format!(
                "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"child\"\nversion = \"1.0.0\"\n\n[environments.one]\nchain_id = \"ckb\"\ngenesis_hash = \"0x{0}\"\n\n[environments.two]\nchain_id = \"ckb\"\ngenesis_hash = \"0x{0}\"\n",
                "11".repeat(32)
            ),
        )
        .unwrap();
    let ambiguous = PackageManager::new(root).resolve_dependencies_with_options(&options).unwrap_err();
    assert!(ambiguous.message.contains("multiple environments matching root chain identity"), "{}", ambiguous.message);
}

#[test]
fn explicit_environment_independence_skips_foreign_overrides_but_preserves_the_root_identity() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write_path_package(root, "deps/child", "child", "1.0.0");
    write_path_package(root, "deps/base", "base", "1.0.0");
    write_path_package(root, "deps/alternate", "base", "2.0.0");
    std::fs::write(
            root.join("deps/child/Cell.toml"),
            format!(
                "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"child\"\nversion = \"1.0.0\"\n\n[dependencies.base]\npath = \"../base\"\n\n[environments.foreign]\nchain_id = \"ckb-fork\"\ngenesis_hash = \"0x{}\"\n\n[dependency_overrides.foreign.base]\npath = \"../alternate\"\n",
                "22".repeat(32)
            ),
        )
        .unwrap();
    std::fs::write(
            root.join("Cell.toml"),
            format!(
                "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies.child]\npath = \"deps/child\"\nenvironment_independent = true\n\n[environments.mainnet]\nchain_id = \"ckb\"\ngenesis_hash = \"0x{}\"\n",
                "11".repeat(32)
            ),
        )
        .unwrap();
    let options = ResolutionOptions { environment: Some("mainnet".to_string()), ..ResolutionOptions::default() };
    let mut manager = PackageManager::new(root);
    manager.resolve_dependencies_with_options(&options).unwrap();
    let child = &manager.get_resolved()[&manager.root_dependencies()["child"]];
    assert!(child.node_id.contains("environment-independent"), "{}", child.node_id);
    assert_eq!(manager.get_resolved()[&child.dependencies["base"]].version, "1.0.0");
}

#[test]
fn locked_environment_mapping_rejects_a_rebound_dependency_manifest() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    write_path_package(root, "deps/child", "child", "1.0.0");
    let genesis = "11".repeat(32);
    std::fs::write(
            root.join("deps/child/Cell.toml"),
            format!(
                "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"child\"\nversion = \"1.0.0\"\n\n[environments.production]\nchain_id = \"ckb\"\ngenesis_hash = \"0x{genesis}\"\n"
            ),
        )
        .unwrap();
    std::fs::write(
            root.join("Cell.toml"),
            format!(
                "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies.child]\npath = \"deps/child\"\n\n[environments.mainnet]\nchain_id = \"ckb\"\ngenesis_hash = \"0x{genesis}\"\n"
            ),
        )
        .unwrap();
    let options = ResolutionOptions { environment: Some("mainnet".to_string()), ..ResolutionOptions::default() };
    write_test_lock(root, &options);
    let mut lockfile = Lockfile::read_from_root(root).unwrap().unwrap();
    let node_id = lockfile.environments["mainnet"].dependencies["child"].clone();
    std::fs::write(
            root.join("deps/child/Cell.toml"),
            format!(
                "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"child\"\nversion = \"1.0.0\"\n\n[environments.renamed]\nchain_id = \"ckb\"\ngenesis_hash = \"0x{genesis}\"\n"
            ),
        )
        .unwrap();
    lockfile.dependencies.get_mut(&node_id).unwrap().manifest_digest = compute_manifest_digest(&root.join("deps/child")).unwrap();
    lockfile.dependencies.get_mut(&node_id).unwrap().source_hash =
        Some(registry::compute_source_hash(&root.join("deps/child")).unwrap());
    lockfile.write_to_root(root).unwrap();

    let error = PackageManager::new(root).resolve_locked_dependencies(&options).unwrap_err();
    assert!(error.message.contains("chain-identity-safe environment selection requires"), "{}", error.message);
}

#[cfg(unix)]
#[test]
fn transitive_external_resolver_uses_validated_identity_without_inherited_name_lookup() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let owner = temp.path().join("owner");
    std::fs::create_dir_all(&owner).unwrap();
    let response = serde_json::json!({
        "schema": EXTERNAL_RESOLVER_RESPONSE_SCHEMA,
        "dependency": { "package": "math", "version": "1.2.3", "namespace": "cellscript" }
    });
    let resolver_path = owner.join("resolver.sh");
    let request_path = owner.join("request.json");
    std::fs::write(&resolver_path, format!("#!/bin/sh\ncat >'{}'\nprintf '%s\\n' '{}'\n", request_path.display(), response)).unwrap();
    let mut permissions = std::fs::metadata(&resolver_path).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&resolver_path, permissions).unwrap();
    let digest = sha256_file(&resolver_path).unwrap();
    std::fs::write(
            owner.join("Cell.toml"),
            format!(
                "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"owner\"\nversion = \"1.0.0\"\n\n[resolvers.local]\ncommand = \"{}\"\nsha256 = \"sha256:{digest}\"\n\n[dependencies.math]\nversion = \"^1.2.0\"\nresolver = \"local\"\n",
                resolver_path.display()
            ),
        )
        .unwrap();
    let manifest: PackageManifest = toml::from_str(&std::fs::read_to_string(owner.join("Cell.toml")).unwrap()).unwrap();
    let Dependency::Detailed(dependency) = &manifest.dependencies["math"] else {
        panic!("expected detailed dependency");
    };
    let environment = SelectedEnvironmentContext {
        root_name: "mainnet".to_string(),
        local_name: Some("production".to_string()),
        chain_id: "ckb".to_string(),
        genesis_hash: format!("0x{}", "11".repeat(32)),
        policy: EnvironmentSelectionPolicy::InheritByChainIdentity,
    };
    // Executable resolvers are a root-manifest authority boundary: run the
    // identity checks with the resolver owner as the package root.
    let resolved = PackageManager::new(&owner)
        .resolve_external_dependency("math", "math", dependency, &owner, &ResolutionOptions::default(), Some(&environment))
        .unwrap();
    assert_eq!(resolved.version, "=1.2.3");
    assert_eq!(resolved.namespace.as_deref(), Some("cellscript"));
    let request: serde_json::Value = serde_json::from_slice(&std::fs::read(request_path).unwrap()).unwrap();
    assert_eq!(request["schema"], EXTERNAL_RESOLVER_REQUEST_SCHEMA);
    assert_eq!(request["environment"]["root_name"], "mainnet");
    assert_eq!(request["environment"]["local_name"], "production");
    assert_eq!(request["environment"]["chain_id"], "ckb");
    assert_eq!(request["environment"]["genesis_hash"], format!("0x{}", "11".repeat(32)));
}

#[test]
fn build_dependencies_fail_closed_until_isolated_execution_exists() {
    let manifest: PackageManifest = toml::from_str(
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "0.1.0"

[build.dependencies]
codegen = "1.0.0"
"#,
    )
    .unwrap();
    let temp = tempdir().unwrap();
    PackageManager::new(temp.path()).write_manifest(&manifest).unwrap();
    let error = PackageManager::new(temp.path()).resolve_dependencies().unwrap_err();
    assert!(error.message.contains("reserved"), "{}", error.message);
}

#[test]
fn lockfile_consistency_reports_stale_and_mismatched_path_sources() {
    let manifest: PackageManifest = toml::from_str(
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "0.1.0"

[dependencies.math]
version = "0.1.0"
path = "deps/math"
"#,
    )
    .unwrap();
    let mut lockfile = Lockfile::new();
    lockfile.root.dependencies.insert("math".to_string(), "math-node".to_string());
    lockfile.dependencies.insert("math-node".to_string(), locked_path("math", "0.2.0", "deps/old-math", BTreeMap::new()));
    lockfile.dependencies.insert(
        "stale-node".to_string(),
        LockedDependency {
            name: "stale".to_string(),
            namespace: Some("stale".to_string()),
            version: "1.0.0".to_string(),
            source: LockedSource::Registry {
                registry: "cellscript-registry".to_string(),
                url: "https://github.com/example/stale".to_string(),
                revision: "abc123".to_string(),
                namespace: "stale".to_string(),
                version: "1.0.0".to_string(),
            },
            source_hash: Some("hash-stale".to_string()),
            manifest_digest: "manifest-stale".to_string(),
            dependencies: BTreeMap::new(),
            build: None,
            compiler_requirement: "*".to_string(),
            resolver_compiler_version: crate::VERSION.to_string(),
        },
    );

    let issues = lockfile.consistency_issues(&manifest);

    assert!(issues.iter().any(|issue| issue.contains("expects path source 'deps/math'")), "{issues:?}");
    assert!(issues.iter().any(|issue| issue.contains("requires '0.1.0'")), "{issues:?}");
    assert!(issues.iter().any(|issue| issue.contains("unreachable dependency node 'stale-node'")), "{issues:?}");
    assert!(!lockfile.is_consistent(&manifest));
}

#[test]
fn lockfile_consistency_allows_resolved_transitive_path_dependencies() {
    let manifest: PackageManifest = toml::from_str(
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "0.1.0"

[dependencies.math]
version = "0.1.0"
path = "deps/math"
"#,
    )
    .unwrap();
    let mut lockfile = Lockfile::new();
    lockfile.package.compiler_requirement = manifest.package.cellscript_version.clone();
    lockfile.root.dependencies.insert("math".to_string(), "math-node".to_string());
    let math_edges = BTreeMap::from([("util".to_string(), "util-node".to_string())]);
    lockfile.dependencies.insert("math-node".to_string(), locked_path("math", "0.1.0", "deps/math", math_edges.clone()));
    lockfile.dependencies.insert("util-node".to_string(), locked_path("util", "0.1.0", "deps/math/../util", BTreeMap::new()));
    let mut resolved = BTreeMap::new();
    resolved.insert("math-node".to_string(), resolved_path("math", "0.1.0", "deps/math", math_edges));
    resolved.insert("util-node".to_string(), resolved_path("util", "0.1.0", "deps/math/../util", BTreeMap::new()));

    let issues = lockfile.consistency_issues_with_resolved(&manifest, &resolved);

    assert!(issues.is_empty(), "{issues:?}");
}

#[test]
fn lockfile_replace_with_resolved_prunes_removed_dependencies() {
    let mut lockfile = Lockfile::new();
    lockfile.dependencies.insert(
        "old".to_string(),
        LockedDependency {
            name: "old".to_string(),
            namespace: Some("old".to_string()),
            version: "1.0.0".to_string(),
            source: LockedSource::Registry {
                registry: "cellscript-registry".to_string(),
                url: "https://github.com/example/old".to_string(),
                revision: "def456".to_string(),
                namespace: "old".to_string(),
                version: "1.0.0".to_string(),
            },
            source_hash: Some("hash-old".to_string()),
            manifest_digest: "manifest-old".to_string(),
            dependencies: BTreeMap::new(),
            build: None,
            compiler_requirement: "*".to_string(),
            resolver_compiler_version: crate::VERSION.to_string(),
        },
    );

    let mut resolved = BTreeMap::new();
    resolved.insert("math".to_string(), resolved_path("math", "0.1.0", "deps/math", BTreeMap::new()));

    lockfile.replace_with_resolved(&resolved);

    assert!(lockfile.dependencies.contains_key("math"));
    assert!(!lockfile.dependencies.contains_key("old"));
}

#[test]
fn lockfile_read_from_root_rejects_malformed_lockfiles() {
    let temp = tempdir().unwrap();
    std::fs::write(temp.path().join("Cell.lock"), "not = [valid").unwrap();

    let error = Lockfile::read_from_root(temp.path()).unwrap_err();

    assert!(error.message.contains("failed to parse lockfile"), "{}", error.message);
}

#[test]
fn lockfile_requires_the_single_package_coordinate_resolver_model() {
    let mut lockfile = Lockfile::new();
    assert_eq!(lockfile.version, 5);
    assert_eq!(lockfile.resolver_model, Lockfile::CURRENT_RESOLVER_MODEL);

    lockfile.resolver_model = "multi-package-coordinate-v1".to_string();
    let error = lockfile.validate_schema().unwrap_err();
    assert_eq!(error.code.as_deref(), Some("E2601"));
    assert!(error.message.contains("unsupported Cell.lock resolver model"), "{}", error.message);
}

#[test]
fn lockfile_requires_package_and_build_profile_identity() {
    let missing_package = toml::from_str::<Lockfile>(
        r#"
version = 3
schema = "cellscript-lock-v0.24-graph-v1"
resolver_model = "legacy"

[root]

[dependencies]
"#,
    )
    .unwrap_err();
    assert!(missing_package.to_string().contains("missing field `package`"));

    let missing_profile = toml::from_str::<Lockfile>(
        r#"
version = 3
schema = "cellscript-lock-v0.24-graph-v1"
resolver_model = "legacy"

[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"

[root]

[package_build]
edition = "2027"

[dependencies]
"#,
    )
    .unwrap_err();
    assert!(missing_profile.to_string().contains("missing field `compatibility_profile_hash`"));
}

#[test]
fn package_manager_rejects_registry_dependencies_fail_closed() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "0.1.0"

[dependencies]
remote = "1.2.3"
"#,
    )
    .unwrap();

    let mut manager = PackageManager::new(temp.path());
    let error = manager.resolve_dependencies().unwrap_err();

    // Registry dependencies require a namespace — without one, fail closed
    assert!(error.message.contains("namespace") || error.message.contains("registry"), "{}", error.message);
    assert!(manager.get_resolved().is_empty());
}

#[test]
fn package_manager_git_dependency_fails_for_invalid_url() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "0.1.0"

[dependencies.remote]
version = "0.1.0"
git = "https://example.invalid/remote.git"
rev = "abc123"
"#,
    )
    .unwrap();

    let mut manager = PackageManager::new(temp.path());
    let error = manager.resolve_dependencies().unwrap_err();

    assert!(error.message.contains("remote"));
    assert!(error.message.contains("https://example.invalid/remote.git"));
    assert!(manager.get_resolved().is_empty());
}

#[test]
fn deployed_manifest_round_trip() {
    let manifest = DeployedManifest {
        version: DeployedManifest::CURRENT_VERSION,
        schema: DEPLOYED_MANIFEST_SCHEMA.to_string(),
        package: DeployedPackageInfo {
            edition: CURRENT_EDITION,
            name: "amm_pool".to_string(),
            version: "1.2.0".to_string(),
            source_hash: Some("blake2b:0xabcd".to_string()),
        },
        build: Some(DeployedBuildInfo {
            edition: CURRENT_EDITION,
            compatibility_profile_hash: "test-compatibility-profile".to_string(),
            compiler_version: Some("0.19.0".to_string()),
            artifact_hash: Some("blake2b:0x1234".to_string()),
            metadata_hash: None,
            schema_hash: None,
            cell_data_codec_manifest_hash: None,
            abi_hash: None,
            constraints_hash: None,
        }),
        deployments: vec![DeploymentRecord {
            edition: CURRENT_EDITION,
            compatibility_profile_hash: "test-compatibility-profile".to_string(),
            network: "aggron4".to_string(),
            chain_id: "ckb-testnet".to_string(),
            tx_hash: "0xaaaa".to_string(),
            output_index: 0,
            code_hash: "0xbbbb".to_string(),
            hash_type: "data1".to_string(),
            dep_type: "code".to_string(),
            data_hash: "0xcccc".to_string(),
            out_point: "0xaaaa:0".to_string(),
            artifact_hash: None,
            metadata_hash: None,
            schema_hash: None,
            cell_data_codec_manifest_hash: None,
            abi_hash: None,
            constraints_hash: None,
            compiler_version: None,
            type_id: Some("0xdddd".to_string()),
            script_role: Some(ScriptRole::Type),
            status: Some(DeploymentStatus::Candidate),
            upgrade_lineage: None,
            audit_report_hash: None,
            publisher_signature: None,
            cell_deps: vec![DeploymentCellDep {
                name: Some("secp256k1".to_string()),
                tx_hash: "0xeeee".to_string(),
                output_index: 1,
                dep_type: "dep_group".to_string(),
                hash_type: Some("type".to_string()),
                data_hash: None,
                type_id: None,
            }],
        }],
    };

    let toml_str = toml::to_string_pretty(&manifest).unwrap();
    assert!(toml_str.contains("network = \"aggron4\""));
    assert!(toml_str.contains("code_hash = \"0xbbbb\""));

    let parsed: DeployedManifest = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.version, DeployedManifest::CURRENT_VERSION);
    assert_eq!(parsed.package.name, "amm_pool");
    assert_eq!(parsed.deployments.len(), 1);
    assert_eq!(parsed.deployments[0].network, "aggron4");
    assert_eq!(parsed.deployments[0].cell_deps.len(), 1);
}

#[test]
fn deployed_manifest_rejects_legacy_identity() {
    let toml_str = r#"
version = 1

[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "token"
version = "0.3.0"

[[deployments]]
network = "ckb-mainnet"
chain_id = "ckb-mainnet"
tx_hash = "0x1111"
output_index = 0
code_hash = "0x2222"
hash_type = "type"
dep_type = "code"
data_hash = "0x3333"
        out_point = "0x1111:0"
"#;
    let error = toml::from_str::<DeployedManifest>(toml_str).unwrap_err();
    assert!(error.to_string().contains("missing field"));
}

#[test]
fn deployed_manifest_requires_profile_identity() {
    let toml_str = r#"
version = 2
schema = "cellscript-deployed-v2"

[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "token"
version = "0.3.0"

[[deployments]]
edition = "2027"
network = "ckb-mainnet"
chain_id = "ckb-mainnet"
tx_hash = "0x1111"
output_index = 0
code_hash = "0x2222"
hash_type = "type"
dep_type = "code"
data_hash = "0x3333"
out_point = "0x1111:0"
"#;
    let error = toml::from_str::<DeployedManifest>(toml_str).unwrap_err();
    assert!(error.to_string().contains("missing field `compatibility_profile_hash`"));
}
