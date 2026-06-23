use crate::error::{CompileError, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

pub mod registry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    pub package: PackageInfo,
    #[serde(default)]
    pub workspace: Option<WorkspaceConfig>,
    #[serde(default)]
    pub dependencies: HashMap<String, Dependency>,
    #[serde(default)]
    pub dev_dependencies: HashMap<String, Dependency>,
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default)]
    pub policy: PolicyConfig,
    #[serde(default)]
    pub deploy: DeployConfig,
    #[serde(default)]
    pub metadata: HashMap<String, toml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub repository: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub documentation: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub cellscript_version: String,
    #[serde(default = "default_entry")]
    pub entry: String,
    #[serde(default)]
    pub source_roots: Vec<String>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

fn default_entry() -> String {
    "src/main.cell".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// A virtual manifest that contains only a `[workspace]` section with no `[package]`.
/// This represents a workspace root that is not itself a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    pub workspace: WorkspaceConfig,
}

impl WorkspaceManifest {
    pub fn read_from_dir(dir: &Path) -> Result<Option<Self>> {
        let manifest_path = dir.join("Cell.toml");
        if !manifest_path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| CompileError::without_span(format!("failed to read '{}': {}", manifest_path.display(), e)))?;
        // Try parsing as a workspace-only manifest first.
        let ws: std::result::Result<WorkspaceManifest, _> = toml::from_str(&content);
        if let Ok(manifest) = ws {
            // Make sure it really has no [package] section — if it does,
            // the caller should use PackageManifest instead.
            if !content.contains("[package]") {
                return Ok(Some(manifest));
            }
        }
        Ok(None)
    }

    pub fn write_to_dir(&self, dir: &Path) -> Result<()> {
        let manifest_path = dir.join("Cell.toml");
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&manifest_path, content)?;
        Ok(())
    }

    /// Resolve member paths relative to the workspace root directory.
    pub fn resolve_member_paths(&self, root: &Path) -> Result<Vec<PathBuf>> {
        let mut members = Vec::new();
        for member_pattern in &self.workspace.members {
            let member_path = root.join(member_pattern);
            if member_path.is_dir() && member_path.join("Cell.toml").exists() {
                members.push(canonical_path(&member_path)?);
            } else {
                return Err(CompileError::without_span(format!(
                    "workspace member '{}' does not exist or is not a valid package directory",
                    member_pattern
                )));
            }
        }
        Ok(members)
    }
}

fn canonical_path(path: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(path).map_err(|e| CompileError::without_span(format!("failed to canonicalize '{}': {}", path.display(), e)))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    Simple(String),
    Detailed(DetailedDependency),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedDependency {
    #[serde(default = "default_any_version")]
    pub version: String,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub git: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub rev: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default = "default_true")]
    pub default_features: bool,
}

fn default_true() -> bool {
    true
}

