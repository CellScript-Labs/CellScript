use super::super::output::CommandOutcome;
use super::deployment_verification::verify_live_deployments;
use super::{
    compare_optional_build_field, current_utc_timestamp, hash_json_value, push_deployment_status_violation,
    push_missing_deployed_build_identity, push_missing_locked_build_identity, verify_registry_trust_metadata, CommandExecutor,
    PackageVerifyArgs, RegistryAddArgs, RegistryEditArgs, RegistryVerifyArgs, CELLSCRIPT_CKB_RPC_URL_ENV,
};
use crate::error::Result;
use crate::package::{Lockfile, PackageManager};
use colored::Colorize;
use std::collections::{BTreeMap, BTreeSet};

impl CommandExecutor {
    pub(super) fn registry_verify(args: RegistryVerifyArgs) -> Result<()> {
        let root = std::path::Path::new(".");

        // Read Cell.lock
        let lockfile = Lockfile::read_from_root(root)?
            .ok_or_else(|| crate::error::CompileError::without_span("Cell.lock not found; run 'cellc build' first"))?;

        // Read Deployed.toml
        let deployed = crate::package::DeployedManifest::read_from_root(root)?
            .ok_or_else(|| crate::error::CompileError::without_span("Deployed.toml not found; deploy the contract first"))?;

        let mut violations = Vec::new();

        if lockfile.package.name != deployed.package.name {
            violations.push(format!(
                "package name mismatch: Cell.lock has '{}', Deployed.toml has '{}'",
                lockfile.package.name, deployed.package.name
            ));
        }
        if lockfile.package.version != deployed.package.version {
            violations.push(format!(
                "package version mismatch: Cell.lock has '{}', Deployed.toml has '{}'",
                lockfile.package.version, deployed.package.version
            ));
        }
        if lockfile.package.edition != deployed.package.edition {
            violations.push(format!(
                "package edition mismatch: Cell.lock has '{}', Deployed.toml has '{}'",
                lockfile.package.edition, deployed.package.edition
            ));
        }
        if let (Some(lock_hash), Some(deployed_hash)) = (&lockfile.package.source_hash, &deployed.package.source_hash) {
            if lock_hash != deployed_hash {
                violations.push(format!("source_hash mismatch: Cell.lock has '{}', Deployed.toml has '{}'", lock_hash, deployed_hash));
            }
        } else {
            violations.push("source_hash must be present in both Cell.lock and Deployed.toml".to_string());
        }

        match &lockfile.package_build {
            Some(build) => push_missing_locked_build_identity("Cell.lock [package.build]", build, &mut violations),
            None => violations.push("Cell.lock has no [package.build]".to_string()),
        }
        match &deployed.build {
            Some(build) => push_missing_deployed_build_identity("Deployed.toml [build]", build, &mut violations),
            None => violations.push("Deployed.toml has no [build]".to_string()),
        }

        if let (Some(build), Some(deployed_build)) = (&lockfile.package_build, &deployed.build) {
            if build.edition != deployed_build.edition {
                violations.push(format!(
                    "build edition mismatch: Cell.lock has '{}', Deployed.toml has '{}'",
                    build.edition, deployed_build.edition
                ));
            }
            if build.compatibility_profile_hash != deployed_build.compatibility_profile_hash {
                violations.push(format!(
                    "compatibility_profile_hash mismatch: Cell.lock has '{}', Deployed.toml has '{}'",
                    build.compatibility_profile_hash, deployed_build.compatibility_profile_hash
                ));
            }
            compare_optional_build_field(
                "compiler_version",
                &build.compiler_version,
                &deployed_build.compiler_version,
                &mut violations,
            );
            compare_optional_build_field("artifact_hash", &build.artifact_hash, &deployed_build.artifact_hash, &mut violations);
            compare_optional_build_field("metadata_hash", &build.metadata_hash, &deployed_build.metadata_hash, &mut violations);
            compare_optional_build_field("schema_hash", &build.schema_hash, &deployed_build.schema_hash, &mut violations);
            compare_optional_build_field(
                "cell_data_codec_manifest_hash",
                &build.cell_data_codec_manifest_hash,
                &deployed_build.cell_data_codec_manifest_hash,
                &mut violations,
            );
            compare_optional_build_field("abi_hash", &build.abi_hash, &deployed_build.abi_hash, &mut violations);
            compare_optional_build_field(
                "constraints_hash",
                &build.constraints_hash,
                &deployed_build.constraints_hash,
                &mut violations,
            );
        }

        // Check deployment records
        let mut seen_networks = BTreeSet::new();
        for deployment in &deployed.deployments {
            seen_networks.insert(deployment.network.clone());
            push_deployment_status_violation(deployment, &mut violations);

            let Some(deployment_ref) = lockfile.deployment.get(&deployment.network) else {
                violations.push(format!("deployment for network '{}' is missing from Cell.lock", deployment.network));
                continue;
            };

            if deployment_ref.record.is_empty() {
                violations.push(format!("deployment ref for network '{}' has empty record", deployment.network));
            } else {
                let chain_record = format!("{}:{}", deployment.chain_id, deployment.out_point);
                let network_record = format!("{}:{}", deployment.network, deployment.out_point);
                if deployment_ref.record != deployment.out_point
                    && deployment_ref.record != chain_record
                    && deployment_ref.record != network_record
                {
                    violations.push(format!(
                        "deployment record mismatch for network '{}': Cell.lock has '{}', Deployed.toml out_point is '{}'",
                        deployment.network, deployment_ref.record, deployment.out_point
                    ));
                }
            }

            match &deployment_ref.code_hash {
                Some(code_hash) if code_hash == &deployment.code_hash => {}
                Some(code_hash) => violations.push(format!(
                    "code_hash mismatch for network '{}': Cell.lock has '{}', Deployed.toml has '{}'",
                    deployment.network, code_hash, deployment.code_hash
                )),
                None => violations.push(format!("deployment ref for network '{}' has no code_hash", deployment.network)),
            }
            match &deployment_ref.out_point {
                Some(out_point) if out_point == &deployment.out_point => {}
                Some(out_point) => violations.push(format!(
                    "out_point mismatch for network '{}': Cell.lock has '{}', Deployed.toml has '{}'",
                    deployment.network, out_point, deployment.out_point
                )),
                None => violations.push(format!("deployment ref for network '{}' has no out_point", deployment.network)),
            }
            match &deployment_ref.data_hash {
                Some(data_hash) if data_hash == &deployment.data_hash => {}
                Some(data_hash) => violations.push(format!(
                    "data_hash mismatch for network '{}': Cell.lock has '{}', Deployed.toml has '{}'",
                    deployment.network, data_hash, deployment.data_hash
                )),
                None => violations.push(format!("deployment ref for network '{}' has no data_hash", deployment.network)),
            }
            if let Some(record_hash) = &deployment_ref.record_hash {
                let computed = hash_json_value("deployment record", deployment)?;
                if record_hash != &computed {
                    violations.push(format!(
                        "record_hash mismatch for network '{}': Cell.lock has '{}', computed '{}'",
                        deployment.network, record_hash, computed
                    ));
                }
            }

            // TYPE_ID upgrade lineage (Phase 2, off-chain consistency): when a
            // deployment declares `upgrade_lineage`, it must not point at itself
            // and must not be empty. We do not require it to match a record kept
            // in Deployed.toml, because historical deployments are often pruned;
            // on-chain TYPE_ID upgrade-chain verification remains a live-RPC
            // concern. This off-chain check catches the common copy-paste error
            // where a lineage field accidentally names the current deployment.
            if let Some(lineage) = &deployment.upgrade_lineage {
                if lineage.trim().is_empty() {
                    violations.push(format!(
                        "upgrade_lineage for network '{}' is empty; remove the field or point it at a prior out_point",
                        deployment.network
                    ));
                } else if lineage.trim() == deployment.out_point {
                    violations.push(format!(
                        "upgrade_lineage for network '{}' points at the deployment's own out_point '{}'; lineage must reference a prior deployment",
                        deployment.network, deployment.out_point
                    ));
                }
            }
        }
        for network in lockfile.deployment.keys() {
            if !seen_networks.contains(network) {
                violations.push(format!("Cell.lock has stale deployment ref for network '{}'", network));
            }
        }
        let trust_report =
            verify_registry_trust_metadata(&deployed, args.require_publisher_signature, args.require_audit_report, &mut violations);
        let live_report = if args.live {
            let rpc_url = args.rpc_url.clone().or_else(|| std::env::var(CELLSCRIPT_CKB_RPC_URL_ENV).ok()).ok_or_else(|| {
                crate::error::CompileError::without_span(format!(
                    "registry verify --live requires --rpc-url or {}",
                    CELLSCRIPT_CKB_RPC_URL_ENV
                ))
            })?;
            Some(verify_live_deployments(&deployed, &rpc_url, args.network.as_deref(), &mut violations)?)
        } else {
            None
        };

        let passed = violations.is_empty();
        let outcome = CommandOutcome {
            machine: serde_json::json!({
                "status": if passed { "ok" } else { "failed" },
                "trust": trust_report,
                "live": live_report.unwrap_or_else(|| serde_json::json!({
                    "enabled": false,
                    "evidence": []
                })),
                "violations": violations,
            }),
            human_lines: if passed {
                vec!["Registry verification passed".green().to_string()]
            } else {
                let mut lines = vec!["Registry verification failed".red().to_string()];
                lines.extend(violations.iter().map(|violation| format!("  - {}", violation)));
                lines
            },
        };
        if passed {
            outcome.emit(args.json)
        } else {
            Err(outcome.failure(format!("registry verification failed:\n  - {}", violations.join("\n  - "))))
        }
    }

