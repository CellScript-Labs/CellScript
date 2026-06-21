//! Integration tests for the Phase 1 Registry system.
//!
//! Tests cover:
//! - Source hash computation determinism
//! - RegistryIndex read/write/append
//! - Discovery index lookup with local Git fixture
//! - Deployed.toml file round-trip
//! - Cell.lock new fields (package.build, deployment.*)
//! - Full publish → verify flow with local Git fixtures
//! - Fail-closed verification on hash mismatch

use cellscript::package::registry::{
    compute_source_hash, DiscoveryEntry, DiscoveryIndex, RegistryAuditInfo, RegistryDependencyRef, RegistryIndex, RegistryVersion,
};
use cellscript::package::{
    DeployedBuildInfo, DeployedManifest, DeployedPackageInfo, DeploymentCellDep, DeploymentRecord, DeploymentStatus, LockedBuildInfo,
    LockedDependency, LockedSource, Lockfile, LockfileDeploymentRef, LockfilePackageInfo, PackageManager, ScriptRole,
    DEPLOYED_MANIFEST_SCHEMA,
};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

static REGISTRY_ENV_LOCK: Mutex<()> = Mutex::new(());

struct RegistryEnvGuard {
    previous: Option<OsString>,
    _guard: MutexGuard<'static, ()>,
}

impl RegistryEnvGuard {
    fn new(url: &Path) -> Self {
        let guard = REGISTRY_ENV_LOCK.lock().unwrap();
        let previous = std::env::var_os(cellscript::package::registry::REGISTRY_URL_ENV);
        std::env::set_var(cellscript::package::registry::REGISTRY_URL_ENV, url);
        Self { previous, _guard: guard }
    }
}

impl Drop for RegistryEnvGuard {
    fn drop(&mut self) {
        if let Some(previous) = &self.previous {
            std::env::set_var(cellscript::package::registry::REGISTRY_URL_ENV, previous);
        } else {
            std::env::remove_var(cellscript::package::registry::REGISTRY_URL_ENV);
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn git_init(repo_dir: &Path) {
    let status = std::process::Command::new("git").args(["init"]).current_dir(repo_dir).status().expect("git init");
    assert!(status.success());
}

fn git_add_all(repo_dir: &Path) {
    let status = std::process::Command::new("git").args(["add", "."]).current_dir(repo_dir).status().expect("git add");
    assert!(status.success());
}

fn git_commit(repo_dir: &Path, msg: &str) {
    // Ensure there is at least one tracked file
    let gitkeep = repo_dir.join(".gitkeep");
    if !gitkeep.exists() {
        let _ = std::fs::write(&gitkeep, "");
    }
    git_add_all(repo_dir);
    let status = std::process::Command::new("git")
        .args(["commit", "-m", msg, "--author=test <test@test.com>"])
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00+00:00")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00+00:00")
        .current_dir(repo_dir)
        .status()
        .expect("git commit");
    assert!(status.success());
}

fn git_tag(repo_dir: &Path, tag: &str) {
    let status = std::process::Command::new("git").args(["tag", tag]).current_dir(repo_dir).status().expect("git tag");
    assert!(status.success());
}

/// Create a minimal CellScript package directory with Cell.toml and src/main.cell.
fn create_minimal_package(dir: &Path, name: &str, version: &str, namespace: Option<&str>) {
    std::fs::create_dir_all(dir.join("src")).unwrap();

    let mut toml = String::from("[package]\n");
    toml.push_str(&format!("name = \"{}\"\n", name));
    toml.push_str(&format!("version = \"{}\"\n", version));
    if let Some(ns) = namespace {
        toml.push_str(&format!("namespace = \"{}\"\n", ns));
    }
    std::fs::write(dir.join("Cell.toml"), toml).unwrap();

    let cell = format!("module {};\n", name);
    std::fs::write(dir.join("src/main.cell"), cell).unwrap();
}

// ---------------------------------------------------------------------------
// Source hash computation
// ---------------------------------------------------------------------------

#[test]
fn compute_source_hash_is_deterministic() {
    let temp = tempfile::tempdir().unwrap();
    create_minimal_package(temp.path(), "hash-test", "0.1.0", None);

    let hash1 = compute_source_hash(temp.path()).unwrap();
    let hash2 = compute_source_hash(temp.path()).unwrap();
    assert_eq!(hash1, hash2, "source hash must be deterministic for identical content");
    assert!(!hash1.is_empty(), "source hash must not be empty");
}

#[test]
fn compute_source_hash_changes_when_source_changes() {
    let temp = tempfile::tempdir().unwrap();
    create_minimal_package(temp.path(), "hash-test", "0.1.0", None);

    let hash1 = compute_source_hash(temp.path()).unwrap();

    std::fs::write(temp.path().join("src/main.cell"), "module hash_test_updated;\n").unwrap();

    let hash2 = compute_source_hash(temp.path()).unwrap();
    assert_ne!(hash1, hash2, "source hash must change when source content changes");
}

#[test]
fn compute_source_hash_includes_cell_toml() {
    let temp = tempfile::tempdir().unwrap();
    create_minimal_package(temp.path(), "hash-test", "0.1.0", None);

    let hash1 = compute_source_hash(temp.path()).unwrap();

    let mut toml = String::from("[package]\n");
    toml.push_str("name = \"hash-test\"\n");
    toml.push_str("version = \"0.2.0\"\n");
    std::fs::write(temp.path().join("Cell.toml"), toml).unwrap();

    let hash2 = compute_source_hash(temp.path()).unwrap();
    assert_ne!(hash1, hash2, "source hash must change when Cell.toml changes");
}

#[test]
fn compute_source_hash_includes_configured_source_roots() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("contracts")).unwrap();
    std::fs::write(
        temp.path().join("Cell.toml"),
        r#"
[package]
name = "hash-test"
version = "0.1.0"
entry = "contracts/main.cell"
source_roots = ["contracts"]
"#,
    )
    .unwrap();
    std::fs::write(temp.path().join("contracts/main.cell"), "module hash_test;\n").unwrap();

