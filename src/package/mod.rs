use crate::error::{CompileError, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    pub package: PackageInfo,
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
    pub role: Option<String>,
    #[serde(default)]
    pub verifier_id: Option<String>,
    #[serde(default)]
    pub ipc_abi: Option<String>,
    #[serde(default)]
    pub artifact_hash: Option<String>,
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
}

#[derive(Debug, Clone)]
pub enum PackageSource {
    Local(PathBuf),
    Git { url: String, revision: String },
    Registry { name: String, version: String },
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

    fn resolve_dependency_from_root(&mut self, name: &str, dep: &Dependency, base_root: &Path, stack: &mut Vec<String>) -> Result<()> {
        if stack.iter().any(|item| item == name) {
            let mut cycle = stack.clone();
            cycle.push(name.to_string());
            return Err(CompileError::without_span(format!("Circular dependency detected: {}", cycle.join(" -> "))));
        }

        if self.resolved.contains_key(name) {
            return Ok(());
        }

        stack.push(name.to_string());

        let (resolved, child_dependencies) = match dep {
            Dependency::Simple(version) => (self.resolve_from_registry(name, version)?, HashMap::new()),
            Dependency::Detailed(detailed) => {
                if let Some(path) = &detailed.path {
                    let (resolved, manifest) = self.resolve_from_path_at(name, path, base_root)?;
                    (resolved, manifest.dependencies)
                } else if let Some(git) = &detailed.git {
                    let (resolved, manifest) = self.resolve_from_git_with_manifest(name, git, detailed)?;
                    (resolved, manifest.dependencies)
                } else {
                    (self.resolve_from_registry(name, &detailed.version)?, HashMap::new())
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
        Err(CompileError::without_span(format!(
            "registry dependency '{}' with version '{}' is not supported yet; use a local path dependency",
            name, version
        )))
    }

    pub fn resolve_from_path(&self, name: &str, path: &str) -> Result<ResolvedPackage> {
        let (resolved, _) = self.resolve_from_path_at(name, path, &self.root)?;
        Ok(resolved)
    }

    fn resolve_from_path_at(&self, name: &str, path: &str, base_root: &Path) -> Result<(ResolvedPackage, PackageManifest)> {
        let package_path = canonical_package_child_path(base_root, path, &format!("dependency '{}' path", name))?;
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
        validate_git_url(url)?;
        validate_git_dependency_pin(name, detailed)?;

        let cache_dir = self.git_cache_dir();
        std::fs::create_dir_all(&cache_dir).map_err(|e| {
            CompileError::without_span(format!("failed to create git cache directory '{}': {}", cache_dir.display(), e))
        })?;
        let cache_dir = std::fs::canonicalize(&cache_dir).map_err(|e| {
            CompileError::without_span(format!("failed to canonicalize git cache directory '{}': {}", cache_dir.display(), e))
        })?;

        let requested_ref = detailed.rev.as_ref();
        let cache_name = git_cache_entry_name(name, url, requested_ref.map(String::as_str));
        let clone_dir = cache_dir.join(&cache_name);
        ensure_git_cache_child(&cache_dir, &clone_dir)?;

        let git_result = if clone_dir.exists() && clone_dir.join(".git").exists() {
            Self::git_update(&clone_dir)
        } else {
            remove_git_cache_child(&cache_dir, &clone_dir).map_err(|e| {
                CompileError::without_span(format!("failed to remove stale git cache entry '{}': {}", clone_dir.display(), e))
            })?;
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
            },
            manifest,
        ))
    }

    fn git_cache_dir(&self) -> PathBuf {
        self.root.join(".cell/git-cache")
    }

    fn git_clone(url: &str, target: &Path) -> std::result::Result<(), String> {
        validate_git_url(url).map_err(|error| error.message)?;
        let mut command = Self::git_command();
        let output = command.args(Self::git_clone_args(url, target)).output().map_err(|e| format!("failed to execute git: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git clone failed: {}", stderr.trim()));
        }

        Ok(())
    }

