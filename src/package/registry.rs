//! Public registry and offline Git-mirror clients for CellScript packages.
//!
//! Production resolution reads accepted package state from the public registry
//! API, then downloads and verifies the registry's immutable source snapshot.
//! The repository URL and tag remain provenance/audit fields rather than an
//! availability dependency. The historical Git discovery index remains
//! available only through an explicit
//! `CELLSCRIPT_REGISTRY_URL` override for tests, private mirrors, and offline
//! workflows.
//!
//! Resolution priority: path > git > registry

use crate::error::{CompileError, Result};
use crate::package::PackageManifest;
#[cfg(feature = "cli")]
use base64::Engine;
use serde::{Deserialize, Serialize};
#[cfg(feature = "cli")]
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
#[cfg(feature = "cli")]
use std::io::Read;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Discovery Index
// ---------------------------------------------------------------------------

/// Default discovery index repository URL.
pub const DEFAULT_REGISTRY_URL: &str = "https://github.com/cellscript/cellscript-registry";
pub const REGISTRY_URL_ENV: &str = "CELLSCRIPT_REGISTRY_URL";
pub const DEFAULT_PUBLIC_REGISTRY_ORIGIN: &str = "https://api.registry.cellscript.dev";
pub const REGISTRY_API_URL_ENV: &str = "CELLSCRIPT_REGISTRY_API_URL";
pub const REGISTRY_ORIGIN_ENV: &str = "CELLSCRIPT_REGISTRY_ORIGIN";
pub const REGISTRY_AUTH_PROTOCOL: &str = "cellscript-registry-auth-v1";
pub const AUTHORIZE_CAPABILITY_ACTION: &str = "authorize_capability";
pub const REVOKE_CAPABILITY_ACTION: &str = "revoke_capability";
pub const REGISTRY_PUBLISH_PROTOCOL: &str = "cellscript-registry-publish-v1";
pub const PUBLISH_ACTION: &str = "publish";

/// Compute the cross-process identity of a parsed package manifest.
///
/// `PackageManifest` contains hash maps, so serializing it directly can emit a
/// different key order in another process. Registry identities must instead
/// hash recursively key-sorted JSON so the publisher and isolated verifier
/// agree for the same `Cell.toml`.
pub fn compute_package_manifest_hash(manifest: &PackageManifest) -> Result<String> {
    let value = serde_json::to_value(manifest)
        .map_err(|error| CompileError::without_span(format!("failed to serialize package manifest for digest: {error}")))?;
    let bytes = serde_json::to_vec(&canonical_json_value(&value))
        .map_err(|error| CompileError::without_span(format!("failed to serialize canonical package manifest: {error}")))?;
    Ok(crate::hex_encode(&crate::ckb_blake2b256(&bytes)))
}

fn canonical_json_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(items) => serde_json::Value::Array(items.iter().map(canonical_json_value).collect()),
        serde_json::Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            let mut canonical = serde_json::Map::new();
            for key in keys {
                if let Some(item) = object.get(key) {
                    canonical.insert(key.clone(), canonical_json_value(item));
                }
            }
            serde_json::Value::Object(canonical)
        }
        other => other.clone(),
    }
}

/// Effective discovery index URL.
///
/// The environment override is intentionally small: it lets tests and private
/// registries use the same Git-based resolver without adding a separate config
/// file or service dependency.
pub fn default_registry_url() -> String {
    std::env::var(REGISTRY_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_REGISTRY_URL.to_string())
}

/// Effective production resolver URL.
///
/// `CELLSCRIPT_REGISTRY_URL` deliberately has highest priority as the legacy
/// explicit Git-mirror override. Without that override, resolution uses the
/// public API configured for publish/auth, then the production default.
pub fn resolver_registry_url() -> String {
    std::env::var(REGISTRY_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| std::env::var(REGISTRY_API_URL_ENV).ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty()))
        .or_else(|| std::env::var(REGISTRY_ORIGIN_ENV).ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty()))
        .unwrap_or_else(|| DEFAULT_PUBLIC_REGISTRY_ORIGIN.to_string())
}

/// A single entry in the discovery index: maps `namespace/name` to a source repo URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryEntry {
    pub name: String,
    pub namespace: String,
    pub source: String,
}

pub struct RegistryResolution {
    pub registry_url: String,
    pub entry: DiscoveryEntry,
    /// Accepted production statuses from the public API. Git-mirror overrides
    /// leave this empty and continue to read `registry.json` from the source.
    pub authoritative_index: Option<RegistryIndex>,
    /// Immutable install snapshots keyed by package version. Git-mirror
    /// overrides leave this empty.
    pub source_snapshots: BTreeMap<String, PublicRegistrySourceSnapshot>,
}

/// Resolve package discovery through the production API unless the caller has
/// explicitly selected the legacy Git discovery index.
pub fn lookup_for_resolution(namespace: &str, name: &str, cache_dir: &Path) -> Result<RegistryResolution> {
    if let Some(registry_url) =
        std::env::var(REGISTRY_URL_ENV).ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
    {
        let entry = DiscoveryIndex::new(&registry_url, cache_dir).lookup(namespace, name)?;
        return Ok(RegistryResolution { registry_url, entry, authoritative_index: None, source_snapshots: BTreeMap::new() });
    }

    let registry_url = resolver_registry_url();
    let (entry, authoritative_index, source_snapshots) = lookup_public_registry(&registry_url, namespace, name)?;
    Ok(RegistryResolution { registry_url, entry, authoritative_index: Some(authoritative_index), source_snapshots })
}

#[cfg(feature = "cli")]
fn lookup_public_registry(
    registry_url: &str,
    namespace: &str,
    name: &str,
) -> Result<(DiscoveryEntry, RegistryIndex, BTreeMap<String, PublicRegistrySourceSnapshot>)> {
    let url = format!("{}/v1/packages/{}/{}", registry_url.trim_end_matches('/'), namespace, name);
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|error| CompileError::without_span(format!("failed to initialize public registry client: {error}")))?
        .get(&url)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, format!("cellc/{}", env!("CARGO_PKG_VERSION")))
        .send()
        .map_err(|error| CompileError::without_span(format!("public registry request '{}' failed: {error}", url)))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(CompileError::without_span(format!("package '{namespace}/{name}' is not present in the public registry")));
    }
    if !response.status().is_success() {
        return Err(CompileError::without_span(format!("public registry request '{}' returned HTTP {}", url, response.status())));
    }
    let payload: PublicRegistryPackage = response
        .json()
        .map_err(|error| CompileError::without_span(format!("public registry response '{}' is invalid: {error}", url)))?;
    payload.into_resolution(namespace, name)
}

#[cfg(not(feature = "cli"))]
fn lookup_public_registry(
    _registry_url: &str,
    namespace: &str,
    name: &str,
) -> Result<(DiscoveryEntry, RegistryIndex, BTreeMap<String, PublicRegistrySourceSnapshot>)> {
    Err(CompileError::without_span(format!("public registry resolution for '{namespace}/{name}' requires the 'cli' feature")))
}

#[cfg(feature = "cli")]
#[derive(Debug, Deserialize)]
struct PublicRegistryPackage {
    schema: String,
    namespace: String,
    name: String,
    repository: Option<String>,
    versions: Vec<PublicRegistryVersion>,
}