fn default_any_version() -> String {
    "*".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildConfig {
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub target_profile: Option<String>,
    #[serde(default)]
    pub out_dir: Option<String>,
    #[serde(default)]
    pub dependencies: HashMap<String, Dependency>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyConfig {
    #[serde(default)]
    pub production: bool,
    #[serde(default)]
    pub deny_fail_closed: bool,
    #[serde(default)]
    pub deny_ckb_runtime: bool,
    #[serde(default)]
    pub deny_runtime_obligations: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeployConfig {
    #[serde(default)]
    pub ckb: Option<CkbDeployConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CkbDeployConfig {
    #[serde(default)]
    pub artifact_hash: Option<String>,
    #[serde(default)]
    pub data_hash: Option<String>,
    #[serde(default)]
    pub out_point: Option<String>,
    #[serde(default)]
    pub dep_type: Option<String>,
    #[serde(default)]
    pub hash_type: Option<String>,
    #[serde(default)]
    pub type_id: Option<String>,
    #[serde(default)]
    pub cell_deps: Vec<CkbCellDepConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CkbCellDepConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub out_point: Option<String>,
    #[serde(default)]
    pub tx_hash: Option<String>,
    #[serde(default)]
    pub index: Option<u32>,
    #[serde(default)]
    pub dep_type: Option<String>,
    #[serde(default)]
    pub data_hash: Option<String>,
    #[serde(default)]
    pub hash_type: Option<String>,
    #[serde(default)]
    pub type_id: Option<String>,
}

pub struct PackageManager {
    root: PathBuf,
    resolved: HashMap<String, ResolvedPackage>,
}

#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub source: PackageSource,
    pub dependencies: Vec<String>,
    pub namespace: Option<String>,
    pub source_hash: Option<String>,
}

/// Emit yank-related notices to stderr during registry resolution.
///
/// The registry resolver never silently picks a yanked version for a range
/// request (yanked entries are filtered out by `find_matching_version`). A
/// yanked version can only be reached when the caller pins it explicitly (for
/// example via an `=x.y.z` exact requirement or a lockfile that names it). In
/// that case we warn and suggest the latest non-yanked version, honouring the
/// Phase 1 contract that resolving a yanked version is surfaced to the user
/// rather than failing or passing silently.
fn emit_yank_notices(namespace: &str, name: &str, requested: &str, selected: &str, index: &registry::RegistryIndex) {
    let Some(entry) = index.versions.iter().find(|v| v.version == selected) else {
        return;
    };
    if !entry.yanked {
        return;
    }
    // Prefer the publisher-declared replacement (`replaced_by`) when present;
    // otherwise fall back to the latest non-yanked version.
    let suggestion = entry.replaced_by.clone().or_else(|| {
        index.versions.iter().filter(|v| !v.yanked && v.version != selected).map(|v| v.version.clone()).max_by(|a, b| {
            let a_parts = parse_numeric_version(a);
            let b_parts = parse_numeric_version(b);
            compare_version_tuples(&a_parts, &b_parts)
        })
    });
    let reason = entry.yanked_reason.as_deref().map(|r| format!(" (reason: {})", r)).unwrap_or_default();
    match suggestion {
        Some(v) => eprintln!(
            "warning: {}/{}@{} resolves to yanked version {}{}; consider upgrading to {}",
            namespace, name, requested, selected, reason, v
        ),
        None => eprintln!(
            "warning: {}/{}@{} resolves to yanked version {}{} with no non-yanked alternative published",
            namespace, name, requested, selected, reason
        ),
    }
}

fn registry_resolution_blocked_error(
    namespace: &str,
    name: &str,
    requested: &str,
    version: &registry::RegistryVersion,
    policy: registry::RegistryResolutionPolicy,
) -> CompileError {
    let reason = version
        .resolver_block_reason(policy, matches!(crate::package::version::parse_version_req(requested), Ok(VersionReq::Exact(_))));
    let status = version.effective_status();
    let hint = match reason {
        Some("unverified") => "use --allow-unverified for an explicit direct install, or wait until the entry reaches verified_build",
        Some("quarantined") => "use --allow-quarantined only for an explicit incident-review install",
        Some("deprecated") => "pin the version exactly or select a non-deprecated replacement",
        Some("yanked") => "pin the version exactly or select a non-yanked replacement",
        _ => "select a version that is eligible for default registry resolution",
    };

    CompileError::without_span(format!(
        "registry package '{}/{}@{}' matched version '{}' but status '{}' is not eligible for default resolution; {}",
        namespace,
        name,
        requested,
        version.version,
        status.as_str(),
        hint
    ))
}

fn parse_numeric_version(version: &str) -> Vec<u32> {
    let core = version.split_once('-').map(|(c, _)| c).unwrap_or(version);
    core.split('.').filter_map(|p| p.parse().ok()).collect()
}

fn compare_version_tuples(a: &[u32], b: &[u32]) -> std::cmp::Ordering {
    let max_len = a.len().max(b.len());
    for i in 0..max_len {
        match a.get(i).cmp(&b.get(i)) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

#[derive(Debug, Clone)]
pub enum PackageSource {
    Local(PathBuf),
    Git { url: String, revision: String },
    Registry { registry: String, url: String, revision: String, namespace: String, version: String },
}

#[derive(Debug, Clone)]
pub enum VersionReq {
    Exact(String),
    Compatible(String),
    Range(String),
    Any,
}

impl PackageManager {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();

        Self { root, resolved: HashMap::new() }
    }

    pub fn read_manifest(&self) -> Result<PackageManifest> {
        let manifest_path = self.root.join("Cell.toml");

        if !manifest_path.exists() {
            return Err(CompileError::without_span("Cell.toml not found. Run 'cellc init' to create a new package."));
        }

        let content = std::fs::read_to_string(&manifest_path)?;
        let manifest: PackageManifest = toml::from_str(&content)?;

        Ok(manifest)
    }

    pub fn write_manifest(&self, manifest: &PackageManifest) -> Result<()> {
        let manifest_path = self.root.join("Cell.toml");
        let content = toml::to_string_pretty(manifest)?;
        std::fs::write(&manifest_path, content)?;
        Ok(())
    }

    pub fn init(&self, name: &str) -> Result<()> {
        self.init_with_entry(
            name,
            "src/main.cell",
            format!(
                r#"module {};

// Entry point for {}
"#,
                name, name
            ),
        )
    }

    pub fn init_library(&self, name: &str) -> Result<()> {
        self.init_with_entry(name, "src/lib.cell", format!("module {};\n", name))
    }

    fn init_with_entry(&self, name: &str, entry: &str, entry_content: String) -> Result<()> {
        std::fs::create_dir_all(self.root.join("src"))?;
        std::fs::create_dir_all(self.root.join("tests"))?;
        std::fs::create_dir_all(self.root.join("examples"))?;

        let manifest = PackageManifest {
            package: PackageInfo {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                namespace: None,
                authors: vec![],
                description: String::new(),
                license: String::new(),
                repository: String::new(),
                homepage: String::new(),
                documentation: String::new(),
                keywords: vec![],
                categories: vec![],
                cellscript_version: String::new(),
                entry: entry.to_string(),
                source_roots: vec![],
                include: vec![],
                exclude: vec![],
            },
            workspace: None,
            dependencies: HashMap::new(),
            dev_dependencies: HashMap::new(),
            build: BuildConfig::default(),
            policy: PolicyConfig::default(),
            deploy: DeployConfig::default(),
            metadata: HashMap::new(),
        };

        self.write_manifest(&manifest)?;
        std::fs::write(self.root.join(entry), entry_content)?;

        let gitignore = r#"# CellScript
.cell/
build/
dist/
*.o
*.bin
"#;
        std::fs::write(self.root.join(".gitignore"), gitignore)?;

        Ok(())
    }

    pub fn add_dependency(&self, name: &str, version: &str) -> Result<()> {
        let mut manifest = self.read_manifest()?;

        manifest.dependencies.insert(name.to_string(), Dependency::Simple(version.to_string()));

        self.write_manifest(&manifest)?;
        Ok(())
    }

    pub fn remove_dependency(&self, name: &str) -> Result<()> {
        let mut manifest = self.read_manifest()?;
        manifest.dependencies.remove(name);
        self.write_manifest(&manifest)?;
        Ok(())
    }

    pub fn resolve_dependencies(&mut self) -> Result<()> {
        let manifest = self.read_manifest()?;

        for (name, dep) in &manifest.dependencies {
            self.resolve_dependency_from_root(name, dep, &self.root.clone(), &mut Vec::new())?;
        }

        Ok(())
    }

    /// Extract the version-requirement string carried by a dependency, if any.
    ///
    /// Path and git dependencies without a meaningful version return `None`,
    /// which the unified resolver treats as "no constraint to check". Only
    /// registry dependencies (Simple or Detailed with a version) contribute a
    /// constraint that must be reconciled across the graph.
    fn version_requirement_of(&self, dep: &Dependency) -> Option<String> {
        match dep {
            Dependency::Simple(version) => Some(version.clone()),
            Dependency::Detailed(detailed) => {
                // Path/git sources and wildcard versions carry no constraint
                // for the unified resolver to reconcile across the graph.
                if detailed.path.is_some() || detailed.git.is_some() || detailed.version.is_empty() || detailed.version == "*" {
                    None
                } else {
                    Some(detailed.version.clone())
                }
            }
        }
    }

    fn resolve_dependency_from_root(&mut self, name: &str, dep: &Dependency, base_root: &Path, stack: &mut Vec<String>) -> Result<()> {
        if stack.iter().any(|item| item == name) {
            let mut cycle = stack.clone();
            cycle.push(name.to_string());
            return Err(CompileError::without_span(format!("Circular dependency detected: {}", cycle.join(" -> "))));
        }

        // Unified (single-version-per-package) resolution: if this package was
        // already resolved elsewhere in the graph, the new version requirement
        // must be satisfied by the already-selected version. If it is not, the
        // dependency graph is unsatisfiable and we fail closed instead of
        // silently keeping whichever version was resolved first.
        if let Some(existing) = self.resolved.get(name) {
            if let Some(req_str) = self.version_requirement_of(dep) {
                let req = version::parse_version_req(&req_str)?;
                if !version::satisfies(&existing.version, &req) {
                    return Err(CompileError::without_span(format!(
                        "version conflict for '{}': already resolved to '{}', which does not satisfy requirement '{}'",
                        name, existing.version, req_str
                    )));
                }
            }
            return Ok(());
        }

        stack.push(name.to_string());

        let (resolved, child_dependencies) = match dep {
            Dependency::Simple(version) => {
                let (resolved, manifest) =
                    self.resolve_from_registry_with_manifest(name, version, None, registry::RegistryResolutionPolicy::default())?;
                (resolved, manifest.dependencies)
            }
            Dependency::Detailed(detailed) => {
                if let Some(path) = &detailed.path {
                    let (resolved, manifest) = self.resolve_from_path_at(name, path, base_root)?;
                    (resolved, manifest.dependencies)
                } else if let Some(git) = &detailed.git {
                    let (resolved, manifest) = self.resolve_from_git_with_manifest(name, git, detailed)?;
                    (resolved, manifest.dependencies)
                } else {
                    let ns = detailed.namespace.as_deref();
                    let (resolved, manifest) = self.resolve_from_registry_with_manifest(
                        name,
                        &detailed.version,
                        ns,
                        registry::RegistryResolutionPolicy::default(),
                    )?;
                    (resolved, manifest.dependencies)
                }
            }
        };

        let package_root = resolved.path.clone();
        self.resolved.insert(name.to_string(), resolved);

        for (child_name, child_dep) in child_dependencies {
            self.resolve_dependency_from_root(&child_name, &child_dep, &package_root, stack)?;
        }

        stack.pop();
        Ok(())
    }

    pub fn resolve_from_registry(&self, name: &str, version: &str) -> Result<ResolvedPackage> {
        self.resolve_from_registry_with_namespace(name, version, None)
    }

    pub fn resolve_from_registry_with_namespace(&self, name: &str, version: &str, namespace: Option<&str>) -> Result<ResolvedPackage> {
        let (resolved, _) =
            self.resolve_from_registry_with_manifest(name, version, namespace, registry::RegistryResolutionPolicy::default())?;
        Ok(resolved)
    }

    pub fn resolve_from_registry_with_namespace_and_policy(
        &self,
        name: &str,
        version: &str,
        namespace: Option<&str>,
        policy: registry::RegistryResolutionPolicy,
    ) -> Result<ResolvedPackage> {
        let (resolved, _) = self.resolve_from_registry_with_manifest(name, version, namespace, policy)?;
        Ok(resolved)
    }

    fn resolve_from_registry_with_manifest(
        &self,
        name: &str,
        version: &str,
        namespace: Option<&str>,
        policy: registry::RegistryResolutionPolicy,
    ) -> Result<(ResolvedPackage, PackageManifest)> {
        // 1. Determine namespace: explicit > consuming package namespace > error
        let resolved_namespace = namespace
            .map(str::to_string)
            .or_else(|| {
                // Try to use consuming package's namespace
                self.read_manifest().ok().and_then(|m| m.package.namespace)
            })
            .ok_or_else(|| {
                CompileError::without_span(format!(
                    "registry dependency '{}' requires a namespace; specify namespace in dependency or set namespace in [package]",
                    name
                ))
            })?;

        // 2. Clone/update discovery index → find source repo URL
        let cache_dir = self.registry_cache_dir();
        let registry_url = registry::default_registry_url();
        let discovery = registry::DiscoveryIndex::new(&registry_url, &cache_dir);
        let entry = discovery.lookup(&resolved_namespace, name).map_err(|e| {
            CompileError::without_span(format!(
                "failed to resolve registry dependency '{}/{}@{}' via discovery index '{}': {}",
                resolved_namespace, name, version, registry_url, e
            ))
        })?;

        // 3. Clone source repo
        let source_url = &entry.source;
        let source_cache = self.git_cache_dir();
        std::fs::create_dir_all(&source_cache)
            .map_err(|e| CompileError::without_span(format!("failed to create source cache directory: {}", e)))?;

        let cache_key = format!("{}#{}", source_url, version);
        let cache_name = format!("{}-{:016x}", name, simple_hash(&cache_key));
        let clone_dir = source_cache.join(&cache_name);

        if clone_dir.exists() && clone_dir.join(".git").exists() {
            registry::git_update(&clone_dir).map_err(CompileError::without_span)?;
        } else {
            let _ = std::fs::remove_dir_all(&clone_dir);
            registry::git_clone(source_url, &clone_dir).map_err(CompileError::without_span)?;
        }

        // 4. Resolve version from registry.json and check out its declared tag.
        let reg_index = registry::RegistryIndex::read_from_repo(&clone_dir)?;
        if reg_index.schema_version != registry::RegistryIndex::CURRENT_SCHEMA_VERSION {
            return Err(CompileError::without_span(format!(
                "registry package '{}/{}' uses unsupported registry.json schema_version {}; expected {}",
                resolved_namespace,
                name,
                reg_index.schema_version,
                registry::RegistryIndex::CURRENT_SCHEMA_VERSION
            )));
        }
        if reg_index.name != name || reg_index.namespace != resolved_namespace {
            return Err(CompileError::without_span(format!(
                "registry.json identity mismatch for '{}/{}': found '{}/{}'",
                resolved_namespace, name, reg_index.namespace, reg_index.name
            )));
        }
        let selected_version = reg_index.find_matching_version_for_resolution(version, policy).cloned().ok_or_else(|| {
            if let Some(blocked) = reg_index.find_matching_version_allowing_yanked_pin(version) {
                return registry_resolution_blocked_error(&resolved_namespace, name, version, blocked, policy);
            }
            CompileError::without_span(format!("no matching version found for '{}/{}@{}'", resolved_namespace, name, version))
        })?;
        emit_yank_notices(&resolved_namespace, name, version, &selected_version.version, &reg_index);
        if selected_version.source_hash.is_empty() {
            return Err(CompileError::without_span(format!(
                "registry package '{}/{}@{}' has no source_hash in registry.json",
                resolved_namespace, name, selected_version.version
            )));
        }
        registry::git_checkout(&clone_dir, &selected_version.tag).map_err(CompileError::without_span)?;

        let revision = registry::git_revision(&clone_dir).unwrap_or_else(|_| "unknown".to_string());

        // 5. Re-read registry.json at the checked-out tag and verify source_hash.
        let tagged_index = registry::RegistryIndex::read_from_repo(&clone_dir)?;
        if tagged_index.schema_version != registry::RegistryIndex::CURRENT_SCHEMA_VERSION {
            return Err(CompileError::without_span(format!(
                "registry package '{}/{}@{}' uses unsupported registry.json schema_version {}; expected {}",
                resolved_namespace,
                name,
                selected_version.version,
                tagged_index.schema_version,
                registry::RegistryIndex::CURRENT_SCHEMA_VERSION
            )));
        }
        if tagged_index.name != name || tagged_index.namespace != resolved_namespace {
            return Err(CompileError::without_span(format!(
                "registry.json identity mismatch for checked-out '{}/{}@{}': found '{}/{}'",
                resolved_namespace, name, selected_version.version, tagged_index.namespace, tagged_index.name
            )));
        }
        let tagged_version = tagged_index.versions.iter().find(|v| v.version == selected_version.version).ok_or_else(|| {
            CompileError::without_span(format!(
                "registry package '{}/{}@{}' tag '{}' does not contain a matching registry.json version entry",
                resolved_namespace, name, selected_version.version, selected_version.tag
            ))
        })?;
        if tagged_version.source_hash.is_empty() {
            return Err(CompileError::without_span(format!(
                "registry package '{}/{}@{}' has no source_hash in registry.json",
                resolved_namespace, name, tagged_version.version
            )));
        }
        if tagged_version
            .resolver_block_reason(policy, matches!(crate::package::version::parse_version_req(version), Ok(VersionReq::Exact(_))))
            .is_some()
        {
            return Err(registry_resolution_blocked_error(&resolved_namespace, name, version, tagged_version, policy));
        }
        let computed_source_hash = registry::compute_source_hash(&clone_dir)?;
        if computed_source_hash != tagged_version.source_hash {
            return Err(CompileError::without_span(format!(
                "source_hash mismatch for '{}/{}@{}': expected '{}', got '{}'",
                resolved_namespace, name, tagged_version.version, tagged_version.source_hash, computed_source_hash
            )));
        }

        // 6. Read Cell.toml and resolve transitive dependencies
        let manifest_path = clone_dir.join("Cell.toml");
        if !manifest_path.exists() {
            return Err(CompileError::without_span(format!(
                "registry package '{}/{}' does not contain Cell.toml",
                resolved_namespace, name
            )));
        }

        let content = std::fs::read_to_string(&manifest_path)?;
        let manifest: PackageManifest = toml::from_str(&content)?;
        if manifest.package.name != name {
            return Err(CompileError::without_span(format!(
                "registry package '{}/{}@{}' Cell.toml declares package name '{}'",
                resolved_namespace, name, tagged_version.version, manifest.package.name
            )));
        }
        if manifest.package.version != tagged_version.version {
            return Err(CompileError::without_span(format!(
                "registry package '{}/{}' registry.json version '{}' does not match Cell.toml version '{}'",
                resolved_namespace, name, tagged_version.version, manifest.package.version
            )));
        }
        if manifest.package.namespace.as_deref() != Some(resolved_namespace.as_str()) {
            return Err(CompileError::without_span(format!(
                "registry package '{}/{}@{}' Cell.toml must declare namespace '{}'",
                resolved_namespace, name, tagged_version.version, resolved_namespace
            )));
        }

        Ok((
            ResolvedPackage {
                name: name.to_string(),
                version: manifest.package.version.clone(),
                path: clone_dir.clone(),
                source: PackageSource::Registry {
                    registry: registry_url,
                    url: source_url.clone(),
                    revision,
                    namespace: resolved_namespace.clone(),
                    version: manifest.package.version.clone(),
                },
                dependencies: manifest.dependencies.keys().cloned().collect(),
                namespace: Some(resolved_namespace),
                source_hash: Some(computed_source_hash),
            },
            manifest,
        ))
    }

    fn registry_cache_dir(&self) -> PathBuf {
        self.root.join(".cell/registry-cache")
    }

    pub fn resolve_from_path(&self, name: &str, path: &str) -> Result<ResolvedPackage> {
        let (resolved, _) = self.resolve_from_path_at(name, path, &self.root)?;
        Ok(resolved)
    }

    fn resolve_from_path_at(&self, name: &str, path: &str, base_root: &Path) -> Result<(ResolvedPackage, PackageManifest)> {
        let package_path = base_root.join(path);
        let manifest_path = package_path.join("Cell.toml");

        if !manifest_path.exists() {
            return Err(CompileError::without_span(format!("Dependency '{}' not found at path '{}'", name, path)));
        }

        let content = std::fs::read_to_string(&manifest_path)?;
        let manifest: PackageManifest = toml::from_str(&content)?;

        let source_path = if base_root == self.root {
            PathBuf::from(path)
        } else {
            package_path.strip_prefix(&self.root).unwrap_or(&package_path).to_path_buf()
        };

        Ok((
            ResolvedPackage {
                name: name.to_string(),
                version: manifest.package.version.clone(),
                path: package_path,
                source: PackageSource::Local(source_path),
                dependencies: manifest.dependencies.keys().cloned().collect(),
                namespace: manifest.package.namespace.clone(),
                source_hash: None,
            },
            manifest,
        ))
    }

    pub fn resolve_from_git(&self, name: &str, url: &str, detailed: &DetailedDependency) -> Result<ResolvedPackage> {
        let (resolved, _) = self.resolve_from_git_with_manifest(name, url, detailed)?;
        Ok(resolved)
    }

    fn resolve_from_git_with_manifest(
        &self,
        name: &str,
        url: &str,
        detailed: &DetailedDependency,
    ) -> Result<(ResolvedPackage, PackageManifest)> {
        let cache_dir = self.git_cache_dir();
        std::fs::create_dir_all(&cache_dir).map_err(|e| {
            CompileError::without_span(format!("failed to create git cache directory '{}': {}", cache_dir.display(), e))
        })?;

        let requested_ref = detailed.rev.as_ref().or(detailed.tag.as_ref()).or(detailed.branch.as_ref());
        let cache_key = format!("{}#{}", url, requested_ref.map(String::as_str).unwrap_or("HEAD"));
        let cache_name = format!("{}-{:016x}", name, simple_hash(&cache_key));
        let clone_dir = cache_dir.join(&cache_name);

        let git_result = if clone_dir.exists() && clone_dir.join(".git").exists() {
            Self::git_update(&clone_dir)
        } else {
            let _ = std::fs::remove_dir_all(&clone_dir);
            Self::git_clone(url, &clone_dir)
        };

        git_result.map_err(|e| CompileError::without_span(format!("git dependency '{}' from '{}' failed: {}", name, url, e)))?;

        if let Some(ref_str) = requested_ref {
            Self::git_checkout(&clone_dir, ref_str).map_err(|e| {
                CompileError::without_span(format!("git dependency '{}' failed to checkout '{}': {}", name, ref_str, e))
            })?;
        }

        let revision = Self::git_revision(&clone_dir).unwrap_or_else(|_| "unknown".to_string());

        let manifest_path = clone_dir.join("Cell.toml");
        if !manifest_path.exists() {
            return Err(CompileError::without_span(format!(
                "git dependency '{}' from '{}' does not contain Cell.toml at repository root",
                name, url
            )));
        }

        let content = std::fs::read_to_string(&manifest_path)?;
        let manifest: PackageManifest = toml::from_str(&content)?;

        Ok((
            ResolvedPackage {
                name: name.to_string(),
                version: manifest.package.version.clone(),
                path: clone_dir.clone(),
                source: PackageSource::Git { url: url.to_string(), revision },
                dependencies: manifest.dependencies.keys().cloned().collect(),
                namespace: manifest.package.namespace.clone(),
                source_hash: None,
            },
            manifest,
        ))
    }

    fn git_cache_dir(&self) -> PathBuf {
        self.root.join(".cell/git-cache")
    }

    fn git_clone(url: &str, target: &Path) -> std::result::Result<(), String> {
        let output = std::process::Command::new("git")
            .args(["clone", url, &target.to_string_lossy()])
            .output()
            .map_err(|e| format!("failed to execute git: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git clone failed: {}", stderr.trim()));
        }

        Ok(())
    }

    fn git_update(clone_dir: &Path) -> std::result::Result<(), String> {
        let output = std::process::Command::new("git")
            .args(["fetch", "--tags", "--prune", "origin"])
            .current_dir(clone_dir)
            .output()
            .map_err(|e| format!("failed to execute git: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git fetch failed for {}: {}", clone_dir.display(), stderr.trim()));
        }

        Ok(())
    }

    fn git_checkout(clone_dir: &Path, ref_str: &str) -> std::result::Result<(), String> {
        let _output = std::process::Command::new("git")
            .args(["fetch", "origin", ref_str])
            .current_dir(clone_dir)
            .output()
            .map_err(|e| format!("failed to execute git fetch: {}", e))?;

        let output = std::process::Command::new("git")
            .args(["checkout", ref_str])
            .current_dir(clone_dir)
            .output()
            .map_err(|e| format!("failed to execute git checkout: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git checkout {} failed: {}", ref_str, stderr.trim()));
        }

        Ok(())
    }

    fn git_revision(clone_dir: &Path) -> std::result::Result<String, String> {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(clone_dir)
            .output()
            .map_err(|e| format!("failed to execute git rev-parse: {}", e))?;

        if !output.status.success() {
            return Err("git rev-parse failed".to_string());
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub fn get_resolved(&self) -> &HashMap<String, ResolvedPackage> {
        &self.resolved
    }

    pub fn build_dependency_graph(&self) -> DependencyGraph {
        let mut graph = DependencyGraph::new();

        for (name, package) in &self.resolved {
            graph.add_node(name.clone());
            for dep in &package.dependencies {
                graph.add_edge(name.clone(), dep.clone());
            }
        }

        graph
    }

    pub fn check_circular_deps(&self) -> Result<()> {
        let graph = self.build_dependency_graph();

        if let Some(cycle) = graph.find_cycle() {
            return Err(CompileError::without_span(format!("Circular dependency detected: {}", cycle.join(" -> "))));
        }

        Ok(())
    }

    pub fn get_source_paths(&self) -> Vec<PathBuf> {
        self.resolved.values().map(|p| p.path.join("src")).collect()
    }
}

pub struct DependencyGraph {
    nodes: Vec<String>,
    edges: HashMap<String, Vec<String>>,
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self { nodes: Vec::new(), edges: HashMap::new() }
    }

    pub fn add_node(&mut self, name: String) {
        if !self.nodes.contains(&name) {
            self.nodes.push(name);
        }
    }

    pub fn add_edge(&mut self, from: String, to: String) {
        self.edges.entry(from).or_default().push(to);
    }

    pub fn find_cycle(&self) -> Option<Vec<String>> {
        let mut visited = HashMap::new();
        let mut rec_stack = Vec::new();

        for node in &self.nodes {
            if !visited.contains_key(node) {
                if let Some(cycle) = self.dfs_find_cycle(node, &mut visited, &mut rec_stack) {
                    return Some(cycle);
                }
            }
        }

        None
    }

    fn dfs_find_cycle(&self, node: &str, visited: &mut HashMap<String, bool>, rec_stack: &mut Vec<String>) -> Option<Vec<String>> {
        visited.insert(node.to_string(), true);
        rec_stack.push(node.to_string());

        if let Some(neighbors) = self.edges.get(node) {
            for neighbor in neighbors {
                if !visited.contains_key(neighbor) {
                    if let Some(cycle) = self.dfs_find_cycle(neighbor, visited, rec_stack) {
                        return Some(cycle);
                    }
                } else if rec_stack.contains(neighbor) {
                    let idx = rec_stack.iter().position(|n| n == neighbor).unwrap();
                    let mut cycle = rec_stack[idx..].to_vec();
                    cycle.push(neighbor.to_string());
                    return Some(cycle);
                }
            }
        }

        rec_stack.pop();
        None
    }
}

fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    pub version: u32,
    #[serde(default)]
    pub package: LockfilePackageInfo,
    pub dependencies: BTreeMap<String, LockedDependency>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_build: Option<LockedBuildInfo>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deployment: BTreeMap<String, LockfileDeploymentRef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LockfilePackageInfo {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiler_source_hash: Option<String>,
}

/// A reference from Cell.lock [deployment.<network>] to a Deployed.toml entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockfileDeploymentRef {
    pub record: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out_point: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_hash: Option<String>,
}

impl Lockfile {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            package: LockfilePackageInfo::default(),
            dependencies: BTreeMap::new(),
            package_build: None,
            deployment: BTreeMap::new(),
        }
    }

    pub fn read_from_root(root: &Path) -> Result<Option<Self>> {
        let lock_path = root.join("Cell.lock");
        if !lock_path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&lock_path)
            .map_err(|error| CompileError::without_span(format!("failed to read lockfile '{}': {}", lock_path.display(), error)))?;
        let lockfile = toml::from_str(&content)
            .map_err(|error| CompileError::without_span(format!("failed to parse lockfile '{}': {}", lock_path.display(), error)))?;
        Ok(Some(lockfile))
    }

    pub fn write_to_root(&self, root: &Path) -> Result<()> {
        let lock_path = root.join("Cell.lock");
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&lock_path, content)?;
        Ok(())
    }

    pub fn update_from_resolved(&mut self, resolved: &HashMap<String, ResolvedPackage>) {
        for (name, package) in resolved {
            let locked = LockedDependency {
                version: package.version.clone(),
                source: match &package.source {
                    PackageSource::Local(path) => LockedSource::Path { path: path.to_string_lossy().to_string() },
                    PackageSource::Git { url, revision } => LockedSource::Git { url: url.clone(), revision: revision.clone() },
                    PackageSource::Registry { registry, url, revision, namespace, version } => LockedSource::Registry {
                        registry: registry.clone(),
                        url: url.clone(),
                        revision: revision.clone(),
                        namespace: namespace.clone(),
                        version: version.clone(),
                    },
                },
                source_hash: package.source_hash.clone(),
                build: None,
            };
            self.dependencies.insert(name.clone(), locked);
        }
    }

    pub fn replace_with_resolved(&mut self, resolved: &HashMap<String, ResolvedPackage>) {
        self.dependencies.clear();
        self.update_from_resolved(resolved);
    }

    pub fn is_consistent(&self, manifest: &PackageManifest) -> bool {
        self.consistency_issues(manifest).is_empty()
    }

    pub fn consistency_issues(&self, manifest: &PackageManifest) -> Vec<String> {
        self.consistency_issues_with_expected(manifest, None)
    }

    pub fn consistency_issues_with_resolved(
        &self,
        manifest: &PackageManifest,
        resolved: &HashMap<String, ResolvedPackage>,
    ) -> Vec<String> {
        self.consistency_issues_with_expected(manifest, Some(resolved))
    }

    fn consistency_issues_with_expected(
        &self,
        manifest: &PackageManifest,
        resolved: Option<&HashMap<String, ResolvedPackage>>,
    ) -> Vec<String> {
        let mut issues = Vec::new();
        if self.version != Self::CURRENT_VERSION {
            issues.push(format!("Cell.lock version {} is not supported; expected {}", self.version, Self::CURRENT_VERSION));
        }

        for name in manifest.dependencies.keys() {
            let Some(locked) = self.dependencies.get(name) else {
                issues.push(format!("dependency '{}' is missing from Cell.lock", name));
                continue;
            };
            if let Some(dep) = manifest.dependencies.get(name) {
                issues.extend(lock_dependency_consistency_issues(name, dep, locked, manifest.package.namespace.as_deref()));
            }
        }

        if let Some(resolved) = resolved {
            for (name, package) in resolved {
                let Some(locked) = self.dependencies.get(name) else {
                    issues.push(format!("resolved dependency '{}' is missing from Cell.lock", name));
                    continue;
                };
                issues.extend(resolved_dependency_consistency_issues(name, package, locked));
            }
        }

        for name in self.dependencies.keys() {
            let expected_by_manifest = manifest.dependencies.contains_key(name);
            let expected_by_resolved = resolved.is_some_and(|resolved| resolved.contains_key(name));
            if !expected_by_manifest && !expected_by_resolved {
                issues.push(format!("Cell.lock contains stale dependency '{}' not present in Cell.toml", name));
            }
        }

        issues
    }
}

