use super::{refresh_lockfile_from_manifest, validate_not_self_dependency, CommandExecutor, InstallArgs};
use crate::error::Result;
use crate::package::{Dependency, DetailedDependency, PackageManager};
use colored::Colorize;

impl CommandExecutor {
    pub(super) fn install(args: InstallArgs) -> Result<()> {
        let pm = PackageManager::new(".");

        let _manifest = pm.read_manifest()?;

        if let Some(git_url) = &args.git {
            let crate_name = args.crate_name.clone().unwrap_or_else(|| {
                git_url.trim_end_matches('/').trim_end_matches(".git").split('/').next_back().unwrap_or("unknown").to_string()
            });

            let dep = DetailedDependency {
                version: args.version.clone().unwrap_or_else(|| "*".to_string()),
                namespace: None,
                package: None,
                resolver: None,
                git: Some(git_url.clone()),
                branch: None,
                tag: None,
                rev: None,
                path: None,
                optional: false,
                features: Vec::new(),
                default_features: true,
                use_environment: None,
                environment_independent: false,
                allow_unverified: false,
                allow_quarantined: false,
            };

            pm.resolve_from_git(&crate_name, git_url, &dep)?;

            let mut manifest = pm.read_manifest()?;
            manifest.dependencies.insert(crate_name.clone(), Dependency::Detailed(dep));
            pm.write_manifest(&manifest)?;

            refresh_lockfile_from_manifest(std::path::Path::new("."))?;

            println!("{}", format!("Installed {} from git {}", crate_name, git_url).green());
            Ok(())
        } else if let Some(path) = &args.path {
            let crate_name =
                args.crate_name.clone().unwrap_or_else(|| path.file_name().unwrap_or_default().to_string_lossy().to_string());

            let dep = DetailedDependency {
                version: args.version.clone().unwrap_or_else(|| "*".to_string()),
                namespace: None,
                package: None,
                resolver: None,
                git: None,
                branch: None,
                tag: None,
                rev: None,
                path: Some(path.to_string_lossy().to_string()),
                optional: false,
                features: Vec::new(),
                default_features: true,
                use_environment: None,
                environment_independent: false,
                allow_unverified: false,
                allow_quarantined: false,
            };

            let manifest_for_check = pm.read_manifest()?;
            validate_not_self_dependency(&crate_name, &Dependency::Detailed(dep.clone()), &manifest_for_check)?;

            pm.resolve_from_path(&crate_name, &path.to_string_lossy())?;

            let mut manifest = pm.read_manifest()?;
            manifest.dependencies.insert(crate_name.clone(), Dependency::Detailed(dep));
            pm.write_manifest(&manifest)?;

            refresh_lockfile_from_manifest(std::path::Path::new("."))?;

            println!("{}", format!("Installed {} from path {}", crate_name, path.display()).green());
            Ok(())
        } else if let Some(crate_name) = &args.crate_name {
            // Support both:
            //   cellc install cellscript/amm@1.2.0   (Go-style combined format)
            //   cellc install amm --namespace cellscript --version 1.2.0
            let (resolved_name, resolved_namespace, resolved_version) = if args.namespace.is_none() && args.version.is_none() {
                // Try parsing namespace/name@version format
                if let Some((ns, rest)) = crate_name.split_once('/') {
                    if let Some((name, ver)) = rest.split_once('@') {
                        (name.to_string(), Some(ns.to_string()), Some(ver.to_string()))
                    } else {
                        (rest.to_string(), Some(ns.to_string()), None)
                    }
                } else if let Some((name, ver)) = crate_name.split_once('@') {
                    (name.to_string(), None, Some(ver.to_string()))
                } else {
                    (crate_name.clone(), None, None)
                }
            } else {
                (crate_name.clone(), args.namespace.clone(), args.version.clone())
            };

            let version = resolved_version.unwrap_or_else(|| "*".to_string());

            let _resolved = pm.resolve_from_registry_with_namespace_and_policy(
                &resolved_name,
                &version,
                resolved_namespace.as_deref(),
                crate::package::registry::RegistryResolutionPolicy {
                    allow_unverified: args.allow_unverified,
                    allow_quarantined: args.allow_quarantined,
                },
            )?;

            let dep = if resolved_namespace.is_some() || args.allow_unverified || args.allow_quarantined {
                Dependency::Detailed(DetailedDependency {
                    version,
                    namespace: resolved_namespace.clone(),
                    package: None,
                    resolver: None,
                    git: None,
                    branch: None,
                    tag: None,
                    rev: None,
                    path: None,
                    optional: false,
                    features: Vec::new(),
                    default_features: true,
                    use_environment: None,
                    environment_independent: false,
                    allow_unverified: args.allow_unverified,
                    allow_quarantined: args.allow_quarantined,
                })
            } else {
                Dependency::Simple(version)
            };

            let mut manifest = pm.read_manifest()?;
            validate_not_self_dependency(&resolved_name, &dep, &manifest)?;
            manifest.dependencies.insert(resolved_name.clone(), dep);
            pm.write_manifest(&manifest)?;

            refresh_lockfile_from_manifest(std::path::Path::new("."))?;

            let ns_display = resolved_namespace.as_deref().unwrap_or("<default>");
            println!("{}", format!("Installed {}/{} from registry", ns_display, resolved_name).green());
            Ok(())
        } else {
            refresh_lockfile_from_manifest(std::path::Path::new("."))?;

            println!("{}", "Dependencies resolved and lockfile updated".green());
            Ok(())
        }
    }
}
