use super::{
    active_lockfile_override, compiler_error_with_incoming_edge, compute_manifest_digest, dependency_environment_context,
    dependency_resolution_options, lock_dependency_consistency_issues, locked_source_to_package_source, manifest_digest,
    normalized_genesis_hash, package_node_id, registry, root_environment_context, validate_locked_root_compiler_contract, Dependency,
    DependencyScope, LockedDependency, LockedSource, Lockfile, PackageManager, PackageManifest, ResolutionOptions, ResolvedPackage,
    SelectedEnvironmentContext,
};
use crate::error::{CompileError, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;

impl PackageManager {
    pub fn resolve_locked_dependencies(&mut self, options: &ResolutionOptions) -> Result<()> {
        self.resolved.clear();
        self.root_dependencies.clear();
        self.selected_coordinates.clear();
        let result = self.resolve_locked_dependencies_inner(options);
        if result.is_err() {
            self.resolved.clear();
            self.root_dependencies.clear();
            self.selected_coordinates.clear();
        }
        result
    }

    fn resolve_locked_dependencies_inner(&mut self, options: &ResolutionOptions) -> Result<()> {
        let manifest = self.read_manifest()?;
        self.validate_manifest_package_contract(&manifest)?;
        let root_environment = root_environment_context(&manifest, options)?;
        let selected = self.selected_dependencies(&manifest, options, true)?;
        if selected.is_empty() {
            if let Some(lockfile) = active_lockfile_override(&self.root)?.or(Lockfile::read_from_root(&self.root)?) {
                validate_locked_root_compiler_contract(&lockfile, &manifest)?;
                let actual_manifest_digest = compute_manifest_digest(&self.root)?;
                if !lockfile.root.manifest_digest.is_empty() && lockfile.root.manifest_digest != actual_manifest_digest {
                    return Err(CompileError::without_span(format!(
                        "Cell.lock manifest digest '{}' does not match Cell.toml '{}'; run 'cellc lock' or 'cellc update' explicitly",
                        lockfile.root.manifest_digest, actual_manifest_digest
                    )));
                }
            }
            return Ok(());
        }

        let lockfile = active_lockfile_override(&self.root)?.or(Lockfile::read_from_root(&self.root)?).ok_or_else(|| {
            CompileError::without_span(
                "Cell.toml declares dependencies but Cell.lock is missing; run 'cellc lock' or 'cellc update' explicitly",
            )
        })?;
        validate_locked_root_compiler_contract(&lockfile, &manifest)?;
        let manifest_bytes = std::fs::read(self.root.join("Cell.toml"))?;
        let actual_manifest_digest = manifest_digest(&manifest_bytes);
        if lockfile.root.manifest_digest != actual_manifest_digest {
            return Err(CompileError::without_span(format!(
                "Cell.lock manifest digest '{}' does not match Cell.toml '{}'; run 'cellc lock' or 'cellc update' explicitly",
                lockfile.root.manifest_digest, actual_manifest_digest
            )));
        }

        let (runtime_edges, dev_edges) = if let Some(environment_name) = options.environment.as_deref() {
            let locked_environment = lockfile.environments.get(environment_name).ok_or_else(|| {
                CompileError::without_span(format!(
                    "environment '{}' is not pinned in Cell.lock; run 'cellc lock --environment {}'",
                    environment_name, environment_name
                ))
            })?;
            let manifest_environment = manifest
                .environments
                .get(environment_name)
                .ok_or_else(|| CompileError::without_span(format!("unknown package environment '{}'", environment_name)))?;
            if locked_environment.chain_id != manifest_environment.chain_id
                || normalized_genesis_hash(&locked_environment.genesis_hash)
                    != normalized_genesis_hash(&manifest_environment.genesis_hash)
            {
                return Err(CompileError::without_span(format!(
                    "environment '{}' chain identity differs between Cell.toml and Cell.lock; run 'cellc update --environment {}'",
                    environment_name, environment_name
                )));
            }
            (&locked_environment.dependencies, &locked_environment.dev_dependencies)
        } else {
            (&lockfile.root.dependencies, &lockfile.root.dev_dependencies)
        };

        for (alias, dependency) in selected {
            let edges = if options.scope == DependencyScope::Test && manifest.dev_dependencies.contains_key(&alias) {
                dev_edges
            } else {
                runtime_edges
            };
            let node_id = edges.get(&alias).ok_or_else(|| {
                CompileError::without_span(format!(
                    "dependency alias '{}' is not pinned for the selected mode/environment; run 'cellc lock' or 'cellc update' explicitly",
                    alias
                ))
            })?;
            let locked = lockfile
                .dependencies
                .get(node_id)
                .ok_or_else(|| CompileError::without_span(format!("Cell.lock edge '{}' targets missing node '{}'", alias, node_id)))?;
            let issues = lock_dependency_consistency_issues(
                &alias,
                &dependency,
                locked,
                manifest.package.namespace.as_deref(),
                Some((&self.root, &self.root)),
            );
            if !issues.is_empty() {
                return Err(CompileError::without_span(format!(
                    "Cell.lock dependency '{}' is inconsistent with Cell.toml: {}; run 'cellc update' explicitly",
                    alias,
                    issues.join("; ")
                )));
            }
            self.materialize_locked_node(
                node_id,
                &lockfile,
                options,
                root_environment.as_ref(),
                &alias,
                &dependency,
                &mut Vec::new(),
            )?;
            self.root_dependencies.insert(alias, node_id.clone());
        }

        Ok(())
    }

    fn materialize_locked_node(
        &mut self,
        node_id: &str,
        lockfile: &Lockfile,
        parent_options: &ResolutionOptions,
        parent_environment: Option<&SelectedEnvironmentContext>,
        edge_alias: &str,
        edge_dependency: &Dependency,
        stack: &mut Vec<String>,
    ) -> Result<()> {
        if stack.iter().any(|candidate| candidate == node_id) {
            let mut cycle = stack.clone();
            cycle.push(node_id.to_string());
            return Err(CompileError::without_span(format!("Cell.lock dependency cycle: {}", cycle.join(" -> "))));
        }
        let locked = lockfile
            .dependencies
            .get(node_id)
            .ok_or_else(|| CompileError::without_span(format!("Cell.lock is missing dependency node '{}'", node_id)))?;
        let package_path = self.locked_source_path(locked, parent_options.offline)?;
        let manifest_path = package_path.join("Cell.toml");
        let bytes = std::fs::read(&manifest_path).map_err(|error| {
            CompileError::without_span(format!("failed to read locked dependency manifest '{}': {}", manifest_path.display(), error))
        })?;
        let digest = manifest_digest(&bytes);
        if digest != locked.manifest_digest {
            return Err(CompileError::without_span(format!(
                "locked dependency '{}' manifest digest mismatch: expected '{}', got '{}'",
                node_id, locked.manifest_digest, digest
            )));
        }
        let manifest_source = std::str::from_utf8(&bytes).map_err(|error| {
            CompileError::without_span(format!("locked dependency manifest '{}' is not UTF-8: {}", manifest_path.display(), error))
        })?;
        let manifest: PackageManifest = toml::from_str(manifest_source).map_err(|error| {
            CompileError::without_span(format!("failed to parse locked dependency manifest '{}': {}", manifest_path.display(), error))
        })?;
        self.validate_manifest_package_contract(&manifest).map_err(|error| {
            let parent_package = stack
                .last()
                .and_then(|parent| lockfile.dependencies.get(parent))
                .map(|parent| parent.name.as_str())
                .unwrap_or(lockfile.package.name.as_str());
            compiler_error_with_incoming_edge(error, parent_package, edge_alias, &locked.name)
        })?;
        if locked.compiler_requirement != manifest.package.cellscript_version {
            return Err(CompileError::without_span(format!(
                "locked dependency '{}' compiler requirement '{}' does not match Cell.toml '{}'; run 'cellc update' explicitly",
                node_id, locked.compiler_requirement, manifest.package.cellscript_version
            ))
            .with_code(crate::error::DiagnosticCode::E2600)
            .with_details(serde_json::json!({
                "package": manifest.package.name,
                "package_version": manifest.package.version,
                "locked_compiler_requirement": locked.compiler_requirement,
                "compiler_requirement": manifest.package.cellscript_version,
                "active_compiler_version": crate::VERSION,
                "incoming_edge": {
                    "from_package": stack
                        .last()
                        .and_then(|parent| lockfile.dependencies.get(parent))
                        .map(|parent| parent.name.as_str())
                        .unwrap_or(lockfile.package.name.as_str()),
                    "alias": edge_alias,
                    "to_package": manifest.package.name,
                },
                "phase": "locked-materialization",
            })));
        }
        if manifest.package.name != locked.name || manifest.package.version != locked.version {
            return Err(CompileError::without_span(format!(
                "locked dependency '{}' manifest identity is '{}@{}', expected '{}@{}'",
                node_id, manifest.package.name, manifest.package.version, locked.name, locked.version
            )));
        }
        let source_hash = registry::compute_source_hash(&package_path)?;
        if locked.source_hash.as_deref() != Some(source_hash.as_str()) {
            return Err(CompileError::without_span(format!(
                "locked dependency '{}' source hash mismatch: expected '{}', got '{}'",
                node_id,
                locked.source_hash.as_deref().unwrap_or("<missing>"),
                source_hash
            )));
        }

        let node_environment = dependency_environment_context(edge_alias, edge_dependency, &manifest, parent_environment)?;
        let node_options = dependency_resolution_options(edge_dependency, parent_options, node_environment.as_ref());
        let locked_package = ResolvedPackage {
            node_id: String::new(),
            name: locked.name.clone(),
            version: locked.version.clone(),
            path: package_path.clone(),
            source: locked_source_to_package_source(&locked.source),
            dependencies: BTreeMap::new(),
            namespace: locked.namespace.clone(),
            source_hash: Some(source_hash.clone()),
            manifest_digest: digest.clone(),
            compiler_requirement: manifest.package.cellscript_version.clone(),
        };
        let expected_node_id = package_node_id(&locked_package, &node_options, node_environment.as_ref());
        if node_id != expected_node_id {
            return Err(CompileError::without_span(format!(
                "Cell.lock dependency edge '{}' records node '{}' but its chain-identity-safe environment selection requires '{}'; run 'cellc update' explicitly",
                edge_alias, node_id, expected_node_id
            )));
        }
        let parent_package = stack
            .last()
            .and_then(|parent| lockfile.dependencies.get(parent))
            .map(|parent| parent.name.as_str())
            .unwrap_or(lockfile.package.name.as_str());
        let selected_node = self.register_package_instance(
            &locked_package,
            node_id,
            &node_options,
            node_environment.as_ref(),
            parent_package,
            edge_alias,
            edge_dependency,
            true,
        )?;
        if selected_node != node_id {
            return Err(CompileError::without_span(format!(
                "Cell.lock edge '{}' selected non-canonical duplicate package node '{}' instead of '{}'",
                edge_alias, node_id, selected_node
            ))
            .with_code(crate::error::DiagnosticCode::E2601));
        }
        if self.resolved.contains_key(node_id) {
            return Ok(());
        }

        let selected_dependencies = self.selected_dependencies(&manifest, &node_options, false)?;
        let mut selected_edges = BTreeMap::new();
        stack.push(node_id.to_string());
        for (alias, dependency) in selected_dependencies {
            let target = locked.dependencies.get(&alias).ok_or_else(|| {
                CompileError::without_span(format!(
                    "locked dependency node '{}' has no edge for selected dependency alias '{}'",
                    node_id, alias
                ))
            })?;
            let target_lock = lockfile.dependencies.get(target).ok_or_else(|| {
                CompileError::without_span(format!(
                    "locked dependency node '{}' edge '{}' targets missing node '{}'",
                    node_id, alias, target
                ))
            })?;
            let issues = lock_dependency_consistency_issues(
                &alias,
                &dependency,
                target_lock,
                manifest.package.namespace.as_deref(),
                Some((&package_path, &self.root)),
            );
            if !issues.is_empty() {
                return Err(CompileError::without_span(format!(
                    "locked dependency node '{}' edge '{}' is inconsistent with its manifest: {}",
                    node_id,
                    alias,
                    issues.join("; ")
                )));
            }
            self.materialize_locked_node(target, lockfile, &node_options, node_environment.as_ref(), &alias, &dependency, stack)?;
            selected_edges.insert(alias, target.clone());
        }
        stack.pop();

        self.resolved.insert(
            node_id.to_string(),
            ResolvedPackage {
                node_id: node_id.to_string(),
                name: locked.name.clone(),
                version: locked.version.clone(),
                path: package_path,
                source: locked_source_to_package_source(&locked.source),
                dependencies: selected_edges,
                namespace: locked.namespace.clone(),
                source_hash: Some(source_hash),
                manifest_digest: digest,
                compiler_requirement: manifest.package.cellscript_version.clone(),
            },
        );
        Ok(())
    }

    pub(super) fn locked_source_path(&self, locked: &LockedDependency, offline: bool) -> Result<PathBuf> {
        match &locked.source {
            LockedSource::Path { path } => {
                let path = self.root.join(path);
                if !path.is_dir() {
                    return Err(CompileError::without_span(format!("locked path dependency '{}' does not exist", path.display())));
                }
                Ok(path)
            }
            LockedSource::Git { url, revision } => {
                registry::validate_cache_path_component(&locked.name, "locked dependency name")?;
                if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err(CompileError::without_span(format!(
                        "locked git revision '{revision}' for '{}' is not a 40-hex commit id",
                        locked.name
                    )));
                }
                let path = self.git_cache_dir().join(format!("{}-git-{}", locked.name, revision));
                if !path.exists() {
                    if offline {
                        return Err(CompileError::without_span(format!(
                            "offline mode cannot materialize missing git cache '{}' for {}",
                            path.display(),
                            locked.name
                        )));
                    }
                    std::fs::create_dir_all(self.git_cache_dir())?;
                    Self::git_materialize_locked(url, &path, revision).map_err(CompileError::without_span)?;
                }
                let actual = Self::git_revision(&path).map_err(CompileError::without_span)?;
                if actual != *revision {
                    return Err(CompileError::without_span(format!(
                        "locked git cache '{}' has revision '{}', expected '{}'",
                        path.display(),
                        actual,
                        revision
                    )));
                }
                Ok(path)
            }
            LockedSource::Registry { url, revision, namespace, version, .. } => {
                registry::validate_cache_path_component(&locked.name, "locked dependency name")?;
                let suffix = revision.trim_start_matches("sha256:");
                if suffix.len() != 64 || !suffix.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err(CompileError::without_span(format!(
                        "locked Registry snapshot hash '{revision}' for '{}' is not sha256:<64 hex>",
                        locked.name
                    )));
                }
                let path = self.git_cache_dir().join(format!("{}-snapshot-{}", locked.name, suffix));
                if !path.exists() {
                    if offline {
                        return Err(CompileError::without_span(format!(
                            "offline mode cannot materialize missing Registry cache '{}' for {}",
                            path.display(),
                            locked.name
                        )));
                    }
                    registry::materialize_locked_public_source_snapshot(
                        url,
                        revision,
                        &self.git_cache_dir(),
                        namespace,
                        &locked.name,
                        version,
                        locked.source_hash.as_deref().unwrap_or_default(),
                    )?;
                }
                Ok(path)
            }
        }
    }
}
