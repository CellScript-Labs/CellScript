//! Project metadata diagnostics over the compiler-owned loaded graph.
use super::*;

pub(crate) fn compile_file_metadata_with_diagnostics(
    path: &Utf8Path,
    mut options: CompileOptions,
    source_overrides: &BTreeMap<Utf8PathBuf, String>,
) -> CompileMetadataDiagnosticReport {
    let manifest = match find_package_root(path).and_then(|root| root.map(|root| load_manifest(&root)).transpose()) {
        Ok(manifest) => manifest,
        Err(error) => return CompileMetadataDiagnosticReport { metadata: None, diagnostics: vec![error] },
    };
    if let Some(manifest) = manifest.as_ref() {
        options.edition = manifest.package.edition;
    }
    let project =
        match load_project_for_entry_diagnostics(path, source_overrides, manifest.as_ref().and_then(|m| m.deploy.ckb.as_ref())) {
            Ok(project) => project,
            Err(diagnostics) => return CompileMetadataDiagnosticReport { metadata: None, diagnostics },
        };
    compile_loaded_metadata_with_diagnostics(&project, &options, manifest.as_ref(), None, |project| {
        let package_root = find_package_root(&project.entry().path)?;
        Ok(project
            .modules
            .iter()
            .map(|module| {
                let role = if module.path == project.entry().path {
                    "entry"
                } else if package_root.as_ref().is_some_and(|root| module.path.starts_with(root)) {
                    "package"
                } else {
                    "dependency"
                };
                source_unit_from_bytes(module.path.to_string(), role, module.source.as_bytes())
            })
            .collect())
    })
}

/// The caller owns discovery and source-record acquisition. Compiler phases
/// consume the prepared graph without reading source bytes again.
pub(crate) fn compile_loaded_metadata_with_diagnostics(
    project: &LoadedProject,
    options: &CompileOptions,
    manifest: Option<&PackageManifest>,
    artifact_name: Option<&str>,
    source_units: impl FnOnce(&LoadedProject) -> Result<Vec<SourceUnitMetadata>>,
) -> CompileMetadataDiagnosticReport {
    let build = manifest.as_ref().map(|manifest| &manifest.build);

    if let Err(error) = validate_compile_options(options) {
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics: vec![error] };
    }
    let target_profile = match TargetProfile::from_options(options, build).and_then(|profile| {
        profile.ensure_compile_supported()?;
        Ok(profile)
    }) {
        Ok(profile) => profile,
        Err(error) => return CompileMetadataDiagnosticReport { metadata: None, diagnostics: vec![error] },
    };
    let artifact_format = match ArtifactFormat::from_target(resolve_target(options, build)) {
        Ok(format) => format,
        Err(error) => return CompileMetadataDiagnosticReport { metadata: None, diagnostics: vec![error] },
    };

    let check_all_modules = manifest.is_some();
    let mut diagnostics = project_frontend_diagnostics(project, options, check_all_modules);
    if diagnostics.iter().any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error) {
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }

    let entry = project.entry();

    let ir = match ir::generate_with_resolver_diagnostics(&entry.ast, &project.resolver, &entry.ast.name) {
        Ok(ir) => ir,
        Err(errors) => {
            diagnostics.extend(attach_default_file(errors, &entry.path));
            return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
        }
    };
    let declarations = manifest.map_or(&[][..], |manifest| manifest.artifacts.as_slice());
    let resolved = diagnostic_entry_scope(declarations, &entry.ast, artifact_name)
        .and_then(|scope| entry_scope::select_compile_entry(ir, scope.as_ref()));
    let ir = match resolved.and_then(|mut ir| {
        effects::populate(&mut ir)?;
        Ok(ir)
    }) {
        Ok(ir) => ir,
        Err(error) => {
            diagnostics.push(attach_file_if_missing(error, &entry.path));
            return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
        }
    };
    if let Err(error) = executable_surface::validate_ir_module(&ir) {
        diagnostics.push(attach_file_if_missing(error, &entry.path));
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }
    let mut metadata =
        compile_metadata_from_ir(&ir, artifact_format, target_profile, options.edition, options.primitive_compat.as_deref());
    if let Err(error) = bind_public_interface(&mut metadata, &entry.ast, Some(&project.resolver)) {
        diagnostics.push(error);
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }
    let trusted_external_verifiers = manifest
        .as_ref()
        .and_then(|manifest| manifest.deploy.ckb.as_ref())
        .map_or(&[][..], |ckb| ckb.trusted_external_verifiers.as_slice());
    if let Err(error) = apply_trusted_external_verifiers(&mut metadata, &ir, trusted_external_verifiers) {
        diagnostics.push(error);
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }
    if let Some(manifest) = &manifest
        && let Err(error) = zk_package::validate(&ir, manifest.deploy.ckb.as_ref().unwrap_or(&package::CkbDeployConfig::default()))
    {
        diagnostics.push(error);
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }
    if let Err(error) = bind_typed_semantics(&mut metadata, &ir, &entry.ast, Some(&project.resolver), options.source_contracts) {
        diagnostics.push(error);
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }
    // The loaded graph owns the exact bytes that produced these diagnostics.
    // A later disk read could replace an unsaved overlay or race a file edit.
    let source_units = match source_units(project) {
        Ok(units) => units,
        Err(error) => {
            diagnostics.push(error);
            return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
        }
    };
    bind_source_metadata(&mut metadata, source_units);
    let validation = (|| {
        if let Some(manifest) = manifest.as_ref() {
            apply_manifest_deploy_metadata(&mut metadata, manifest)?;
        }
        validate_compile_metadata(&metadata, artifact_format)
    })();
    match validation {
        Ok(()) => CompileMetadataDiagnosticReport { metadata: Some(metadata), diagnostics },
        Err(error) => {
            diagnostics.push(error);
            CompileMetadataDiagnosticReport { metadata: None, diagnostics }
        }
    }
}

fn diagnostic_entry_scope(
    declarations: &[artifact::ArtifactDeclaration],
    module: &ast::Module,
    artifact_name: Option<&str>,
) -> Result<Option<CompileEntryScope>> {
    let local_action = |name: &str| module.items.iter().any(|item| matches!(item, ast::Item::Action(action) if action.name == name));
    let mut local = declarations.iter().filter(|declaration| {
        declaration.entry.as_deref().is_some_and(|name| match declaration.context {
            artifact::ArtifactContext::TypeGroup {} => local_action(name),
            artifact::ArtifactContext::LockGroup {} => {
                module.items.iter().any(|item| matches!(item, ast::Item::Lock(lock) if lock.name == name))
            }
        }) || declaration.actions.iter().any(|export| local_action(&export.action))
    });
    if let Some(name) = artifact_name {
        if let Some(declaration) = declarations.iter().find(|declaration| declaration.name == name) {
            return Ok(local.any(|candidate| candidate.name == name).then(|| CompileEntryScope::Artifact(declaration.clone())));
        }
        if local.next().is_some() {
            return Err(CompileError::without_span(format!("artifact '{name}' is not declared in Cell.toml"))
                .with_code(crate::error::DiagnosticCode::E2101));
        }
        return Ok(None);
    }
    let scope = artifact::resolve_single_entry_scope(local.clone(), None)?;
    Ok(scope.or_else(|| {
        let first = local.next()?;
        local.next().is_none().then(|| CompileEntryScope::Artifact(first.clone()))
    }))
}