fn resolved_dependency_consistency_issues(name: &str, package: &ResolvedPackage, locked: &LockedDependency) -> Vec<String> {
    let mut issues = Vec::new();

    if locked.version != package.version {
        issues.push(format!(
            "resolved dependency '{}' has package version '{}' but Cell.lock records '{}'",
            name, package.version, locked.version
        ));
    }

    match (&package.source, &locked.source) {
        (PackageSource::Local(path), LockedSource::Path { path: locked_path }) if locked_path == path.to_string_lossy().as_ref() => {}
        (PackageSource::Git { url, revision }, LockedSource::Git { url: locked_url, revision: locked_revision })
            if locked_url == url && locked_revision == revision => {}
        (
            PackageSource::Registry { registry, url, revision, namespace, version },
            LockedSource::Registry {
                registry: locked_registry,
                url: locked_url,
                revision: locked_revision,
                namespace: locked_namespace,
                version: locked_version,
            },
        ) if locked_registry == registry
            && locked_url == url
            && locked_revision == revision
            && locked_namespace == namespace
            && locked_version == version => {}
        (_, source) => issues.push(format!(
            "resolved dependency '{}' expects {} but Cell.lock records {}",
            name,
            package_source_display(&package.source),
            locked_source_display(source)
        )),
    }

    if let Some(expected_hash) = &package.source_hash {
        match &locked.source_hash {
            Some(locked_hash) if locked_hash == expected_hash => {}
            Some(locked_hash) => issues.push(format!(
                "resolved dependency '{}' source_hash '{}' does not match Cell.lock '{}'",
                name, expected_hash, locked_hash
            )),
            None => issues.push(format!("resolved dependency '{}' is missing source_hash in Cell.lock", name)),
        }
    } else if matches!(package.source, PackageSource::Registry { .. }) {
        issues.push(format!("resolved registry dependency '{}' did not produce a source_hash", name));
    }

    issues
}