    fn git_update(clone_dir: &Path) -> std::result::Result<(), String> {
        let mut command = Self::git_command();
        let output = command
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
        validate_git_revision(ref_str).map_err(|error| error.message)?;
        let mut fetch_command = Self::git_command();
        let fetch = fetch_command
            .args(Self::git_fetch_ref_args(ref_str))
            .current_dir(clone_dir)
            .output()
            .map_err(|e| format!("failed to execute git fetch: {}", e))?;
        if !fetch.status.success() {
            let stderr = String::from_utf8_lossy(&fetch.stderr);
            return Err(format!("git fetch {} failed: {}", ref_str, stderr.trim()));
        }

        let mut checkout_command = Self::git_command();
        let output = checkout_command
            .args(Self::git_checkout_args(ref_str))
            .current_dir(clone_dir)
            .output()
            .map_err(|e| format!("failed to execute git checkout: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git checkout {} failed: {}", ref_str, stderr.trim()));
        }

        Ok(())
    }

    fn git_command() -> Command {
        let mut command = Command::new("git");
        command.args(["-c", "protocol.ext.allow=never", "-c", "protocol.file.allow=never"]);
        command
    }

    fn git_clone_args(url: &str, target: &Path) -> Vec<OsString> {
        vec![OsString::from("clone"), OsString::from("--"), OsString::from(url), target.as_os_str().to_os_string()]
    }

    fn git_fetch_ref_args(ref_str: &str) -> Vec<OsString> {
        vec![OsString::from("fetch"), OsString::from("origin"), OsString::from("--"), OsString::from(ref_str)]
    }

    fn git_checkout_args(ref_str: &str) -> Vec<OsString> {
        vec![OsString::from("checkout"), OsString::from("--"), OsString::from(ref_str)]
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

fn canonical_package_child_path(base_root: &Path, raw_path: &str, label: &str) -> Result<PathBuf> {
    reject_package_path_escape(raw_path, label)?;
    let canonical_root = std::fs::canonicalize(base_root)
        .map_err(|e| CompileError::without_span(format!("failed to canonicalize package root '{}': {}", base_root.display(), e)))?;
    let candidate = base_root.join(raw_path);
    if !candidate.exists() {
        return Err(CompileError::without_span(format!("{} '{}' does not exist", label, candidate.display())));
    }
    let canonical_candidate = std::fs::canonicalize(&candidate)
        .map_err(|e| CompileError::without_span(format!("failed to canonicalize {} '{}': {}", label, candidate.display(), e)))?;
    if !canonical_candidate.starts_with(&canonical_root) {
        return Err(CompileError::without_span(format!(
            "{} '{}' resolves outside package root '{}'",
            label,
            raw_path,
            canonical_root.display()
        )));
    }
    Ok(canonical_candidate)
}

fn reject_package_path_escape(raw_path: &str, label: &str) -> Result<()> {
    let path = Path::new(raw_path);
    if raw_path.is_empty() {
        return Err(CompileError::without_span(format!("{} path must not be empty", label)));
    }
    if path.is_absolute() {
        return Err(CompileError::without_span(format!("{} '{}' must be relative to the package root", label, raw_path)));
    }
    if path.components().any(|component| matches!(component, Component::ParentDir | Component::Prefix(_) | Component::RootDir)) {
        return Err(CompileError::without_span(format!("{} '{}' must stay inside the package root", label, raw_path)));
    }
    Ok(())
}

fn validate_git_url(url: &str) -> Result<()> {
    if url.is_empty() || url.trim() != url {
        return Err(CompileError::without_span("git dependency URL must not be empty or padded with whitespace"));
    }
    if url.bytes().any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace()) {
        return Err(CompileError::without_span(format!("git dependency URL '{}' contains whitespace or control characters", url)));
    }
    if url.contains("::") {
        return Err(CompileError::without_span(format!(
            "git dependency URL '{}' uses a Git remote-helper/ext-style transport; use https://, http://, git://, ssh://, or scp-like SSH",
            url
        )));
    }
    if url.bytes().any(is_git_url_shell_metachar) {
        return Err(CompileError::without_span(format!("git dependency URL '{}' contains unsupported shell metacharacters", url)));
    }

    if let Some((scheme, _rest)) = url.split_once("://") {
        let scheme = scheme.to_ascii_lowercase();
        if matches!(scheme.as_str(), "https" | "http" | "git" | "ssh") {
            return Ok(());
        }
        return Err(CompileError::without_span(format!(
            "git dependency URL '{}' uses unsupported scheme '{}'; allowed schemes are https, http, git, and ssh",
            url, scheme
        )));
    }

    if is_scp_like_ssh_url(url) {
        return Ok(());
    }

    Err(CompileError::without_span(format!(
        "git dependency URL '{}' must use https://, http://, git://, ssh://, or scp-like SSH",
        url
    )))
}