#[cfg(feature = "cli")]
#[derive(Debug, Deserialize)]
struct PublicRegistryVersion {
    version: String,
    status: RegistryEntryStatus,
    registry_entry: RegistryIndex,
    source_snapshot: PublicRegistrySourceSnapshot,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PublicRegistrySourceSnapshot {
    pub schema: String,
    pub url: String,
    pub snapshot_hash: String,
    pub source_hash: String,
    pub size_bytes: u64,
    pub content_type: String,
}

#[cfg(feature = "cli")]
impl PublicRegistryPackage {
    fn into_resolution(
        self,
        expected_namespace: &str,
        expected_name: &str,
    ) -> Result<(DiscoveryEntry, RegistryIndex, BTreeMap<String, PublicRegistrySourceSnapshot>)> {
        if self.schema != "cellscript-public-registry-package-v1" {
            return Err(CompileError::without_span(format!("public registry returned unsupported schema '{}'", self.schema)));
        }
        if self.namespace != expected_namespace || self.name != expected_name {
            return Err(CompileError::without_span(format!(
                "public registry identity mismatch for '{expected_namespace}/{expected_name}': found '{}/{}'",
                self.namespace, self.name
            )));
        }
        let source = self.repository.filter(|value| !value.trim().is_empty()).unwrap_or_default();
        let mut versions = Vec::with_capacity(self.versions.len());
        let mut source_snapshots = BTreeMap::new();
        for public_version in self.versions {
            if public_version.registry_entry.schema_version != RegistryIndex::CURRENT_SCHEMA_VERSION
                || public_version.registry_entry.namespace != expected_namespace
                || public_version.registry_entry.name != expected_name
            {
                return Err(CompileError::without_span(format!(
                    "public registry version '{}' contains mismatched registry identity",
                    public_version.version
                )));
            }
            let mut matching = public_version
                .registry_entry
                .versions
                .into_iter()
                .find(|version| version.version == public_version.version)
                .ok_or_else(|| {
                    CompileError::without_span(format!(
                        "public registry version '{}' has no matching signed version entry",
                        public_version.version
                    ))
                })?;
            matching.status = public_version.status.clone();
            matching.yanked = matches!(public_version.status, RegistryEntryStatus::Yanked);
            if public_version.source_snapshot.schema != "cellscript-registry-source-snapshot-v1"
                || public_version.source_snapshot.source_hash != matching.source_hash
            {
                return Err(CompileError::without_span(format!(
                    "public registry version '{}' contains invalid source snapshot identity",
                    public_version.version
                )));
            }
            if source_snapshots.insert(public_version.version.clone(), public_version.source_snapshot).is_some() {
                return Err(CompileError::without_span(format!(
                    "public registry returned duplicate version '{}'",
                    public_version.version
                )));
            }
            versions.push(matching);
        }
        if versions.is_empty() {
            return Err(CompileError::without_span(format!(
                "public registry package '{expected_namespace}/{expected_name}' has no visible versions"
            )));
        }
        Ok((
            DiscoveryEntry { name: expected_name.to_string(), namespace: expected_namespace.to_string(), source },
            RegistryIndex {
                schema_version: RegistryIndex::CURRENT_SCHEMA_VERSION,
                name: expected_name.to_string(),
                namespace: expected_namespace.to_string(),
                versions,
            },
            source_snapshots,
        ))
    }
}

#[cfg(feature = "cli")]
const MAX_PUBLIC_SOURCE_SNAPSHOT_BYTES: u64 = 5 * 1024 * 1024;

#[cfg(feature = "cli")]
#[derive(Debug, Deserialize)]
struct GeneratedSourceSnapshot {
    schema: String,
    package: GeneratedSourceSnapshotPackage,
    files: Vec<GeneratedSourceSnapshotFile>,
}

#[cfg(feature = "cli")]
#[derive(Debug, Deserialize)]
struct GeneratedSourceSnapshotPackage {
    namespace: Option<String>,
    name: String,
    version: String,
}

#[cfg(feature = "cli")]
#[derive(Debug, Deserialize)]
struct GeneratedSourceSnapshotFile {
    path: String,
    blake2b256: String,
    content_base64: String,
}

/// Download, authenticate, and atomically materialize a public Registry source
/// snapshot. The current source-package profile accepts only the generated JSON
/// snapshot shape; opaque archives remain publish evidence but are not executed
/// or unpacked by the dependency resolver.
#[cfg(feature = "cli")]
pub fn materialize_public_source_snapshot(
    snapshot: &PublicRegistrySourceSnapshot,
    cache_root: &Path,
    namespace: &str,
    name: &str,
    version: &str,
    expected_source_hash: &str,
) -> Result<PathBuf> {
    validate_public_source_snapshot_descriptor(snapshot, expected_source_hash)?;
    let bytes = download_public_source_snapshot(snapshot)?;
    std::fs::create_dir_all(cache_root).map_err(|error| {
        CompileError::without_span(format!("failed to create source snapshot cache '{}': {error}", cache_root.display()))
    })?;
    let cache_suffix = snapshot.snapshot_hash.trim_start_matches("sha256:");
    let target = cache_root.join(format!("{name}-snapshot-{cache_suffix}"));
    let temporary = unique_snapshot_temp_dir(cache_root, name)?;
    let materialized = (|| {
        materialize_generated_source_snapshot_bytes(&bytes, &temporary, namespace, name, version, expected_source_hash)?;
        remove_cache_entry(&target)?;
        std::fs::rename(&temporary, &target).map_err(|error| {
            CompileError::without_span(format!(
                "failed to commit source snapshot cache '{}' to '{}': {error}",
                temporary.display(),
                target.display()
            ))
        })?;
        Ok(target.clone())
    })();
    if materialized.is_err() {
        let _ = std::fs::remove_dir_all(&temporary);
    }
    materialized
}

/// Authenticate and materialize the generated JSON source-snapshot profile
/// into a caller-owned, non-existent directory. This is shared by dependency
/// resolution and the isolated Registry build-verification worker so both
/// paths enforce identical identity, path, per-file hash, size, and whole-tree
/// source-hash checks.
#[cfg(feature = "cli")]
pub fn materialize_generated_source_snapshot_bytes(
    bytes: &[u8],
    destination: &Path,
    namespace: &str,
    name: &str,
    version: &str,
    expected_source_hash: &str,
) -> Result<()> {
    if destination.exists() {
        return Err(CompileError::without_span(format!("source snapshot destination '{}' already exists", destination.display())));
    }
    unpack_generated_source_snapshot(bytes, destination, namespace, name, version)?;
    let computed_source_hash = compute_source_hash(destination)?;
    if computed_source_hash != expected_source_hash {
        return Err(CompileError::without_span(format!(
            "public registry source snapshot for '{namespace}/{name}@{version}' has source_hash '{computed_source_hash}', expected '{expected_source_hash}'"
        )));
    }
    Ok(())
}

#[cfg(not(feature = "cli"))]
pub fn materialize_public_source_snapshot(
    _snapshot: &PublicRegistrySourceSnapshot,
    _cache_root: &Path,
    namespace: &str,
    name: &str,
    version: &str,
    _expected_source_hash: &str,
) -> Result<PathBuf> {
    Err(CompileError::without_span(format!(
        "public registry source snapshot resolution for '{namespace}/{name}@{version}' requires the 'cli' feature"
    )))
}

#[cfg(feature = "cli")]
fn validate_public_source_snapshot_descriptor(snapshot: &PublicRegistrySourceSnapshot, expected_source_hash: &str) -> Result<()> {
    if snapshot.schema != "cellscript-registry-source-snapshot-v1" {
        return Err(CompileError::without_span(format!("unsupported public registry source snapshot schema '{}'", snapshot.schema)));
    }
    let digest = snapshot
        .snapshot_hash
        .strip_prefix("sha256:")
        .ok_or_else(|| CompileError::without_span("public registry source snapshot hash must use the sha256:<hex> form"))?;
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CompileError::without_span("public registry source snapshot hash must contain 32 bytes of hex"));
    }
    if snapshot.source_hash != expected_source_hash {
        return Err(CompileError::without_span("public registry source snapshot does not match the selected package source_hash"));
    }
    if snapshot.size_bytes == 0 || snapshot.size_bytes > MAX_PUBLIC_SOURCE_SNAPSHOT_BYTES {
        return Err(CompileError::without_span(format!(
            "public registry source snapshot size must be between 1 and {MAX_PUBLIC_SOURCE_SNAPSHOT_BYTES} bytes"
        )));
    }
    if snapshot.content_type != "application/vnd.cellscript.source-snapshot+json" {
        return Err(CompileError::without_span(format!(
            "public registry dependency resolution does not support source snapshot content type '{}'",
            snapshot.content_type
        )));
    }
    let url = reqwest::Url::parse(&snapshot.url)
        .map_err(|error| CompileError::without_span(format!("public registry source snapshot URL is invalid: {error}")))?;
    if !matches!(url.scheme(), "http" | "https") || !url.username().is_empty() || url.password().is_some() || url.fragment().is_some()
    {
        return Err(CompileError::without_span(
            "public registry source snapshot URL must be an HTTP(S) URL without credentials or a fragment",
        ));
    }
    Ok(())
}