fn lock_dependency_consistency_issues(
    name: &str,
    dep: &Dependency,
    locked: &LockedDependency,
    consuming_namespace: Option<&str>,
) -> Vec<String> {
    let mut issues = Vec::new();

    match dep {
        Dependency::Simple(version) => match &locked.source {
            LockedSource::Registry { namespace: locked_namespace, version: locked_version, .. }
                if Some(locked_namespace.as_str()) == consuming_namespace && locked_version == version => {}
            source => issues.push(format!(
                "dependency '{}' expects registry source {}@{} but Cell.lock records {}",
                name,
                name,
                version,
                locked_source_display(source)
            )),
        },
        Dependency::Detailed(detail) => {
            if let Some(path) = &detail.path {
                match &locked.source {
                    LockedSource::Path { path: locked_path } if locked_path == path => {}
                    source => issues.push(format!(
                        "dependency '{}' expects path source '{}' but Cell.lock records {}",
                        name,
                        path,
                        locked_source_display(source)
                    )),
                }
                push_locked_version_issue(name, &detail.version, &locked.version, &mut issues);
            } else if let Some(git) = &detail.git {
                match &locked.source {
                    LockedSource::Git { url, revision } if url == git => {
                        if let Some(rev) = &detail.rev {
                            let rev_matches = revision == rev || revision.starts_with(rev) || rev.starts_with(revision);
                            if !rev_matches {
                                issues.push(format!(
                                    "dependency '{}' expects git revision '{}' but Cell.lock records '{}'",
                                    name, rev, revision
                                ));
                            }
                        }
                    }
                    source => issues.push(format!(
                        "dependency '{}' expects git source '{}' but Cell.lock records {}",
                        name,
                        git,
                        locked_source_display(source)
                    )),
                }
                push_locked_version_issue(name, &detail.version, &locked.version, &mut issues);
            } else {
                match &locked.source {
                    LockedSource::Registry { namespace: locked_namespace, version: locked_version, .. }
                        if Some(locked_namespace.as_str()) == detail.namespace.as_deref().or(consuming_namespace)
                            && locked_version == &detail.version => {}
                    source => issues.push(format!(
                        "dependency '{}' expects registry source {}@{} but Cell.lock records {}",
                        name,
                        name,
                        detail.version,
                        locked_source_display(source)
                    )),
                }
            }
        }
    }

    issues
}

