use super::{
    canonical_path, manifest_digest, registry, relative_path, simple_hash, DependencyGraph, DetailedDependency, PackageManager,
    PackageManifest, PackageSource, ResolvedPackage,
};
use crate::artifact::validate_declarations;
use crate::error::{CompileError, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

impl PackageManager {
    pub(super) fn resolve_from_path_at(&self, name: &str, path: &str, base_root: &Path) -> Result<(ResolvedPackage, PackageManifest)> {
        let requested_path = base_root.join(path);
        let package_path = canonical_path(&requested_path).map_err(|_| {
            CompileError::without_span(format!("Dependency '{}' not found at path '{}'", name, requested_path.display()))
        })?;
        let manifest_path = package_path.join("Cell.toml");

        if !manifest_path.exists() {
            return Err(CompileError::without_span(format!("Dependency '{}' not found at path '{}'", name, path)));
        }

        let content = std::fs::read_to_string(&manifest_path)?;
        let manifest: PackageManifest = toml::from_str(&content)?;
        validate_declarations(&manifest.artifacts)?;
        let source_hash = registry::compute_source_hash(&package_path)?;

        let canonical_root = canonical_path(&self.root)?;
        let source_path = relative_path(&canonical_root, &package_path).unwrap_or_else(|| package_path.clone());

        Ok((
            ResolvedPackage {
                node_id: String::new(),
                name: name.to_string(),
                version: manifest.package.version.clone(),
                path: package_path,
                source: PackageSource::Local(source_path),
                dependencies: BTreeMap::new(),
                namespace: manifest.package.namespace.clone(),
                source_hash: Some(source_hash),
                manifest_digest: manifest_digest(content.as_bytes()),
                compiler_requirement: manifest.package.cellscript_version.clone(),
            },
            manifest,
        ))
    }

    pub fn resolve_from_git(&self, name: &str, url: &str, detailed: &DetailedDependency) -> Result<ResolvedPackage> {
        let (resolved, _) = self.resolve_from_git_with_manifest(name, url, detailed)?;
        Ok(resolved)
    }

    pub(super) fn resolve_from_git_with_manifest(
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
            let checkout_ref = detailed.branch.as_ref().map(|branch| format!("origin/{branch}")).unwrap_or_else(|| ref_str.clone());
            Self::git_checkout(&clone_dir, &checkout_ref).map_err(|e| {
                CompileError::without_span(format!("git dependency '{}' failed to checkout '{}': {}", name, checkout_ref, e))
            })?;
        }

        let revision = Self::git_revision(&clone_dir).map_err(|error| {
            CompileError::without_span(format!("git dependency '{}' could not resolve an immutable revision: {}", name, error))
        })?;
        if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(CompileError::without_span(format!(
                "git dependency '{}' resolved non-canonical revision '{}'; expected a full 40-hex commit",
                name, revision
            )));
        }
        let immutable_dir = cache_dir.join(format!("{}-git-{}", name, revision));
        if immutable_dir.exists() {
            let cached_revision = Self::git_revision(&immutable_dir).map_err(CompileError::without_span)?;
            if cached_revision != revision {
                return Err(CompileError::without_span(format!(
                    "immutable git cache '{}' has revision '{}', expected '{}'",
                    immutable_dir.display(),
                    cached_revision,
                    revision
                )));
            }
        } else {
            Self::git_materialize_immutable(&clone_dir, &immutable_dir, &revision).map_err(|error| {
                CompileError::without_span(format!("failed to materialize immutable git dependency '{}': {}", name, error))
            })?;
        }

        let manifest_path = immutable_dir.join("Cell.toml");
        if !manifest_path.exists() {
            return Err(CompileError::without_span(format!(
                "git dependency '{}' from '{}' does not contain Cell.toml at repository root",
                name, url
            )));
        }

        let content = std::fs::read_to_string(&manifest_path)?;
        let manifest: PackageManifest = toml::from_str(&content)?;
        validate_declarations(&manifest.artifacts)?;
        let source_hash = registry::compute_source_hash(&immutable_dir)?;

        Ok((
            ResolvedPackage {
                node_id: String::new(),
                name: name.to_string(),
                version: manifest.package.version.clone(),
                path: immutable_dir,
                source: PackageSource::Git { url: url.to_string(), revision },
                dependencies: BTreeMap::new(),
                namespace: manifest.package.namespace.clone(),
                source_hash: Some(source_hash),
                manifest_digest: manifest_digest(content.as_bytes()),
                compiler_requirement: manifest.package.cellscript_version.clone(),
            },
            manifest,
        ))
    }

    pub(super) fn git_cache_dir(&self) -> PathBuf {
        self.root.join(".cell/git-cache")
    }

    pub(super) fn git_clone(url: &str, target: &Path) -> std::result::Result<(), String> {
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

    pub(super) fn git_update(clone_dir: &Path) -> std::result::Result<(), String> {
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

    pub(super) fn git_checkout(clone_dir: &Path, ref_str: &str) -> std::result::Result<(), String> {
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

    pub(super) fn git_revision(clone_dir: &Path) -> std::result::Result<String, String> {
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

    fn git_materialize_immutable(source: &Path, target: &Path, revision: &str) -> std::result::Result<(), String> {
        let output = std::process::Command::new("git")
            .args(["clone", "--no-checkout", "--no-hardlinks", &source.to_string_lossy(), &target.to_string_lossy()])
            .output()
            .map_err(|error| format!("failed to clone immutable cache: {error}"))?;
        if !output.status.success() {
            return Err(format!("git clone failed: {}", String::from_utf8_lossy(&output.stderr).trim()));
        }
        let output = std::process::Command::new("git")
            .args(["checkout", "--detach", revision])
            .current_dir(target)
            .output()
            .map_err(|error| format!("failed to checkout immutable revision: {error}"))?;
        if !output.status.success() {
            let _ = std::fs::remove_dir_all(target);
            return Err(format!("git checkout failed: {}", String::from_utf8_lossy(&output.stderr).trim()));
        }
        Ok(())
    }

    pub(super) fn git_materialize_locked(url: &str, target: &Path, revision: &str) -> std::result::Result<(), String> {
        let output = std::process::Command::new("git")
            .args(["clone", "--no-checkout", url, &target.to_string_lossy()])
            .output()
            .map_err(|error| format!("failed to clone locked git source: {error}"))?;
        if !output.status.success() {
            return Err(format!("git clone failed: {}", String::from_utf8_lossy(&output.stderr).trim()));
        }
        let output = std::process::Command::new("git")
            .args(["checkout", "--detach", revision])
            .current_dir(target)
            .output()
            .map_err(|error| format!("failed to checkout locked git revision: {error}"))?;
        if !output.status.success() {
            let _ = std::fs::remove_dir_all(target);
            return Err(format!("git checkout failed: {}", String::from_utf8_lossy(&output.stderr).trim()));
        }
        Ok(())
    }

    pub fn get_resolved(&self) -> &BTreeMap<String, ResolvedPackage> {
        &self.resolved
    }

    pub fn root_dependencies(&self) -> &BTreeMap<String, String> {
        &self.root_dependencies
    }

    pub fn build_dependency_graph(&self) -> DependencyGraph {
        let mut graph = DependencyGraph::new();

        for (node_id, package) in &self.resolved {
            graph.add_node(node_id.clone());
            for dependency_id in package.dependencies.values() {
                graph.add_edge(node_id.clone(), dependency_id.clone());
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
