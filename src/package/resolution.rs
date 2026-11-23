use super::{
    active_optional_dependencies, compiler_error_with_incoming_edge, dependency_environment_context, dependency_is_optional,
    dependency_package_name, dependency_requirement_for_diagnostic, dependency_resolution_options, environment_node_identity,
    incoming_package_request_json, package_node_id, package_source_authority, package_source_identity, registry, validate_environment,
    validate_package_compiler_requirement, version, Dependency, DependencyScope, FeatureSelection, IncomingPackageRequest,
    PackageCoordinate, PackageManager, PackageManifest, PackageSource, ResolutionOptions, ResolutionTraversal, ResolvedPackage,
    SelectedEnvironmentContext, SelectedPackageInstance,
};
use crate::artifact::validate_declarations;
use crate::error::{CompileError, Result};
use std::collections::BTreeMap;
use std::path::Path;

impl PackageManager {
    pub(super) fn validate_manifest_package_contract(&self, manifest: &PackageManifest) -> Result<()> {
        validate_package_compiler_requirement(&manifest.package)?;
        self.validate_manifest_package_structure(manifest)
    }

    fn validate_manifest_package_structure(&self, manifest: &PackageManifest) -> Result<()> {
        validate_declarations(&manifest.artifacts)?;
        semver::Version::parse(&manifest.package.version).map_err(|error| {
            CompileError::without_span(format!(
                "package '{}' has invalid semantic version '{}': {error}",
                manifest.package.name, manifest.package.version
            ))
        })?;
        for (name, environment) in &manifest.environments {
            validate_environment(name, environment)?;
        }
        for environment in manifest.dependency_overrides.keys() {
            if !manifest.environments.contains_key(environment) {
                return Err(CompileError::without_span(format!(
                    "dependency override environment '{}' has no matching [environments.{}] declaration",
                    environment, environment
                )));
            }
        }
        for (name, resolver) in &manifest.resolvers {
            if name.trim().is_empty() {
                return Err(CompileError::without_span("resolver names must not be empty"));
            }
            let command = Path::new(&resolver.command);
            if !command.is_absolute() {
                return Err(CompileError::without_span(format!("resolver '{}' command must be an absolute executable path", name)));
            }
            let digest = resolver.sha256.strip_prefix("sha256:").unwrap_or(&resolver.sha256);
            if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(CompileError::without_span(format!("resolver '{}' sha256 must contain exactly 32 bytes of hex", name)));
            }
        }
        let declared_dependencies = manifest
            .dependencies
            .iter()
            .chain(manifest.dev_dependencies.iter())
            .chain(manifest.dependency_overrides.values().flat_map(|dependencies| dependencies.iter()));
        for (alias, dependency) in declared_dependencies {
            if let Dependency::Detailed(detail) = dependency {
                if detail.use_environment.as_deref().is_some_and(|name| name.trim().is_empty()) {
                    return Err(CompileError::without_span(format!("dependency '{}' use_environment must not be empty", alias)));
                }
                if detail.use_environment.is_some() && detail.environment_independent {
                    return Err(CompileError::without_span(format!(
                        "dependency '{}' cannot combine use_environment with environment_independent",
                        alias
                    )));
                }
                if let Some(resolver) = detail.resolver.as_deref() {
                    if detail.path.is_some() || detail.git.is_some() {
                        return Err(CompileError::without_span(format!(
                            "dependency '{}' cannot combine resolver with path or git",
                            alias
                        )));
                    }
                    if !manifest.resolvers.contains_key(resolver) {
                        return Err(CompileError::without_span(format!(
                            "dependency '{}' selects undeclared resolver '{}'",
                            alias, resolver
                        )));
                    }
                }
            }
        }
        if !manifest.build.dependencies.is_empty() {
            return Err(CompileError::without_span(
                "[build.dependencies] is reserved until isolated build-script execution is implemented; use [dependencies] for compile-time imports",
            ));
        }
        Ok(())
    }

    pub(super) fn selected_dependencies(
        &self,
        manifest: &PackageManifest,
        options: &ResolutionOptions,
        root: bool,
    ) -> Result<BTreeMap<String, Dependency>> {
        let mut dependencies: BTreeMap<String, Dependency> =
            manifest.dependencies.iter().map(|(name, dep)| (name.clone(), dep.clone())).collect();
        if options.scope == DependencyScope::Test && root {
            for (name, dep) in &manifest.dev_dependencies {
                if dependencies.insert(name.clone(), dep.clone()).is_some() {
                    return Err(CompileError::without_span(format!(
                        "dependency alias '{}' is declared in both [dependencies] and [dev_dependencies]",
                        name
                    )));
                }
            }
        }

        if let Some(environment) = options.environment.as_deref() {
            if root && !manifest.environments.contains_key(environment) {
                return Err(CompileError::without_span(format!(
                    "unknown package environment '{}'; declare [environments.{}] with chain_id and genesis_hash",
                    environment, environment
                )));
            }
            if let Some(overrides) = manifest.dependency_overrides.get(environment) {
                for (alias, dependency) in overrides {
                    if !dependencies.contains_key(alias) {
                        return Err(CompileError::without_span(format!(
                            "environment '{}' overrides unknown dependency alias '{}'",
                            environment, alias
                        )));
                    }
                    dependencies.insert(alias.clone(), dependency.clone());
                }
            }
        } else if root && !manifest.dependency_overrides.is_empty() {
            return Err(CompileError::without_span(
                "Cell.toml declares environment-specific dependency overrides; select one explicitly with --environment",
            ));
        }

        let active_optional = active_optional_dependencies(manifest, options)?;
        dependencies.retain(|alias, dependency| !dependency_is_optional(dependency) || active_optional.contains(alias));
        Ok(dependencies)
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

    pub(super) fn register_package_instance(
        &mut self,
        package: &ResolvedPackage,
        node_id: &str,
        options: &ResolutionOptions,
        environment: Option<&SelectedEnvironmentContext>,
        parent_package: &str,
        alias: &str,
        dependency: &Dependency,
        locked: bool,
    ) -> Result<String> {
        let coordinate = PackageCoordinate::from_package(package);
        let features = FeatureSelection::from_options(options);
        let environment = environment.map(environment_node_identity);
        let source_authority = package_source_authority(package);
        let source_identity = package_source_identity(package);
        let reusable_requirement =
            self.version_requirement_of(dependency).and_then(|requirement| version::parse_version_req(&requirement).ok());
        let current = IncomingPackageRequest {
            parent_package: parent_package.to_string(),
            alias: alias.to_string(),
            version_requirement: dependency_requirement_for_diagnostic(dependency),
            candidate_version: package.version.clone(),
            candidate_source: source_identity.clone(),
            features: features.clone(),
            environment: environment.clone(),
        };
        let Some(selected) = self.selected_coordinates.get_mut(&coordinate) else {
            self.selected_coordinates.insert(
                coordinate,
                SelectedPackageInstance {
                    node_id: node_id.to_string(),
                    version: package.version.clone(),
                    manifest_digest: package.manifest_digest.clone(),
                    compiler_requirement: package.compiler_requirement.clone(),
                    source_authority,
                    source_identity,
                    source_is_registry: matches!(package.source, PackageSource::Registry { .. }),
                    features,
                    environment,
                    incoming: vec![current],
                },
            );
            return Ok(node_id.to_string());
        };

        let conflict_kind = if selected.environment != environment {
            Some("environment")
        } else if selected.features != features {
            Some("feature")
        } else if selected.version == package.version {
            (selected.source_identity != source_identity
                || selected.manifest_digest != package.manifest_digest
                || selected.compiler_requirement != package.compiler_requirement)
                .then_some("source")
        } else if locked || !selected.source_is_registry || !matches!(package.source, PackageSource::Registry { .. }) {
            Some(if selected.source_authority == source_authority { "version" } else { "source" })
        } else if selected.source_authority != source_authority {
            Some("source")
        } else if reusable_requirement.is_some_and(|requirement| !version::satisfies(&selected.version, &requirement)) {
            Some("version")
        } else {
            None
        };

        if let Some(conflict_kind) = conflict_kind {
            let mut incoming = selected.incoming.clone();
            incoming.push(current);
            let selected_json = serde_json::json!({
                "node_id": selected.node_id,
                "version": selected.version,
                "source": selected.source_identity,
                "features": selected.features.labels(),
                "environment": selected.environment,
            });
            return Err(CompileError::without_span(format!(
                "package instance conflict for '{}': selected {} cannot satisfy incoming dependency '{} -> {}' ({conflict_kind}); align every incoming dependency declaration on one explicit package instance",
                coordinate.display(),
                selected.node_id,
                parent_package,
                alias,
            ))
            .with_code(crate::error::DiagnosticCode::E2601)
            .with_details(serde_json::json!({
                "coordinate": {
                    "namespace": coordinate.namespace,
                    "name": coordinate.name,
                },
                "conflict_kind": conflict_kind,
                "selected": selected_json,
                "incoming_edges": incoming.iter().map(incoming_package_request_json).collect::<Vec<_>>(),
                "locked": locked,
                "phase": if locked { "locked-package-unification" } else { "package-unification" },
            })));
        }

        selected.incoming.push(current);
        Ok(selected.node_id.clone())
    }

    pub(super) fn resolve_dependency_from_root(
        &mut self,
        alias: &str,
        dep: &Dependency,
        parent_package: &str,
        parent_namespace: Option<&str>,
        base_root: &Path,
        parent_options: &ResolutionOptions,
        parent_environment: Option<&SelectedEnvironmentContext>,
        traversal: &mut ResolutionTraversal<'_>,
    ) -> Result<Option<String>> {
        let package_name = dependency_package_name(alias, dep);
        let resolution: Result<(ResolvedPackage, PackageManifest)> = (|| match dep {
            Dependency::Simple(version) => self.resolve_from_registry_with_manifest(
                &package_name,
                version,
                None,
                parent_namespace,
                registry::RegistryResolutionPolicy::default(),
            ),
            Dependency::Detailed(detailed) => {
                if detailed.resolver.is_some() {
                    let normalized = self.resolve_external_dependency(
                        alias,
                        &package_name,
                        detailed,
                        base_root,
                        parent_options,
                        parent_environment,
                    )?;
                    let resolved = if let Some(git) = &normalized.git {
                        self.resolve_from_git_with_manifest(&package_name, git, &normalized)?
                    } else {
                        self.resolve_from_registry_with_manifest(
                            &package_name,
                            &normalized.version,
                            normalized.namespace.as_deref(),
                            parent_namespace,
                            registry::RegistryResolutionPolicy::default(),
                        )?
                    };
                    let exact = version::parse_version_req(&normalized.version)?;
                    if !version::satisfies(&resolved.0.version, &exact) {
                        return Err(CompileError::without_span(format!(
                            "external resolver for '{}' declared version inconsistent with materialized package '{}'",
                            alias, resolved.0.version
                        )));
                    }
                    Ok(resolved)
                } else if let Some(path) = &detailed.path {
                    self.resolve_from_path_at(&package_name, path, base_root)
                } else if let Some(git) = &detailed.git {
                    self.resolve_from_git_with_manifest(&package_name, git, detailed)
                } else {
                    let ns = detailed.namespace.as_deref();
                    self.resolve_from_registry_with_manifest(
                        &package_name,
                        &detailed.version,
                        ns,
                        parent_namespace,
                        registry::RegistryResolutionPolicy {
                            allow_unverified: detailed.allow_unverified,
                            allow_quarantined: detailed.allow_quarantined,
                        },
                    )
                }
            }
        })();
        let (mut resolved, manifest) = match resolution {
            Ok(resolution) => resolution,
            Err(error) if error.code.as_deref() == Some("E2600") => {
                traversal.compiler_errors.push(compiler_error_with_incoming_edge(error, parent_package, alias, &package_name));
                return Ok(None);
            }
            Err(error) => return Err(error),
        };

        if let Err(error) = validate_package_compiler_requirement(&manifest.package) {
            traversal.compiler_errors.push(compiler_error_with_incoming_edge(error, parent_package, alias, &package_name));
        }
        self.validate_manifest_package_structure(&manifest)?;
        if manifest.package.name != package_name {
            return Err(CompileError::without_span(format!(
                "dependency alias '{}' expects package '{}' but '{}' declares package name '{}'",
                alias,
                package_name,
                resolved.path.display(),
                manifest.package.name
            )));
        }
        let child_environment = dependency_environment_context(alias, dep, &manifest, parent_environment)?;
        let child_options = dependency_resolution_options(dep, parent_options, child_environment.as_ref());
        let candidate_node_id = package_node_id(&resolved, &child_options, child_environment.as_ref());
        if let Some(requirement) = self.version_requirement_of(dep) {
            let requirement = version::parse_version_req(&requirement)?;
            if !version::satisfies(&resolved.version, &requirement) {
                return Err(CompileError::without_span(format!(
                    "dependency alias '{}' resolved package '{}' to '{}', which does not satisfy its requirement",
                    alias, package_name, resolved.version
                )));
            }
        }
        let node_id = self.register_package_instance(
            &resolved,
            &candidate_node_id,
            &child_options,
            child_environment.as_ref(),
            parent_package,
            alias,
            dep,
            false,
        )?;
        if let Some(position) = traversal.stack_ids.iter().position(|item| item == &node_id) {
            let mut cycle = traversal.stack_labels[position..].to_vec();
            cycle.push(alias.to_string());
            return Err(CompileError::without_span(format!("Circular dependency detected: {}", cycle.join(" -> "))));
        }
        if let Some(existing) = self.resolved.get(&node_id) {
            if (existing.manifest_digest != resolved.manifest_digest || existing.source_hash != resolved.source_hash)
                && node_id == candidate_node_id
            {
                return Err(CompileError::without_span(format!(
                    "dependency node '{}' resolved with conflicting manifest or source identity",
                    node_id
                )));
            }
            return Ok(Some(node_id));
        }

        traversal.stack_ids.push(node_id.clone());
        traversal.stack_labels.push(alias.to_string());
        let child_dependencies = self.selected_dependencies(&manifest, &child_options, false)?;
        let mut child_edges = BTreeMap::new();
        for (child_alias, child_dep) in child_dependencies {
            let child_id = self.resolve_dependency_from_root(
                &child_alias,
                &child_dep,
                &manifest.package.name,
                manifest.package.namespace.as_deref(),
                &resolved.path,
                &child_options,
                child_environment.as_ref(),
                traversal,
            )?;
            if let Some(child_id) = child_id {
                child_edges.insert(child_alias, child_id);
            }
        }
        traversal.stack_ids.pop();
        traversal.stack_labels.pop();

        resolved.node_id = node_id.clone();
        resolved.dependencies = child_edges;
        self.resolved.insert(node_id.clone(), resolved);
        Ok(Some(node_id))
    }
}