fn push_locked_version_issue(name: &str, expected: &str, actual: &str, issues: &mut Vec<String>) {
    if expected != "*" && expected != actual {
        issues.push(format!("dependency '{}' expects package version '{}' but Cell.lock records '{}'", name, expected, actual));
    }
}

fn locked_source_display(source: &LockedSource) -> String {
    match source {
        LockedSource::Path { path } => format!("path '{}'", path),
        LockedSource::Git { url, revision } => format!("git '{}#{}'", url, revision),
        LockedSource::Registry { registry, namespace, version, .. } => format!("registry {}/{}@{}", registry, namespace, version),
    }
}

fn package_source_display(source: &PackageSource) -> String {
    match source {
        PackageSource::Local(path) => format!("path '{}'", path.display()),
        PackageSource::Git { url, revision } => format!("git '{}#{}'", url, revision),
        PackageSource::Registry { registry, namespace, version, .. } => format!("registry {}/{}@{}", registry, namespace, version),
    }
}

impl Default for Lockfile {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LockedBuildInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiler_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_data_codec_manifest_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abi_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedDependency {
    pub version: String,
    pub source: LockedSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<LockedBuildInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LockedSource {
    Path { path: String },
    Git { url: String, revision: String },
    Registry { registry: String, url: String, revision: String, namespace: String, version: String },
}

pub mod version {
    use super::*;