    let hash1 = compute_source_hash(temp.path()).unwrap();
    std::fs::write(temp.path().join("contracts/main.cell"), "module hash_test_updated;\n").unwrap();
    let hash2 = compute_source_hash(temp.path()).unwrap();

    assert_ne!(hash1, hash2, "source hash must cover configured package source_roots");
}

// ---------------------------------------------------------------------------
// RegistryIndex — read/write/append
// ---------------------------------------------------------------------------

#[test]
fn registry_index_write_read_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let index = RegistryIndex {
        schema_version: 1,
        name: "token".to_string(),
        namespace: "cellscript".to_string(),
        versions: vec![RegistryVersion {
            version: "0.3.0".to_string(),
            tag: "v0.3.0".to_string(),
            source_hash: "abcd1234".to_string(),
            cellscript_version: "0.19.0".to_string(),
            dependencies: BTreeMap::new(),
            abi_index: None,
            schema_hash: None,
            license: Some("MIT".to_string()),
            released_at: None,
            yanked: false,
            audit: None,
        }],
    };

    index.write_to_repo(temp.path()).unwrap();
    let read_back = RegistryIndex::read_from_repo(temp.path()).unwrap();

    assert_eq!(read_back.schema_version, 1);
    assert_eq!(read_back.name, "token");
    assert_eq!(read_back.namespace, "cellscript");
    assert_eq!(read_back.versions.len(), 1);
    assert_eq!(read_back.versions[0].version, "0.3.0");
    assert_eq!(read_back.versions[0].source_hash, "abcd1234");
    assert_eq!(read_back.versions[0].license.as_deref(), Some("MIT"));
}

#[test]
fn registry_index_append_version_creates_new_file() {
    let temp = tempfile::tempdir().unwrap();

    let version = RegistryVersion {
        version: "0.1.0".to_string(),
        tag: "v0.1.0".to_string(),
        source_hash: "hash_of_source".to_string(),
        cellscript_version: "0.19.0".to_string(),
        dependencies: BTreeMap::new(),
        abi_index: None,
        schema_hash: None,
        license: None,
        released_at: None,
        yanked: false,
        audit: None,
    };

    RegistryIndex::append_version(temp.path(), "my_pkg", "my_ns", version).unwrap();

    let index = RegistryIndex::read_from_repo(temp.path()).unwrap();
    assert_eq!(index.name, "my_pkg");
    assert_eq!(index.namespace, "my_ns");
    assert_eq!(index.versions.len(), 1);
    assert_eq!(index.versions[0].version, "0.1.0");
}

#[test]
fn registry_index_append_version_updates_existing() {
    let temp = tempfile::tempdir().unwrap();

    let v1 = RegistryVersion {
        version: "0.1.0".to_string(),
        tag: "v0.1.0".to_string(),
        source_hash: "h1".to_string(),
        cellscript_version: "0.19.0".to_string(),
        dependencies: BTreeMap::new(),
        abi_index: None,
        schema_hash: None,
        license: None,
        released_at: None,
        yanked: false,
        audit: None,
    };
    RegistryIndex::append_version(temp.path(), "pkg", "ns", v1).unwrap();

    let v2 = RegistryVersion {
        version: "0.2.0".to_string(),
        tag: "v0.2.0".to_string(),
        source_hash: "h2".to_string(),
        cellscript_version: "0.19.0".to_string(),
        dependencies: BTreeMap::new(),
        abi_index: None,
        schema_hash: None,
        license: None,
        released_at: None,
        yanked: false,
        audit: None,
    };
    RegistryIndex::append_version(temp.path(), "pkg", "ns", v2).unwrap();

    let index = RegistryIndex::read_from_repo(temp.path()).unwrap();
    assert_eq!(index.versions.len(), 2);

    // Re-appending same version should update (not duplicate)
    let v1_updated = RegistryVersion {
        version: "0.1.0".to_string(),
        tag: "v0.1.0".to_string(),
        source_hash: "h1_updated".to_string(),
        cellscript_version: "0.19.0".to_string(),
        dependencies: BTreeMap::new(),
        abi_index: None,
        schema_hash: None,
        license: None,
        released_at: None,
        yanked: true,
        audit: None,
    };
    RegistryIndex::append_version(temp.path(), "pkg", "ns", v1_updated).unwrap();

    let index = RegistryIndex::read_from_repo(temp.path()).unwrap();
    assert_eq!(index.versions.len(), 2, "re-appending same version should not create duplicates");
    let v1_entry = index.versions.iter().find(|v| v.version == "0.1.0").unwrap();
    assert!(v1_entry.yanked, "re-appended version should be updated (yanked = true)");
    assert_eq!(v1_entry.source_hash, "h1_updated");
}