    pub(super) fn package_verify(args: PackageVerifyArgs) -> Result<()> {
        let root = std::path::Path::new(".");
        let pm = PackageManager::new(root);
        let manifest = pm.read_manifest()?;

        // Read Cell.lock
        let lockfile = Lockfile::read_from_root(root)?
            .ok_or_else(|| crate::error::CompileError::without_span("Cell.lock not found; run 'cellc build' first"))?;

        let mut violations = Vec::new();

        if lockfile.package.name != manifest.package.name {
            violations.push(format!(
                "package name mismatch: Cell.toml has '{}', Cell.lock has '{}'",
                manifest.package.name, lockfile.package.name
            ));
        }
        if lockfile.package.version != manifest.package.version {
            violations.push(format!(
                "package version mismatch: Cell.toml has '{}', Cell.lock has '{}'",
                manifest.package.version, lockfile.package.version
            ));
        }
        if lockfile.package.edition != manifest.package.edition {
            violations.push(format!(
                "package edition mismatch: Cell.toml has '{}', Cell.lock has '{}'",
                manifest.package.edition, lockfile.package.edition
            ));
        }
        if lockfile.package.namespace != manifest.package.namespace {
            violations.push(format!(
                "package namespace mismatch: Cell.toml has '{:?}', Cell.lock has '{:?}'",
                manifest.package.namespace, lockfile.package.namespace
            ));
        }

        match &lockfile.package.source_hash {
            Some(source_hash) => {
                let computed = crate::package::registry::compute_source_hash(root)?;
                if &computed != source_hash {
                    violations.push(format!("source_hash mismatch: Cell.lock has '{}', computed '{}'", source_hash, computed));
                }
            }
            None => violations.push("Cell.lock [package] has no source_hash; run 'cellc build' to populate".to_string()),
        }

        match &lockfile.package_build {
            Some(build) => push_missing_locked_build_identity("Cell.lock [package.build]", build, &mut violations),
            None => violations.push("Cell.lock has no [package.build]; run 'cellc build' to populate build identity".to_string()),
        }

        let computed_manifest_digest = crate::package::compute_manifest_digest(root)?;
        if lockfile.root.manifest_digest != computed_manifest_digest {
            violations.push(format!(
                "manifest digest mismatch: Cell.lock has '{}', computed '{}'",
                lockfile.root.manifest_digest, computed_manifest_digest
            ));
        }
        for issue in lockfile.consistency_issues(&manifest) {
            violations.push(issue);
        }
        let mut verification_modes = Vec::new();
        if manifest.dependency_overrides.is_empty() {
            verification_modes.push(None);
        }
        verification_modes.extend(manifest.environments.keys().cloned().map(Some));
        let mut resolved = BTreeMap::new();
        let mut resolve_graphs = Vec::new();
        for environment in verification_modes {
            let options = crate::package::ResolutionOptions {
                scope: crate::package::DependencyScope::Test,
                all_features: true,
                environment: environment.clone(),
                ..crate::package::ResolutionOptions::default()
            };
            match crate::package::inspection::resolve_graph(root, None, &options) {
                Ok(graph) => resolve_graphs.push(serde_json::json!({
                    "schema": graph.schema,
                    "graph_digest": graph.graph_digest,
                    "environment": environment,
                    "nodes": graph.nodes.len(),
                    "edges": graph.edges.len(),
                    "stale_nodes": graph.stale_nodes.len(),
                })),
                Err(error) => violations.push(format!("resolve-graph inspection failed: {}", error.message)),
            }
            let mut mode_manager = PackageManager::new(root);
            match mode_manager.resolve_locked_dependencies(&options) {
                Ok(()) => resolved.extend(mode_manager.get_resolved().clone()),
                Err(error) => violations.push(format!("locked dependency materialization failed: {}", error)),
            }
        }
        for issue in lockfile.consistency_issues_with_resolved(&manifest, &resolved) {
            if !violations.contains(&issue) {
                violations.push(issue);
            }
        }
        for (name, locked) in &lockfile.dependencies {
            if matches!(locked.source, crate::package::LockedSource::Registry { .. }) && locked.source_hash.is_none() {
                violations.push(format!("registry dependency '{}' has no source_hash in Cell.lock", name));
            }
        }

        let passed = violations.is_empty();
        let outcome = CommandOutcome {
            machine: serde_json::json!({
                "status": if passed { "ok" } else { "failed" },
                "resolve_graphs": resolve_graphs,
                "violations": violations,
            }),
            human_lines: if passed {
                vec!["Package verification passed".green().to_string()]
            } else {
                let mut lines = vec!["Package verification failed".red().to_string()];
                lines.extend(violations.iter().map(|violation| format!("  - {}", violation)));
                lines
            },
        };
        if passed {
            outcome.emit(args.json)
        } else {
            Err(outcome.failure(format!("package verification failed:\n  - {}", violations.join("\n  - "))))
        }
    }