    pub fn parse_version_req(req: &str) -> Result<VersionReq> {
        if req == "*" {
            return Ok(VersionReq::Any);
        }

        if let Some(stripped) = req.strip_prefix('^') {
            return Ok(VersionReq::Compatible(stripped.to_string()));
        }

        if let Some(stripped) = req.strip_prefix('=') {
            return Ok(VersionReq::Exact(stripped.to_string()));
        }

        if req.contains(',') || req.contains('>') || req.contains('<') {
            return Ok(VersionReq::Range(req.to_string()));
        }

        Ok(VersionReq::Compatible(req.to_string()))
    }

    pub fn satisfies(version: &str, req: &VersionReq) -> bool {
        match req {
            VersionReq::Any => true,
            VersionReq::Exact(v) => version == v,
            VersionReq::Compatible(v) => is_compatible(version, v),
            VersionReq::Range(r) => satisfies_range(version, r),
        }
    }

    fn is_compatible(version: &str, base: &str) -> bool {
        let Some(v_parts) = parse_numeric_version(version) else {
            return false;
        };
        let Some(b_parts) = parse_numeric_version(base) else {
            return false;
        };

        if v_parts[0] != b_parts[0] {
            return false;
        }

        if v_parts[0] == 0 {
            if v_parts.len() < 2 || b_parts.len() < 2 {
                return false;
            }
            if v_parts[1] != b_parts[1] {
                return false;
            }
        }

        true
    }