#[cfg(feature = "cli")]
fn download_public_source_snapshot(snapshot: &PublicRegistrySourceSnapshot) -> Result<Vec<u8>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| CompileError::without_span(format!("failed to initialize source snapshot client: {error}")))?;
    let response = client
        .get(&snapshot.url)
        .header(reqwest::header::ACCEPT, snapshot.content_type.as_str())
        .header(reqwest::header::USER_AGENT, format!("cellc/{}", env!("CARGO_PKG_VERSION")))
        .send()
        .map_err(|error| CompileError::without_span(format!("source snapshot request '{}' failed: {error}", snapshot.url)))?;
    if !response.status().is_success() {
        return Err(CompileError::without_span(format!(
            "source snapshot request '{}' returned HTTP {}",
            snapshot.url,
            response.status()
        )));
    }
    if response.content_length().is_some_and(|length| length != snapshot.size_bytes) {
        return Err(CompileError::without_span("public registry source snapshot Content-Length does not match its descriptor"));
    }
    let mut bytes = Vec::with_capacity(snapshot.size_bytes as usize);
    response
        .take(snapshot.size_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| CompileError::without_span(format!("failed to read source snapshot '{}': {error}", snapshot.url)))?;
    if bytes.len() as u64 != snapshot.size_bytes {
        return Err(CompileError::without_span("downloaded public registry source snapshot size does not match its descriptor"));
    }
    let actual = format!("sha256:{}", hex::encode(Sha256::digest(&bytes)));
    if actual != snapshot.snapshot_hash.to_ascii_lowercase() {
        return Err(CompileError::without_span(format!(
            "public registry source snapshot hash mismatch: expected '{}', got '{actual}'",
            snapshot.snapshot_hash
        )));
    }
    Ok(bytes)
}

#[cfg(feature = "cli")]
fn unpack_generated_source_snapshot(bytes: &[u8], destination: &Path, namespace: &str, name: &str, version: &str) -> Result<()> {
    let snapshot: GeneratedSourceSnapshot = serde_json::from_slice(bytes)
        .map_err(|error| CompileError::without_span(format!("failed to parse public registry source snapshot: {error}")))?;
    if snapshot.schema != "cellscript-source-snapshot-v1" {
        return Err(CompileError::without_span(format!("unsupported generated source snapshot schema '{}'", snapshot.schema)));
    }
    if snapshot.package.namespace.as_deref() != Some(namespace) || snapshot.package.name != name || snapshot.package.version != version
    {
        return Err(CompileError::without_span(format!(
            "public registry source snapshot identity does not match '{namespace}/{name}@{version}'"
        )));
    }
    if snapshot.files.is_empty() || snapshot.files.len() > 4096 {
        return Err(CompileError::without_span("public registry source snapshot must contain between 1 and 4096 files"));
    }
    std::fs::create_dir(destination).map_err(|error| {
        CompileError::without_span(format!("failed to create source snapshot staging directory '{}': {error}", destination.display()))
    })?;
    let mut paths = std::collections::BTreeSet::new();
    let mut decoded_bytes = 0_u64;
    for file in snapshot.files {
        let relative = validated_snapshot_path(&file.path)?;
        if !paths.insert(relative.clone()) {
            return Err(CompileError::without_span(format!("source snapshot contains duplicate path '{}'", file.path)));
        }
        let content = base64::engine::general_purpose::STANDARD.decode(&file.content_base64).map_err(|error| {
            CompileError::without_span(format!("source snapshot file '{}' has invalid base64: {error}", file.path))
        })?;
        decoded_bytes = decoded_bytes.saturating_add(content.len() as u64);
        if decoded_bytes > MAX_PUBLIC_SOURCE_SNAPSHOT_BYTES {
            return Err(CompileError::without_span("decoded source snapshot exceeds the package size limit"));
        }
        let actual_file_hash = crate::hex_encode(&crate::ckb_blake2b256(&content));
        if actual_file_hash != file.blake2b256.to_ascii_lowercase() {
            return Err(CompileError::without_span(format!("source snapshot file '{}' failed its blake2b256 check", file.path)));
        }
        let output = destination.join(&relative);
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        let mut handle = options.open(&output).map_err(|error| {
            CompileError::without_span(format!("failed to create source snapshot file '{}': {error}", output.display()))
        })?;
        std::io::Write::write_all(&mut handle, &content)?;
    }
    if !destination.join("Cell.toml").is_file() {
        return Err(CompileError::without_span("public registry source snapshot does not contain Cell.toml"));
    }
    Ok(())
}

#[cfg(feature = "cli")]
fn validated_snapshot_path(value: &str) -> Result<PathBuf> {
    if value.is_empty() || value.len() > 1024 || value.starts_with('/') || value.ends_with('/') || value.contains('\\') {
        return Err(CompileError::without_span(format!("source snapshot path '{value}' is unsafe")));
    }
    let segments: Vec<_> = value.split('/').collect();
    if segments.len() > 32 || segments.iter().any(|segment| segment.is_empty() || *segment == "." || *segment == "..") {
        return Err(CompileError::without_span(format!("source snapshot path '{value}' is unsafe")));
    }
    let allowed = value == "Cell.toml" || value == "Cell.lock" || value.ends_with(".cell");
    if !allowed || segments.first().is_some_and(|segment| segment.starts_with('.')) {
        return Err(CompileError::without_span(format!("source snapshot path '{value}' is outside the source-package profile")));
    }
    Ok(segments.iter().collect())
}

#[cfg(feature = "cli")]
fn unique_snapshot_temp_dir(cache_root: &Path, name: &str) -> Result<PathBuf> {
    for attempt in 0..100_u32 {
        let candidate = cache_root.join(format!(".{name}-snapshot-{}-{attempt}.tmp", std::process::id()));
        if std::fs::symlink_metadata(&candidate).is_err() {
            return Ok(candidate);
        }
    }
    Err(CompileError::without_span(format!("failed to allocate a source snapshot staging path in '{}'", cache_root.display())))
}

