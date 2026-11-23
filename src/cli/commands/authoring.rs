use super::super::output::CommandOutcome;
use super::{
    collect_workspace_incremental_caches, dependency_from_add_args, dependency_map_mut, dependency_target_label,
    display_doc_output_format, ensure_new_package_destination, ensure_workspace_directory_for_removal, init_git_repo,
    refresh_lockfile_from_manifest, validate_dependency_target_flags, validate_not_self_dependency, AddArgs, CleanArgs,
    CommandExecutor, DocArgs, FmtArgs, InitArgs, NewArgs, RemoveArgs,
};
use crate::docgen::{DocGenerator, OutputFormat};
use crate::error::Result;
use crate::fmt::format_source;
use crate::package::PackageManager;
use crate::{compile_path, load_modules_for_input, load_own_package_modules_for_input, CompileOptions};
use camino::Utf8Path;
use colored::Colorize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

impl CommandExecutor {
    pub(super) fn doc(args: DocArgs) -> Result<()> {
        let output = Self::generate_docs(&args)?;
        let output_size_bytes = std::fs::metadata(&output).map(|metadata| metadata.len()).unwrap_or(0);

        let outcome = CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "format": display_doc_output_format(&args.output_format),
                "output": output.display().to_string(),
                "output_size_bytes": output_size_bytes,
                "opened": args.open,
            }),
            human_lines: vec!["Documentation generated".green().to_string(), format!("  Output: {}", output.display())],
        };

        if args.open {
            let _ = std::process::Command::new("open").arg(&output).status();
        }

        outcome.emit(args.json)
    }

    pub(super) fn generate_docs(args: &DocArgs) -> Result<PathBuf> {
        let modules = load_modules_for_input(".")?;
        let compile_result = compile_path(
            ".",
            CompileOptions {
                source_contracts: false,
                edition: crate::CURRENT_EDITION,
                diagnostics: crate::CompilerDiagnostics::default(),
                output: None,
                debug: false,
                target: None,
                target_profile: None,
                primitive_compat: None,
            },
        )?;
        let mut generator = DocGenerator::new(args.output_format);
        for module in &modules {
            generator.add_module(&module.ast);
        }
        generator.set_compile_metadata(&compile_result.metadata);
        let docs = generator.generate()?;
        let output = match args.output_format {
            OutputFormat::Html => PathBuf::from("docs/cellscript-api.html"),
            OutputFormat::Markdown => PathBuf::from("docs/cellscript-api.md"),
            OutputFormat::Json => PathBuf::from("docs/cellscript-api.json"),
        };
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&output, docs)?;

        Ok(output)
    }

    pub(super) fn fmt(args: FmtArgs) -> Result<()> {
        let modules = if args.files.is_empty() {
            load_own_package_modules_for_input(".")?
        } else {
            let mut modules = Vec::new();
            for path in &args.files {
                let utf8 = Utf8Path::from_path(path).ok_or_else(|| {
                    crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", path.display()))
                })?;
                modules.extend(load_own_package_modules_for_input(utf8)?);
            }
            modules
        };

        let mut changed = Vec::new();
        let mut prepared = BTreeMap::new();
        for module in modules {
            let formatted = format_source(&module.source, module.edition, Default::default())?;
            if formatted != module.source {
                prepared.insert(module.path.clone(), (module.source, formatted));
            }
        }
        changed.extend(prepared.keys().cloned());
        if !args.check {
            crate::cli::format::write_sources(&prepared)?;
        }
        let changed_files = changed.iter().map(|path| path.as_str()).collect::<Vec<_>>();

        if args.check {
            if changed.is_empty() {
                CommandOutcome {
                    machine: serde_json::json!({
                        "status": "ok",
                        "mode": "check",
                        "changed": 0,
                        "changed_files": changed_files,
                    }),
                    human_lines: vec!["Formatting is clean".green().to_string()],
                }
                .emit(args.json)
            } else {
                let outcome = CommandOutcome {
                    machine: serde_json::json!({
                        "status": "failed",
                        "mode": "check",
                        "changed": changed.len(),
                        "changed_files": changed_files,
                    }),
                    human_lines: Vec::new(),
                };
                Err(outcome.failure(format!(
                    "format check failed for {} file(s): {}",
                    changed.len(),
                    changed.iter().map(|path| path.as_str()).collect::<Vec<_>>().join(", ")
                )))
            }
        } else {
            CommandOutcome {
                machine: serde_json::json!({
                    "status": "ok",
                    "mode": "write",
                    "changed": changed.len(),
                    "changed_files": changed_files,
                }),
                human_lines: vec!["Formatting complete".green().to_string(), format!("  Updated {} file(s)", changed.len())],
            }
            .emit(args.json)
        }
    }

    pub(super) fn init(args: InitArgs) -> Result<()> {
        let path = args.path.unwrap_or_else(|| PathBuf::from("."));
        let name = args.name.unwrap_or_else(|| path.file_name().unwrap_or_default().to_string_lossy().to_string());

        let pm = PackageManager::new(&path);
        if args.lib {
            pm.init_library(&name)?;
        } else {
            pm.init(&name)?;
        }
        if let Some(namespace) = &args.namespace {
            let mut manifest = pm.read_manifest()?;
            manifest.package.namespace = Some(namespace.clone());
            pm.write_manifest(&manifest)?;
        }

        let entry = if args.lib { "src/lib.cell" } else { "src/main.cell" };
        CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "kind": if args.lib { "library" } else { "binary" },
                "package": name,
                "path": path.display().to_string(),
                "manifest": path.join("Cell.toml").display().to_string(),
                "entry": entry,
                "namespace": args.namespace,
                "created_files": [
                    path.join("Cell.toml").display().to_string(),
                    path.join(entry).display().to_string(),
                    path.join(".gitignore").display().to_string(),
                ],
            }),
            human_lines: vec![
                "Created package successfully".green().to_string(),
                "  To get started:".to_string(),
                format!("    cd {}", path.display()),
                "    cellc build".to_string(),
            ],
        }
        .emit(args.json)
    }

    pub(super) fn create_new(args: NewArgs) -> Result<()> {
        let path = args.path.unwrap_or_else(|| PathBuf::from(&args.name));
        ensure_new_package_destination(&path)?;

        let pm = PackageManager::new(&path);
        if args.lib {
            pm.init_library(&args.name)?;
        } else {
            pm.init(&args.name)?;
        }

        let git_initialized = match args.vcs.as_str() {
            "git" => init_git_repo(&path)?,
            "none" => false,
            other => {
                return Err(crate::error::CompileError::without_span(format!("unsupported VCS '{}'; expected 'git' or 'none'", other)))
            }
        };

        let entry = if args.lib { "src/lib.cell" } else { "src/main.cell" };
        CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "command": "new",
                "kind": if args.lib { "library" } else { "binary" },
                "package": args.name,
                "path": path.display().to_string(),
                "manifest": path.join("Cell.toml").display().to_string(),
                "entry": entry,
                "vcs": args.vcs,
                "git_initialized": git_initialized,
                "created_files": [
                    path.join("Cell.toml").display().to_string(),
                    path.join(entry).display().to_string(),
                    path.join(".gitignore").display().to_string(),
                ],
            }),
            human_lines: vec![
                "Created package successfully".green().to_string(),
                "  To get started:".to_string(),
                format!("    cd {}", path.display()),
                "    cellc build".to_string(),
            ],
        }
        .emit(args.json)
    }

    pub(super) fn add(args: AddArgs) -> Result<()> {
        validate_dependency_target_flags(args.dev, args.build)?;
        if args.git.is_some() && args.path.is_some() {
            return Err(crate::error::CompileError::without_span("cellc add accepts either --git or --path, not both"));
        }
        if args.package.is_some() && args.crates.len() != 1 {
            return Err(crate::error::CompileError::without_span("cellc add --package requires exactly one local dependency alias"));
        }
        if args.use_environment.is_some() && args.environment_independent {
            return Err(crate::error::CompileError::without_span(
                "cellc add accepts either --use-environment or --environment-independent, not both",
            ));
        }

        let pm = PackageManager::new(".");
        let mut manifest = pm.read_manifest()?;
        let dependency = dependency_from_add_args(&args);
        let target = dependency_target_label(args.dev, args.build);
        let mut added = Vec::new();

        for crate_name in &args.crates {
            validate_not_self_dependency(crate_name, &dependency, &manifest)?;
            dependency_map_mut(&mut manifest, args.dev, args.build).insert(crate_name.clone(), dependency.clone());
            added.push(crate_name.clone());
        }

        pm.write_manifest(&manifest)?;
        refresh_lockfile_from_manifest(Path::new("."))?;

        CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "target": target,
                "added": added,
                "dependency": dependency,
            }),
            human_lines: vec!["Dependencies added successfully".green().to_string()],
        }
        .emit(args.json)
    }

    pub(super) fn remove(args: RemoveArgs) -> Result<()> {
        validate_dependency_target_flags(args.dev, args.build)?;
        let pm = PackageManager::new(".");
        let mut manifest = pm.read_manifest()?;
        let target = dependency_target_label(args.dev, args.build);
        let mut removed = Vec::new();
        let mut missing = Vec::new();

        for crate_name in &args.crates {
            if dependency_map_mut(&mut manifest, args.dev, args.build).remove(crate_name).is_some() {
                removed.push(crate_name.clone());
            } else {
                missing.push(crate_name.clone());
            }
        }

        pm.write_manifest(&manifest)?;
        if !removed.is_empty() {
            refresh_lockfile_from_manifest(Path::new("."))?;
        }

        CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "target": target,
                "removed": removed,
                "missing": missing,
            }),
            human_lines: vec!["Dependencies removed successfully".green().to_string()],
        }
        .emit(args.json)
    }

    pub(super) fn clean(args: CleanArgs) -> Result<()> {
        let workspace_root = std::fs::canonicalize(".")?;
        let mut paths = vec![PathBuf::from("target"), PathBuf::from(".cell/cache")];
        if args.cache {
            collect_workspace_incremental_caches(Path::new("."), &mut paths)?;
        }
        paths.sort();
        paths.dedup();
        let mut removed_paths = Vec::new();

        for path in paths {
            if path.exists() {
                let removable = ensure_workspace_directory_for_removal(&workspace_root, &path)?;
                std::fs::remove_dir_all(removable)?;
                removed_paths.push(path.to_string_lossy().into_owned());
            }
        }

        CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "removed": removed_paths.len(),
                "removed_paths": removed_paths,
            }),
            human_lines: vec!["Clean complete".green().to_string()],
        }
        .emit(args.json)
    }

    pub(super) fn repl() -> Result<()> {
        crate::repl::run_repl().map_err(|e| crate::error::CompileError::without_span(e.to_string()))
    }
}