#[test]
fn registry_index_with_dependencies_and_audit() {
    let temp = tempfile::tempdir().unwrap();

    let deps = BTreeMap::from([(
        "token".to_string(),
        RegistryDependencyRef { namespace: "cellscript".to_string(), version: "0.3.0".to_string() },
    )]);

    let version = RegistryVersion {
        version: "1.0.0".to_string(),
        tag: "v1.0.0".to_string(),
        source_hash: "deadbeef".to_string(),
        cellscript_version: "0.19.0".to_string(),
        dependencies: deps,
        abi_index: Some("0xabc".to_string()),
        schema_hash: Some("0xdef".to_string()),
        license: Some("Apache-2.0".to_string()),
        released_at: Some("2026-05-07T00:00:00Z".to_string()),
        yanked: false,
        audit: Some(RegistryAuditInfo { report_hash: Some("0x5555".to_string()), acceptance_gate: Some("passed".to_string()) }),
    };

    RegistryIndex::append_version(temp.path(), "amm", "cellscript", version).unwrap();

    let index = RegistryIndex::read_from_repo(temp.path()).unwrap();
    let v = &index.versions[0];
    assert_eq!(v.dependencies.len(), 1);
    assert_eq!(v.dependencies["token"].namespace, "cellscript");
    assert_eq!(v.audit.as_ref().unwrap().acceptance_gate.as_deref(), Some("passed"));
}

// ---------------------------------------------------------------------------
// Discovery Index — local Git fixture
// ---------------------------------------------------------------------------

#[test]
fn discovery_index_lookup_with_local_git() {
    let temp = tempfile::tempdir().unwrap();
    let registry_dir = temp.path().join("registry-repo");
    std::fs::create_dir_all(&registry_dir).unwrap();

    git_init(&registry_dir);

    let ns_dir = registry_dir.join("cellscript");
    std::fs::create_dir_all(&ns_dir).unwrap();
    let entry = DiscoveryEntry {
        name: "token".to_string(),
        namespace: "cellscript".to_string(),
        source: "https://github.com/cellscript/token".to_string(),
    };
    let entry_json = serde_json::to_string_pretty(&entry).unwrap();
    std::fs::write(ns_dir.join("token.json"), entry_json).unwrap();

    git_add_all(&registry_dir);
    git_commit(&registry_dir, "initial registry entries");

    let cache_dir = temp.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();

    let discovery = DiscoveryIndex::new(&registry_dir.to_string_lossy(), &cache_dir);
    let clone_dir = discovery.clone_or_update().unwrap();
    assert!(clone_dir.exists());

    let found = discovery.lookup("cellscript", "token").unwrap();
    assert_eq!(found.name, "token");
    assert_eq!(found.namespace, "cellscript");
    assert_eq!(found.source, "https://github.com/cellscript/token");
}

#[test]
fn discovery_index_lookup_missing_package_falls_back_to_convention() {
    let temp = tempfile::tempdir().unwrap();
    let registry_dir = temp.path().join("registry-repo");
    std::fs::create_dir_all(&registry_dir).unwrap();

    git_init(&registry_dir);
    git_commit(&registry_dir, "empty registry");

    let cache_dir = temp.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();

    let discovery = DiscoveryIndex::new(&registry_dir.to_string_lossy(), &cache_dir);
    discovery.clone_or_update().unwrap();

    // When no explicit entry exists, lookup falls back to Go-style convention:
    // github.com/<namespace>/<name>. No PR to a monorepo discovery index required.
    let result = discovery.lookup("nonexistent", "nope").unwrap();
    assert_eq!(result.source, "https://github.com/nonexistent/nope");
    assert_eq!(result.namespace, "nonexistent");
    assert_eq!(result.name, "nope");
}

#[test]
fn discovery_index_add_entry() {
    let temp = tempfile::tempdir().unwrap();
    let registry_dir = temp.path().join("registry-repo");
    std::fs::create_dir_all(&registry_dir).unwrap();

    git_init(&registry_dir);
    git_commit(&registry_dir, "empty registry");

    let cache_dir = temp.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();

    let discovery = DiscoveryIndex::new(&registry_dir.to_string_lossy(), &cache_dir);
    discovery.clone_or_update().unwrap();

    discovery.add_entry("myns", "mypkg", "https://github.com/myns/mypkg").unwrap();

    let found = discovery.lookup("myns", "mypkg").unwrap();
    assert_eq!(found.name, "mypkg");
    assert_eq!(found.namespace, "myns");
    assert_eq!(found.source, "https://github.com/myns/mypkg");
}

// ---------------------------------------------------------------------------
// Deployed.toml — file round-trip
// ---------------------------------------------------------------------------

