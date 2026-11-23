use crate::artifact::{validate_declarations, ArtifactDeclaration};
use crate::edition::{CellScriptEdition, CURRENT_EDITION};
use crate::error::{CompileError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

mod external_resolution;
mod local_dependencies;
mod locked_resolution;
mod registry_resolution;
mod resolution;

#[cfg(all(feature = "cli", not(feature = "wasm")))]
pub mod frozen_interface;
pub mod inspection;
pub mod registry;
#[cfg(feature = "cli")]
pub mod upgrade;
pub mod workspace;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    pub package: PackageInfo,
    #[serde(default)]
    pub workspace: Option<WorkspaceConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactDeclaration>,
    #[serde(default)]
    pub dependencies: HashMap<String, Dependency>,
    #[serde(default)]
    pub dev_dependencies: HashMap<String, Dependency>,
    #[serde(default)]
    pub features: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub environments: BTreeMap<String, CkbEnvironment>,
    #[serde(default)]
    pub dependency_overrides: BTreeMap<String, BTreeMap<String, Dependency>>,
    #[serde(default)]
    pub resolvers: BTreeMap<String, ResolverConfig>,
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
    pub edition: CellScriptEdition,
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
    /// SemVer requirement for the CellScript compiler that may load this
    /// package. Omission preserves legacy packages as unconstrained; newly
    /// created packages record an explicit minimum.
    #[serde(default = "default_compiler_requirement")]
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

fn default_compiler_requirement() -> String {
    "*".to_string()
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

fn relative_path(from: &Path, to: &Path) -> Option<PathBuf> {
    let from_components: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();
    let common = from_components.iter().zip(&to_components).take_while(|(left, right)| left == right).count();
    if common == 0 {
        return None;
    }
    let mut relative = PathBuf::new();
    for _ in common..from_components.len() {
        relative.push("..");
    }
    for component in &to_components[common..] {
        relative.push(component.as_os_str());
    }
    Some(if relative.as_os_str().is_empty() { PathBuf::from(".") } else { relative })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
// Keep the public manifest API and untagged TOML representation source-compatible;
// boxing Detailed would force every programmatic manifest author to wrap it.
#[allow(clippy::large_enum_variant)]
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
    /// Declared package name when the dependency's local alias differs.
    #[serde(default)]
    pub package: Option<String>,
    /// Name of a bounded external resolver declared in `[resolvers.<name>]`.
    /// It is invoked only by explicit lock/update operations and is normalized
    /// to an ordinary immutable Registry or Git source before Cell.lock is written.
    #[serde(default)]
    pub resolver: Option<String>,
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
    /// Explicit dependency-local environment name for this edge. The selected
    /// environment must have the same CKB chain identity as the root package.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_environment: Option<String>,
    /// Declare that this dependency edge does not apply a dependency-local
    /// environment override. The root chain identity is still preserved for
    /// the dependency's transitive edges and external resolver requests.
    #[serde(default, skip_serializing_if = "is_false")]
    pub environment_independent: bool,
    /// Persisted acknowledgement that this dependency may resolve from a
    /// source_published or indexed_pending Registry entry.
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_unverified: bool,
    /// Persisted incident-review acknowledgement for quarantined entries.
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_quarantined: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolverConfig {
    pub command: String,
    pub sha256: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ExternalResolverRequest<'a> {
    schema: &'static str,
    alias: &'a str,
    package: &'a str,
    version_requirement: &'a str,
    environment: Option<ExternalResolverEnvironment<'a>>,
}

#[derive(Debug, Serialize)]
struct ExternalResolverEnvironment<'a> {
    root_name: &'a str,
    local_name: Option<&'a str>,
    chain_id: &'a str,
    genesis_hash: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExternalResolverResponse {
    schema: String,
    dependency: ExternalResolvedDependency,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExternalResolvedDependency {
    package: String,
    version: String,
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    git: Option<String>,
    #[serde(default)]
    rev: Option<String>,
}

const EXTERNAL_RESOLVER_REQUEST_SCHEMA: &str = "cellscript-dependency-resolver-request-v2";
const EXTERNAL_RESOLVER_RESPONSE_SCHEMA: &str = "cellscript-dependency-resolver-response-v1";
const EXTERNAL_RESOLVER_TIMEOUT: Duration = Duration::from_secs(10);
const EXTERNAL_RESOLVER_MAX_OUTPUT_BYTES: u64 = 1024 * 1024;
/// Publisher-supplied Cell.lock graphs are fully validated before dependency
/// selection, so node admission stays bounded: the recursive cycle search
/// and linear-membership graph operations must never see an unbounded map.
const MAX_LOCKFILE_GRAPH_NODES: usize = 4096;

fn is_false(value: &bool) -> bool {
    !*value
}

fn default_true() -> bool {
    true
}

fn default_any_version() -> String {
    "*".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    #[serde(default)]
    pub trusted_external_verifiers: Vec<CkbTrustedExternalVerifierConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zk_verifiers: Vec<crate::zk_package::NamedZkVerifier>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CkbTrustedExternalVerifierConfig {
    pub schema: String,
    pub name: String,
    pub scope: String,
    pub operation: String,
    pub adapter: String,
    pub code_hash: String,
    pub hash_type: String,
    pub source_identity: String,
    pub applicability: String,
    pub trust_basis: String,
    pub guarantees: Vec<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CkbEnvironment {
    pub chain_id: String,
    pub genesis_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyScope {
    Runtime,
    Test,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionOptions {
    pub scope: DependencyScope,
    pub features: BTreeSet<String>,
    pub all_features: bool,
    pub no_default_features: bool,
    pub environment: Option<String>,
    pub offline: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnvironmentSelectionPolicy {
    Root,
    InheritByChainIdentity,
    ExplicitLocalName,
    EnvironmentIndependent,
}

impl EnvironmentSelectionPolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::InheritByChainIdentity => "inherit-by-chain-identity",
            Self::ExplicitLocalName => "explicit-local-name",
            Self::EnvironmentIndependent => "environment-independent",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectedEnvironmentContext {
    root_name: String,
    local_name: Option<String>,
    chain_id: String,
    genesis_hash: String,
    policy: EnvironmentSelectionPolicy,
}

impl Default for ResolutionOptions {
    fn default() -> Self {
        Self {
            scope: DependencyScope::Runtime,
            features: BTreeSet::new(),
            all_features: false,
            no_default_features: false,
            environment: None,
            offline: false,
        }
    }
}

thread_local! {
    static RESOLUTION_OPTIONS_STACK: RefCell<Vec<ResolutionOptions>> = const { RefCell::new(Vec::new()) };
    static LOCKFILE_OVERRIDE_STACK: RefCell<Vec<(PathBuf, Lockfile)>> = const { RefCell::new(Vec::new()) };
}

pub fn with_resolution_options<T>(options: ResolutionOptions, operation: impl FnOnce() -> T) -> T {
    RESOLUTION_OPTIONS_STACK.with(|stack| stack.borrow_mut().push(options));
    struct PopResolutionOptions;
    impl Drop for PopResolutionOptions {
        fn drop(&mut self) {
            RESOLUTION_OPTIONS_STACK.with(|stack| {
                stack.borrow_mut().pop();
            });
        }
    }
    let _guard = PopResolutionOptions;
    operation()
}

pub(crate) fn active_resolution_options(scope: DependencyScope) -> ResolutionOptions {
    RESOLUTION_OPTIONS_STACK.with(|stack| {
        let mut options = stack.borrow().last().cloned().unwrap_or_default();
        options.scope = scope;
        options
    })
}

/// Execute a compiler operation against an in-memory candidate lockfile.
///
/// Upgrade planning uses this boundary to compile the old and candidate graphs
/// without ever replacing the authoritative `Cell.lock` on disk.
pub fn with_lockfile_override<T>(root: &Path, lockfile: Lockfile, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    let root = std::fs::canonicalize(root).map_err(|error| {
        CompileError::without_span(format!("failed to canonicalize lockfile override root '{}': {error}", root.display()))
    })?;
    LOCKFILE_OVERRIDE_STACK.with(|stack| stack.borrow_mut().push((root, lockfile)));
    struct PopLockfileOverride;
    impl Drop for PopLockfileOverride {
        fn drop(&mut self) {
            LOCKFILE_OVERRIDE_STACK.with(|stack| {
                stack.borrow_mut().pop();
            });
        }
    }
    let _guard = PopLockfileOverride;
    operation()
}

pub(crate) fn active_lockfile_override(root: &Path) -> Result<Option<Lockfile>> {
    let root = std::fs::canonicalize(root)
        .map_err(|error| CompileError::without_span(format!("failed to canonicalize package root '{}': {error}", root.display())))?;
    Ok(LOCKFILE_OVERRIDE_STACK
        .with(|stack| stack.borrow().iter().rev().find(|(candidate, _)| candidate == &root).map(|(_, lockfile)| lockfile.clone())))
}

/// Execute a compiler operation with dependency materialization disabled: a
/// missing locked dependency is an error, never an outbound request.
/// Verification subprocesses compile untrusted snapshots and must stay
/// network-silent end to end.
pub fn with_offline_dependencies<T>(operation: impl FnOnce() -> Result<T>) -> Result<T> {
    RESOLUTION_OPTIONS_STACK.with(|stack| {
        let mut options = stack.borrow().last().cloned().unwrap_or_default();
        options.offline = true;
        stack.borrow_mut().push(options);
    });
    struct PopResolutionOptions;
    impl Drop for PopResolutionOptions {
        fn drop(&mut self) {
            RESOLUTION_OPTIONS_STACK.with(|stack| {
                stack.borrow_mut().pop();
            });
        }
    }
    let _guard = PopResolutionOptions;
    operation()
}

pub struct PackageManager {
    root: PathBuf,
    resolved: BTreeMap<String, ResolvedPackage>,
    root_dependencies: BTreeMap<String, String>,
    selected_coordinates: BTreeMap<PackageCoordinate, SelectedPackageInstance>,
}

#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    pub node_id: String,
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub source: PackageSource,
    pub dependencies: BTreeMap<String, String>,
    pub namespace: Option<String>,
    pub source_hash: Option<String>,
    pub manifest_digest: String,
    pub compiler_requirement: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PackageCoordinate {
    namespace: Option<String>,
    name: String,
}

impl PackageCoordinate {
    fn from_package(package: &ResolvedPackage) -> Self {
        Self { namespace: package.namespace.clone(), name: package.name.clone() }
    }

    fn display(&self) -> String {
        self.namespace.as_deref().map_or_else(|| self.name.clone(), |namespace| format!("{namespace}/{}", self.name))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FeatureSelection {
    features: BTreeSet<String>,
    all_features: bool,
    default_features: bool,
}

impl FeatureSelection {
    fn from_options(options: &ResolutionOptions) -> Self {
        Self { features: options.features.clone(), all_features: options.all_features, default_features: !options.no_default_features }
    }

    fn labels(&self) -> Vec<String> {
        let mut labels = self.features.iter().cloned().collect::<Vec<_>>();
        if self.all_features {
            labels.push("*".to_string());
        }
        if self.default_features {
            labels.push("default".to_string());
        }
        labels.sort();
        labels
    }
}

#[derive(Debug, Clone)]
struct IncomingPackageRequest {
    parent_package: String,
    alias: String,
    version_requirement: Option<String>,
    candidate_version: String,
    candidate_source: String,
    features: FeatureSelection,
    environment: Option<String>,
}

#[derive(Debug, Clone)]
struct SelectedPackageInstance {
    node_id: String,
    version: String,
    manifest_digest: String,
    compiler_requirement: String,
    source_authority: String,
    source_identity: String,
    source_is_registry: bool,
    features: FeatureSelection,
    environment: Option<String>,
    incoming: Vec<IncomingPackageRequest>,
}

struct ResolutionTraversal<'a> {
    stack_ids: &'a mut Vec<String>,
    stack_labels: &'a mut Vec<String>,
    compiler_errors: &'a mut Vec<CompileError>,
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
        index
            .versions
            .iter()
            .filter(|v| !v.yanked && v.version != selected)
            .map(|v| v.version.clone())
            .max_by(|a, b| compare_semver(a, b))
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

fn compare_semver(left: &str, right: &str) -> std::cmp::Ordering {
    match (semver::Version::parse(left), semver::Version::parse(right)) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        (Ok(_), Err(_)) => std::cmp::Ordering::Greater,
        (Err(_), Ok(_)) => std::cmp::Ordering::Less,
        (Err(_), Err(_)) => left.cmp(right),
    }
}

#[derive(Debug, Clone)]
pub enum PackageSource {
    Local(PathBuf),
    Git { url: String, revision: String },
    Registry { registry: String, url: String, revision: String, namespace: String, version: String },
}

fn package_source_authority(package: &ResolvedPackage) -> String {
    match &package.source {
        PackageSource::Local(path) => format!("path:{}", path.to_string_lossy().replace('\\', "/")),
        PackageSource::Git { url, .. } => format!("git:{url}"),
        PackageSource::Registry { registry, namespace, .. } => format!("registry:{registry}:{namespace}/{}", package.name),
    }
}

fn package_source_identity(package: &ResolvedPackage) -> String {
    match &package.source {
        PackageSource::Local(path) => format!("path:{}", path.to_string_lossy().replace('\\', "/")),
        PackageSource::Git { url, revision } => format!("git:{url}#{revision}"),
        PackageSource::Registry { registry, url, revision, namespace, version } => {
            format!("registry:{registry}:{namespace}/{}@{version}#{revision}:{url}", package.name)
        }
    }
}

fn dependency_requirement_for_diagnostic(dependency: &Dependency) -> Option<String> {
    match dependency {
        Dependency::Simple(requirement) => Some(requirement.clone()),
        Dependency::Detailed(detail) if !detail.version.trim().is_empty() => Some(detail.version.clone()),
        Dependency::Detailed(_) => None,
    }
}

fn incoming_package_request_json(request: &IncomingPackageRequest) -> serde_json::Value {
    serde_json::json!({
        "from_package": request.parent_package,
        "alias": request.alias,
        "version_requirement": request.version_requirement,
        "candidate_version": request.candidate_version,
        "candidate_source": request.candidate_source,
        "features": request.features.labels(),
        "environment": request.environment,
    })
}

#[derive(Debug, Clone)]
pub enum VersionReq {
    Exact(String),
    Compatible(String),
    Range(String),
    Any,
}

fn manifest_digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

pub fn compute_manifest_digest(root: &Path) -> Result<String> {
    let path = root.join("Cell.toml");
    let bytes = std::fs::read(&path)
        .map_err(|error| CompileError::without_span(format!("failed to read manifest '{}': {}", path.display(), error)))?;
    Ok(manifest_digest(&bytes))
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn read_bounded_resolver_output(path: &Path) -> Result<Vec<u8>> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > EXTERNAL_RESOLVER_MAX_OUTPUT_BYTES {
        return Err(CompileError::without_span(format!("external resolver output '{}' exceeds 1 MiB", path.display())));
    }
    Ok(std::fs::read(path)?)
}

fn sanitize_node_component(value: &str) -> String {
    value
        .chars()
        .map(|character| if character.is_ascii_alphanumeric() || character == '-' || character == '_' { character } else { '_' })
        .take(80)
        .collect()
}

fn package_node_id(
    package: &ResolvedPackage,
    options: &ResolutionOptions,
    environment: Option<&SelectedEnvironmentContext>,
) -> String {
    let source = match &package.source {
        PackageSource::Local(path) => format!("path:{}", path.to_string_lossy().replace('\\', "/")),
        PackageSource::Git { url, revision } => format!("git:{url}#{revision}"),
        PackageSource::Registry { registry, namespace, version, revision, .. } => {
            format!("registry:{registry}:{namespace}/{}@{version}#{revision}", package.name)
        }
    };
    let mut features: Vec<_> = options.features.iter().cloned().collect();
    if options.all_features {
        features.push("*".to_string());
    }
    if !options.no_default_features {
        features.push("default".to_string());
    }
    features.sort();
    let environment = environment.map(environment_node_identity).unwrap_or_else(|| "default".to_string());
    format!(
        "{}@{}|{}|compiler={}|env={}|features={}",
        package.name,
        package.version,
        source,
        hex::encode(package.compiler_requirement.as_bytes()),
        environment,
        features.join(",")
    )
}

/// Parse the public `[package].cellscript_version` compatibility contract.
///
/// Existing manifests used bare versions such as `0.16` to mean the minimum
/// compiler generation they were written for. Preserve that meaning by
/// normalising a bare version to `>=version`; explicit operators use standard
/// SemVer requirement syntax.
pub fn parse_compiler_requirement(requirement: &str) -> Result<semver::VersionReq> {
    let requirement = requirement.trim();
    if requirement.is_empty() {
        return Err(CompileError::without_span(
            "package cellscript_version must not be empty; omit it for an unconstrained legacy package or use a SemVer requirement",
        )
        .with_code(crate::error::DiagnosticCode::E2600));
    }
    let normalized = if requirement == "*"
        || requirement.chars().any(|character| matches!(character, '<' | '>' | '=' | '^' | '~' | '*' | ',' | '|'))
    {
        requirement.to_string()
    } else {
        format!(">={requirement}")
    };
    semver::VersionReq::parse(&normalized).map_err(|error| {
        CompileError::without_span(format!("invalid package cellscript_version requirement '{}': {error}", requirement))
            .with_code(crate::error::DiagnosticCode::E2600)
    })
}

pub fn compiler_requirement_matches(requirement: &str, compiler_version: &str) -> Result<bool> {
    let requirement = parse_compiler_requirement(requirement)?;
    let compiler_version = semver::Version::parse(compiler_version).map_err(|error| {
        CompileError::without_span(format!("active CellScript compiler version '{compiler_version}' is not valid SemVer: {error}"))
            .with_code(crate::error::DiagnosticCode::E2600)
    })?;
    Ok(requirement.matches(&compiler_version))
}

/// Manifest-selected path fields join the package root on every hashing,
/// compilation, and publication surface. They must stay relative, inside the
/// package, and free of parent traversal or platform-specific prefixes, or a
/// crafted manifest could read sources or write build outputs outside the
/// package directory.
pub fn validate_package_path_selection(manifest: &PackageManifest) -> Result<()> {
    let check = |value: &str, label: &str| -> Result<()> {
        let path = std::path::Path::new(value);
        if value.is_empty()
            || value.contains('\0')
            || path.is_absolute()
            || path.components().any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Err(CompileError::without_span(format!(
                "{label} '{value}' must be a relative path that stays inside the package root"
            ))
            .with_code(crate::error::DiagnosticCode::E2604));
        }
        Ok(())
    };
    check(&manifest.package.entry, "package entry")?;
    for root in &manifest.package.source_roots {
        check(root, "package source root")?;
    }
    for pattern in manifest.package.include.iter().chain(manifest.package.exclude.iter()) {
        check(pattern, "package include/exclude path")?;
    }
    if let Some(out_dir) = &manifest.build.out_dir {
        check(out_dir, "build out_dir")?;
    }
    Ok(())
}

pub fn validate_package_compiler_requirement(package: &PackageInfo) -> Result<()> {
    if compiler_requirement_matches(&package.cellscript_version, crate::VERSION)? {
        return Ok(());
    }
    Err(CompileError::without_span(format!(
        "package '{}@{}' requires CellScript compiler '{}', but active cellc is '{}'; select a compatible package/compiler version before loading source",
        package.name, package.version, package.cellscript_version, crate::VERSION
    ))
    .with_code(crate::error::DiagnosticCode::E2600)
    .with_details(serde_json::json!({
        "package": package.name,
        "package_version": package.version,
        "compiler_requirement": package.cellscript_version,
        "active_compiler_version": crate::VERSION,
        "phase": "manifest-before-source",
    })))
}

fn compiler_error_with_incoming_edge(mut error: CompileError, parent_package: &str, alias: &str, package: &str) -> CompileError {
    if error.code.as_deref() != Some("E2600") {
        return error;
    }
    let mut details = error.details.take().and_then(|value| value.as_object().cloned()).unwrap_or_default();
    details.entry("package".to_string()).or_insert_with(|| serde_json::Value::String(package.to_string()));
    details.insert(
        "incoming_edge".to_string(),
        serde_json::json!({
            "from_package": parent_package,
            "alias": alias,
            "to_package": package,
        }),
    );
    error.details = Some(serde_json::Value::Object(details));
    error
}

fn aggregate_compiler_incompatibilities(errors: Vec<CompileError>) -> CompileError {
    let entries = errors
        .iter()
        .map(|error| {
            let mut entry = error.details.clone().unwrap_or_else(|| serde_json::json!({}));
            if let Some(object) = entry.as_object_mut() {
                object.insert("message".to_string(), serde_json::Value::String(error.message.clone()));
            }
            entry
        })
        .collect::<Vec<_>>();
    let packages =
        entries.iter().filter_map(|entry| entry.get("package").and_then(serde_json::Value::as_str)).collect::<Vec<_>>().join(", ");
    CompileError::without_span(format!(
        "{} package compiler requirement(s) are incompatible with cellc {}: {}",
        errors.len(),
        crate::VERSION,
        packages
    ))
    .with_code(crate::error::DiagnosticCode::E2600)
    .with_details(serde_json::json!({
        "active_compiler_version": crate::VERSION,
        "phase": "dependency-compatibility-preflight",
        "incompatible_packages": entries,
    }))
    .with_related(errors)
}

fn environment_node_identity(environment: &SelectedEnvironmentContext) -> String {
    let local_name = environment.local_name.as_deref().map(hex::encode).unwrap_or_else(|| "-".to_string());
    format!(
        "{}:root={}:local={}:chain={}:genesis={}",
        environment.policy.as_str(),
        hex::encode(environment.root_name.as_bytes()),
        local_name,
        hex::encode(environment.chain_id.as_bytes()),
        environment.genesis_hash
    )
}

fn dependency_package_name(alias: &str, dependency: &Dependency) -> String {
    match dependency {
        Dependency::Detailed(detail) => detail.package.clone().unwrap_or_else(|| alias.to_string()),
        Dependency::Simple(_) => alias.to_string(),
    }
}

fn dependency_is_optional(dependency: &Dependency) -> bool {
    matches!(dependency, Dependency::Detailed(detail) if detail.optional)
}

fn dependency_resolution_options(
    dependency: &Dependency,
    parent: &ResolutionOptions,
    environment: Option<&SelectedEnvironmentContext>,
) -> ResolutionOptions {
    let mut options = ResolutionOptions {
        scope: DependencyScope::Runtime,
        environment: environment.and_then(|selected| selected.local_name.clone()),
        offline: parent.offline,
        ..ResolutionOptions::default()
    };
    if let Dependency::Detailed(detail) = dependency {
        options.features.extend(detail.features.iter().cloned());
        options.no_default_features = !detail.default_features;
    }
    options
}

fn active_optional_dependencies(manifest: &PackageManifest, options: &ResolutionOptions) -> Result<BTreeSet<String>> {
    let mut requested = options.features.clone();
    if options.all_features {
        requested.extend(manifest.features.keys().filter(|name| name.as_str() != "default").cloned());
    }
    if !options.no_default_features && manifest.features.contains_key("default") {
        requested.insert("default".to_string());
    }

    let mut active_dependencies = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut visiting = Vec::new();
    for feature in requested {
        expand_feature(manifest, &feature, &mut visited, &mut visiting, &mut active_dependencies)?;
    }
    Ok(active_dependencies)
}

fn expand_feature(
    manifest: &PackageManifest,
    feature: &str,
    visited: &mut BTreeSet<String>,
    visiting: &mut Vec<String>,
    active_dependencies: &mut BTreeSet<String>,
) -> Result<()> {
    if visited.contains(feature) {
        return Ok(());
    }
    if visiting.iter().any(|candidate| candidate == feature) {
        let mut cycle = visiting.clone();
        cycle.push(feature.to_string());
        return Err(CompileError::without_span(format!("feature cycle detected: {}", cycle.join(" -> "))));
    }
    let members = manifest
        .features
        .get(feature)
        .ok_or_else(|| CompileError::without_span(format!("unknown package feature '{}'", feature)))?
        .clone();
    visiting.push(feature.to_string());
    for member in members {
        if let Some(alias) = member.strip_prefix("dep:") {
            let dependency = manifest.dependencies.get(alias).or_else(|| manifest.dev_dependencies.get(alias)).ok_or_else(|| {
                CompileError::without_span(format!("feature '{}' activates unknown dependency alias '{}'", feature, alias))
            })?;
            if !dependency_is_optional(dependency) {
                return Err(CompileError::without_span(format!(
                    "feature '{}' uses dep:{} but dependency '{}' is not optional",
                    feature, alias, alias
                )));
            }
            active_dependencies.insert(alias.to_string());
        } else {
            expand_feature(manifest, &member, visited, visiting, active_dependencies)?;
        }
    }
    visiting.pop();
    visited.insert(feature.to_string());
    Ok(())
}

fn validate_environment(name: &str, environment: &CkbEnvironment) -> Result<()> {
    if name.trim().is_empty() || environment.chain_id.trim().is_empty() {
        return Err(CompileError::without_span("package environment names and chain_id values must not be empty"));
    }
    let hash = environment.genesis_hash.strip_prefix("0x").unwrap_or(&environment.genesis_hash);
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CompileError::without_span(format!("environment '{}' genesis_hash must contain exactly 32 bytes of hex", name)));
    }
    Ok(())
}

fn normalized_genesis_hash(value: &str) -> String {
    format!("0x{}", value.strip_prefix("0x").unwrap_or(value).to_ascii_lowercase())
}

fn same_chain_identity(left: &CkbEnvironment, right: &SelectedEnvironmentContext) -> bool {
    left.chain_id == right.chain_id && normalized_genesis_hash(&left.genesis_hash) == right.genesis_hash
}

fn root_environment_context(manifest: &PackageManifest, options: &ResolutionOptions) -> Result<Option<SelectedEnvironmentContext>> {
    let Some(name) = options.environment.as_deref() else {
        return Ok(None);
    };
    let environment = manifest.environments.get(name).ok_or_else(|| {
        CompileError::without_span(format!(
            "unknown package environment '{}'; declare [environments.{}] with chain_id and genesis_hash",
            name, name
        ))
    })?;
    validate_environment(name, environment)?;
    Ok(Some(SelectedEnvironmentContext {
        root_name: name.to_string(),
        local_name: Some(name.to_string()),
        chain_id: environment.chain_id.clone(),
        genesis_hash: normalized_genesis_hash(&environment.genesis_hash),
        policy: EnvironmentSelectionPolicy::Root,
    }))
}

fn dependency_environment_directive(dependency: &Dependency) -> (Option<&str>, bool) {
    match dependency {
        Dependency::Simple(_) => (None, false),
        Dependency::Detailed(detail) => (detail.use_environment.as_deref(), detail.environment_independent),
    }
}

fn dependency_environment_context(
    alias: &str,
    dependency: &Dependency,
    manifest: &PackageManifest,
    parent: Option<&SelectedEnvironmentContext>,
) -> Result<Option<SelectedEnvironmentContext>> {
    let (explicit_name, environment_independent) = dependency_environment_directive(dependency);
    if explicit_name.is_some() && environment_independent {
        return Err(CompileError::without_span(format!(
            "dependency '{}' cannot combine use_environment with environment_independent",
            alias
        )));
    }

    let Some(parent) = parent else {
        if let Some(name) = explicit_name {
            return Err(CompileError::without_span(format!(
                "dependency '{}' selects dependency environment '{}' without a root --environment chain identity",
                alias, name
            )));
        }
        if !environment_independent && !manifest.dependency_overrides.is_empty() {
            return Err(CompileError::without_span(format!(
                "dependency '{}' declares environment-specific dependency overrides but the edge has no selected chain identity; select a root --environment or set environment_independent = true",
                alias
            )));
        }
        return Ok(None);
    };

    let selected = if environment_independent {
        return Ok(Some(SelectedEnvironmentContext {
            root_name: parent.root_name.clone(),
            local_name: None,
            chain_id: parent.chain_id.clone(),
            genesis_hash: parent.genesis_hash.clone(),
            policy: EnvironmentSelectionPolicy::EnvironmentIndependent,
        }));
    } else if let Some(name) = explicit_name {
        let environment = manifest.environments.get(name).ok_or_else(|| {
            CompileError::without_span(format!("dependency '{}' maps to unknown dependency-local environment '{}'", alias, name))
        })?;
        if !same_chain_identity(environment, parent) {
            return Err(CompileError::without_span(format!(
                "dependency '{}' environment '{}' has chain identity '{}:{}' but the root selected '{}:{}'",
                alias,
                name,
                environment.chain_id,
                normalized_genesis_hash(&environment.genesis_hash),
                parent.chain_id,
                parent.genesis_hash
            )));
        }
        (name.to_string(), EnvironmentSelectionPolicy::ExplicitLocalName)
    } else {
        let matches = manifest
            .environments
            .iter()
            .filter(|(_, environment)| same_chain_identity(environment, parent))
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [name] => (name.clone(), EnvironmentSelectionPolicy::InheritByChainIdentity),
            [] if manifest.dependency_overrides.is_empty() => {
                return Ok(Some(SelectedEnvironmentContext {
                    root_name: parent.root_name.clone(),
                    local_name: None,
                    chain_id: parent.chain_id.clone(),
                    genesis_hash: parent.genesis_hash.clone(),
                    policy: EnvironmentSelectionPolicy::EnvironmentIndependent,
                }));
            }
            [] => {
                return Err(CompileError::without_span(format!(
                    "dependency '{}' has environment-specific overrides but no environment matches root chain identity '{}:{}'; set use_environment to an exact matching local environment or declare environment_independent = true",
                    alias, parent.chain_id, parent.genesis_hash
                )));
            }
            names => {
                return Err(CompileError::without_span(format!(
                    "dependency '{}' has multiple environments matching root chain identity '{}:{}': {}; set use_environment explicitly",
                    alias,
                    parent.chain_id,
                    parent.genesis_hash,
                    names.join(", ")
                )));
            }
        }
    };

    Ok(Some(SelectedEnvironmentContext {
        root_name: parent.root_name.clone(),
        local_name: Some(selected.0),
        chain_id: parent.chain_id.clone(),
        genesis_hash: parent.genesis_hash.clone(),
        policy: selected.1,
    }))
}

impl PackageManager {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();

        Self { root, resolved: BTreeMap::new(), root_dependencies: BTreeMap::new(), selected_coordinates: BTreeMap::new() }
    }

    pub fn read_manifest(&self) -> Result<PackageManifest> {
        let manifest_path = self.root.join("Cell.toml");

        if !manifest_path.exists() {
            return Err(CompileError::without_span("Cell.toml not found. Run 'cellc init' to create a new package.")
                .with_code(crate::error::DiagnosticCode::CLI0006));
        }

        let content = std::fs::read_to_string(&manifest_path)?;
        let manifest: PackageManifest = toml::from_str(&content)?;
        validate_declarations(&manifest.artifacts)?;
        validate_package_compiler_requirement(&manifest.package)?;
        validate_package_path_selection(&manifest)?;
        Ok(manifest)
    }

    pub fn write_manifest(&self, manifest: &PackageManifest) -> Result<()> {
        validate_declarations(&manifest.artifacts)?;
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
                edition: CURRENT_EDITION,
                namespace: None,
                authors: vec![],
                description: String::new(),
                license: String::new(),
                repository: String::new(),
                homepage: String::new(),
                documentation: String::new(),
                keywords: vec![],
                categories: vec![],
                cellscript_version: format!(">={}", crate::VERSION),
                entry: entry.to_string(),
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
        self.resolve_dependencies_with_options(&ResolutionOptions::default())
    }

    pub fn resolve_dependencies_with_options(&mut self, options: &ResolutionOptions) -> Result<()> {
        let manifest = self.read_manifest()?;
        self.resolve_dependencies_from_manifest_with_options(&manifest, options)
    }

    pub(crate) fn resolve_dependencies_from_manifest_with_options(
        &mut self,
        manifest: &PackageManifest,
        options: &ResolutionOptions,
    ) -> Result<()> {
        self.validate_manifest_package_contract(manifest)?;
        let root_environment = root_environment_context(manifest, options)?;
        self.resolved.clear();
        self.root_dependencies.clear();
        self.selected_coordinates.clear();

        let result = (|| {
            let dependencies = self.selected_dependencies(manifest, options, true)?;
            let mut compiler_errors = Vec::new();
            for (alias, dep) in dependencies {
                let mut stack_ids = Vec::new();
                let mut stack_labels = Vec::new();
                let mut traversal = ResolutionTraversal {
                    stack_ids: &mut stack_ids,
                    stack_labels: &mut stack_labels,
                    compiler_errors: &mut compiler_errors,
                };
                let node_id = self.resolve_dependency_from_root(
                    &alias,
                    &dep,
                    &manifest.package.name,
                    manifest.package.namespace.as_deref(),
                    &self.root.clone(),
                    options,
                    root_environment.as_ref(),
                    &mut traversal,
                )?;
                if let Some(node_id) = node_id {
                    self.root_dependencies.insert(alias, node_id);
                }
            }
            if !compiler_errors.is_empty() {
                return Err(aggregate_compiler_incompatibilities(compiler_errors));
            }
            Ok(())
        })();
        if result.is_err() {
            self.resolved.clear();
            self.root_dependencies.clear();
            self.selected_coordinates.clear();
        }
        result
    }
}

fn locked_source_to_package_source(source: &LockedSource) -> PackageSource {
    match source {
        LockedSource::Path { path } => PackageSource::Local(PathBuf::from(path)),
        LockedSource::Git { url, revision } => PackageSource::Git { url: url.clone(), revision: revision.clone() },
        LockedSource::Registry { registry, url, revision, namespace, version } => PackageSource::Registry {
            registry: registry.clone(),
            url: url.clone(),
            revision: revision.clone(),
            namespace: namespace.clone(),
            version: version.clone(),
        },
    }
}

fn validate_locked_root_compiler_contract(lockfile: &Lockfile, manifest: &PackageManifest) -> Result<()> {
    if lockfile.package.compiler_requirement != manifest.package.cellscript_version {
        return Err(CompileError::without_span(format!(
            "Cell.lock compiler requirement '{}' does not match Cell.toml '{}'; run 'cellc update' explicitly",
            lockfile.package.compiler_requirement, manifest.package.cellscript_version
        ))
        .with_code(crate::error::DiagnosticCode::E2600)
        .with_details(serde_json::json!({
            "package": manifest.package.name,
            "package_version": manifest.package.version,
            "locked_compiler_requirement": lockfile.package.compiler_requirement,
            "compiler_requirement": manifest.package.cellscript_version,
            "active_compiler_version": crate::VERSION,
            "incoming_edge": serde_json::Value::Null,
            "phase": "locked-root-validation",
        })));
    }
    // `validate_manifest_package_contract` already checked the active compiler.
    // Keep the lock's resolver version informational so a later compatible
    // compiler can reproduce locked source selection without an exact pin.
    Ok(())
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
            if !visited.contains_key(node)
                && let Some(cycle) = self.dfs_find_cycle(node, &mut visited, &mut rec_stack)
            {
                return Some(cycle);
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
    pub schema: String,
    #[serde(default)]
    pub resolver_model: String,
    pub package: LockfilePackageInfo,
    pub root: LockedRootGraph,
    pub dependencies: BTreeMap<String, LockedDependency>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub environments: BTreeMap<String, LockedEnvironment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_build: Option<LockedBuildInfo>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deployment: BTreeMap<String, LockfileDeploymentRef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LockedRootGraph {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub manifest_digest: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dev_dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedEnvironment {
    pub chain_id: String,
    pub genesis_hash: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dev_dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LockfilePackageInfo {
    pub edition: CellScriptEdition,
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
    /// Source compatibility range declared by `[package].cellscript_version`.
    #[serde(default)]
    pub compiler_requirement: String,
    /// Compiler release that resolved this lock graph. This is evidence, not
    /// an exact build pin; locked builds revalidate `compiler_requirement`.
    #[serde(default)]
    pub resolver_compiler_version: String,
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
    pub const CURRENT_VERSION: u32 = 5;
    pub const CURRENT_SCHEMA: &'static str = "cellscript-lock-v0.30-single-package-coordinate-v1";
    pub const CURRENT_RESOLVER_MODEL: &'static str = "single-package-coordinate-v1";

    pub fn new() -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            schema: Self::CURRENT_SCHEMA.to_string(),
            resolver_model: Self::CURRENT_RESOLVER_MODEL.to_string(),
            package: LockfilePackageInfo {
                compiler_requirement: "*".to_string(),
                resolver_compiler_version: crate::VERSION.to_string(),
                ..LockfilePackageInfo::default()
            },
            root: LockedRootGraph::default(),
            dependencies: BTreeMap::new(),
            environments: BTreeMap::new(),
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
        let lockfile: Self = toml::from_str(&content)
            .map_err(|error| CompileError::without_span(format!("failed to parse lockfile '{}': {}", lock_path.display(), error)))?;
        lockfile.validate_schema()?;
        Ok(Some(lockfile))
    }

    /// Read prior pins only for an explicitly requested lock/update operation.
    /// Old lock generations contribute no trusted state:
    /// the caller must resolve and write a fresh graph from the current manifest.
    /// Ordinary build/check/test readers continue to reject these files.
    pub(crate) fn read_for_explicit_repin(root: &Path) -> Result<Self> {
        match Self::read_from_root(root) {
            Ok(Some(lockfile)) => Ok(lockfile),
            Ok(None) => Ok(Self::new()),
            Err(error) => {
                let source = std::fs::read_to_string(root.join("Cell.lock")).map_err(|_| error.clone())?;
                let value: toml::Value = toml::from_str(&source).map_err(|_| error.clone())?;
                let version = value.get("version").and_then(toml::Value::as_integer);
                let old_generation = version.is_some_and(|version| matches!(version, 1..=4));
                if old_generation {
                    Ok(Self::new())
                } else {
                    Err(error)
                }
            }
        }
    }

    pub fn write_to_root(&self, root: &Path) -> Result<()> {
        self.validate_schema()?;
        let lock_path = root.join("Cell.lock");
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&lock_path, content)?;
        Ok(())
    }

    pub fn validate_schema(&self) -> Result<()> {
        if self.version != Self::CURRENT_VERSION {
            return Err(CompileError::without_span(format!(
                "unsupported Cell.lock version {}; expected {}",
                self.version,
                Self::CURRENT_VERSION
            )));
        }
        if self.schema != Self::CURRENT_SCHEMA {
            return Err(CompileError::without_span(format!(
                "unsupported Cell.lock schema '{}'; expected '{}'",
                self.schema,
                Self::CURRENT_SCHEMA
            )));
        }
        if self.resolver_model != Self::CURRENT_RESOLVER_MODEL {
            return Err(CompileError::without_span(format!(
                "unsupported Cell.lock resolver model '{}'; expected '{}'",
                self.resolver_model,
                Self::CURRENT_RESOLVER_MODEL
            ))
            .with_code(crate::error::DiagnosticCode::E2601));
        }
        if self.package.compiler_requirement.is_empty() || self.package.resolver_compiler_version.is_empty() {
            return Err(CompileError::without_span(
                "Cell.lock v5 package requires compiler_requirement and resolver_compiler_version; run 'cellc lock' or 'cellc update' explicitly",
            )
            .with_code(crate::error::DiagnosticCode::E2600));
        }
        parse_compiler_requirement(&self.package.compiler_requirement)?;
        semver::Version::parse(&self.package.resolver_compiler_version).map_err(|error| {
            CompileError::without_span(format!(
                "Cell.lock package resolver_compiler_version '{}' is invalid: {error}",
                self.package.resolver_compiler_version
            ))
        })?;
        if let Some(build) = &self.package_build {
            if build.edition != self.package.edition {
                return Err(CompileError::without_span(format!(
                    "Cell.lock package/build edition mismatch: package is '{}' but build is '{}'",
                    self.package.edition, build.edition
                )));
            }
            if build.compatibility_profile_hash.is_empty() {
                return Err(CompileError::without_span("Cell.lock v5 package_build requires compatibility_profile_hash"));
            }
        }
        self.validate_graph()?;
        Ok(())
    }

    fn validate_graph(&self) -> Result<()> {
        if self.dependencies.len() > MAX_LOCKFILE_GRAPH_NODES {
            return Err(CompileError::without_span(format!(
                "Cell.lock dependency graph exceeds the admission budget of {MAX_LOCKFILE_GRAPH_NODES} nodes"
            ))
            .with_code(crate::error::DiagnosticCode::E2600));
        }
        for (node_id, dependency) in &self.dependencies {
            if dependency.name.is_empty()
                || dependency.manifest_digest.is_empty()
                || dependency.source_hash.as_deref().is_none_or(str::is_empty)
                || dependency.compiler_requirement.is_empty()
                || dependency.resolver_compiler_version.is_empty()
            {
                return Err(CompileError::without_span(format!(
                    "Cell.lock dependency node '{}' requires name, manifest_digest, source_hash, compiler_requirement, and resolver_compiler_version",
                    node_id
                ))
                .with_code(crate::error::DiagnosticCode::E2600));
            }
            parse_compiler_requirement(&dependency.compiler_requirement)?;
            semver::Version::parse(&dependency.resolver_compiler_version).map_err(|error| {
                CompileError::without_span(format!(
                    "Cell.lock dependency node '{}' resolver_compiler_version '{}' is invalid: {error}",
                    node_id, dependency.resolver_compiler_version
                ))
            })?;
            for (alias, target) in &dependency.dependencies {
                if !self.dependencies.contains_key(target) {
                    return Err(CompileError::without_span(format!(
                        "Cell.lock dependency node '{}' edge '{}' targets missing node '{}'",
                        node_id, alias, target
                    )));
                }
            }
        }
        self.validate_root_edges("root dependencies", &self.root.dependencies)?;
        self.validate_root_edges("root dev-dependencies", &self.root.dev_dependencies)?;
        for (name, environment) in &self.environments {
            validate_environment(
                name,
                &CkbEnvironment { chain_id: environment.chain_id.clone(), genesis_hash: environment.genesis_hash.clone() },
            )?;
            self.validate_root_edges(&format!("environment '{}' dependencies", name), &environment.dependencies)?;
            self.validate_root_edges(&format!("environment '{}' dev-dependencies", name), &environment.dev_dependencies)?;
        }
        let graph = self.dependency_graph();
        if let Some(cycle) = graph.find_cycle() {
            return Err(CompileError::without_span(format!("Cell.lock dependency graph contains a cycle: {}", cycle.join(" -> "))));
        }
        Ok(())
    }

    fn validate_root_edges(&self, label: &str, edges: &BTreeMap<String, String>) -> Result<()> {
        for (alias, target) in edges {
            if !self.dependencies.contains_key(target) {
                return Err(CompileError::without_span(format!(
                    "Cell.lock {} edge '{}' targets missing node '{}'",
                    label, alias, target
                )));
            }
        }
        Ok(())
    }

    fn dependency_graph(&self) -> DependencyGraph {
        let mut graph = DependencyGraph::new();
        for (node_id, dependency) in &self.dependencies {
            graph.add_node(node_id.clone());
            for target in dependency.dependencies.values() {
                graph.add_edge(node_id.clone(), target.clone());
            }
        }
        graph
    }

    pub fn update_from_resolved(&mut self, resolved: &BTreeMap<String, ResolvedPackage>) {
        for (node_id, package) in resolved {
            let mut locked = LockedDependency {
                name: package.name.clone(),
                namespace: package.namespace.clone(),
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
                manifest_digest: package.manifest_digest.clone(),
                dependencies: package.dependencies.clone(),
                build: None,
                compiler_requirement: package.compiler_requirement.clone(),
                resolver_compiler_version: crate::VERSION.to_string(),
            };
            if let Some(previous) = self.dependencies.get(node_id)
                && locked_dependency_build_identity(previous, &locked)
            {
                locked.build = previous.build.clone();
            }
            self.dependencies.insert(node_id.clone(), locked);
        }
    }

    pub fn replace_with_resolved(&mut self, resolved: &BTreeMap<String, ResolvedPackage>) {
        self.dependencies.clear();
        self.update_from_resolved(resolved);
    }

    pub fn replace_with_resolution(
        &mut self,
        manager: &PackageManager,
        manifest: &PackageManifest,
        options: &ResolutionOptions,
    ) -> Result<()> {
        self.dependencies.clear();
        self.root = LockedRootGraph::default();
        self.environments.clear();
        self.merge_resolution(manager, manifest, options)
    }

    pub fn merge_resolution(
        &mut self,
        manager: &PackageManager,
        manifest: &PackageManifest,
        options: &ResolutionOptions,
    ) -> Result<()> {
        self.package.edition = manifest.package.edition;
        self.package.name = manifest.package.name.clone();
        self.package.version = manifest.package.version.clone();
        self.package.namespace = manifest.package.namespace.clone();
        self.package.compiler_requirement = manifest.package.cellscript_version.clone();
        self.package.resolver_compiler_version = crate::VERSION.to_string();
        self.update_from_resolved(manager.get_resolved());
        let manifest_bytes = std::fs::read(manager.root.join("Cell.toml"))?;
        self.root.manifest_digest = manifest_digest(&manifest_bytes);

        let mut runtime = BTreeMap::new();
        let mut dev = BTreeMap::new();
        for (alias, node_id) in manager.root_dependencies() {
            if options.scope == DependencyScope::Test && manifest.dev_dependencies.contains_key(alias) {
                dev.insert(alias.clone(), node_id.clone());
            } else {
                runtime.insert(alias.clone(), node_id.clone());
            }
        }

        if let Some(environment_name) = options.environment.as_deref() {
            let environment = manifest
                .environments
                .get(environment_name)
                .ok_or_else(|| CompileError::without_span(format!("unknown package environment '{}'", environment_name)))?;
            self.environments.insert(
                environment_name.to_string(),
                LockedEnvironment {
                    chain_id: environment.chain_id.clone(),
                    genesis_hash: normalized_genesis_hash(&environment.genesis_hash),
                    dependencies: runtime,
                    dev_dependencies: dev,
                },
            );
        } else {
            self.root.dependencies = runtime;
            self.root.dev_dependencies = dev;
        }
        self.validate_schema()
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
        resolved: &BTreeMap<String, ResolvedPackage>,
    ) -> Vec<String> {
        self.consistency_issues_with_expected(manifest, Some(resolved))
    }

    fn consistency_issues_with_expected(
        &self,
        manifest: &PackageManifest,
        resolved: Option<&BTreeMap<String, ResolvedPackage>>,
    ) -> Vec<String> {
        let mut issues = Vec::new();
        if self.version != Self::CURRENT_VERSION {
            issues.push(format!("Cell.lock version {} is not supported; expected {}", self.version, Self::CURRENT_VERSION));
        }
        if self.package.edition != manifest.package.edition {
            issues.push(format!(
                "package edition mismatch: Cell.toml has '{}' but Cell.lock records '{}'",
                manifest.package.edition, self.package.edition
            ));
        }
        if self.package.compiler_requirement != manifest.package.cellscript_version {
            issues.push(format!(
                "package compiler requirement mismatch: Cell.toml has '{}' but Cell.lock records '{}'",
                manifest.package.cellscript_version, self.package.compiler_requirement
            ));
        }

        if manifest.dependency_overrides.is_empty() {
            issues.extend(self.root_graph_consistency_issues(
                "root",
                &manifest.dependencies,
                &manifest.dev_dependencies,
                &self.root.dependencies,
                &self.root.dev_dependencies,
                manifest.package.namespace.as_deref(),
            ));
        }
        for (environment_name, environment) in &manifest.environments {
            let Some(locked_environment) = self.environments.get(environment_name) else {
                issues.push(format!("environment '{}' is missing from Cell.lock", environment_name));
                continue;
            };
            if locked_environment.chain_id != environment.chain_id
                || normalized_genesis_hash(&locked_environment.genesis_hash) != normalized_genesis_hash(&environment.genesis_hash)
            {
                issues.push(format!("environment '{}' chain identity differs between Cell.toml and Cell.lock", environment_name));
            }
            let mut dependencies = manifest.dependencies.clone();
            if let Some(overrides) = manifest.dependency_overrides.get(environment_name) {
                dependencies.extend(overrides.clone());
            }
            issues.extend(self.root_graph_consistency_issues(
                &format!("environment '{}'", environment_name),
                &dependencies,
                &manifest.dev_dependencies,
                &locked_environment.dependencies,
                &locked_environment.dev_dependencies,
                manifest.package.namespace.as_deref(),
            ));
        }

        if let Some(resolved) = resolved {
            for (node_id, package) in resolved {
                let Some(locked) = self.dependencies.get(node_id) else {
                    issues.push(format!("resolved dependency node '{}' is missing from Cell.lock", node_id));
                    continue;
                };
                issues.extend(resolved_dependency_consistency_issues(node_id, package, locked));
            }
        }

        let reachable = self.reachable_nodes();
        for node_id in self.dependencies.keys() {
            if !reachable.contains(node_id) {
                issues.push(format!("Cell.lock contains unreachable dependency node '{}'", node_id));
            }
        }

        issues
    }

    fn root_graph_consistency_issues(
        &self,
        label: &str,
        dependencies: &HashMap<String, Dependency>,
        dev_dependencies: &HashMap<String, Dependency>,
        locked_dependencies: &BTreeMap<String, String>,
        locked_dev_dependencies: &BTreeMap<String, String>,
        namespace: Option<&str>,
    ) -> Vec<String> {
        let mut issues = Vec::new();
        for (alias, dependency) in dependencies {
            let Some(node_id) = locked_dependencies.get(alias) else {
                if !dependency_is_optional(dependency) {
                    issues.push(format!("{} dependency '{}' is missing from Cell.lock", label, alias));
                }
                continue;
            };
            match self.dependencies.get(node_id) {
                Some(locked) => issues.extend(lock_dependency_consistency_issues(alias, dependency, locked, namespace, None)),
                None => issues.push(format!("{} dependency '{}' targets missing node '{}'", label, alias, node_id)),
            }
        }
        for (alias, dependency) in dev_dependencies {
            let Some(node_id) = locked_dev_dependencies.get(alias) else {
                if !dependency_is_optional(dependency) {
                    issues.push(format!("{} dev-dependency '{}' is missing from Cell.lock", label, alias));
                }
                continue;
            };
            match self.dependencies.get(node_id) {
                Some(locked) => issues.extend(lock_dependency_consistency_issues(alias, dependency, locked, namespace, None)),
                None => issues.push(format!("{} dev-dependency '{}' targets missing node '{}'", label, alias, node_id)),
            }
        }
        for alias in locked_dependencies.keys() {
            if !dependencies.contains_key(alias) {
                issues.push(format!("{} contains stale dependency alias '{}'", label, alias));
            }
        }
        for alias in locked_dev_dependencies.keys() {
            if !dev_dependencies.contains_key(alias) {
                issues.push(format!("{} contains stale dev-dependency alias '{}'", label, alias));
            }
        }
        issues
    }

    fn reachable_nodes(&self) -> BTreeSet<String> {
        let mut pending: Vec<String> = self
            .root
            .dependencies
            .values()
            .chain(self.root.dev_dependencies.values())
            .chain(
                self.environments
                    .values()
                    .flat_map(|environment| environment.dependencies.values().chain(environment.dev_dependencies.values())),
            )
            .cloned()
            .collect();
        let mut reachable = BTreeSet::new();
        while let Some(node_id) = pending.pop() {
            if !reachable.insert(node_id.clone()) {
                continue;
            }
            if let Some(node) = self.dependencies.get(&node_id) {
                pending.extend(node.dependencies.values().cloned());
            }
        }
        reachable
    }
}

fn resolved_dependency_consistency_issues(name: &str, package: &ResolvedPackage, locked: &LockedDependency) -> Vec<String> {
    let mut issues = Vec::new();

    if locked.name != package.name {
        issues.push(format!(
            "resolved dependency node '{}' has package name '{}' but Cell.lock records '{}'",
            name, package.name, locked.name
        ));
    }

    if locked.version != package.version {
        issues.push(format!(
            "resolved dependency '{}' has package version '{}' but Cell.lock records '{}'",
            name, package.version, locked.version
        ));
    }

    if locked.manifest_digest != package.manifest_digest {
        issues.push(format!(
            "resolved dependency node '{}' manifest digest '{}' does not match Cell.lock '{}'",
            name, package.manifest_digest, locked.manifest_digest
        ));
    }
    if locked.compiler_requirement != package.compiler_requirement {
        issues.push(format!(
            "resolved dependency node '{}' compiler requirement '{}' does not match Cell.lock '{}'",
            name, package.compiler_requirement, locked.compiler_requirement
        ));
    }
    if locked.dependencies != package.dependencies {
        issues.push(format!("resolved dependency node '{}' edges do not match Cell.lock", name));
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
    } else {
        issues.push(format!("resolved dependency '{}' did not produce a source_hash", name));
    }

    issues
}

fn lock_dependency_consistency_issues(
    name: &str,
    dep: &Dependency,
    locked: &LockedDependency,
    consuming_namespace: Option<&str>,
    path_context: Option<(&Path, &Path)>,
) -> Vec<String> {
    let mut issues = Vec::new();
    let expected_package = dependency_package_name(name, dep);
    if locked.name != expected_package {
        issues.push(format!(
            "dependency alias '{}' expects package '{}' but Cell.lock node declares '{}'",
            name, expected_package, locked.name
        ));
    }

    match dep {
        Dependency::Simple(version) => match &locked.source {
            LockedSource::Registry { namespace: locked_namespace, version: locked_version, .. }
                if Some(locked_namespace.as_str()) == consuming_namespace && locked_version == &locked.version => {}
            source => issues.push(format!(
                "dependency '{}' expects registry source {}@{} but Cell.lock records {}",
                name,
                name,
                version,
                locked_source_display(source)
            )),
        },
        Dependency::Detailed(detail) => {
            if detail.resolver.is_some() {
                // Update-time resolvers are normalized into the immutable
                // source recorded here. Locked builds never invoke them.
            } else if let Some(path) = &detail.path {
                match &locked.source {
                    LockedSource::Path { path: locked_path }
                        if path_context.map_or_else(
                            || locked_path == path,
                            |(manifest_root, workspace_root)| {
                                canonical_path(&manifest_root.join(path)).ok()
                                    == canonical_path(&workspace_root.join(locked_path)).ok()
                            },
                        ) => {}
                    source => issues.push(format!(
                        "dependency '{}' expects path source '{}' but Cell.lock records {}",
                        name,
                        path,
                        locked_source_display(source)
                    )),
                }
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
            } else {
                match &locked.source {
                    LockedSource::Registry { namespace: locked_namespace, version: locked_version, .. }
                        if Some(locked_namespace.as_str()) == detail.namespace.as_deref().or(consuming_namespace)
                            && locked_version == &locked.version => {}
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

    if let Some(requirement) = match dep {
        Dependency::Simple(requirement) => Some(requirement.as_str()),
        Dependency::Detailed(detail) if detail.version != "*" && !detail.version.is_empty() => Some(detail.version.as_str()),
        Dependency::Detailed(_) => None,
    } {
        match version::parse_version_req(requirement) {
            Ok(requirement) if version::satisfies(&locked.version, &requirement) => {}
            Ok(_) => issues.push(format!(
                "dependency '{}' requires '{}' but Cell.lock records package version '{}'",
                name, requirement, locked.version
            )),
            Err(error) => issues.push(error.message.clone()),
        }
    }

    issues
}

fn locked_source_display(source: &LockedSource) -> String {
    match source {
        LockedSource::Path { path } => format!("path '{}'", path),
        LockedSource::Git { url, revision } => format!("git '{}#{}'", url, revision),
        LockedSource::Registry { registry, namespace, version, .. } => format!("registry {}/{}@{}", registry, namespace, version),
    }
}

fn locked_dependency_build_identity(left: &LockedDependency, right: &LockedDependency) -> bool {
    left.name == right.name
        && left.namespace == right.namespace
        && left.version == right.version
        && serde_json::to_value(&left.source).ok() == serde_json::to_value(&right.source).ok()
        && left.source_hash == right.source_hash
        && left.manifest_digest == right.manifest_digest
        && left.compiler_requirement == right.compiler_requirement
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
    pub edition: CellScriptEdition,
    pub compatibility_profile_hash: String,
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
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    pub version: String,
    pub source: LockedSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub manifest_digest: String,
    /// Source compatibility range declared by the dependency manifest.
    #[serde(default)]
    pub compiler_requirement: String,
    /// Compiler release that resolved this dependency node.
    #[serde(default)]
    pub resolver_compiler_version: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, String>,
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
        let req = req.trim();
        let parsed = if req == "*" {
            VersionReq::Any
        } else if let Some(stripped) = req.strip_prefix('^') {
            VersionReq::Compatible(stripped.to_string())
        } else if let Some(stripped) = req.strip_prefix('=') {
            VersionReq::Exact(stripped.to_string())
        } else if req.contains(',') || req.contains('>') || req.contains('<') || req.contains('~') {
            VersionReq::Range(req.to_string())
        } else {
            // Preserve CellScript's historical bare-version-as-compatible
            // surface while using the standard SemVer compatibility rules.
            VersionReq::Compatible(req.to_string())
        };
        standard_requirement(&parsed)?;
        Ok(parsed)
    }

    pub fn satisfies(version: &str, req: &VersionReq) -> bool {
        let Ok(version) = semver::Version::parse(version) else {
            return false;
        };
        standard_requirement(req).is_ok_and(|requirement| requirement.matches(&version))
    }

    fn standard_requirement(req: &VersionReq) -> Result<semver::VersionReq> {
        let source = match req {
            VersionReq::Any => "*".to_string(),
            VersionReq::Exact(version) => format!("={version}"),
            VersionReq::Compatible(version) => format!("^{version}"),
            VersionReq::Range(range) => range.clone(),
        };
        semver::VersionReq::parse(&source)
            .map_err(|error| CompileError::without_span(format!("invalid semantic version requirement '{source}': {error}")))
    }
}

// ---------------------------------------------------------------------------
// Deployed.toml — Deployment Fact Record
// ---------------------------------------------------------------------------

/// The supported deployment wire schema; its version is independent of source edition.
pub const DEPLOYED_MANIFEST_SCHEMA: &str = "cellscript-deployed-v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployedManifest {
    pub version: u32,
    pub schema: String,
    pub package: DeployedPackageInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<DeployedBuildInfo>,
    #[serde(default)]
    pub deployments: Vec<DeploymentRecord>,
}

impl DeployedManifest {
    pub const CURRENT_VERSION: u32 = 2;

    pub fn read_from_root(root: &Path) -> Result<Option<Self>> {
        let path = root.join("Deployed.toml");
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| CompileError::without_span(format!("failed to read Deployed.toml '{}': {}", path.display(), e)))?;
        let manifest: Self = toml::from_str(&content)
            .map_err(|e| CompileError::without_span(format!("failed to parse Deployed.toml '{}': {}", path.display(), e)))?;
        manifest.validate_schema()?;
        Ok(Some(manifest))
    }

    pub fn write_to_root(&self, root: &Path) -> Result<()> {
        self.validate_schema()?;
        let path = root.join("Deployed.toml");
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    pub fn validate_schema(&self) -> Result<()> {
        if self.version != Self::CURRENT_VERSION || self.schema != DEPLOYED_MANIFEST_SCHEMA {
            return Err(CompileError::without_span(format!(
                "unsupported Deployed.toml identity; expected version {} and schema '{}'",
                Self::CURRENT_VERSION,
                DEPLOYED_MANIFEST_SCHEMA
            )));
        }
        if let Some(build) = &self.build {
            if build.edition != self.package.edition {
                return Err(CompileError::without_span(format!(
                    "Deployed.toml package/build edition mismatch: package is '{}' but build is '{}'",
                    self.package.edition, build.edition
                )));
            }
            if build.compatibility_profile_hash.is_empty() {
                return Err(CompileError::without_span("Deployed.toml v2 build requires compatibility_profile_hash"));
            }
        }
        for deployment in &self.deployments {
            if deployment.edition != self.package.edition {
                return Err(CompileError::without_span(format!(
                    "Deployed.toml package/deployment edition mismatch for network '{}': package is '{}' but deployment is '{}'",
                    deployment.network, self.package.edition, deployment.edition
                )));
            }
            if deployment.compatibility_profile_hash.is_empty() {
                return Err(CompileError::without_span(format!(
                    "Deployed.toml v2 deployment for network '{}' requires compatibility_profile_hash",
                    deployment.network
                )));
            }
            if let Some(build) = &self.build
                && deployment.compatibility_profile_hash != build.compatibility_profile_hash
            {
                return Err(CompileError::without_span(format!(
                    "Deployed.toml build/deployment compatibility profile mismatch for network '{}'",
                    deployment.network
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployedPackageInfo {
    pub name: String,
    pub version: String,
    pub edition: CellScriptEdition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeployedBuildInfo {
    pub edition: CellScriptEdition,
    pub compatibility_profile_hash: String,
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
    pub edition: CellScriptEdition,
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
    pub compatibility_profile_hash: String,

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
mod tests;
