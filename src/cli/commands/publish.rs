use super::super::output::print_json;
use super::registry_keys::sign_registry_publish_payload;
use super::registry_requests::{authorise_registry_publish_key, submit_registry_publish_request};
use super::{
    build_publish_registry_entry, build_publish_registry_version, build_registry_source_snapshot, cellscript_artifact_descriptor,
    current_utc_timestamp, publish_declared_artifact, read_registry_publish_payload, registry_origin_from_api_base,
    registry_publish_canonical_payload, registry_publish_endpoint, registry_publish_nonce, resolve_registry_api_base,
    resolve_registry_publish_idempotency_key, utc_timestamp_after_seconds, validate_publish_payload_matches_local_package,
    CommandExecutor, PublishArgs,
};
use crate::error::Result;
use crate::package::PackageManager;
use crate::{compile_path, CompileOptions};
use colored::Colorize;

impl CommandExecutor {
    pub(super) fn publish(args: PublishArgs) -> Result<()> {
        if let Some(manifest_path) = args.artifact_manifest.clone() {
            return publish_declared_artifact(args, &manifest_path);
        }
        let pm = PackageManager::new(".");
        let manifest = pm.read_manifest()?;
        let artifact = cellscript_artifact_descriptor(args.artifact_kind.as_deref())?;

        if args.dry_run {
            let mut issues = Vec::<String>::new();
            if manifest.package.name.is_empty() {
                issues.push("package name is empty".to_string());
            }
            if manifest.package.version.is_empty() {
                issues.push("package version is empty".to_string());
            }
            if manifest.package.description.is_empty() {
                issues.push("package description is missing".to_string());
            }
            if manifest.package.license.is_empty() {
                issues.push("package license is missing".to_string());
            }
            if manifest.package.repository.is_empty() {
                issues.push("package repository is missing".to_string());
            }
            if manifest.package.namespace.is_none() {
                issues.push("package namespace is missing (required for publishing)".to_string());
            }

            let entry_path = std::path::Path::new(".").join(&manifest.package.entry);
            if !entry_path.exists() {
                issues.push(format!("entry file '{}' does not exist", manifest.package.entry));
            }

            let compile_result = compile_path(".", CompileOptions::default());
            match compile_result {
                Ok(result) => {
                    println!("{}", "Publish dry-run passed".green());
                    println!("  Package: {} v{}", manifest.package.name, manifest.package.version);
                    println!("  Artifact: {} ({} bytes)", result.artifact_format.display_name(), result.artifact_bytes.len());
                }
                Err(e) => {
                    issues.push(format!("compilation failed: {}", e));
                }
            }

            if !issues.is_empty() {
                println!("{}", "Issues found:".yellow());
                for issue in &issues {
                    println!("  - {}", issue);
                }
                return Err(crate::error::CompileError::without_span(format!("publish dry-run found {} issue(s)", issues.len())));
            }

            Ok(())
        } else {
            let namespace = manifest.package.namespace.clone().ok_or_else(|| {
                crate::error::CompileError::without_span(
                    "package namespace is required for publishing; add namespace = \"<your-namespace>\" to [package] in Cell.toml",
                )
            })?;

            if manifest.package.name.is_empty() {
                return Err(crate::error::CompileError::without_span("package name is empty"));
            }
            if manifest.package.version.is_empty() {
                return Err(crate::error::CompileError::without_span("package version is empty"));
            }

            // Compute source_hash
            let source_hash = crate::package::registry::compute_source_hash(std::path::Path::new("."))?;

            // Compile to get build artifact hashes
            let result = compile_path(".", CompileOptions::default())?;

            // Build registry version entry
            let version_entry = build_publish_registry_version(&manifest, &result, &source_hash)?;

            if args.offline {
                // Offline fixture publish: update registry.json without touching the public write API.
                crate::package::registry::RegistryIndex::append_version(
                    std::path::Path::new("."),
                    &manifest.package.name,
                    &namespace,
                    version_entry,
                )?;

                println!("{}", "Published offline registry fixture".green());
                println!("  Package: {}/{} v{}", namespace, manifest.package.name, manifest.package.version);
                println!("  Source hash: {}", source_hash);
                println!();
                println!("  Audit/offline mirror next steps:");
                println!("    git add registry.json");
                println!("    git commit -m \"publish v{}\"", manifest.package.version);
                println!("    git tag v{}", manifest.package.version);
                println!("    git push --tags");
                return Ok(());
            }

            let api_base = resolve_registry_api_base(args.api_url)?;
            let registry_origin = registry_origin_from_api_base(&api_base)?;
            let endpoint = registry_publish_endpoint(&api_base, &namespace, &manifest.package.name);
            let registry_entry = build_publish_registry_entry(
                &manifest,
                &namespace,
                version_entry,
                &artifact,
                &result.metadata.public_interface,
                &result.metadata.interface_hash,
            )?;
            let payload = if let Some(payload_path) = args.payload.as_deref() {
                read_registry_publish_payload(payload_path)?
            } else {
                let capability_key_id = match args.capability_key_id.or_else(|| std::env::var("CELLSCRIPT_CAPABILITY_KEY_ID").ok()) {
                    Some(key_id) => key_id,
                    None if args.authorise => {
                        authorise_registry_publish_key(&api_base, &namespace, &manifest.package.name, &artifact.kind, args.no_open)?
                    }
                    None => {
                        return Err(crate::error::CompileError::without_span(format!(
                            "publishing {}/{} requires a wallet-authorised publishing key; run `cellc publish --authorise` for the continuous browser flow, or pass an existing --capability-key-id",
                            namespace, manifest.package.name
                        ))
                        .with_category(crate::error::CompileErrorCategory::Authentication));
                    }
                };
                let issued_at = current_utc_timestamp();
                let expires_at = utc_timestamp_after_seconds(10 * 60);
                let nonce = registry_publish_nonce(
                    &registry_origin,
                    &namespace,
                    &manifest.package.name,
                    &manifest.package.version,
                    &source_hash,
                    &capability_key_id,
                    &issued_at,
                );
                crate::package::registry::RegistryPublishPayload {
                    protocol: crate::package::registry::REGISTRY_PUBLISH_PROTOCOL.to_string(),
                    action: crate::package::registry::PUBLISH_ACTION.to_string(),
                    registry_origin: registry_origin.clone(),
                    namespace: namespace.clone(),
                    name: manifest.package.name.clone(),
                    version: manifest.package.version.clone(),
                    source_hash: source_hash.clone(),
                    manifest_hash: crate::package::registry::compute_package_manifest_hash(&manifest)?,
                    lockfile_hash: crate::package::registry::compute_lockfile_hash(std::path::Path::new(".")),
                    capability_key_id,
                    nonce,
                    issued_at,
                    expires_at,
                    cli_version: crate::VERSION.to_string(),
                    artifact: artifact.clone(),
                    registry_entry,
                }
            };
            validate_publish_payload_matches_local_package(
                &payload,
                &registry_origin,
                &namespace,
                &manifest,
                &source_hash,
                &result.metadata.public_interface,
                &result.metadata.interface_hash,
            )?;
            let canonical_payload = registry_publish_canonical_payload(&payload)?;

            if args.print_payload {
                let preview = serde_json::json!({
                    "endpoint": endpoint,
                    "payload": payload,
                    "canonical_payload": canonical_payload,
                });
                if args.json {
                    print_json(&preview)?;
                } else {
                    println!("{}", "Registry publish payload".green());
                    println!("  Endpoint: {}", endpoint);
                    println!("  Package: {}/{} v{}", namespace, manifest.package.name, manifest.package.version);
                    println!("  Source hash: {}", source_hash);
                    println!();
                    println!("Canonical payload to sign:");
                    println!("{}", preview["canonical_payload"].as_str().unwrap_or_default());
                }
                return Ok(());
            }

            let capability_signature =
                if let Some(signature) = args.capability_signature.or_else(|| std::env::var("CELLSCRIPT_CAPABILITY_SIGNATURE").ok()) {
                    signature
                } else {
                    sign_registry_publish_payload(&payload.capability_key_id, &canonical_payload)?
                };
            let source_snapshot =
                build_registry_source_snapshot(args.source_snapshot.as_deref(), std::path::Path::new("."), &manifest, &source_hash)?;
            let request = crate::package::registry::RegistryPublishRequest {
                payload,
                capability_signature: crate::package::registry::RegistryCapabilitySignature {
                    algorithm: "p256-sha256".to_string(),
                    signature: capability_signature,
                },
                source_snapshot,
            };
            let idempotency_key = resolve_registry_publish_idempotency_key(args.idempotency_key.as_deref(), &request)?;
            submit_registry_publish_request(&endpoint, &request, &idempotency_key, args.json)
        }
    }
}