    fn satisfies_range(_version: &str, _range: &str) -> bool {
        for clause in _range.split(',').map(str::trim).filter(|clause| !clause.is_empty()) {
            let Some((op, expected)) = parse_range_clause(clause) else {
                return false;
            };
            let Some(ordering) = compare_versions(_version, expected) else {
                return false;
            };
            let satisfied = match op {
                ">" => ordering.is_gt(),
                ">=" => ordering.is_gt() || ordering.is_eq(),
                "<" => ordering.is_lt(),
                "<=" => ordering.is_lt() || ordering.is_eq(),
                "=" | "==" => ordering.is_eq(),
                _ => false,
            };
            if !satisfied {
                return false;
            }
        }
        true
    }

    fn parse_range_clause(clause: &str) -> Option<(&str, &str)> {
        for op in [">=", "<=", "==", ">", "<", "="] {
            if let Some(version) = clause.strip_prefix(op) {
                return Some((op, version.trim()));
            }
        }
        None
    }

    fn compare_versions(left: &str, right: &str) -> Option<std::cmp::Ordering> {
        let left = parse_numeric_version(left)?;
        let right = parse_numeric_version(right)?;
        let max_len = left.len().max(right.len());
        for idx in 0..max_len {
            let lhs = *left.get(idx).unwrap_or(&0);
            let rhs = *right.get(idx).unwrap_or(&0);
            match lhs.cmp(&rhs) {
                std::cmp::Ordering::Equal => {}
                ordering => return Some(ordering),
            }
        }
        Some(std::cmp::Ordering::Equal)
    }

    fn parse_numeric_version(version: &str) -> Option<Vec<u32>> {
        let core = version.split_once('-').map(|(core, _)| core).unwrap_or(version);
        let parts: Option<Vec<u32>> = core.split('.').map(|part| part.parse().ok()).collect();
        let parts = parts?;
        if parts.is_empty() {
            None
        } else {
            Some(parts)
        }
    }
}

// ---------------------------------------------------------------------------
// Deployed.toml — Deployment Fact Record
// ---------------------------------------------------------------------------

/// The schema identifier for Deployed.toml files produced by CellScript v0.19+.
pub const DEPLOYED_MANIFEST_SCHEMA: &str = "cellscript-deployed-v0.19";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployedManifest {
    pub version: u32,
    pub schema: Option<String>,
    pub package: DeployedPackageInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<DeployedBuildInfo>,
    #[serde(default)]
    pub deployments: Vec<DeploymentRecord>,
}

impl DeployedManifest {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn read_from_root(root: &Path) -> Result<Option<Self>> {
        let path = root.join("Deployed.toml");
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| CompileError::without_span(format!("failed to read Deployed.toml '{}': {}", path.display(), e)))?;
        let manifest: Self = toml::from_str(&content)
            .map_err(|e| CompileError::without_span(format!("failed to parse Deployed.toml '{}': {}", path.display(), e)))?;
        Ok(Some(manifest))
    }