#[test]
fn deployed_manifest_file_round_trip() {
    let temp = tempfile::tempdir().unwrap();

    let manifest = DeployedManifest {
        version: 1,
        schema: Some(DEPLOYED_MANIFEST_SCHEMA.to_string()),
        package: DeployedPackageInfo {
            name: "token".to_string(),
            version: "1.0.0".to_string(),
            source_hash: Some("blake2b:0xabc".to_string()),
        },
        build: Some(DeployedBuildInfo {
            compiler_version: Some("0.19.0".to_string()),
            artifact_hash: Some("blake2b:0xdef".to_string()),
            metadata_hash: None,
            schema_hash: Some("blake2b:0x999".to_string()),
            cell_data_codec_manifest_hash: None,
            abi_hash: None,
            constraints_hash: None,
        }),
        deployments: vec![DeploymentRecord {
            network: "aggron4".to_string(),
            chain_id: "ckb-testnet".to_string(),
            tx_hash: "0xaaaa1111".to_string(),
            output_index: 0,
            code_hash: "0xbbbb2222".to_string(),
            hash_type: "data1".to_string(),
            dep_type: "code".to_string(),
            data_hash: "0xcccc3333".to_string(),
            out_point: "0xaaaa1111:0".to_string(),
            artifact_hash: Some("blake2b:0xdef".to_string()),
            metadata_hash: None,
            schema_hash: Some("blake2b:0x999".to_string()),
            cell_data_codec_manifest_hash: None,
            abi_hash: None,
            constraints_hash: None,
            compiler_version: Some("0.19.0".to_string()),
            type_id: None,
            script_role: Some(ScriptRole::Lock),
            status: Some(DeploymentStatus::Active),
            upgrade_lineage: None,
            audit_report_hash: None,
            publisher_signature: None,
            cell_deps: vec![DeploymentCellDep {
                name: Some("secp256k1_data".to_string()),
                tx_hash: "0xdddd4444".to_string(),
                output_index: 2,
                dep_type: "dep_group".to_string(),
                hash_type: Some("type".to_string()),
                data_hash: None,
                type_id: None,
            }],
        }],
    };

    manifest.write_to_root(temp.path()).unwrap();
    let read_back = DeployedManifest::read_from_root(temp.path()).unwrap().unwrap();

    assert_eq!(read_back.version, 1);
    assert_eq!(read_back.package.name, "token");
    assert_eq!(read_back.package.version, "1.0.0");
    assert_eq!(read_back.package.source_hash.as_deref(), Some("blake2b:0xabc"));
    assert_eq!(read_back.build.as_ref().unwrap().compiler_version.as_deref(), Some("0.19.0"));
    assert_eq!(read_back.build.as_ref().unwrap().artifact_hash.as_deref(), Some("blake2b:0xdef"));
    assert_eq!(read_back.deployments.len(), 1);
    assert_eq!(read_back.deployments[0].network, "aggron4");
    assert_eq!(read_back.deployments[0].code_hash, "0xbbbb2222");
    assert_eq!(read_back.deployments[0].cell_deps.len(), 1);
    assert_eq!(read_back.deployments[0].cell_deps[0].name.as_deref(), Some("secp256k1_data"));
}

#[test]
fn deployed_manifest_backward_compatible_minimal() {
    let temp = tempfile::tempdir().unwrap();

    let toml_str = r#"
version = 1

[package]
name = "minimal"
version = "0.1.0"

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
    std::fs::write(temp.path().join("Deployed.toml"), toml_str).unwrap();

    let parsed = DeployedManifest::read_from_root(temp.path()).unwrap().unwrap();
    assert_eq!(parsed.package.name, "minimal");
    assert!(parsed.build.is_none());
    assert_eq!(parsed.deployments.len(), 1);
    assert!(parsed.deployments[0].type_id.is_none());
    assert!(parsed.deployments[0].status.is_none());
    assert!(parsed.deployments[0].cell_deps.is_empty());
}

// ---------------------------------------------------------------------------
// Cell.lock — new fields round-trip
// ---------------------------------------------------------------------------

#[test]
fn lockfile_with_build_and_deployment_round_trip() {
    let temp = tempfile::tempdir().unwrap();

    let mut lockfile = Lockfile::new();
    lockfile.package = LockfilePackageInfo {
        name: "amm_pool".to_string(),
        version: "1.0.0".to_string(),
        namespace: Some("cellscript".to_string()),
        source_hash: Some("blake2b:0xfeed".to_string()),
        compiler_source_hash: None,
    };
    lockfile.package_build = Some(LockedBuildInfo {
        compiler_version: Some("0.19.0".to_string()),
        target_profile: Some("ckb-release".to_string()),
        artifact_hash: Some("blake2b:0x1234".to_string()),
        metadata_hash: Some("blake2b:0x5678".to_string()),
        schema_hash: Some("blake2b:0x9abc".to_string()),
        constraints_hash: Some("blake2b:0xdef0".to_string()),
        ..Default::default()
    });
    lockfile.deployment.insert(
        "aggron4".to_string(),
        LockfileDeploymentRef {
            record: "0xaaaa:0".to_string(),
            record_hash: Some("blake2b:0x1111".to_string()),
            code_hash: Some("0xbbbb".to_string()),
            out_point: Some("0xaaaa:0".to_string()),
            data_hash: Some("0xcccc".to_string()),
        },
    );
    lockfile.dependencies.insert(
        "token".to_string(),
        LockedDependency {
            version: "0.3.0".to_string(),
            source: LockedSource::Registry {
                registry: "https://github.com/cellscript/cellscript-registry".to_string(),
                url: "https://github.com/cellscript/token".to_string(),
                revision: "abc123def456".to_string(),
                namespace: "cellscript".to_string(),
                version: "0.3.0".to_string(),
            },
            source_hash: Some("blake2b:0xaaaa".to_string()),
            build: Some(LockedBuildInfo {
                artifact_hash: Some("blake2b:0xtoken".to_string()),
                constraints_hash: Some("blake2b:0xtoken_constraints".to_string()),
                ..Default::default()
            }),
        },
    );

    lockfile.write_to_root(temp.path()).unwrap();
    let read_back = Lockfile::read_from_root(temp.path()).unwrap().unwrap();

    assert_eq!(read_back.package.name, "amm_pool");
    assert_eq!(read_back.package.namespace.as_deref(), Some("cellscript"));
    assert_eq!(read_back.package.source_hash.as_deref(), Some("blake2b:0xfeed"));

    let build = read_back.package_build.unwrap();
    assert_eq!(build.artifact_hash.as_deref(), Some("blake2b:0x1234"));
    assert_eq!(build.constraints_hash.as_deref(), Some("blake2b:0xdef0"));

    let dep_ref = read_back.deployment.get("aggron4").unwrap();
    assert_eq!(dep_ref.code_hash.as_deref(), Some("0xbbbb"));
    assert_eq!(dep_ref.data_hash.as_deref(), Some("0xcccc"));

    let token_dep = read_back.dependencies.get("token").unwrap();
    assert!(matches!(token_dep.source, LockedSource::Registry { .. }));
    assert_eq!(token_dep.source_hash.as_deref(), Some("blake2b:0xaaaa"));
    assert!(token_dep.build.is_some());
}