fn validate_git_dependency_pin(name: &str, detailed: &DetailedDependency) -> Result<()> {
    if detailed.branch.is_some() || detailed.tag.is_some() {
        return Err(CompileError::without_span(format!(
            "git dependency '{}' must pin an immutable rev; branch and tag refs are not accepted",
            name
        )));
    }

    let Some(rev) = detailed.rev.as_deref() else {
        return Err(CompileError::without_span(format!(
            "git dependency '{}' must specify a full commit rev for provenance; branch/tag/default-branch dependencies are not accepted",
            name
        )));
    };

    validate_git_revision(rev)
}

pub(crate) fn validate_git_revision(rev: &str) -> Result<()> {
    let is_full_sha1 = rev.len() == 40 && rev.bytes().all(|byte| byte.is_ascii_hexdigit());
    let is_full_sha256 = rev.len() == 64 && rev.bytes().all(|byte| byte.is_ascii_hexdigit());
    if is_full_sha1 || is_full_sha256 {
        return Ok(());
    }

    Err(CompileError::without_span("git dependency rev must be a full 40-character SHA-1 or 64-character SHA-256 commit hash"))
}

fn is_git_url_shell_metachar(byte: u8) -> bool {
    matches!(byte, b';' | b'|' | b'&' | b'`' | b'$' | b'<' | b'>' | b'\'' | b'"' | b'\\')
}

fn is_scp_like_ssh_url(url: &str) -> bool {
    let Some((authority, path)) = url.split_once(':') else {
        return false;
    };
    !authority.is_empty()
        && authority.contains('@')
        && !authority.starts_with('-')
        && !path.is_empty()
        && !path.starts_with('-')
        && !path.starts_with('/')
}

fn git_cache_entry_name(name: &str, url: &str, requested_ref: Option<&str>) -> String {
    let cache_key = format!("{}\0{}\0{}", name, url, requested_ref.unwrap_or("HEAD"));
    let digest = blake2b_simd::Params::new().hash_length(16).personal(b"CellPkgGitCache").hash(cache_key.as_bytes());
    format!("git-{}", digest.to_hex())
}

fn ensure_git_cache_child(cache_root: &Path, target: &Path) -> Result<()> {
    if target.file_name().is_none() {
        return Err(CompileError::without_span(format!("invalid git cache target '{}'", target.display())));
    }

    let parent =
        target.parent().ok_or_else(|| CompileError::without_span(format!("invalid git cache target '{}'", target.display())))?;
    let canonical_parent = std::fs::canonicalize(parent).map_err(|e| {
        CompileError::without_span(format!("failed to canonicalize git cache target parent '{}': {}", parent.display(), e))
    })?;
    if canonical_parent != cache_root {
        return Err(CompileError::without_span(format!(
            "git cache target '{}' resolves outside cache root '{}'",
            target.display(),
            cache_root.display()
        )));
    }

    if target.exists() {
        let canonical_target = std::fs::canonicalize(target).map_err(|e| {
            CompileError::without_span(format!("failed to canonicalize git cache target '{}': {}", target.display(), e))
        })?;
        if !canonical_target.starts_with(cache_root) {
            return Err(CompileError::without_span(format!(
                "git cache target '{}' resolves outside cache root '{}'",
                target.display(),
                cache_root.display()
            )));
        }
    }

    Ok(())
}

