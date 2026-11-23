use super::{
    emit_yank_notices, manifest_digest, registry, registry_resolution_blocked_error, simple_hash, PackageManager, PackageManifest,
    PackageSource, ResolvedPackage, VersionReq,
};
use crate::artifact::validate_declarations;
use crate::error::{CompileError, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;

impl PackageManager {
    pub(super) fn resolve_from_registry_with_manifest(
        &self,
        name: &str,
        version: &str,
        namespace: Option<&str>,
        consuming_namespace: Option<&str>,
        policy: registry::RegistryResolutionPolicy,
    ) -> Result<(ResolvedPackage, PackageManifest)> {
        // 1. Determine namespace: explicit > consuming package namespace > error
        let resolved_namespace = namespace
            .map(str::to_string)
            .or_else(|| consuming_namespace.map(str::to_string))
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

        // 2. Resolve accepted public-registry state (or an explicitly selected
        //    offline Git mirror) → find the source repository URL.
        let cache_dir = self.registry_cache_dir();
        let registry_resolution = registry::lookup_for_resolution(&resolved_namespace, name, &cache_dir).map_err(|e| {
            CompileError::without_span(format!(
                "failed to resolve registry dependency '{}/{}@{}': {}",
                resolved_namespace, name, version, e
            ))
        })?;
        let registry::RegistryResolution { registry_url, entry, authoritative_index, mut source_snapshots } = registry_resolution;
        let repository_url = entry.source;

        // 3. Prepare the immutable Registry snapshot cache, or clone the
        //    explicitly selected legacy Git mirror.
        let source_cache = self.git_cache_dir();
        std::fs::create_dir_all(&source_cache)
            .map_err(|e| CompileError::without_span(format!("failed to create source cache directory: {}", e)))?;
        let public_registry_authoritative = authoritative_index.is_some();
        let legacy_clone = if public_registry_authoritative {
            None
        } else {
            let cache_key = format!("{}#{}", repository_url, version);
            let cache_name = format!("{}-{:016x}", name, simple_hash(&cache_key));
            let clone_dir = source_cache.join(&cache_name);
            if clone_dir.exists() && clone_dir.join(".git").exists() {
                registry::git_update(&clone_dir).map_err(CompileError::without_span)?;
            } else {
                let _ = std::fs::remove_dir_all(&clone_dir);
                registry::git_clone(&repository_url, &clone_dir).map_err(CompileError::without_span)?;
            }
            Some(clone_dir)
        };

        // 4. Resolve versions against production-accepted status. A legacy
        //    Git override retains the historical registry.json authority.
        let reg_index = match authoritative_index {
            Some(index) => index,
            None => registry::RegistryIndex::read_from_repo(legacy_clone.as_ref().expect("legacy clone exists"))?,
        };
        reg_index.ensure_current_schema()?;
        if reg_index.name != name || reg_index.namespace != resolved_namespace {
            return Err(CompileError::without_span(format!(
                "registry.json identity mismatch for '{}/{}': found '{}/{}'",
                resolved_namespace, name, reg_index.namespace, reg_index.name
            )));
        }
        let selected_version = reg_index.find_matching_version_for_resolution(version, policy).cloned().ok_or_else(|| {
            if let Some(incompatible) = reg_index.find_matching_version_for_resolution_ignoring_compiler(version, policy) {
                return CompileError::without_span(format!(
                    "registry package '{}/{}@{}' matched version '{}' requiring CellScript compiler '{}', but active cellc is '{}'; update selected the newest compiler-compatible release only",
                    resolved_namespace,
                    name,
                    version,
                    incompatible.version,
                    incompatible.compiler_requirement,
                    crate::VERSION
                ))
                .with_code(crate::error::DiagnosticCode::E2600)
                .with_details(serde_json::json!({
                    "package": name,
                    "namespace": resolved_namespace,
                    "requested_version": version,
                    "matched_package_version": incompatible.version,
                    "compiler_requirement": incompatible.compiler_requirement,
                    "active_compiler_version": crate::VERSION,
                    "phase": "registry-candidate-selection",
                }));
            }
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

        // 5. Public resolution materializes the content-addressed Registry
        //    snapshot. The explicit Git override retains tag/registry.json
        //    cross-checking for offline and private mirrors.
        let (package_dir, revision, source_url, tagged_version) = if public_registry_authoritative {
            let snapshot = source_snapshots.remove(&selected_version.version).ok_or_else(|| {
                CompileError::without_span(format!(
                    "public registry package '{}/{}@{}' has no immutable source snapshot",
                    resolved_namespace, name, selected_version.version
                ))
            })?;
            let source_url = snapshot.url.clone();
            let revision = snapshot.snapshot_hash.clone();
            let package_dir = registry::materialize_public_source_snapshot(
                &snapshot,
                &source_cache,
                &resolved_namespace,
                name,
                &selected_version.version,
                &selected_version.source_hash,
            )?;
            (package_dir, revision, source_url, selected_version.clone())
        } else {
            let clone_dir = legacy_clone.expect("legacy clone exists");
            registry::git_checkout(&clone_dir, &selected_version.tag).map_err(CompileError::without_span)?;
            let revision = registry::git_revision(&clone_dir).unwrap_or_else(|_| "unknown".to_string());
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
            let tagged_version =
                tagged_index.versions.iter().find(|candidate| candidate.version == selected_version.version).cloned().ok_or_else(
                    || {
                        CompileError::without_span(format!(
                            "registry package '{}/{}@{}' tag '{}' does not contain a matching registry.json version entry",
                            resolved_namespace, name, selected_version.version, selected_version.tag
                        ))
                    },
                )?;
            if tagged_version.source_hash != selected_version.source_hash
                || tagged_version.tag != selected_version.tag
                || tagged_version.cellscript_version != selected_version.cellscript_version
                || tagged_version.compiler_requirement != selected_version.compiler_requirement
                || tagged_version.edition != selected_version.edition
                || tagged_version.compatibility_profile_hash != selected_version.compatibility_profile_hash
            {
                return Err(CompileError::without_span(format!(
                    "registry identity mismatch for '{}/{}@{}' between the selected index and checked-out tag",
                    resolved_namespace, name, tagged_version.version
                )));
            }
            if tagged_version
                .resolver_block_reason(policy, matches!(crate::package::version::parse_version_req(version), Ok(VersionReq::Exact(_))))
                .is_some()
            {
                return Err(registry_resolution_blocked_error(&resolved_namespace, name, version, &tagged_version, policy));
            }
            (clone_dir, revision, repository_url.clone(), tagged_version)
        };
        let computed_source_hash = registry::compute_source_hash(&package_dir)?;
        if computed_source_hash != tagged_version.source_hash {
            return Err(CompileError::without_span(format!(
                "source_hash mismatch for '{}/{}@{}': expected '{}', got '{}'",
                resolved_namespace, name, tagged_version.version, tagged_version.source_hash, computed_source_hash
            )));
        }

        // 6. Read Cell.toml and resolve transitive dependencies
        let manifest_path = package_dir.join("Cell.toml");
        if !manifest_path.exists() {
            return Err(CompileError::without_span(format!(
                "registry package '{}/{}' does not contain Cell.toml",
                resolved_namespace, name
            )));
        }

        let content = std::fs::read_to_string(&manifest_path)?;
        let manifest: PackageManifest = toml::from_str(&content)?;
        validate_declarations(&manifest.artifacts)?;
        if manifest.package.name != name {
            return Err(CompileError::without_span(format!(
                "registry package '{}/{}@{}' Cell.toml declares package name '{}'",
                resolved_namespace, name, tagged_version.version, manifest.package.name
            )));
        }
        if manifest.package.version != tagged_version.version {
            return Err(CompileError::without_span(format!(
                "registry package '{}/{}' selected version '{}' does not match Cell.toml version '{}'",
                resolved_namespace, name, tagged_version.version, manifest.package.version
            )));
        }
        if manifest.package.namespace.as_deref() != Some(resolved_namespace.as_str()) {
            return Err(CompileError::without_span(format!(
                "registry package '{}/{}@{}' Cell.toml must declare namespace '{}'",
                resolved_namespace, name, tagged_version.version, resolved_namespace
            )));
        }
        if manifest.package.cellscript_version != tagged_version.compiler_requirement {
            return Err(CompileError::without_span(format!(
                "registry package '{}/{}@{}' compiler requirement '{}' does not match Cell.toml '{}'",
                resolved_namespace,
                name,
                tagged_version.version,
                tagged_version.compiler_requirement,
                manifest.package.cellscript_version
            ))
            .with_code(crate::error::DiagnosticCode::E2600));
        }

        Ok((
            ResolvedPackage {
                node_id: String::new(),
                name: name.to_string(),
                version: manifest.package.version.clone(),
                path: package_dir,
                source: PackageSource::Registry {
                    registry: registry_url,
                    url: source_url,
                    revision,
                    namespace: resolved_namespace.clone(),
                    version: manifest.package.version.clone(),
                },
                dependencies: BTreeMap::new(),
                namespace: Some(resolved_namespace),
                source_hash: Some(computed_source_hash),
                manifest_digest: manifest_digest(content.as_bytes()),
                compiler_requirement: manifest.package.cellscript_version.clone(),
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
}