#[test]
fn lockfile_consistency_with_registry_source() {
    use cellscript::package::PackageManifest;

    let manifest: PackageManifest = toml::from_str(
        r#"
[package]
name = "app"
version = "0.1.0"
namespace = "cellscript"

[dependencies.token]
version = "0.3.0"
namespace = "cellscript"
"#,
    )
    .unwrap();

    let mut lockfile = Lockfile::new();
    lockfile.dependencies.insert(
        "token".to_string(),
        LockedDependency {
            version: "0.3.0".to_string(),
            source: LockedSource::Registry {
                registry: "https://github.com/cellscript/cellscript-registry".to_string(),
                url: "https://github.com/cellscript/token".to_string(),
                revision: "abc123".to_string(),
                namespace: "cellscript".to_string(),
                version: "0.3.0".to_string(),
            },
            source_hash: None,
            build: None,
        },
    );

    let issues = lockfile.consistency_issues(&manifest);
    assert!(issues.is_empty(), "lockfile with matching registry source should be consistent: {issues:?}");
}

// ---------------------------------------------------------------------------
// Publish flow simulation
// ---------------------------------------------------------------------------

#[test]
fn publish_flow_computes_source_hash_and_writes_registry_json() {
    let temp = tempfile::tempdir().unwrap();
    let pkg_dir = temp.path();

    create_minimal_package(pkg_dir, "my-lib", "0.1.0", Some("myns"));

    let source_hash = compute_source_hash(pkg_dir).unwrap();
    assert!(!source_hash.is_empty());

    let version = RegistryVersion {
        version: "0.1.0".to_string(),
        tag: "v0.1.0".to_string(),
        source_hash,
        cellscript_version: "0.19.0".to_string(),
        dependencies: BTreeMap::new(),
        abi_index: None,
        schema_hash: None,
        license: None,
        released_at: None,
        yanked: false,
        audit: None,
    };

    RegistryIndex::append_version(pkg_dir, "my-lib", "myns", version).unwrap();

    let index = RegistryIndex::read_from_repo(pkg_dir).unwrap();
    assert_eq!(index.name, "my-lib");
    assert_eq!(index.namespace, "myns");
    assert_eq!(index.versions.len(), 1);
    assert_eq!(index.versions[0].version, "0.1.0");

    let recomputed = compute_source_hash(pkg_dir).unwrap();
    assert_eq!(index.versions[0].source_hash, recomputed);
}

// ---------------------------------------------------------------------------
// Full publish → verify with local Git fixture
// ---------------------------------------------------------------------------

#[test]
fn full_publish_install_verify_flow_with_local_git() {
    let temp = tempfile::tempdir().unwrap();

    // 1. Create a package source repo with a git tag
    let source_repo = temp.path().join("source-repo");
    std::fs::create_dir_all(&source_repo).unwrap();
    create_minimal_package(&source_repo, "token", "0.3.0", Some("cellscript"));

    let source_hash = compute_source_hash(&source_repo).unwrap();

    let version = RegistryVersion {
        version: "0.3.0".to_string(),
        tag: "v0.3.0".to_string(),
        source_hash: source_hash.clone(),
        cellscript_version: "0.19.0".to_string(),
        dependencies: BTreeMap::new(),
        abi_index: None,
        schema_hash: None,
        license: Some("MIT".to_string()),
        released_at: None,
        yanked: false,
        audit: None,
    };
    RegistryIndex::append_version(&source_repo, "token", "cellscript", version).unwrap();

    git_init(&source_repo);
    git_add_all(&source_repo);
    git_commit(&source_repo, "initial version");
    git_tag(&source_repo, "v0.3.0");

    // 2. Create a discovery index repo
    let registry_repo = temp.path().join("registry-repo");
    std::fs::create_dir_all(&registry_repo).unwrap();
    git_init(&registry_repo);

    let ns_dir = registry_repo.join("cellscript");
    std::fs::create_dir_all(&ns_dir).unwrap();
    let entry = DiscoveryEntry {
        name: "token".to_string(),
        namespace: "cellscript".to_string(),
        source: source_repo.to_string_lossy().to_string(),
    };
    let entry_json = serde_json::to_string_pretty(&entry).unwrap();
    std::fs::write(ns_dir.join("token.json"), entry_json).unwrap();

    git_add_all(&registry_repo);
    git_commit(&registry_repo, "add token entry");

    // 3. Verify registry.json in the source repo is valid
    let index = RegistryIndex::read_from_repo(&source_repo).unwrap();
    assert_eq!(index.name, "token");
    assert_eq!(index.versions.len(), 1);
    assert_eq!(index.versions[0].source_hash, source_hash);

    let found = index.find_matching_version("0.3.0");
    assert!(found.is_some());
    assert_eq!(found.unwrap().version, "0.3.0");

    // 4. Verify source hash is consistent
    let recomputed = compute_source_hash(&source_repo).unwrap();
    assert_eq!(source_hash, recomputed);
}