fn remove_git_cache_child(cache_root: &Path, target: &Path) -> Result<()> {
    ensure_git_cache_child(cache_root, target)?;
    if target.exists() {
        std::fs::remove_dir_all(target)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    pub version: u32,
    pub dependencies: BTreeMap<String, LockedDependency>,
}

impl Lockfile {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new() -> Self {
        Self { version: Self::CURRENT_VERSION, dependencies: BTreeMap::new() }
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
                    PackageSource::Registry { name: reg_name, version } => {
                        LockedSource::Registry { name: reg_name.clone(), version: version.clone() }
                    }
                },
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
                issues.extend(lock_dependency_consistency_issues(name, dep, locked));
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
            PackageSource::Registry { name: package_name, version: package_version },
            LockedSource::Registry { name: locked_name, version: locked_version },
        ) if locked_name == package_name && locked_version == package_version => {}
        (_, source) => issues.push(format!(
            "resolved dependency '{}' expects {} but Cell.lock records {}",
            name,
            package_source_display(&package.source),
            locked_source_display(source)
        )),
    }

    issues
}

fn lock_dependency_consistency_issues(name: &str, dep: &Dependency, locked: &LockedDependency) -> Vec<String> {
    let mut issues = Vec::new();

    match dep {
        Dependency::Simple(version) => match &locked.source {
            LockedSource::Registry { name: locked_name, version: locked_version }
                if locked_name == name && locked_version == version => {}
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
                            if revision != rev {
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
                    LockedSource::Registry { name: locked_name, version: locked_version }
                        if locked_name == name && locked_version == &detail.version => {}
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
        LockedSource::Registry { name, version } => format!("registry {}@{}", name, version),
    }
}

fn package_source_display(source: &PackageSource) -> String {
    match source {
        PackageSource::Local(path) => format!("path '{}'", path.display()),
        PackageSource::Git { url, revision } => format!("git '{}#{}'", url, revision),
        PackageSource::Registry { name, version } => format!("registry {}@{}", name, version),
    }
}

impl Default for Lockfile {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedDependency {
    pub version: String,
    pub source: LockedSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LockedSource {
    Path { path: String },
    Git { url: String, revision: String },
    Registry { name: String, version: String },
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
        assert_eq!(manager.get_source_paths(), vec![std::fs::canonicalize(root.join("deps/math/src")).unwrap()]);
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
        std::fs::create_dir_all(root.join("deps/math/deps/util/src")).unwrap();
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
path = "deps/util"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("deps/math/deps/util/Cell.toml"),
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
    fn package_manager_rejects_local_path_dependency_traversal() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("deps/math/src")).unwrap();
        std::fs::create_dir_all(root.join("outside/src")).unwrap();
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
version = "0.1.0"

[dependencies.outside]
path = "../outside"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("outside/Cell.toml"),
            r#"
[package]
name = "outside"
version = "0.1.0"
"#,
        )
        .unwrap();

        let mut manager = PackageManager::new(root);
        let error = manager.resolve_dependencies().unwrap_err();

        assert!(error.message.contains("must stay inside the package root"), "{}", error.message);
    }

    #[test]
    fn package_manager_rejects_transitive_path_dependency_cycles() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("deps/a/src")).unwrap();
        std::fs::create_dir_all(root.join("deps/a/deps/b/deps/a/src")).unwrap();
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
path = "deps/b"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("deps/a/deps/b/Cell.toml"),
            r#"
[package]
name = "b"
version = "0.1.0"

[dependencies.a]
path = "deps/a"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("deps/a/deps/b/deps/a/Cell.toml"),
            r#"
[package]
name = "a"
version = "0.1.0"
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
            LockedDependency { version: "0.2.0".to_string(), source: LockedSource::Path { path: "deps/old-math".to_string() } },
        );
        lockfile.dependencies.insert(
            "stale".to_string(),
            LockedDependency {
                version: "1.0.0".to_string(),
                source: LockedSource::Registry { name: "stale".to_string(), version: "1.0.0".to_string() },
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
            LockedDependency { version: "0.1.0".to_string(), source: LockedSource::Path { path: "deps/math".to_string() } },
        );
        lockfile.dependencies.insert(
            "util".to_string(),
            LockedDependency { version: "0.1.0".to_string(), source: LockedSource::Path { path: "deps/math/../util".to_string() } },
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
            },
        );

        let issues = lockfile.consistency_issues_with_resolved(&manifest, &resolved);

        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn lockfile_consistency_requires_exact_git_revision_match() {
        let manifest: PackageManifest = toml::from_str(
            r#"
[package]
name = "app"
version = "0.1.0"

[dependencies.math]
version = "0.1.0"
git = "https://example.com/math.git"
rev = "0123456789abcdef0123456789abcdef01234567"
"#,
        )
        .unwrap();
        let mut lockfile = Lockfile::new();
        lockfile.dependencies.insert(
            "math".to_string(),
            LockedDependency {
                version: "0.1.0".to_string(),
                source: LockedSource::Git {
                    url: "https://example.com/math.git".to_string(),
                    revision: "0123456789abcdef0123456789abcdef0123456".to_string(),
                },
            },
        );

        let issues = lockfile.consistency_issues(&manifest);

        assert!(issues.iter().any(|issue| issue.contains("expects git revision")), "{issues:?}");
        assert!(!lockfile.is_consistent(&manifest));
    }

    #[test]
    fn lockfile_replace_with_resolved_prunes_removed_dependencies() {
        let mut lockfile = Lockfile::new();
        lockfile.dependencies.insert(
            "old".to_string(),
            LockedDependency {
                version: "1.0.0".to_string(),
                source: LockedSource::Registry { name: "old".to_string(), version: "1.0.0".to_string() },
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

        assert!(error.message.contains("registry dependency 'remote'"));
        assert!(error.message.contains("not supported yet"));
        assert!(error.message.contains("local path dependency"));
        assert!(manager.get_resolved().is_empty());
    }

    #[test]
    fn git_cache_entry_name_is_hash_only() {
        let cache_name = git_cache_entry_name("../../outside", "https://example.com/repo.git", Some("../rev"));

        assert!(cache_name.starts_with("git-"));
        assert!(!cache_name.contains(".."));
        assert!(!cache_name.contains('/'));
        assert!(!cache_name.contains('\\'));
        let digest = cache_name.strip_prefix("git-").unwrap();
        assert_eq!(digest.len(), 32);
        assert!(digest.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn git_cache_child_check_rejects_path_escape() {
        let temp = tempdir().unwrap();
        let cache_root = temp.path().join(".cell").join("git-cache");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&cache_root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let cache_root = std::fs::canonicalize(cache_root).unwrap();
        let escaped = cache_root.join("..").join("..").join("outside");
        let error = ensure_git_cache_child(&cache_root, &escaped).unwrap_err();

        assert!(error.message.contains("outside cache root"), "{}", error.message);
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
rev = "0123456789abcdef0123456789abcdef01234567"
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
    fn package_manager_rejects_unpinned_git_dependency_before_fetch() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("Cell.toml"),
            r#"
[package]
name = "app"
version = "0.1.0"

[dependencies.remote]
version = "0.1.0"
git = "https://example.com/remote.git"
"#,
        )
        .unwrap();

        let mut manager = PackageManager::new(temp.path());
        let error = manager.resolve_dependencies().unwrap_err();

        assert!(error.message.contains("must specify a full commit rev"), "{}", error.message);
        assert!(manager.get_resolved().is_empty());
    }

    #[test]
    fn package_manager_rejects_branch_or_tag_git_dependency_before_fetch() {
        for ref_key in ["branch", "tag"] {
            let temp = tempdir().unwrap();
            std::fs::write(
                temp.path().join("Cell.toml"),
                format!(
                    r#"
[package]
name = "app"
version = "0.1.0"

[dependencies.remote]
version = "0.1.0"
git = "https://example.com/remote.git"
{ref_key} = "main"
"#
                ),
            )
            .unwrap();

            let mut manager = PackageManager::new(temp.path());
            let error = manager.resolve_dependencies().unwrap_err();

            assert!(error.message.contains("branch and tag refs are not accepted"), "{}", error.message);
            assert!(manager.get_resolved().is_empty());
        }
    }

    #[test]
    fn package_manager_rejects_unsafe_git_url_transports() {
        for url in [
            "ext::sh -c touch /tmp/cellscript-owned",
            "file:///tmp/local.git",
            "hg::https://example.com/repo",
            "https://example.com/repo.git;touch-owned",
            "https://example.com/repo.git\nssh://example.com/other.git",
        ] {
            let error = validate_git_url(url).expect_err("unsafe git URL should be rejected");
            assert!(error.message.contains("git dependency URL"), "unexpected error for {url}: {}", error.message);
        }
    }

    #[test]
    fn package_manager_accepts_allowed_git_url_transports() {
        for url in [
            "https://example.com/org/repo.git",
            "http://example.com/org/repo.git",
            "git://example.com/org/repo.git",
            "ssh://git@example.com/org/repo.git",
            "git@example.com:org/repo.git",
        ] {
            validate_git_url(url).unwrap_or_else(|error| panic!("expected {url} to be accepted: {}", error.message));
        }
    }

    #[test]
    fn package_manager_git_commands_separate_user_controlled_ref_arguments() {
        let target = Path::new("/tmp/cellscript-git-target");
        let clone_args = PackageManager::git_clone_args("--upload-pack=calc.exe", target);
        let fetch_args = PackageManager::git_fetch_ref_args("--upload-pack=calc.exe");
        let checkout_args = PackageManager::git_checkout_args("--upload-pack=calc.exe");

        let clone = clone_args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>();
        let fetch = fetch_args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>();
        let checkout = checkout_args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>();

        assert_eq!(clone, vec!["clone", "--", "--upload-pack=calc.exe", "/tmp/cellscript-git-target"]);
        assert_eq!(fetch, vec!["fetch", "origin", "--", "--upload-pack=calc.exe"]);
        assert_eq!(checkout, vec!["checkout", "--", "--upload-pack=calc.exe"]);
    }

    #[test]
    fn package_manager_git_checkout_revalidates_full_commit_refs() {
        let err = PackageManager::git_checkout(Path::new("/tmp/cellscript-not-a-git-repo"), "--upload-pack=calc.exe")
            .expect_err("git checkout helper must reject non-commit refs before invoking git");

        assert!(err.contains("full 40-character SHA-1"), "unexpected error: {}", err);
    }

    #[test]
    fn package_manager_git_update_fails_closed_on_fetch_error() {
        if std::process::Command::new("git").arg("--version").output().is_err() {
            return;
        }

        let temp = tempdir().unwrap();
        let output = std::process::Command::new("git").arg("init").current_dir(temp.path()).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

        let error = PackageManager::git_update(temp.path()).unwrap_err();
        assert!(error.contains("git fetch failed"), "{}", error);
    }
}