#[cfg(feature = "cli")]
fn remove_cache_entry(path: &Path) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        std::fs::remove_dir_all(path)?;
    } else {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// Schema version file in the discovery index root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverySchema {
    pub schema_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityAuthorisationPayload {
    pub protocol: String,
    pub action: String,
    pub registry_origin: String,
    pub principal_type: String,
    pub principal_id: String,
    pub capability_pubkey: String,
    pub requested_scopes: Vec<String>,
    pub capability_expires_at: String,
    pub nonce: String,
    pub issued_at: String,
    pub expires_at: String,
    pub cli_version: String,
}

impl CapabilityAuthorisationPayload {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        registry_origin: String,
        principal_type: String,
        principal_id: String,
        capability_pubkey: String,
        requested_scopes: Vec<String>,
        capability_expires_at: String,
        nonce: String,
        issued_at: String,
        expires_at: String,
        cli_version: String,
    ) -> Self {
        Self {
            protocol: REGISTRY_AUTH_PROTOCOL.to_string(),
            action: AUTHORIZE_CAPABILITY_ACTION.to_string(),
            registry_origin,
            principal_type,
            principal_id,
            capability_pubkey,
            requested_scopes,
            capability_expires_at,
            nonce,
            issued_at,
            expires_at,
            cli_version,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityRevocationPayload {
    pub protocol: String,
    pub action: String,
    pub registry_origin: String,
    pub principal_type: String,
    pub principal_id: String,
    pub capability_key_id: String,
    pub nonce: String,
    pub issued_at: String,
    pub expires_at: String,
    pub cli_version: String,
}

impl CapabilityRevocationPayload {
    pub fn new(
        registry_origin: String,
        principal_type: String,
        principal_id: String,
        capability_key_id: String,
        nonce: String,
        issued_at: String,
        expires_at: String,
        cli_version: String,
    ) -> Self {
        Self {
            protocol: REGISTRY_AUTH_PROTOCOL.to_string(),
            action: REVOKE_CAPABILITY_ACTION.to_string(),
            registry_origin,
            principal_type,
            principal_id,
            capability_key_id,
            nonce,
            issued_at,
            expires_at,
            cli_version,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryPublishPayload {
    pub protocol: String,
    pub action: String,
    pub registry_origin: String,
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub source_hash: String,
    pub manifest_hash: String,
    pub capability_key_id: String,
    pub nonce: String,
    pub issued_at: String,
    pub expires_at: String,
    pub cli_version: String,
    pub registry_entry: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryCapabilitySignature {
    pub algorithm: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrySourceSnapshot {
    pub content_base64: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub source_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryPublishRequest {
    pub payload: RegistryPublishPayload,
    pub capability_signature: RegistryCapabilitySignature,
    pub source_snapshot: RegistrySourceSnapshot,
}

/// Manages the local clone/cache of the discovery index Git repository.
pub struct DiscoveryIndex {
    registry_url: String,
    cache_dir: PathBuf,
}

impl DiscoveryIndex {
    pub fn new(registry_url: &str, cache_dir: &Path) -> Self {
        Self { registry_url: registry_url.to_string(), cache_dir: cache_dir.to_path_buf() }
    }

    /// Clone or update the discovery index, returning the path to the local clone.
    pub fn clone_or_update(&self) -> Result<PathBuf> {
        let clone_dir = self.clone_dir();
        std::fs::create_dir_all(&self.cache_dir).map_err(|e| {
            CompileError::without_span(format!("failed to create registry cache directory '{}': {}", self.cache_dir.display(), e))
        })?;

        if clone_dir.exists() && clone_dir.join(".git").exists() {
            git_update(&clone_dir).map_err(CompileError::without_span)?;
        } else {
            let _ = std::fs::remove_dir_all(&clone_dir);
            git_clone(&self.registry_url, &clone_dir).map_err(CompileError::without_span)?;
        }

        Ok(clone_dir)
    }

    /// Look up a package in the discovery index by namespace and name.
    ///
    /// Resolution order:
    /// 1. Check the discovery index for an explicit entry.
    /// 2. If not found, fall back to the Go-style convention:
    ///    `github.com/<namespace>/<name>`. This makes the discovery index
    ///    an optional override mechanism, not a mandatory gate.
    pub fn lookup(&self, namespace: &str, name: &str) -> Result<DiscoveryEntry> {
        let fallback_source = format!("https://github.com/{}/{}", namespace, name);
        let fallback = || DiscoveryEntry { name: name.to_string(), namespace: namespace.to_string(), source: fallback_source.clone() };

        let clone_dir = match self.clone_or_update() {
            Ok(clone_dir) => clone_dir,
            Err(_) if self.registry_url == DEFAULT_REGISTRY_URL => return Ok(fallback()),
            Err(error) => return Err(error),
        };
        let entry_path = clone_dir.join(namespace).join(format!("{}.json", name));

        if entry_path.exists() {
            let content = std::fs::read_to_string(&entry_path)
                .map_err(|e| CompileError::without_span(format!("failed to read registry entry '{}': {}", entry_path.display(), e)))?;

            let entry: DiscoveryEntry = serde_json::from_str(&content).map_err(|e| {
                CompileError::without_span(format!("failed to parse registry entry '{}': {}", entry_path.display(), e))
            })?;

            return Ok(entry);
        }

        // Fall back to Go-style convention: github.com/<namespace>/<name>
        Ok(fallback())
    }

    /// Add a new package entry to the discovery index.
    /// Creates the `{namespace}/{name}.json` file in the local clone.
    pub fn add_entry(&self, namespace: &str, name: &str, source_url: &str) -> Result<PathBuf> {
        let clone_dir = self.clone_or_update()?;
        let namespace_dir = clone_dir.join(namespace);
        std::fs::create_dir_all(&namespace_dir)?;

        let entry = DiscoveryEntry { name: name.to_string(), namespace: namespace.to_string(), source: source_url.to_string() };

        let entry_path = namespace_dir.join(format!("{}.json", name));
        let content = serde_json::to_string_pretty(&entry)
            .map_err(|e| CompileError::without_span(format!("failed to serialize discovery entry: {}", e)))?;

        std::fs::write(&entry_path, content)?;
        Ok(entry_path)
    }

    fn clone_dir(&self) -> PathBuf {
        let host_key = simple_hash(&self.registry_url);
        self.cache_dir.join(format!("discovery-{:016x}", host_key))
    }
}

// ---------------------------------------------------------------------------
// Per-Package Version Index (registry.json)
// ---------------------------------------------------------------------------

/// The per-package version index stored in the source repository root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryIndex {
    pub schema_version: u32,
    pub name: String,
    pub namespace: String,
    pub versions: Vec<RegistryVersion>,
}

/// Public registry visibility and resolver eligibility state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryEntryStatus {
    SourcePublished,
    IndexedPending,
    VerifiedBuild,
    Deployed,
    OnChainAttested,
    Deprecated,
    Yanked,
    Quarantined,
}

impl RegistryEntryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SourcePublished => "source_published",
            Self::IndexedPending => "indexed_pending",
            Self::VerifiedBuild => "verified_build",
            Self::Deployed => "deployed",
            Self::OnChainAttested => "on_chain_attested",
            Self::Deprecated => "deprecated",
            Self::Yanked => "yanked",
            Self::Quarantined => "quarantined",
        }
    }

    pub fn is_baseline_verified(&self) -> bool {
        matches!(self, Self::VerifiedBuild | Self::Deployed | Self::OnChainAttested)
    }

    pub fn is_unverified_direct_install(&self) -> bool {
        matches!(self, Self::SourcePublished | Self::IndexedPending)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RegistryResolutionPolicy {
    pub allow_unverified: bool,
    pub allow_quarantined: bool,
}

/// A single version entry in the per-package version index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryVersion {
    pub version: String,
    pub tag: String,
    pub source_hash: String,
    pub cellscript_version: String,
    /// Long-lived source-language semantics epoch.
    pub edition: crate::CellScriptEdition,
    /// Hash of the resolved source/target/assurance/ABI/schema profile.
    pub compatibility_profile_hash: String,
    pub dependencies: BTreeMap<String, RegistryDependencyRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abi_index: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub released_at: Option<String>,
    pub status: RegistryEntryStatus,
    pub yanked: bool,
    /// When the version was yanked (ISO 8601 UTC). Present only when `yanked` is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yanked_at: Option<String>,
    /// Human-readable reason the version was yanked (e.g. a security advisory id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yanked_reason: Option<String>,
    /// Suggested replacement version, if any, after a yank.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<RegistryAuditInfo>,
}

impl RegistryVersion {
    pub fn effective_status(&self) -> RegistryEntryStatus {
        if self.yanked {
            RegistryEntryStatus::Yanked
        } else {
            self.status.clone()
        }
    }

    pub fn resolver_block_reason(&self, policy: RegistryResolutionPolicy, allow_suppressed_exact_pin: bool) -> Option<&'static str> {
        if self.yanked {
            return (!allow_suppressed_exact_pin).then_some("yanked");
        }

        match self.status {
            RegistryEntryStatus::VerifiedBuild | RegistryEntryStatus::Deployed | RegistryEntryStatus::OnChainAttested => None,
            RegistryEntryStatus::SourcePublished | RegistryEntryStatus::IndexedPending if policy.allow_unverified => None,
            RegistryEntryStatus::SourcePublished | RegistryEntryStatus::IndexedPending => Some("unverified"),
            RegistryEntryStatus::Quarantined if policy.allow_quarantined => None,
            RegistryEntryStatus::Quarantined => Some("quarantined"),
            RegistryEntryStatus::Deprecated if allow_suppressed_exact_pin => None,
            RegistryEntryStatus::Deprecated => Some("deprecated"),
            RegistryEntryStatus::Yanked if allow_suppressed_exact_pin => None,
            RegistryEntryStatus::Yanked => Some("yanked"),
        }
    }
}

/// A dependency reference within a registry version entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryDependencyRef {
    pub namespace: String,
    pub version: String,
}

/// Audit information for a registry version entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryAuditInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance_gate: Option<String>,
}

impl RegistryIndex {
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;

    fn ensure_current_schema(&self) -> Result<()> {
        if self.schema_version != Self::CURRENT_SCHEMA_VERSION {
            return Err(CompileError::without_span(format!(
                "unsupported registry.json schema_version {}; current registry contract requires schema_version {}",
                self.schema_version,
                Self::CURRENT_SCHEMA_VERSION,
            )));
        }
        Ok(())
    }

    /// Read registry.json from a repository directory.
    pub fn read_from_repo(repo_dir: &Path) -> Result<Self> {
        let path = repo_dir.join("registry.json");
        if !path.exists() {
            return Err(CompileError::without_span(format!("registry.json not found in '{}'", repo_dir.display())));
        }
        let content =
            std::fs::read_to_string(&path).map_err(|e| CompileError::without_span(format!("failed to read registry.json: {}", e)))?;
        let index: Self =
            serde_json::from_str(&content).map_err(|e| CompileError::without_span(format!("failed to parse registry.json: {}", e)))?;
        index.ensure_current_schema()?;
        Ok(index)
    }

    /// Write registry.json to a repository directory.
    pub fn write_to_repo(&self, repo_dir: &Path) -> Result<()> {
        self.ensure_current_schema()?;
        let path = repo_dir.join("registry.json");
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| CompileError::without_span(format!("failed to serialize registry.json: {}", e)))?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Append a new version entry. If registry.json does not exist, creates it.
    pub fn append_version(repo_dir: &Path, name: &str, namespace: &str, version: RegistryVersion) -> Result<()> {
        let mut index = if repo_dir.join("registry.json").exists() {
            Self::read_from_repo(repo_dir)?
        } else {
            Self {
                schema_version: Self::CURRENT_SCHEMA_VERSION,
                name: name.to_string(),
                namespace: namespace.to_string(),
                versions: Vec::new(),
            }
        };

        // Remove existing version if present (update semantics)
        index.versions.retain(|v| v.version != version.version);
        index.versions.push(version);

        index.write_to_repo(repo_dir)
    }

    /// Find the latest non-yanked version matching a version requirement.
    pub fn find_matching_version(&self, version_req: &str) -> Option<&RegistryVersion> {
        let req = crate::package::version::parse_version_req(version_req).ok()?;

        self.find_matching_version_with_req(&req, false)
    }

    /// Find a matching version for resolver use.
    ///
    /// Range and compatible requirements always skip yanked versions. An exact
    /// `=x.y.z` pin may select a yanked version so the caller can honour an
    /// explicit pin while emitting a warning with the yank metadata.
    pub fn find_matching_version_allowing_yanked_pin(&self, version_req: &str) -> Option<&RegistryVersion> {
        let req = crate::package::version::parse_version_req(version_req).ok()?;
        let allow_yanked = matches!(req, crate::package::VersionReq::Exact(_));

        self.find_matching_version_with_req(&req, allow_yanked)
    }

    pub fn find_matching_version_for_resolution(
        &self,
        version_req: &str,
        policy: RegistryResolutionPolicy,
    ) -> Option<&RegistryVersion> {
        let req = crate::package::version::parse_version_req(version_req).ok()?;
        let allow_suppressed_exact_pin = matches!(req, crate::package::VersionReq::Exact(_));

        self.find_matching_version_with_req_and_policy(&req, allow_suppressed_exact_pin, policy)
    }

    fn find_matching_version_with_req(&self, req: &crate::package::VersionReq, allow_yanked: bool) -> Option<&RegistryVersion> {
        self.find_matching_version_with_req_and_policy(
            req,
            allow_yanked,
            RegistryResolutionPolicy { allow_unverified: true, allow_quarantined: true },
        )
    }

    fn find_matching_version_with_req_and_policy(
        &self,
        req: &crate::package::VersionReq,
        allow_suppressed_exact_pin: bool,
        policy: RegistryResolutionPolicy,
    ) -> Option<&RegistryVersion> {
        self.versions
            .iter()
            .filter(|v| v.resolver_block_reason(policy, allow_suppressed_exact_pin).is_none())
            .filter(|v| crate::package::version::satisfies(&v.version, req))
            .max_by(|a, b| {
                // Compare versions numerically
                let a_parts = parse_version_parts(&a.version);
                let b_parts = parse_version_parts(&b.version);
                compare_version_parts(&a_parts, &b_parts)
            })
    }
}

// ---------------------------------------------------------------------------
// Source hash computation
// ---------------------------------------------------------------------------

/// Compute the source hash of a package directory.
/// Walks all source files, concatenates their relative paths and content,
/// then returns blake2b-256 hex digest.
pub fn compute_source_hash(root: &Path) -> Result<String> {
    let mut hasher = ckb_blake2b256_stream::Hasher::new();

    let manifest_path = root.join("Cell.toml");
    let mut manifest = SourceHashManifest::default();
    if manifest_path.exists() {
        let content = std::fs::read_to_string(&manifest_path)?;
        manifest = toml::from_str(&content)
            .map_err(|e| CompileError::without_span(format!("failed to parse Cell.toml for source hashing: {}", e)))?;
        hasher.update(b"Cell.toml:");
        hasher.update(content.as_bytes());
        hasher.update(b"\n");
    }

    let mut files = collect_hash_source_files(root, &manifest)?;
    files.sort();
    files.dedup();
    for file_path in &files {
        let rel = file_path.strip_prefix(root).unwrap_or(file_path);
        let content = std::fs::read_to_string(file_path)
            .map_err(|e| CompileError::without_span(format!("failed to read '{}': {}", file_path.display(), e)))?;
        hasher.update(rel.to_string_lossy().replace('\\', "/").as_bytes());
        hasher.update(b":");
        hasher.update(content.as_bytes());
        hasher.update(b"\n");
    }

    let hash = hasher.finalize();
    Ok(crate::hex_encode(&hash))
}

#[derive(Debug, Default, Deserialize)]
struct SourceHashManifest {
    #[serde(default)]
    package: Option<SourceHashPackage>,
}

#[derive(Debug, Default, Deserialize)]
struct SourceHashPackage {
    #[serde(default)]
    entry: Option<String>,
    #[serde(default)]
    source_roots: Vec<String>,
}

fn collect_hash_source_files(root: &Path, manifest: &SourceHashManifest) -> Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    let mut seen_roots = std::collections::BTreeSet::new();

    if let Some(package) = &manifest.package {
        for source_root in &package.source_roots {
            let source_root_path = root.join(source_root);
            if !source_root_path.exists() {
                return Err(CompileError::without_span(format!(
                    "configured source root '{}' does not exist",
                    source_root_path.display()
                )));
            }
            if !source_root_path.is_dir() {
                return Err(CompileError::without_span(format!(
                    "configured source root '{}' is not a directory",
                    source_root_path.display()
                )));
            }
            if seen_roots.insert(source_root_path.clone()) {
                roots.push(source_root_path);
            }
        }
    }

    if roots.is_empty() {
        let src_dir = root.join("src");
        if src_dir.exists() && src_dir.is_dir() && seen_roots.insert(src_dir.clone()) {
            roots.push(src_dir);
        }
    }

    let mut explicit_entry = None;
    if let Some(entry) = manifest.package.as_ref().and_then(|package| package.entry.as_deref()) {
        let entry_path = root.join(entry);
        if !entry_path.exists() {
            return Err(CompileError::without_span(format!("package entry '{}' does not exist", entry_path.display())));
        }
        if let Some(parent) = entry_path.parent() {
            let parent = parent.to_path_buf();
            if seen_roots.insert(parent.clone()) {
                roots.push(parent);
            }
        }
        explicit_entry = Some(entry_path);
    }

    let mut files = Vec::new();
    for source_root in roots {
        files.extend(collect_cell_files(&source_root)?);
    }
    if let Some(entry_path) = explicit_entry {
        files.push(entry_path);
    }
    Ok(files)
}

fn collect_cell_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !dir.exists() {
        return Ok(files);
    }
    let entries = std::fs::read_dir(dir)
        .map_err(|e| CompileError::without_span(format!("failed to read directory '{}': {}", dir.display(), e)))?;
    for entry in entries {
        let entry = entry.map_err(|e| CompileError::without_span(format!("failed to read directory entry: {}", e)))?;
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_cell_files(&path)?);
        } else if path.extension().is_some_and(|ext| ext == "cell") {
            files.push(path);
        }
    }
    Ok(files)
}

