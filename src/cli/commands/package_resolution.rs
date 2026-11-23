use super::super::output::CommandOutcome;
use super::{
    parse_schema_baselines, read_generated_lockfile, read_upgrade_schema_acknowledgements, refresh_lockfile_from_manifest,
    CommandExecutor, InfoArgs, PackageLockArgs, UpdateArgs, UpdatePlanArgs,
};
use crate::error::Result;
use crate::package::PackageManager;
use colored::Colorize;
use std::path::PathBuf;

impl CommandExecutor {
    pub(super) fn update_plan(args: UpdatePlanArgs) -> Result<()> {
        if args.schema_version != crate::package::upgrade::UPGRADE_PLAN_SCHEMA_VERSION {
            return Err(crate::error::CompileError::without_span(format!(
                "unsupported upgrade plan schema version {}; supported version: {}",
                args.schema_version,
                crate::package::upgrade::UPGRADE_PLAN_SCHEMA_VERSION
            ))
            .with_code(crate::error::DiagnosticCode::E2800));
        }
        let scope = match args.scope.as_str() {
            "runtime" => crate::package::DependencyScope::Runtime,
            "test" => crate::package::DependencyScope::Test,
            other => {
                return Err(crate::error::CompileError::without_span(format!(
                    "unsupported update scope '{other}'; expected runtime or test"
                ))
                .with_code(crate::error::DiagnosticCode::E2800));
            }
        };
        let all_features = args.all_features || (args.features.is_empty() && !args.no_default_features);
        let options = crate::package::upgrade::UpgradeOptions {
            package: args.package,
            precise: args.precise,
            scope,
            features: args.features.into_iter().collect(),
            all_features,
            no_default_features: args.no_default_features,
            environment: args.environment,
            offline: args.offline,
            acknowledgements: args.acknowledgements.into_iter().collect(),
            schema_baselines: parse_schema_baselines(&args.schema_baselines)?,
            schema_acknowledgements: read_upgrade_schema_acknowledgements(&args.schema_acknowledgements)?,
        };
        let input = args.input.unwrap_or_else(|| PathBuf::from("."));
        let plan = crate::package::upgrade::create_upgrade_plan(&input, &options)?;
        let json = serde_json::to_string_pretty(&plan)
            .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize upgrade plan: {error}")))?;
        if let Some(output) = args.output.as_ref() {
            if matches!(output.file_name().and_then(|name| name.to_str()), Some("Cell.lock" | "Deployed.toml"))
                || std::fs::symlink_metadata(output).is_ok_and(|metadata| metadata.file_type().is_symlink())
            {
                return Err(crate::error::CompileError::without_span(format!(
                    "upgrade plan output '{}' must not target a lock, deployment manifest, or symlink",
                    output.display()
                ))
                .with_code(crate::error::DiagnosticCode::E2800));
            }
            if let Some(parent) = output.parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(output, format!("{json}\n"))?;
        }
        if args.json || args.output.is_none() {
            println!("{json}");
        } else {
            println!("{}", "Transactional upgrade plan generated".green());
            println!("  Hash: {}", plan.plan_hash);
            println!("  Apply status: {}", plan.apply_status);
            println!("  Lockfiles: {}", plan.locks.len());
            println!("  Reverse-dependent builds: {}", plan.reverse_dependents.len());
            if !plan.required_acknowledgements.is_empty() {
                println!("  Required acknowledgements: {}", plan.required_acknowledgements.join(", "));
            }
            if let Some(output) = args.output.as_ref() {
                println!("  Output: {}", output.display());
            }
        }
        Ok(())
    }

    pub(super) fn update(args: UpdateArgs) -> Result<()> {
        let Some(plan_path) = args.plan else {
            return Self::update_plan(args.planning);
        };
        let plan = crate::package::upgrade::read_upgrade_plan(&plan_path)?;
        let acknowledgements = args.planning.acknowledgements.into_iter().collect();
        let applied = crate::package::upgrade::apply_upgrade_plan(&plan, &acknowledgements)?;
        CommandOutcome {
            machine: serde_json::json!({
                "status": "applied",
                "schema": crate::package::upgrade::UPGRADE_PLAN_SCHEMA,
                "plan_hash": plan.plan_hash,
                "plan": plan_path,
                "lockfiles": applied,
                "deployed_manifest_mutated": false,
                "deployment_performed": false,
            }),
            human_lines: vec![
                "Transactional upgrade applied".green().to_string(),
                format!("  Plan: {}", plan_path.display()),
                format!("  Lockfiles: {}", applied.len()),
            ],
        }
        .emit(args.planning.json)
    }

    pub(super) fn lock(args: PackageLockArgs) -> Result<()> {
        refresh_lockfile_from_manifest(std::path::Path::new("."))?;
        let lockfile = read_generated_lockfile(std::path::Path::new("."))?;
        CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "lockfile": "Cell.lock",
                "schema": lockfile.schema,
                "resolver_model": lockfile.resolver_model,
                "dependency_nodes": lockfile.dependencies.len(),
                "environments": lockfile.environments.keys().collect::<Vec<_>>(),
            }),
            human_lines: vec![
                "Dependency graph locked".green().to_string(),
                format!("  {} dependency node(s)", lockfile.dependencies.len()),
            ],
        }
        .emit(args.json)
    }

    pub(super) fn info(args: InfoArgs) -> Result<()> {
        let pm = PackageManager::new(".");
        let manifest = pm.read_manifest()?;
        let mut human_lines = vec![
            "Package Info:".bold().to_string(),
            format!("  Name:        {}", manifest.package.name),
            format!("  Version:     {}", manifest.package.version),
            format!("  Description: {}", manifest.package.description),
            format!("  License:     {}", manifest.package.license),
            format!("  Authors:     {}", manifest.package.authors.join(", ")),
            format!("  Entry:       {}", manifest.package.entry),
            "  Dependencies:".to_string(),
        ];
        human_lines.extend(manifest.dependencies.iter().map(|(name, dep)| format!("    - {}: {:?}", name, dep)));
        CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "manifest": "Cell.toml",
                "package": manifest.package,
                "dependencies": manifest.dependencies,
                "dev_dependencies": manifest.dev_dependencies,
                "features": manifest.features,
                "environments": manifest.environments,
                "dependency_overrides": manifest.dependency_overrides,
                "resolvers": manifest.resolvers,
                "build": manifest.build,
                "policy": manifest.policy,
                "deploy": manifest.deploy,
                "metadata": manifest.metadata,
            }),
            human_lines,
        }
        .emit(args.json)
    }
}