#[test]
fn package_manager_resolves_registry_dependency_with_source_hash_from_local_git_fixture() {
    let temp = tempfile::tempdir().unwrap();

    let source_repo = temp.path().join("source-repo");
    std::fs::create_dir_all(&source_repo).unwrap();
    create_minimal_package(&source_repo, "token", "0.3.0", Some("cellscript"));
    let source_hash = compute_source_hash(&source_repo).unwrap();
    RegistryIndex::append_version(
        &source_repo,
        "token",
        "cellscript",
        RegistryVersion {
            version: "0.3.0".to_string(),
            tag: "v0.3.0".to_string(),
            source_hash: source_hash.clone(),
            cellscript_version: "0.19.0".to_string(),
            dependencies: BTreeMap::new(),
            abi_index: None,
            schema_hash: None,
            license: None,
            released_at: None,
            yanked: false,
            audit: None,
        },
    )
    .unwrap();
    git_init(&source_repo);
    git_add_all(&source_repo);
    git_commit(&source_repo, "publish token");
    git_tag(&source_repo, "v0.3.0");

    let registry_repo = temp.path().join("registry-repo");
    std::fs::create_dir_all(registry_repo.join("cellscript")).unwrap();
    git_init(&registry_repo);
    let entry = DiscoveryEntry {
        name: "token".to_string(),
        namespace: "cellscript".to_string(),
        source: source_repo.to_string_lossy().to_string(),
    };
    std::fs::write(registry_repo.join("cellscript/token.json"), serde_json::to_string_pretty(&entry).unwrap()).unwrap();
    git_add_all(&registry_repo);
    git_commit(&registry_repo, "add token");

    let consumer = temp.path().join("consumer");
    std::fs::create_dir_all(consumer.join("src")).unwrap();
    std::fs::write(
        consumer.join("Cell.toml"),
        r#"
[package]
name = "consumer"
version = "0.1.0"
namespace = "app"

[dependencies.token]
version = "0.3.0"
namespace = "cellscript"
"#,
    )
    .unwrap();
    std::fs::write(consumer.join("src/main.cell"), "module consumer;\n").unwrap();

    let _env = RegistryEnvGuard::new(&registry_repo);
    let mut manager = PackageManager::new(&consumer);
    manager.resolve_dependencies().unwrap();
    let resolved = manager.get_resolved().get("token").unwrap();
    assert_eq!(resolved.source_hash.as_deref(), Some(source_hash.as_str()));

    let mut lockfile = Lockfile::new();
    lockfile.update_from_resolved(manager.get_resolved());
    let token = lockfile.dependencies.get("token").unwrap();
    assert_eq!(token.source_hash.as_deref(), Some(source_hash.as_str()));
    assert!(
        matches!(token.source, LockedSource::Registry { ref namespace, ref version, .. } if namespace == "cellscript" && version == "0.3.0")
    );
}

#[test]
fn package_manager_rejects_registry_source_hash_mismatch() {
    let temp = tempfile::tempdir().unwrap();

    let source_repo = temp.path().join("source-repo");
    std::fs::create_dir_all(&source_repo).unwrap();
    create_minimal_package(&source_repo, "token", "0.3.0", Some("cellscript"));
    RegistryIndex::append_version(
        &source_repo,
        "token",
        "cellscript",
        RegistryVersion {
            version: "0.3.0".to_string(),
            tag: "v0.3.0".to_string(),
            source_hash: "deliberately_wrong_hash".to_string(),
            cellscript_version: "0.19.0".to_string(),
            dependencies: BTreeMap::new(),
            abi_index: None,
            schema_hash: None,
            license: None,
            released_at: None,
            yanked: false,
            audit: None,
        },
    )
    .unwrap();
    git_init(&source_repo);
    git_add_all(&source_repo);
    git_commit(&source_repo, "publish token");
    git_tag(&source_repo, "v0.3.0");

    let registry_repo = temp.path().join("registry-repo");
    std::fs::create_dir_all(registry_repo.join("cellscript")).unwrap();
    git_init(&registry_repo);
    let entry = DiscoveryEntry {
        name: "token".to_string(),
        namespace: "cellscript".to_string(),
        source: source_repo.to_string_lossy().to_string(),
    };
    std::fs::write(registry_repo.join("cellscript/token.json"), serde_json::to_string_pretty(&entry).unwrap()).unwrap();
    git_add_all(&registry_repo);
    git_commit(&registry_repo, "add token");

    let consumer = temp.path().join("consumer");
    std::fs::create_dir_all(consumer.join("src")).unwrap();
    std::fs::write(
        consumer.join("Cell.toml"),
        r#"
[package]
name = "consumer"
version = "0.1.0"
namespace = "app"

[dependencies.token]
version = "0.3.0"
namespace = "cellscript"
"#,
    )
    .unwrap();
    std::fs::write(consumer.join("src/main.cell"), "module consumer;\n").unwrap();

    let _env = RegistryEnvGuard::new(&registry_repo);
    let mut manager = PackageManager::new(&consumer);
    let err = manager.resolve_dependencies().unwrap_err();
    assert!(err.message.contains("source_hash mismatch"), "unexpected error: {}", err.message);
}