    pub fn write_to_root(&self, root: &Path) -> Result<()> {
        let path = root.join("Deployed.toml");
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployedPackageInfo {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeployedBuildInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiler_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_data_codec_manifest_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abi_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints_hash: Option<String>,
}

/// Deployment status lifecycle:
/// candidate -> active -> deprecated -> revoked
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeploymentStatus {
    #[default]
    Candidate,
    Active,
    Deprecated,
    Revoked,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScriptRole {
    #[default]
    Type,
    Lock,
    DualRole,
    Helper,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentRecord {
    // Required fields (Phase 1)
    pub network: String,
    pub chain_id: String,
    pub tx_hash: String,
    pub output_index: u32,
    pub code_hash: String,
    pub hash_type: String,
    pub dep_type: String,
    pub data_hash: String,
    pub out_point: String,

    // Recommended fields (Phase 1 — build provenance binding)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_data_codec_manifest_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abi_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiler_version: Option<String>,

    // Optional fields (Phase 2 — governance and upgrade)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_role: Option<ScriptRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<DeploymentStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upgrade_lineage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_report_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher_signature: Option<String>,

    // Cell deps
    #[serde(default)]
    pub cell_deps: Vec<DeploymentCellDep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentCellDep {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub tx_hash: String,
    pub output_index: u32,
    pub dep_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_manifest_serialization() {
        let manifest = PackageManifest {
            package: PackageInfo {
                name: "test".to_string(),
                version: "0.1.0".to_string(),
                namespace: None,
                authors: vec!["Test Author".to_string()],
                description: "Test package".to_string(),
                license: "MIT".to_string(),
                repository: String::new(),
                homepage: String::new(),
                documentation: String::new(),
                keywords: vec!["test".to_string()],
                categories: vec!["test".to_string()],
                cellscript_version: String::new(),
                entry: "src/main.cell".to_string(),
                source_roots: vec![],
                include: vec![],
                exclude: vec![],
            },
            workspace: None,
            dependencies: HashMap::new(),
            dev_dependencies: HashMap::new(),
            build: BuildConfig::default(),
            policy: PolicyConfig::default(),
            deploy: DeployConfig::default(),
            metadata: HashMap::new(),
        };

        let toml_str = toml::to_string(&manifest).unwrap();
        assert!(toml_str.contains("name = \"test\""));
        assert!(toml_str.contains("version = \"0.1.0\""));
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

    #[test]
    fn test_version_compatibility() {
        assert!(version::satisfies("1.2.3", &VersionReq::Compatible("1.0.0".to_string())));
        assert!(version::satisfies("1.5.0", &VersionReq::Compatible("1.2.3".to_string())));
        assert!(!version::satisfies("2.0.0", &VersionReq::Compatible("1.0.0".to_string())));
        assert!(!version::satisfies("0.2.0", &VersionReq::Compatible("0.1.0".to_string())));
        assert!(version::satisfies("0.1.5", &VersionReq::Compatible("0.1.0".to_string())));
        assert!(version::satisfies("1.2.3", &VersionReq::Range(">=1.0.0, <2.0.0".to_string())));
        assert!(!version::satisfies("2.0.0", &VersionReq::Range(">=1.0.0, <2.0.0".to_string())));
        assert!(!version::satisfies("1.2.3", &VersionReq::Range(">=1.3.0".to_string())));
        assert!(!version::satisfies("1.bad", &VersionReq::Compatible("1.0.0".to_string())));
        assert!(!version::satisfies("1.2.3", &VersionReq::Compatible("1.bad".to_string())));
        assert!(!version::satisfies("1.bad", &VersionReq::Range(">=1.0.0".to_string())));
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
name = "math"
version = "0.1.0"
"#,
        )
        .unwrap();

        let mut manager = PackageManager::new(root);
        manager.resolve_dependencies().unwrap();

        let math = manager.get_resolved().get("math").expect("path dependency should resolve");
        assert_eq!(math.name, "math");
        assert_eq!(math.version, "0.1.0");
        assert!(matches!(math.source, PackageSource::Local(_)));
        assert_eq!(manager.get_source_paths(), vec![root.join("deps/math/src")]);
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
name = "math"
version = "0.2.0"
"#,
        )
        .unwrap();

        let mut manager = PackageManager::new(root);
        manager.resolve_dependencies().unwrap();

        let math = manager.get_resolved().get("math").expect("path dependency should resolve");
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
name = "util"
version = "0.1.0"
"#,
        )
        .unwrap();

        let mut manager = PackageManager::new(root);
        manager.resolve_dependencies().unwrap();

        assert!(manager.get_resolved().contains_key("math"));
        assert!(manager.get_resolved().contains_key("util"));
        assert_eq!(manager.get_resolved()["math"].dependencies, vec!["util"]);
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

    #[test]
    fn lockfile_consistency_reports_stale_and_mismatched_path_sources() {
        let manifest: PackageManifest = toml::from_str(
            r#"
[package]
name = "app"
version = "0.1.0"

[dependencies.math]
version = "0.1.0"
path = "deps/math"
"#,
        )
        .unwrap();
        let mut lockfile = Lockfile::new();
        lockfile.dependencies.insert(
            "math".to_string(),
            LockedDependency {
                version: "0.2.0".to_string(),
                source: LockedSource::Path { path: "deps/old-math".to_string() },
                source_hash: None,
                build: None,
            },
        );
        lockfile.dependencies.insert(
            "stale".to_string(),
            LockedDependency {
                version: "1.0.0".to_string(),
                source: LockedSource::Registry {
                    registry: "cellscript-registry".to_string(),
                    url: "https://github.com/example/stale".to_string(),
                    revision: "abc123".to_string(),
                    namespace: "stale".to_string(),
                    version: "1.0.0".to_string(),
                },
                source_hash: None,
                build: None,
            },
        );

        let issues = lockfile.consistency_issues(&manifest);

        assert!(issues.iter().any(|issue| issue.contains("expects path source 'deps/math'")), "{issues:?}");
        assert!(issues.iter().any(|issue| issue.contains("expects package version '0.1.0'")), "{issues:?}");
        assert!(issues.iter().any(|issue| issue.contains("stale dependency 'stale'")), "{issues:?}");
        assert!(!lockfile.is_consistent(&manifest));
    }

    #[test]
    fn lockfile_consistency_allows_resolved_transitive_path_dependencies() {
        let manifest: PackageManifest = toml::from_str(
            r#"
[package]
name = "app"
version = "0.1.0"

[dependencies.math]
version = "0.1.0"
path = "deps/math"
"#,
        )
        .unwrap();
        let mut lockfile = Lockfile::new();
        lockfile.dependencies.insert(
            "math".to_string(),
            LockedDependency {
                version: "0.1.0".to_string(),
                source: LockedSource::Path { path: "deps/math".to_string() },
                source_hash: None,
                build: None,
            },
        );
        lockfile.dependencies.insert(
            "util".to_string(),
            LockedDependency {
                version: "0.1.0".to_string(),
                source: LockedSource::Path { path: "deps/math/../util".to_string() },
                source_hash: None,
                build: None,
            },
        );
        let mut resolved = HashMap::new();
        resolved.insert(
            "math".to_string(),
            ResolvedPackage {
                name: "math".to_string(),
                version: "0.1.0".to_string(),
                path: PathBuf::from("deps/math"),
                source: PackageSource::Local(PathBuf::from("deps/math")),
                dependencies: vec!["util".to_string()],
                namespace: None,
                source_hash: None,
            },
        );
        resolved.insert(
            "util".to_string(),
            ResolvedPackage {
                name: "util".to_string(),
                version: "0.1.0".to_string(),
                path: PathBuf::from("deps/util"),
                source: PackageSource::Local(PathBuf::from("deps/math/../util")),
                dependencies: Vec::new(),
                namespace: None,
                source_hash: None,
            },
        );

        let issues = lockfile.consistency_issues_with_resolved(&manifest, &resolved);

        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn lockfile_replace_with_resolved_prunes_removed_dependencies() {
        let mut lockfile = Lockfile::new();
        lockfile.dependencies.insert(
            "old".to_string(),
            LockedDependency {
                version: "1.0.0".to_string(),
                source: LockedSource::Registry {
                    registry: "cellscript-registry".to_string(),
                    url: "https://github.com/example/old".to_string(),
                    revision: "def456".to_string(),
                    namespace: "old".to_string(),
                    version: "1.0.0".to_string(),
                },
                source_hash: None,
                build: None,
            },
        );

        let mut resolved = HashMap::new();
        resolved.insert(
            "math".to_string(),
            ResolvedPackage {
                name: "math".to_string(),
                version: "0.1.0".to_string(),
                path: PathBuf::from("deps/math"),
                source: PackageSource::Local(PathBuf::from("deps/math")),
                dependencies: Vec::new(),
                namespace: None,
                source_hash: None,
            },
        );

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
    fn package_manager_rejects_registry_dependencies_fail_closed() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("Cell.toml"),
            r#"
[package]
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
            version: 1,
            schema: Some(DEPLOYED_MANIFEST_SCHEMA.to_string()),
            package: DeployedPackageInfo {
                name: "amm_pool".to_string(),
                version: "1.2.0".to_string(),
                source_hash: Some("blake2b:0xabcd".to_string()),
            },
            build: Some(DeployedBuildInfo {
                compiler_version: Some("0.19.0".to_string()),
                artifact_hash: Some("blake2b:0x1234".to_string()),
                metadata_hash: None,
                schema_hash: None,
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
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.package.name, "amm_pool");
        assert_eq!(parsed.deployments.len(), 1);
        assert_eq!(parsed.deployments[0].network, "aggron4");
        assert_eq!(parsed.deployments[0].cell_deps.len(), 1);
    }

    #[test]
    fn deployed_manifest_backward_compatible() {
        // Old format without new optional fields should parse successfully
        let toml_str = r#"
version = 1

[package]
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
        let parsed: DeployedManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(parsed.package.name, "token");
        assert_eq!(parsed.deployments.len(), 1);
        assert!(parsed.deployments[0].type_id.is_none());
        assert!(parsed.deployments[0].status.is_none());
        assert!(parsed.build.is_none());
    }
}