// ---------------------------------------------------------------------------
// Git helpers (reused from PackageManager)
// ---------------------------------------------------------------------------

pub fn git_clone(url: &str, target: &Path) -> std::result::Result<(), String> {
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

pub fn git_update(clone_dir: &Path) -> std::result::Result<(), String> {
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

pub fn git_checkout(clone_dir: &Path, ref_str: &str) -> std::result::Result<(), String> {
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

pub fn git_revision(clone_dir: &Path) -> std::result::Result<String, String> {
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

/// List git tags in a repository, returning pairs of (tag_name, commit_hash).
pub fn git_list_tags(clone_dir: &Path) -> std::result::Result<Vec<(String, String)>, String> {
    let output = std::process::Command::new("git")
        .args(["tag", "-l"])
        .current_dir(clone_dir)
        .output()
        .map_err(|e| format!("failed to execute git tag: {}", e))?;

    if !output.status.success() {
        return Err("git tag list failed".to_string());
    }

    let tags_str = String::from_utf8_lossy(&output.stdout);
    let mut result = Vec::new();
    for tag in tags_str.lines() {
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }
        // Get the commit hash for each tag
        let rev_output = std::process::Command::new("git")
            .args(["rev-list", "-1", tag])
            .current_dir(clone_dir)
            .output()
            .map_err(|e| format!("failed to get revision for tag '{}': {}", tag, e))?;

        if rev_output.status.success() {
            let rev = String::from_utf8_lossy(&rev_output.stdout).trim().to_string();
            result.push((tag.to_string(), rev));
        }
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Internal utilities
// ---------------------------------------------------------------------------

fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn parse_version_parts(version: &str) -> Vec<u32> {
    let core = version.split_once('-').map(|(c, _)| c).unwrap_or(version);
    core.split('.').filter_map(|p| p.parse().ok()).collect()
}

fn compare_version_parts(a: &[u32], b: &[u32]) -> std::cmp::Ordering {
    let max_len = a.len().max(b.len());
    for i in 0..max_len {
        let av = a.get(i).unwrap_or(&0);
        let bv = b.get(i).unwrap_or(&0);
        match av.cmp(bv) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

/// A streaming blake2b-256 hasher (simplified, using the existing ckb_blake2b256 on final content).
mod ckb_blake2b256_stream {
    use std::collections::VecDeque;

    pub struct Hasher {
        chunks: VecDeque<Vec<u8>>,
    }

    impl Hasher {
        pub fn new() -> Self {
            Self { chunks: VecDeque::new() }
        }

        pub fn update(&mut self, data: &[u8]) {
            self.chunks.push_back(data.to_vec());
        }

        pub fn finalize(self) -> [u8; 32] {
            let mut all = Vec::new();
            for chunk in self.chunks {
                all.extend_from_slice(&chunk);
            }
            crate::ckb_blake2b256(&all)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_manifest_hash_is_independent_of_map_insertion_order() {
        let first: PackageManifest = toml::from_str(
            r#"[package]
edition = "2026"
name = "demo"
version = "1.2.3"

[dependencies]
alpha = "1"
beta = "2"

[metadata]
left = "a"
right = "b"
"#,
        )
        .unwrap();
        let second: PackageManifest = toml::from_str(
            r#"[package]
edition = "2026"
name = "demo"
version = "1.2.3"

[dependencies]
beta = "2"
alpha = "1"

[metadata]
right = "b"
left = "a"
"#,
        )
        .unwrap();

        assert_eq!(compute_package_manifest_hash(&first).unwrap(), compute_package_manifest_hash(&second).unwrap());
    }

    #[test]
    fn public_registry_status_overrides_publisher_claim() {
        let payload: PublicRegistryPackage = serde_json::from_value(serde_json::json!({
            "schema": "cellscript-public-registry-package-v1",
            "namespace": "cellscript",
            "name": "demo",
            "repository": "https://github.com/cellscript/demo",
            "versions": [{
                "version": "1.2.3",
                "status": "deployed",
                "registry_entry": {
                    "schema_version": 1,
                    "namespace": "cellscript",
                    "name": "demo",
                    "versions": [{
                        "version": "1.2.3",
                        "tag": "v1.2.3",
                        "source_hash": "source-hash",
                        "cellscript_version": "0.23.0",
                        "edition": "2026",
                        "compatibility_profile_hash": "profile-hash",
                        "dependencies": {},
                        "status": "source_published",
                        "yanked": false
                    }]
                },
                "source_snapshot": {
                    "schema": "cellscript-registry-source-snapshot-v1",
                    "url": "https://registry.cellscript.dev/source-snapshots/cellscript/demo/1.2.3/example.json",
                    "snapshot_hash": format!("sha256:{}", "1".repeat(64)),
                    "source_hash": "source-hash",
                    "size_bytes": 123,
                    "content_type": "application/vnd.cellscript.source-snapshot+json"
                }
            }]
        }))
        .unwrap();

        let (entry, index, snapshots) = payload.into_resolution("cellscript", "demo").unwrap();
        assert_eq!(entry.source, "https://github.com/cellscript/demo");
        assert_eq!(index.versions.len(), 1);
        assert_eq!(index.versions[0].status, RegistryEntryStatus::Deployed);
        assert!(!index.versions[0].yanked);
        assert_eq!(snapshots["1.2.3"].source_hash, "source-hash");
    }

    #[cfg(feature = "cli")]
    #[test]
    fn generated_source_snapshot_materialization_checks_paths_and_file_hashes() {
        use base64::Engine as _;

        let manifest = b"[package]\nname = \"demo\"\nversion = \"1.2.3\"\nnamespace = \"cellscript\"\nedition = \"2026\"\nentry = \"src/main.cell\"\n";
        let source = b"script Demo {}\n";
        let file = |path: &str, content: &[u8]| {
            serde_json::json!({
                "path": path,
                "blake2b256": crate::hex_encode(&crate::ckb_blake2b256(content)),
                "content_base64": base64::engine::general_purpose::STANDARD.encode(content),
            })
        };
        let snapshot = serde_json::to_vec(&serde_json::json!({
            "schema": "cellscript-source-snapshot-v1",
            "package": { "namespace": "cellscript", "name": "demo", "version": "1.2.3" },
            "files": [file("Cell.toml", manifest), file("src/main.cell", source)],
        }))
        .unwrap();
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("valid");
        unpack_generated_source_snapshot(&snapshot, &destination, "cellscript", "demo", "1.2.3").unwrap();
        assert_eq!(std::fs::read(destination.join("src/main.cell")).unwrap(), source);

        let unsafe_snapshot = serde_json::to_vec(&serde_json::json!({
            "schema": "cellscript-source-snapshot-v1",
            "package": { "namespace": "cellscript", "name": "demo", "version": "1.2.3" },
            "files": [file("../Cell.toml", manifest)],
        }))
        .unwrap();
        let error = unpack_generated_source_snapshot(&unsafe_snapshot, &root.path().join("unsafe"), "cellscript", "demo", "1.2.3")
            .unwrap_err();
        assert!(error.to_string().contains("unsafe"));
        assert!(!root.path().join("Cell.toml").exists());
    }

    #[cfg(feature = "cli")]
    #[test]
    fn public_snapshot_descriptor_rejects_opaque_archives() {
        let snapshot = PublicRegistrySourceSnapshot {
            schema: "cellscript-registry-source-snapshot-v1".to_string(),
            url: "https://registry.cellscript.dev/source-snapshots/demo.tar".to_string(),
            snapshot_hash: format!("sha256:{}", "1".repeat(64)),
            source_hash: "source-hash".to_string(),
            size_bytes: 42,
            content_type: "application/x-tar".to_string(),
        };
        let error = validate_public_source_snapshot_descriptor(&snapshot, "source-hash").unwrap_err();
        assert!(error.to_string().contains("does not support source snapshot content type"));
    }

    #[cfg(feature = "cli")]
    #[test]
    fn public_source_snapshot_download_is_hash_bound_and_materialized_without_git() {
        use base64::Engine as _;
        use std::io::Read as _;

        let source_root = tempfile::tempdir().unwrap();
        std::fs::create_dir(source_root.path().join("src")).unwrap();
        let manifest = b"[package]\nedition = \"2026\"\nname = \"demo\"\nnamespace = \"cellscript\"\nversion = \"1.2.3\"\nentry = \"src/main.cell\"\n";
        let source = b"script Demo {}\n";
        std::fs::write(source_root.path().join("Cell.toml"), manifest).unwrap();
        std::fs::write(source_root.path().join("src/main.cell"), source).unwrap();
        let source_hash = compute_source_hash(source_root.path()).unwrap();
        let snapshot_file = |path: &str, content: &[u8]| {
            serde_json::json!({
                "path": path,
                "blake2b256": crate::hex_encode(&crate::ckb_blake2b256(content)),
                "content_base64": base64::engine::general_purpose::STANDARD.encode(content),
            })
        };
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schema": "cellscript-source-snapshot-v1",
            "package": { "namespace": "cellscript", "name": "demo", "version": "1.2.3" },
            "files": [snapshot_file("Cell.toml", manifest), snapshot_file("src/main.cell", source)],
        }))
        .unwrap();
        let snapshot_hash = format!("sha256:{}", hex::encode(Sha256::digest(&bytes)));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let response_bytes = bytes.clone();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request).unwrap();
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
                response_bytes.len()
            );
            std::io::Write::write_all(&mut stream, headers.as_bytes()).unwrap();
            std::io::Write::write_all(&mut stream, &response_bytes).unwrap();
        });
        let descriptor = PublicRegistrySourceSnapshot {
            schema: "cellscript-registry-source-snapshot-v1".to_string(),
            url: format!("http://{address}/snapshot.json"),
            snapshot_hash: snapshot_hash.clone(),
            source_hash: source_hash.clone(),
            size_bytes: bytes.len() as u64,
            content_type: "application/vnd.cellscript.source-snapshot+json".to_string(),
        };
        let cache = tempfile::tempdir().unwrap();
        let materialized =
            materialize_public_source_snapshot(&descriptor, cache.path(), "cellscript", "demo", "1.2.3", &source_hash).unwrap();
        server.join().unwrap();
        assert_eq!(compute_source_hash(&materialized).unwrap(), source_hash);
        assert_eq!(std::fs::read(materialized.join("src/main.cell")).unwrap(), source);
        assert!(materialized
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(snapshot_hash.trim_start_matches("sha256:").get(..16).unwrap()));
    }

    #[test]
    fn registry_index_find_matching_version() {
        let index = RegistryIndex {
            schema_version: RegistryIndex::CURRENT_SCHEMA_VERSION,
            name: "token".to_string(),
            namespace: "cellscript".to_string(),
            versions: vec![
                RegistryVersion {
                    edition: crate::CURRENT_EDITION,
                    compatibility_profile_hash: "test-compatibility-profile".to_string(),
                    version: "0.1.0".to_string(),
                    tag: "v0.1.0".to_string(),
                    source_hash: "hash1".to_string(),
                    cellscript_version: "0.19.0".to_string(),
                    dependencies: BTreeMap::new(),
                    abi_index: None,
                    schema_hash: None,
                    license: None,
                    released_at: None,
                    status: RegistryEntryStatus::VerifiedBuild,
                    yanked: false,
                    yanked_at: None,
                    yanked_reason: None,
                    replaced_by: None,
                    audit: None,
                },
                RegistryVersion {
                    edition: crate::CURRENT_EDITION,
                    compatibility_profile_hash: "test-compatibility-profile".to_string(),
                    version: "0.3.2".to_string(),
                    tag: "v0.3.2".to_string(),
                    source_hash: "hash2".to_string(),
                    cellscript_version: "0.19.0".to_string(),
                    dependencies: BTreeMap::new(),
                    abi_index: None,
                    schema_hash: None,
                    license: None,
                    released_at: None,
                    status: RegistryEntryStatus::VerifiedBuild,
                    yanked: false,
                    yanked_at: None,
                    yanked_reason: None,
                    replaced_by: None,
                    audit: None,
                },
                RegistryVersion {
                    edition: crate::CURRENT_EDITION,
                    compatibility_profile_hash: "test-compatibility-profile".to_string(),
                    version: "0.3.0".to_string(),
                    tag: "v0.3.0".to_string(),
                    source_hash: "hash3".to_string(),
                    cellscript_version: "0.19.0".to_string(),
                    dependencies: BTreeMap::new(),
                    abi_index: None,
                    schema_hash: None,
                    license: None,
                    released_at: None,
                    status: RegistryEntryStatus::VerifiedBuild,
                    yanked: false,
                    yanked_at: None,
                    yanked_reason: None,
                    replaced_by: None,
                    audit: None,
                },
            ],
        };

        // Should find the latest 0.3.x version
        let found = index.find_matching_version("0.3.0").unwrap();
        assert_eq!(found.version, "0.3.2");
        assert_eq!(found.tag, "v0.3.2");

        // Should find the only 0.1.x version
        let found = index.find_matching_version("0.1.0").unwrap();
        assert_eq!(found.version, "0.1.0");

        // Should not find a non-existent major version
        assert!(index.find_matching_version("1.0.0").is_none());
    }

    #[test]
    fn registry_index_skips_yanked_versions() {
        let index = RegistryIndex {
            schema_version: RegistryIndex::CURRENT_SCHEMA_VERSION,
            name: "pkg".to_string(),
            namespace: "ns".to_string(),
            versions: vec![RegistryVersion {
                edition: crate::CURRENT_EDITION,
                compatibility_profile_hash: "test-compatibility-profile".to_string(),
                version: "1.0.0".to_string(),
                tag: "v1.0.0".to_string(),
                source_hash: "h1".to_string(),
                cellscript_version: "0.19.0".to_string(),
                dependencies: BTreeMap::new(),
                abi_index: None,
                schema_hash: None,
                license: None,
                released_at: None,
                status: RegistryEntryStatus::VerifiedBuild,
                yanked: true,
                yanked_at: None,
                yanked_reason: None,
                replaced_by: None,
                audit: None,
            }],
        };

        assert!(index.find_matching_version("1.0.0").is_none());
        assert!(index.find_matching_version_allowing_yanked_pin("1.0.0").is_none());
        let exact = index.find_matching_version_allowing_yanked_pin("=1.0.0").unwrap();
        assert_eq!(exact.version, "1.0.0");
        assert!(exact.yanked);
    }

    #[test]
    fn registry_version_yank_metadata_round_trip() {
        // A yanked version with full Phase 2 metadata must survive JSON
        // serialization and also omit cleanly when the fields are absent.
        let yanked = RegistryVersion {
            edition: crate::CURRENT_EDITION,
            compatibility_profile_hash: "test-compatibility-profile".to_string(),
            version: "1.2.0".to_string(),
            tag: "v1.2.0".to_string(),
            source_hash: "h".to_string(),
            cellscript_version: "0.19.0".to_string(),
            dependencies: BTreeMap::new(),
            abi_index: None,
            schema_hash: None,
            license: None,
            released_at: None,
            status: RegistryEntryStatus::VerifiedBuild,
            yanked: true,
            yanked_at: Some("2026-06-01T00:00:00Z".to_string()),
            yanked_reason: Some("security advisory".to_string()),
            replaced_by: Some("1.2.1".to_string()),
            audit: None,
        };

        let json = serde_json::to_string_pretty(&yanked).unwrap();
        let parsed: RegistryVersion = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.yanked_at.as_deref(), Some("2026-06-01T00:00:00Z"));
        assert_eq!(parsed.yanked_reason.as_deref(), Some("security advisory"));
        assert_eq!(parsed.replaced_by.as_deref(), Some("1.2.1"));

        // The optional yank fields are omitted from JSON when absent, so older
        // registry.json files without them still parse (backward compatible).
        let clean = RegistryVersion {
            edition: crate::CURRENT_EDITION,
            compatibility_profile_hash: "test-compatibility-profile".to_string(),
            yanked_at: None,
            yanked_reason: None,
            replaced_by: None,
            ..yanked
        };
        let clean_json = serde_json::to_string(&clean).unwrap();
        assert!(!clean_json.contains("yanked_at"));
        assert!(!clean_json.contains("yanked_reason"));
        assert!(!clean_json.contains("replaced_by"));
    }

    #[test]
    fn registry_resolution_policy_gates_unverified_and_quarantined_entries() {
        let index = RegistryIndex {
            schema_version: RegistryIndex::CURRENT_SCHEMA_VERSION,
            name: "amm".to_string(),
            namespace: "cellscript".to_string(),
            versions: vec![
                RegistryVersion {
                    edition: crate::CURRENT_EDITION,
                    compatibility_profile_hash: "test-compatibility-profile".to_string(),
                    version: "0.1.0".to_string(),
                    tag: "v0.1.0".to_string(),
                    source_hash: "hash-v010".to_string(),
                    cellscript_version: "0.20.0".to_string(),
                    dependencies: BTreeMap::new(),
                    abi_index: None,
                    schema_hash: None,
                    license: None,
                    released_at: None,
                    status: RegistryEntryStatus::VerifiedBuild,
                    yanked: false,
                    yanked_at: None,
                    yanked_reason: None,
                    replaced_by: None,
                    audit: None,
                },
                RegistryVersion {
                    edition: crate::CURRENT_EDITION,
                    compatibility_profile_hash: "test-compatibility-profile".to_string(),
                    version: "0.2.0".to_string(),
                    tag: "v0.2.0".to_string(),
                    source_hash: "hash-v020".to_string(),
                    cellscript_version: "0.20.0".to_string(),
                    dependencies: BTreeMap::new(),
                    abi_index: None,
                    schema_hash: None,
                    license: None,
                    released_at: None,
                    status: RegistryEntryStatus::SourcePublished,
                    yanked: false,
                    yanked_at: None,
                    yanked_reason: None,
                    replaced_by: None,
                    audit: None,
                },
                RegistryVersion {
                    edition: crate::CURRENT_EDITION,
                    compatibility_profile_hash: "test-compatibility-profile".to_string(),
                    version: "0.3.0".to_string(),
                    tag: "v0.3.0".to_string(),
                    source_hash: "hash-v030".to_string(),
                    cellscript_version: "0.20.0".to_string(),
                    dependencies: BTreeMap::new(),
                    abi_index: None,
                    schema_hash: None,
                    license: None,
                    released_at: None,
                    status: RegistryEntryStatus::Quarantined,
                    yanked: false,
                    yanked_at: None,
                    yanked_reason: None,
                    replaced_by: None,
                    audit: None,
                },
            ],
        };

        let default_selected = index
            .find_matching_version_for_resolution("*", RegistryResolutionPolicy::default())
            .expect("verified baseline should resolve");
        assert_eq!(default_selected.version, "0.1.0");

        let unverified_selected = index
            .find_matching_version_for_resolution("*", RegistryResolutionPolicy { allow_unverified: true, allow_quarantined: false })
            .expect("unverified direct install should resolve with explicit policy");
        assert_eq!(unverified_selected.version, "0.2.0");

        let quarantined_selected = index
            .find_matching_version_for_resolution("*", RegistryResolutionPolicy { allow_unverified: true, allow_quarantined: true })
            .expect("quarantined direct install should require explicit policy");
        assert_eq!(quarantined_selected.version, "0.3.0");
    }

    #[test]
    fn registry_index_rejects_missing_required_version_fields() {
        let complete = serde_json::json!({
            "schema_version": 1,
            "name": "amm",
            "namespace": "cellscript",
            "versions": [{
                "version": "1.0.0",
                "tag": "v1.0.0",
                "source_hash": "hash-v100",
                "cellscript_version": "0.20.0",
                "edition": "2026",
                "compatibility_profile_hash": "test-compatibility-profile",
                "dependencies": {},
                "status": "source_published",
                "yanked": false
            }]
        });

        for field in ["dependencies", "status", "yanked"] {
            let mut incomplete = complete.clone();
            incomplete["versions"][0].as_object_mut().unwrap().remove(field);
            let error = serde_json::from_value::<RegistryIndex>(incomplete).unwrap_err();
            assert!(error.to_string().contains(&format!("missing field `{field}`")));
        }
    }

    #[test]
    fn registry_index_rejects_unknown_schema() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("registry.json"),
            r#"{
              "schema_version": 2,
              "name": "amm",
              "namespace": "cellscript",
              "versions": [{
                "version": "1.0.0",
                "tag": "v1.0.0",
                "source_hash": "hash-v100",
                "cellscript_version": "0.23.0",
                "edition": "2026",
                "compatibility_profile_hash": "test-compatibility-profile",
                "dependencies": {},
                "status": "source_published",
                "yanked": false
              }]
            }"#,
        )
        .unwrap();

        let error = RegistryIndex::read_from_repo(dir.path()).unwrap_err();
        assert!(error.to_string().contains("current registry contract requires schema_version 1"));
    }

    #[test]
    fn discovery_entry_serialization_round_trip() {
        let entry = DiscoveryEntry {
            name: "amm".to_string(),
            namespace: "cellscript".to_string(),
            source: "https://github.com/cellscript/amm".to_string(),
        };

        let json = serde_json::to_string_pretty(&entry).unwrap();
        let parsed: DiscoveryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "amm");
        assert_eq!(parsed.namespace, "cellscript");
        assert_eq!(parsed.source, "https://github.com/cellscript/amm");
    }

    #[test]
    fn registry_index_serialization_round_trip() {
        let index = RegistryIndex {
            schema_version: RegistryIndex::CURRENT_SCHEMA_VERSION,
            name: "amm_pool".to_string(),
            namespace: "cellscript".to_string(),
            versions: vec![RegistryVersion {
                edition: crate::CURRENT_EDITION,
                compatibility_profile_hash: "test-compatibility-profile".to_string(),
                version: "1.2.0".to_string(),
                tag: "v1.2.0".to_string(),
                source_hash: "blake2b:0xabcd".to_string(),
                cellscript_version: "0.19.0".to_string(),
                dependencies: BTreeMap::from([(
                    "token".to_string(),
                    RegistryDependencyRef { namespace: "cellscript".to_string(), version: "0.3.0".to_string() },
                )]),
                abi_index: Some("blake2b:0xdef0".to_string()),
                schema_hash: Some("blake2b:0x9abc".to_string()),
                license: Some("MIT".to_string()),
                released_at: Some("2026-05-06T00:00:00Z".to_string()),
                status: RegistryEntryStatus::VerifiedBuild,
                yanked: false,
                yanked_at: None,
                yanked_reason: None,
                replaced_by: None,
                audit: Some(RegistryAuditInfo {
                    report_hash: Some("blake2b:0x5555".to_string()),
                    acceptance_gate: Some("passed".to_string()),
                }),
            }],
        };

        let json = serde_json::to_string_pretty(&index).unwrap();
        let parsed: RegistryIndex = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "amm_pool");
        assert_eq!(parsed.versions.len(), 1);
        assert_eq!(parsed.versions[0].version, "1.2.0");
        assert_eq!(parsed.versions[0].dependencies.len(), 1);
    }
}