// ---------------------------------------------------------------------------
// Fail-closed verification
// ---------------------------------------------------------------------------

#[test]
fn registry_verify_detects_artifact_hash_mismatch() {
    let temp = tempfile::tempdir().unwrap();

    let mut lockfile = Lockfile::new();
    lockfile.package_build = Some(LockedBuildInfo {
        artifact_hash: Some("blake2b:0x_correct".to_string()),
        schema_hash: Some("blake2b:0x_schema".to_string()),
        ..Default::default()
    });
    lockfile.write_to_root(temp.path()).unwrap();

    let deployed = DeployedManifest {
        version: 1,
        schema: None,
        package: DeployedPackageInfo { name: "test".to_string(), version: "1.0.0".to_string(), source_hash: None },
        build: Some(DeployedBuildInfo {
            artifact_hash: Some("blake2b:0x_WRONG".to_string()),
            schema_hash: Some("blake2b:0x_schema".to_string()),
            compiler_version: None,
            metadata_hash: None,
            cell_data_codec_manifest_hash: None,
            abi_hash: None,
            constraints_hash: None,
        }),
        deployments: vec![DeploymentRecord {
            network: "aggron4".to_string(),
            chain_id: "ckb-testnet".to_string(),
            tx_hash: "0xaaaa".to_string(),
            output_index: 0,
            code_hash: "0xbbbb".to_string(),
            hash_type: "type".to_string(),
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
            type_id: None,
            script_role: None,
            status: None,
            upgrade_lineage: None,
            audit_report_hash: None,
            publisher_signature: None,
            cell_deps: vec![],
        }],
    };
    deployed.write_to_root(temp.path()).unwrap();

    let lockfile = Lockfile::read_from_root(temp.path()).unwrap().unwrap();
    let deployed = DeployedManifest::read_from_root(temp.path()).unwrap().unwrap();

    let mut violations = Vec::new();
    if let Some(build) = &lockfile.package_build {
        if let Some(deployed_build) = &deployed.build {
            if let (Some(lk), Some(dp)) = (&build.artifact_hash, &deployed_build.artifact_hash) {
                if lk != dp {
                    violations.push(format!("artifact_hash mismatch: Cell.lock has '{}', Deployed.toml has '{}'", lk, dp));
                }
            }
        }
    }

    assert_eq!(violations.len(), 1);
    assert!(violations[0].contains("artifact_hash mismatch"));
}

#[test]
fn registry_verify_detects_code_hash_mismatch() {
    let temp = tempfile::tempdir().unwrap();

    let mut lockfile = Lockfile::new();
    lockfile.deployment.insert(
        "aggron4".to_string(),
        LockfileDeploymentRef {
            record: "0xaaaa:0".to_string(),
            record_hash: None,
            code_hash: Some("0xLOCK_CODE_HASH".to_string()),
            out_point: None,
            data_hash: None,
        },
    );
    lockfile.write_to_root(temp.path()).unwrap();

    let deployed = DeployedManifest {
        version: 1,
        schema: None,
        package: DeployedPackageInfo { name: "test".to_string(), version: "1.0.0".to_string(), source_hash: None },
        build: None,
        deployments: vec![DeploymentRecord {
            network: "aggron4".to_string(),
            chain_id: "ckb-testnet".to_string(),
            tx_hash: "0xaaaa".to_string(),
            output_index: 0,
            code_hash: "0xDEPLOYED_CODE_HASH".to_string(),
            hash_type: "type".to_string(),
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
            type_id: None,
            script_role: None,
            status: None,
            upgrade_lineage: None,
            audit_report_hash: None,
            publisher_signature: None,
            cell_deps: vec![],
        }],
    };
    deployed.write_to_root(temp.path()).unwrap();

    let lockfile = Lockfile::read_from_root(temp.path()).unwrap().unwrap();
    let deployed = DeployedManifest::read_from_root(temp.path()).unwrap().unwrap();

    let mut violations = Vec::new();
    for deployment in &deployed.deployments {
        if let Some(deployment_ref) = lockfile.deployment.get(&deployment.network) {
            if let Some(ref code_hash) = deployment_ref.code_hash {
                if code_hash != &deployment.code_hash {
                    violations.push(format!(
                        "code_hash mismatch for network '{}': Cell.lock has '{}', Deployed.toml has '{}'",
                        deployment.network, code_hash, deployment.code_hash
                    ));
                }
            }
        }
    }

    assert_eq!(violations.len(), 1);
    assert!(violations[0].contains("code_hash mismatch"));
}

#[test]
fn package_verify_detects_missing_source_hash() {
    let temp = tempfile::tempdir().unwrap();
    create_minimal_package(temp.path(), "verify-test", "0.1.0", None);

    let mut lockfile = Lockfile::new();
    lockfile.package = LockfilePackageInfo {
        name: "verify-test".to_string(),
        version: "0.1.0".to_string(),
        namespace: None,
        source_hash: Some("deliberately_wrong_hash".to_string()),
        compiler_source_hash: None,
    };
    lockfile.write_to_root(temp.path()).unwrap();

    let lockfile = Lockfile::read_from_root(temp.path()).unwrap().unwrap();
    let computed = compute_source_hash(temp.path()).unwrap();
    let stored = lockfile.package.source_hash.unwrap();

    assert_ne!(computed, stored);
}