    pub(super) fn registry_add(args: RegistryAddArgs) -> Result<()> {
        let root = std::path::Path::new(".");
        let cache_dir = root.join(".cell/registry-cache");
        let registry_url = crate::package::registry::default_registry_url();
        let discovery = crate::package::registry::DiscoveryIndex::new(&registry_url, &cache_dir);

        let entry_path = discovery.add_entry(&args.namespace, &args.name, &args.source)?;
        let discovery_clone = entry_path.parent().and_then(|path| path.parent()).unwrap_or(cache_dir.as_path());
        let entry_rel = entry_path.strip_prefix(discovery_clone).unwrap_or(entry_path.as_path());

        println!("{}", "Registry entry added".green());
        println!("  Namespace: {}", args.namespace);
        println!("  Name: {}", args.name);
        println!("  Source: {}", args.source);
        println!();
        println!("  Next steps:");
        println!("    cd {} && git add {}", discovery_clone.display(), entry_rel.display());
        println!("    git commit -m \"add {}/{}\"", args.namespace, args.name);
        println!("    Open a PR to the cellscript-registry repository");

        Ok(())
    }

    pub(super) fn registry_edit(args: RegistryEditArgs) -> Result<()> {
        let version =
            args.yank.ok_or_else(|| crate::error::CompileError::without_span("registry edit currently requires --yank <VERSION>"))?;
        let root = std::path::Path::new(".");
        let mut index = crate::package::registry::RegistryIndex::read_from_repo(root)?;
        let Some(entry) = index.versions.iter_mut().find(|entry| entry.version == version) else {
            return Err(crate::error::CompileError::without_span(format!(
                "registry.json has no version '{}' for {}/{}",
                version, index.namespace, index.name
            )));
        };

        entry.yanked = true;
        entry.yanked_at = Some(args.yanked_at.unwrap_or_else(current_utc_timestamp));
        entry.yanked_reason = args.reason;
        entry.replaced_by = args.replaced_by;
        let reason = entry.yanked_reason.clone();
        let replaced_by = entry.replaced_by.clone();
        index.write_to_repo(root)?;

        println!("{}", "Registry version yanked".green());
        println!("  Package: {}/{}", index.namespace, index.name);
        println!("  Version: {}", version);
        if let Some(reason) = reason.as_deref() {
            println!("  Reason: {}", reason);
        }
        if let Some(replacement) = replaced_by.as_deref() {
            println!("  Replaced by: {}", replacement);
        }
        println!();
        println!("  Next steps:");
        println!("    git add registry.json");
        println!("    git commit -m \"yank {}/{}@{}\"", index.namespace, index.name, version);
        println!("    Open a PR with the advisory or replacement evidence");

        Ok(())
    }
}
