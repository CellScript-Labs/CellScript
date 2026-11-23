use super::{
    read_bounded_resolver_output, registry, sanitize_node_component, sha256_file, version, DetailedDependency,
    ExternalResolverEnvironment, ExternalResolverRequest, ExternalResolverResponse, PackageManager, PackageManifest,
    ResolutionOptions, ResolvedPackage, SelectedEnvironmentContext, EXTERNAL_RESOLVER_MAX_OUTPUT_BYTES,
    EXTERNAL_RESOLVER_REQUEST_SCHEMA, EXTERNAL_RESOLVER_RESPONSE_SCHEMA, EXTERNAL_RESOLVER_TIMEOUT,
};
use crate::error::{CompileError, Result};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

impl PackageManager {
    pub(super) fn resolve_external_dependency(
        &self,
        alias: &str,
        package_name: &str,
        dependency: &DetailedDependency,
        owner_root: &Path,
        options: &ResolutionOptions,
        environment_context: Option<&SelectedEnvironmentContext>,
    ) -> Result<DetailedDependency> {
        if options.offline {
            return Err(CompileError::without_span(format!(
                "offline mode cannot invoke external resolver for dependency '{}'",
                alias
            )));
        }
        // Executable resolvers are a root-authority boundary: a transitive
        // package's manifest must never select a local executable to run.
        let owner_is_root = std::fs::canonicalize(owner_root).ok() == std::fs::canonicalize(&self.root).ok();
        if !owner_is_root {
            return Err(CompileError::without_span(format!(
                "dependency '{}' selects an external resolver from a non-root manifest; only the workspace root may declare executable resolvers",
                alias
            )));
        }
        let resolver_name = dependency
            .resolver
            .as_deref()
            .ok_or_else(|| CompileError::without_span(format!("dependency '{}' has no external resolver name", alias)))?;
        let owner_manifest_path = owner_root.join("Cell.toml");
        let owner_manifest: PackageManifest = toml::from_str(&std::fs::read_to_string(&owner_manifest_path).map_err(|error| {
            CompileError::without_span(format!(
                "failed to read resolver owner manifest '{}': {}",
                owner_manifest_path.display(),
                error
            ))
        })?)?;
        let resolver = owner_manifest.resolvers.get(resolver_name).ok_or_else(|| {
            CompileError::without_span(format!("dependency '{}' selects undeclared resolver '{}'", alias, resolver_name))
        })?;
        if resolver.args.len() > 64 || resolver.args.iter().any(|argument| argument.len() > 4096) {
            return Err(CompileError::without_span(format!("resolver '{}' exceeds the bounded argument contract", resolver_name)));
        }
        let command_path = Path::new(&resolver.command);
        if !command_path.is_absolute() || !command_path.is_file() {
            return Err(CompileError::without_span(format!(
                "resolver '{}' command must be an existing absolute executable path",
                resolver_name
            )));
        }
        let expected_digest = resolver.sha256.strip_prefix("sha256:").unwrap_or(&resolver.sha256).to_ascii_lowercase();
        let actual_digest = sha256_file(command_path)?;
        if actual_digest != expected_digest {
            return Err(CompileError::without_span(format!(
                "resolver '{}' executable digest mismatch: expected sha256:{}, got sha256:{}",
                resolver_name, expected_digest, actual_digest
            )));
        }

        let environment = environment_context.map(|selected| ExternalResolverEnvironment {
            root_name: &selected.root_name,
            local_name: selected.local_name.as_deref(),
            chain_id: &selected.chain_id,
            genesis_hash: &selected.genesis_hash,
        });
        let request = ExternalResolverRequest {
            schema: EXTERNAL_RESOLVER_REQUEST_SCHEMA,
            alias,
            package: package_name,
            version_requirement: &dependency.version,
            environment,
        };
        let request = serde_json::to_vec(&request)?;
        let temp_root = self.root.join(".cell/resolver-tmp");
        std::fs::create_dir_all(&temp_root)?;
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let stem = format!("{}-{}-{nonce}", std::process::id(), sanitize_node_component(alias));
        let stdout_path = temp_root.join(format!("{stem}.stdout"));
        let stderr_path = temp_root.join(format!("{stem}.stderr"));
        let stdout_file = std::fs::OpenOptions::new().write(true).create_new(true).open(&stdout_path)?;
        let stderr_file = std::fs::OpenOptions::new().write(true).create_new(true).open(&stderr_path)?;
        let mut child = std::process::Command::new(command_path)
            .args(&resolver.args)
            .current_dir(owner_root)
            .env_clear()
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::from(stdout_file))
            .stderr(std::process::Stdio::from(stderr_file))
            .spawn()
            .map_err(|error| CompileError::without_span(format!("failed to start resolver '{}': {}", resolver_name, error)))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(&request)?;
            stdin.write_all(b"\n")?;
        }

        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            let output_too_large = [&stdout_path, &stderr_path]
                .iter()
                .any(|path| std::fs::metadata(path).is_ok_and(|metadata| metadata.len() > EXTERNAL_RESOLVER_MAX_OUTPUT_BYTES));
            if output_too_large || started.elapsed() >= EXTERNAL_RESOLVER_TIMEOUT {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&stdout_path);
                let _ = std::fs::remove_file(&stderr_path);
                let reason = if output_too_large { "output exceeded 1 MiB" } else { "timed out after 10 seconds" };
                return Err(CompileError::without_span(format!("resolver '{}' {}", resolver_name, reason)));
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let stdout = read_bounded_resolver_output(&stdout_path)?;
        let stderr = read_bounded_resolver_output(&stderr_path)?;
        let _ = std::fs::remove_file(&stdout_path);
        let _ = std::fs::remove_file(&stderr_path);
        if !status.success() {
            return Err(CompileError::without_span(format!(
                "resolver '{}' exited with {}: {}",
                resolver_name,
                status,
                String::from_utf8_lossy(&stderr).trim()
            )));
        }
        let response: ExternalResolverResponse = serde_json::from_slice(&stdout)
            .map_err(|error| CompileError::without_span(format!("resolver '{}' returned invalid JSON: {}", resolver_name, error)))?;
        if response.schema != EXTERNAL_RESOLVER_RESPONSE_SCHEMA {
            return Err(CompileError::without_span(format!(
                "resolver '{}' returned unsupported schema '{}'",
                resolver_name, response.schema
            )));
        }
        if response.dependency.package != package_name {
            return Err(CompileError::without_span(format!(
                "resolver '{}' returned package '{}', expected '{}'",
                resolver_name, response.dependency.package, package_name
            )));
        }
        semver::Version::parse(&response.dependency.version).map_err(|error| {
            CompileError::without_span(format!(
                "resolver '{}' version '{}' is not exact SemVer: {}",
                resolver_name, response.dependency.version, error
            ))
        })?;
        let requested = version::parse_version_req(&dependency.version)?;
        if !version::satisfies(&response.dependency.version, &requested) {
            return Err(CompileError::without_span(format!(
                "resolver '{}' returned version '{}' outside requested range '{}'",
                resolver_name, response.dependency.version, dependency.version
            )));
        }

        match (&response.dependency.git, &response.dependency.namespace) {
            (Some(git), None) => {
                let revision =
                    response.dependency.rev.as_deref().ok_or_else(|| {
                        CompileError::without_span(format!("resolver '{}' Git response requires rev", resolver_name))
                    })?;
                if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err(CompileError::without_span(format!(
                        "resolver '{}' Git rev must be a full 40-hex commit",
                        resolver_name
                    )));
                }
                Ok(DetailedDependency {
                    version: format!("={}", response.dependency.version),
                    namespace: None,
                    package: dependency.package.clone(),
                    resolver: None,
                    git: Some(git.clone()),
                    branch: None,
                    tag: None,
                    rev: Some(revision.to_string()),
                    path: None,
                    optional: dependency.optional,
                    features: dependency.features.clone(),
                    default_features: dependency.default_features,
                    use_environment: None,
                    environment_independent: false,
                    allow_unverified: false,
                    allow_quarantined: false,
                })
            }
            (None, Some(namespace)) => {
                if response.dependency.rev.is_some() || namespace.trim().is_empty() {
                    return Err(CompileError::without_span(format!("resolver '{}' Registry response is malformed", resolver_name)));
                }
                Ok(DetailedDependency {
                    version: format!("={}", response.dependency.version),
                    namespace: Some(namespace.clone()),
                    package: dependency.package.clone(),
                    resolver: None,
                    git: None,
                    branch: None,
                    tag: None,
                    rev: None,
                    path: None,
                    optional: dependency.optional,
                    features: dependency.features.clone(),
                    default_features: dependency.default_features,
                    use_environment: None,
                    environment_independent: false,
                    allow_unverified: dependency.allow_unverified,
                    allow_quarantined: dependency.allow_quarantined,
                })
            }
            _ => Err(CompileError::without_span(format!("resolver '{}' must return exactly one of git or namespace", resolver_name))),
        }
    }

    pub fn resolve_from_registry(&self, name: &str, version: &str) -> Result<ResolvedPackage> {
        self.resolve_from_registry_with_namespace(name, version, None)
    }

    pub fn resolve_from_registry_with_namespace(&self, name: &str, version: &str, namespace: Option<&str>) -> Result<ResolvedPackage> {
        let (resolved, _) =
            self.resolve_from_registry_with_manifest(name, version, namespace, None, registry::RegistryResolutionPolicy::default())?;
        Ok(resolved)
    }

    pub fn resolve_from_registry_with_namespace_and_policy(
        &self,
        name: &str,
        version: &str,
        namespace: Option<&str>,
        policy: registry::RegistryResolutionPolicy,
    ) -> Result<ResolvedPackage> {
        let (resolved, _) = self.resolve_from_registry_with_manifest(name, version, namespace, None, policy)?;
        Ok(resolved)
    }
}