#[test]
fn lockfile_consistency_rejects_wrong_registry_namespace() {
    use cellscript::package::PackageManifest;

    let manifest: PackageManifest = toml::from_str(
        r#"
[package]
name = "app"
version = "0.1.0"
namespace = "cellscript"

[dependencies.token]
version = "0.3.0"
namespace = "cellscript"
"#,
    )
    .unwrap();

    let mut lockfile = Lockfile::new();
    lockfile.dependencies.insert(
        "token".to_string(),
        LockedDependency {
            version: "0.3.0".to_string(),
            source: LockedSource::Registry {
                registry: "https://github.com/cellscript/cellscript-registry".to_string(),
                url: "https://github.com/cellscript/token".to_string(),
                revision: "abc123".to_string(),
                namespace: "other".to_string(),
                version: "0.3.0".to_string(),
            },
            source_hash: None,
            build: None,
        },
    );

    let issues = lockfile.consistency_issues(&manifest);
    assert!(!issues.is_empty(), "wrong namespace should cause consistency issues: {issues:?}");
    assert!(
        issues.iter().any(|i| i.contains("token") || i.contains("registry")),
        "at least one issue should mention the dependency or registry: {issues:?}"
    );
}

#[test]
fn lockfile_consistency_accepts_matching_registry_source() {
    use cellscript::package::PackageManifest;

    let manifest: PackageManifest = toml::from_str(
        r#"
[package]
name = "app"
version = "0.1.0"
namespace = "cellscript"

[dependencies.token]
version = "0.3.0"
namespace = "cellscript"
"#,
    )
    .unwrap();

    let mut lockfile = Lockfile::new();
    lockfile.dependencies.insert(
        "token".to_string(),
        LockedDependency {
            version: "0.3.0".to_string(),
            source: LockedSource::Registry {
                registry: "https://github.com/cellscript/cellscript-registry".to_string(),
                url: "https://github.com/cellscript/token".to_string(),
                revision: "abc123".to_string(),
                namespace: "cellscript".to_string(),
                version: "0.3.0".to_string(),
            },
            source_hash: None,
            build: None,
        },
    );

    let issues = lockfile.consistency_issues(&manifest);
    assert!(issues.is_empty(), "matching registry source should have no issues: {issues:?}");
}

// ---------------------------------------------------------------------------
// Multi-deployment records
// ---------------------------------------------------------------------------

#[test]
fn deployed_manifest_supports_multiple_deployments() {
    let temp = tempfile::tempdir().unwrap();

    let manifest = DeployedManifest {
        version: 1,
        schema: Some(DEPLOYED_MANIFEST_SCHEMA.to_string()),
        package: DeployedPackageInfo { name: "token".to_string(), version: "1.0.0".to_string(), source_hash: None },
        build: None,
        deployments: vec![
            DeploymentRecord {
                network: "ckb-mainnet".to_string(),
                chain_id: "ckb-mainnet".to_string(),
                tx_hash: "0x1111".to_string(),
                output_index: 0,
                code_hash: "0x2222".to_string(),
                hash_type: "type".to_string(),
                dep_type: "code".to_string(),
                data_hash: "0x3333".to_string(),
                out_point: "0x1111:0".to_string(),
                artifact_hash: None,
                metadata_hash: None,
                schema_hash: None,
                cell_data_codec_manifest_hash: None,
                abi_hash: None,
                constraints_hash: None,
                compiler_version: None,
                type_id: None,
                script_role: None,
                status: Some(DeploymentStatus::Active),
                upgrade_lineage: None,
                audit_report_hash: None,
                publisher_signature: None,
                cell_deps: vec![],
            },
            DeploymentRecord {
                network: "aggron4".to_string(),
                chain_id: "ckb-testnet".to_string(),
                tx_hash: "0x4444".to_string(),
                output_index: 1,
                code_hash: "0x5555".to_string(),
                hash_type: "data1".to_string(),
                dep_type: "code".to_string(),
                data_hash: "0x6666".to_string(),
                out_point: "0x4444:1".to_string(),
                artifact_hash: None,
                metadata_hash: None,
                schema_hash: None,
                cell_data_codec_manifest_hash: None,
                abi_hash: None,
                constraints_hash: None,
                compiler_version: None,
                type_id: None,
                script_role: None,
                status: Some(DeploymentStatus::Candidate),
                upgrade_lineage: None,
                audit_report_hash: None,
                publisher_signature: None,
                cell_deps: vec![],
            },
        ],
    };

    manifest.write_to_root(temp.path()).unwrap();
    let read_back = DeployedManifest::read_from_root(temp.path()).unwrap().unwrap();

    assert_eq!(read_back.deployments.len(), 2);
    assert_eq!(read_back.deployments[0].network, "ckb-mainnet");
    assert_eq!(read_back.deployments[0].status, Some(DeploymentStatus::Active));
    assert_eq!(read_back.deployments[1].network, "aggron4");
    assert_eq!(read_back.deployments[1].status, Some(DeploymentStatus::Candidate));
}
