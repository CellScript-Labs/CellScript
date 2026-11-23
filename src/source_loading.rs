use super::edition::{CellScriptEdition, CURRENT_EDITION};
use super::error::{CompileError, Result};
use super::package::PackageManifest;
use super::resolve::ModuleResolver;
use super::{
    artifact, ast, attach_file_if_missing, collect_source_paths_for_compile_file, diagnostics_to_compile_error, error,
    find_package_root, frontend, generics, resolve_input_path, types, InMemorySource, MAX_SOURCE_BYTES,
};
use camino::{Utf8Path, Utf8PathBuf};
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub fn load_modules_for_input<P: AsRef<Utf8Path>>(input: P) -> Result<Vec<LoadedModule>> {
    let resolved = resolve_input_path(input.as_ref())?;
    load_project_modules_for_entry(&resolved, None)
}

/// Formatting owns the selected package's sources only: unlike compilation
/// it must never rewrite materialized dependency sources under
/// `.cell/git-cache` or locked local dependency roots.
pub fn load_own_package_modules_for_input<P: AsRef<Utf8Path>>(input: P) -> Result<Vec<LoadedModule>> {
    let resolved = resolve_input_path(input.as_ref())?;
    let entry_path = canonical_utf8_path(&resolved)?;
    let mut source_paths = BTreeSet::new();
    if let Some(package_root) = find_package_root(&entry_path)? {
        for source_path in collect_package_cell_files(&package_root)? {
            source_paths.insert(source_path);
        }
    } else if let Some(parent) = entry_path.parent() {
        for source_path in collect_cell_files(parent)? {
            source_paths.insert(source_path);
        }
    }
    source_paths.insert(entry_path.clone());
    source_paths
        .into_iter()
        .map(|path| {
            let edition = source_edition(&path)?;
            let source = read_module_source(&path)?;
            parse_loaded_module(path, source, edition)
        })
        .collect()
}

pub(super) fn load_project_for_entry(
    entry_path: &Utf8Path,
    entry_source_override: Option<&str>,
    deploy: Option<&crate::package::CkbDeployConfig>,
) -> Result<LoadedProject> {
    let entry_path = canonical_utf8_path(entry_path)?;
    let mut modules = load_project_modules_for_entry(&entry_path, entry_source_override)?;
    let Some(entry_index) = modules.iter().position(|module| module.path == entry_path) else {
        return Err(CompileError::new(
            format!("entry source '{}' was not loaded into the project graph", entry_path),
            error::Span::default(),
        ));
    };
    crate::zk_witness_policy::bind(&mut modules[entry_index].ast, deploy).map_err(|err| attach_file_if_missing(err, &entry_path))?;
    let source_resolver = build_module_resolver_from_loaded_modules(&modules)?;
    monomorphize_loaded_project(&mut modules, entry_index, &source_resolver)?;
    let resolver = build_module_resolver_from_loaded_modules(&modules)?;
    Ok(LoadedProject { entry_index, modules, resolver })
}

pub(super) fn load_project_for_entry_diagnostics(
    entry_path: &Utf8Path,
    source_overrides: &BTreeMap<Utf8PathBuf, String>,
    deploy: Option<&crate::package::CkbDeployConfig>,
) -> std::result::Result<LoadedProject, Vec<CompileError>> {
    let entry_path = canonical_utf8_path(entry_path).map_err(|error| vec![error])?;
    let modules = load_project_modules_for_entry_diagnostics(&entry_path, source_overrides)?;
    prepare_loaded_project_for_entry_diagnostics(&entry_path, modules, deploy, |module| loaded_module_package_id(&module.path))
}

// Captured callers supply package identities from the same observation as the
// source graph. Resolution and monomorphization remain compiler-owned.
pub(super) fn prepare_loaded_project_for_entry_diagnostics(
    entry_path: &Utf8Path,
    mut modules: Vec<LoadedModule>,
    deploy: Option<&crate::package::CkbDeployConfig>,
    mut package_id: impl FnMut(&LoadedModule) -> Result<String>,
) -> std::result::Result<LoadedProject, Vec<CompileError>> {
    let Some(entry_index) = modules.iter().position(|module| module.path.as_path() == entry_path) else {
        return Err(vec![CompileError::new(
            format!("entry source '{}' was not loaded into the project graph", entry_path),
            error::Span::default(),
        )]);
    };
    crate::zk_witness_policy::bind(&mut modules[entry_index].ast, deploy)
        .map_err(|err| vec![attach_file_if_missing(err, entry_path)])?;
    let source_resolver = build_module_resolver_with_package_ids(&modules, &mut package_id).map_err(|error| vec![error])?;
    monomorphize_loaded_project_diagnostics(&mut modules, entry_index, &source_resolver)?;
    let resolver = build_module_resolver_with_package_ids(&modules, package_id).map_err(|error| vec![error])?;
    Ok(LoadedProject { entry_index, modules, resolver })
}

pub(super) fn load_virtual_project_for_entry_diagnostics(
    sources: &[InMemorySource],
    entry_path: &str,
    edition: CellScriptEdition,
) -> std::result::Result<LoadedProject, Vec<CompileError>> {
    if sources.is_empty() {
        return Err(vec![CompileError::without_span("multi-file compile requires at least one source")]);
    }
    let total_source_bytes = sources
        .iter()
        .try_fold(0usize, |total, source| total.checked_add(source.source.len()))
        .ok_or_else(|| vec![CompileError::without_span("multi-file source byte count overflowed")])?;
    if total_source_bytes > MAX_SOURCE_BYTES {
        return Err(vec![CompileError::without_span(format!("source set exceeds the {} byte compiler limit", MAX_SOURCE_BYTES))]);
    }
    let entry_path_buf = Utf8PathBuf::from(entry_path);
    let mut seen_paths = HashSet::new();
    let mut modules = Vec::with_capacity(sources.len());
    let mut diagnostics = Vec::new();
    for source in sources {
        if source.path.is_empty() {
            diagnostics.push(CompileError::without_span("multi-file compile source path must not be empty"));
            continue;
        }
        if !seen_paths.insert(source.path.clone()) {
            diagnostics.push(CompileError::without_span(format!("duplicate multi-file compile source path '{}'", source.path)));
            continue;
        }
        match parse_loaded_module_diagnostics(Utf8PathBuf::from(source.path.clone()), source.source.clone(), edition) {
            Ok(module) => modules.push(module),
            Err(errors) => diagnostics.extend(errors),
        }
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    let Some(entry_index) = modules.iter().position(|module| module.path == entry_path_buf) else {
        return Err(vec![CompileError::without_span(format!("entry source '{}' was not provided", entry_path))]);
    };
    let source_resolver = build_module_resolver_from_loaded_modules_in_single_package(&modules).map_err(|error| vec![error])?;
    monomorphize_loaded_project_diagnostics(&mut modules, entry_index, &source_resolver)?;
    let resolver = build_module_resolver_from_loaded_modules_in_single_package(&modules).map_err(|error| vec![error])?;
    Ok(LoadedProject { entry_index, modules, resolver })
}

fn load_project_modules_for_entry(entry_path: &Utf8Path, entry_source_override: Option<&str>) -> Result<Vec<LoadedModule>> {
    let entry_path = canonical_utf8_path(entry_path)?;
    let source_paths = collect_source_paths_for_compile_file(&entry_path)?;
    source_paths
        .into_iter()
        .map(|path| {
            let edition = source_edition(&path)?;
            let source = if path == entry_path {
                if let Some(source) = entry_source_override {
                    source.to_string()
                } else {
                    read_module_source(&path)?
                }
            } else {
                read_module_source(&path)?
            };
            parse_loaded_module(path, source, edition)
        })
        .collect()
}

pub(crate) fn load_project_modules_for_entry_diagnostics(
    entry_path: &Utf8Path,
    source_overrides: &BTreeMap<Utf8PathBuf, String>,
) -> std::result::Result<Vec<LoadedModule>, Vec<CompileError>> {
    let entry_path = canonical_utf8_path(entry_path).map_err(|error| vec![error])?;
    let source_paths = collect_source_paths_for_compile_file(&entry_path).map_err(|error| vec![error])?;
    let mut modules = Vec::with_capacity(source_paths.len());
    let mut diagnostics = Vec::new();
    for path in source_paths {
        let edition = match source_edition(&path) {
            Ok(edition) => edition,
            Err(error) => {
                diagnostics.push(error);
                continue;
            }
        };
        let source = if let Some(source) = source_overrides.get(&path) {
            source.clone()
        } else {
            match read_module_source(&path) {
                Ok(source) => source,
                Err(error) => {
                    diagnostics.push(error);
                    continue;
                }
            }
        };
        match parse_loaded_module_diagnostics(path, source, edition) {
            Ok(module) => modules.push(module),
            Err(errors) => diagnostics.extend(errors),
        }
    }
    if diagnostics.is_empty() {
        Ok(modules)
    } else {
        Err(diagnostics)
    }
}

pub(super) fn read_module_source(path: &Utf8Path) -> Result<String> {
    std::fs::read_to_string(path)
        .map_err(|e| CompileError::new(format!("failed to read module '{}': {}", path, e), error::Span::default()))
}

pub(super) fn parse_loaded_module(path: Utf8PathBuf, source: String, edition: CellScriptEdition) -> Result<LoadedModule> {
    let ast = frontend::parse(&source, edition).map_err(|e| e.with_file(path.clone()))?;
    Ok(LoadedModule { path, source, ast, edition })
}

fn parse_loaded_module_diagnostics(
    path: Utf8PathBuf,
    source: String,
    edition: CellScriptEdition,
) -> std::result::Result<LoadedModule, Vec<CompileError>> {
    let ast = frontend::parse_diagnostics(&source, edition)
        .map_err(|errors| errors.into_iter().map(|error| error.with_file(path.clone())).collect::<Vec<_>>())?;
    Ok(LoadedModule { path, source, ast, edition })
}

fn monomorphize_loaded_project(modules: &mut [LoadedModule], entry_index: usize, source_resolver: &ModuleResolver) -> Result<()> {
    monomorphize_loaded_project_diagnostics(modules, entry_index, source_resolver).map_err(diagnostics_to_compile_error)
}

fn monomorphize_loaded_project_diagnostics(
    modules: &mut [LoadedModule],
    entry_index: usize,
    source_resolver: &ModuleResolver,
) -> std::result::Result<(), Vec<CompileError>> {
    let module_indices =
        modules.iter().enumerate().map(|(index, module)| (module.ast.name.clone(), index)).collect::<BTreeMap<_, _>>();
    let Some(entry_module) = modules.get(entry_index) else {
        return Err(vec![CompileError::without_span("entry module index is outside the loaded project")]);
    };
    let mut reachable = BTreeSet::from([entry_index]);
    let mut pending = vec![entry_module.ast.name.clone()];
    while let Some(module_name) = pending.pop() {
        for import in source_resolver.imports_for_module(&module_name) {
            let owner_module = import.module_path.join("::");
            let Some(&owner_index) = module_indices.get(&owner_module) else {
                continue;
            };
            if reachable.insert(owner_index) {
                pending.push(owner_module);
            }
        }
    }
    // Interface arguments have a source lexical role. Check original nodes
    // before generic expansion can move them into their template's owner.
    let mut role_diagnostics = Vec::new();
    for &index in &reachable {
        let module = &modules[index];
        if let Err(error) = types::validate_source_open_handle_roles(&module.ast, source_resolver) {
            role_diagnostics.push(attach_file_if_missing(error, &module.path));
        }
    }
    if !role_diagnostics.is_empty() {
        return Err(role_diagnostics);
    }
    let source_modules = modules.iter().map(|module| module.ast.clone()).collect::<Vec<_>>();
    let owner_bindings = source_modules
        .iter()
        .map(|module| (module.name.clone(), std::sync::Arc::new(source_resolver.type_binding_identities(&module.name))))
        .collect::<BTreeMap<_, _>>();
    let mut meanings = BTreeMap::<String, std::collections::BTreeSet<ast::DeclarationIdentity>>::new();
    for &index in &reachable {
        for (name, identity) in owner_bindings[&source_modules[index].name].iter() {
            meanings.entry(name.clone()).or_default().insert(identity.clone());
        }
    }
    let ambiguous_bindings =
        std::sync::Arc::new(meanings.into_iter().filter_map(|(name, owners)| (owners.len() > 1).then_some(name)).collect());
    let external_items = source_modules
        .iter()
        .map(|module| {
            source_resolver
                .imports_for_module(&module.name)
                .into_iter()
                .filter_map(|import| {
                    let owner_module = import.module_path.join("::");
                    let owner = source_resolver.module(&owner_module)?;
                    let item = owner
                        .items
                        .iter()
                        .chain(owner.interface_templates.iter())
                        .find(|item| item.name() == Some(import.name.as_str()))?
                        .clone();
                    Some(generics::ExternalGenericItem {
                        local_name: import.alias.unwrap_or_else(|| import.name.clone()),
                        owner_bindings: owner_bindings[&owner_module].clone(),
                        owner_module,
                        source_name: import.name,
                        item,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut seeds = vec![BTreeMap::<String, generics::SeedInstantiation>::new(); modules.len()];
    let mut final_outputs = loop {
        let mut outputs = BTreeMap::new();
        let mut discovered = Vec::new();
        let mut diagnostics = Vec::new();
        for &index in &reachable {
            let source_module = &source_modules[index];
            let module_seeds = seeds[index].values().cloned().collect::<Vec<_>>();
            match generics::monomorphize_with_project_context(
                source_module,
                &external_items[index],
                &module_seeds,
                modules[index].edition,
                std::sync::Arc::clone(&ambiguous_bindings),
            ) {
                Ok(output) => {
                    discovered.extend(output.external_requests.iter().cloned());
                    outputs.insert(index, output);
                }
                Err(error) => diagnostics.push(attach_file_if_missing(error, &modules[index].path)),
            }
        }
        if !diagnostics.is_empty() {
            return Err(diagnostics);
        }

        let mut changed = false;
        for request in discovered {
            let Some(&owner_index) = module_indices.get(&request.owner_module) else {
                return Err(vec![CompileError::without_span(format!(
                    "generic template owner module '{}' was not loaded",
                    request.owner_module
                ))]);
            };
            let key = format!(
                "{}<{}>",
                request.source_name,
                request.seed.args.iter().map(generics::render_type).collect::<Vec<_>>().join(",")
            );
            if let std::collections::btree_map::Entry::Vacant(entry) = seeds[owner_index].entry(key) {
                entry.insert(request.seed);
                changed = true;
            }
        }
        let total_seeds = seeds.iter().map(BTreeMap::len).sum::<usize>();
        if total_seeds > generics::MAX_GENERIC_INSTANTIATIONS {
            return Err(vec![CompileError::without_span(format!(
                "cross-module generic instantiation count exceeds the limit of {}",
                generics::MAX_GENERIC_INSTANTIATIONS
            ))
            .with_code(crate::error::DiagnosticCode::E2112)]);
        }
        if !changed {
            break outputs;
        }
    };

    let mut linked_exports = vec![BTreeSet::<String>::new(); modules.len()];
    for (&requester_index, output) in &mut final_outputs {
        let generic_source_imports = external_items[requester_index]
            .iter()
            .filter(|external| {
                matches!(&external.item, ast::Item::Struct(def) if !def.type_params.is_empty())
                    || matches!(&external.item, ast::Item::Enum(def) if !def.type_params.is_empty())
                    || matches!(&external.item, ast::Item::Function(def) if !def.type_params.is_empty())
            })
            .map(|external| (external.owner_module.clone(), external.source_name.clone()))
            .collect::<BTreeSet<_>>();
        for item in &mut output.module.items {
            if let ast::Item::Use(imports) = item {
                let owner = imports.module_path.join("::");
                imports.imports.retain(|import| !generic_source_imports.contains(&(owner.clone(), import.name.clone())));
            }
        }
        output.module.items.retain(|item| !matches!(item, ast::Item::Use(imports) if imports.imports.is_empty()));
        for request in &output.external_requests {
            let owner_index = module_indices[&request.owner_module];
            linked_exports[owner_index].insert(request.owner_concrete_name.clone());
            output.module.items.push(ast::Item::Use(ast::UseStmt {
                module_path: request.owner_module.split("::").map(str::to_string).collect(),
                imports: vec![ast::UseImport {
                    name: request.owner_concrete_name.clone(),
                    alias: (request.local_concrete_name != request.owner_concrete_name).then(|| request.local_concrete_name.clone()),
                }],
                span: error::Span::default(),
            }));
        }
        // Emitted specializations may reference concrete names materialized in
        // a third module (an imported template instantiated with another
        // imported template's application). Import them under the requesting
        // spelling so later phases resolve the reference.
        for foreign in &output.foreign_imports {
            if let Some(&owner_index) = module_indices.get(&foreign.owner_module) {
                if foreign.origin.is_some() {
                    linked_exports[owner_index].insert(foreign.owner_concrete_name.clone());
                }
                output.module.items.push(ast::Item::Use(ast::UseStmt {
                    module_path: foreign.owner_module.split("::").map(str::to_string).collect(),
                    imports: vec![ast::UseImport { name: foreign.owner_concrete_name.clone(), alias: foreign.import_alias() }],
                    span: error::Span::default(),
                }));
            }
        }
    }
    for (index, output) in final_outputs {
        let mut module = output.module;
        for name in &linked_exports[index] {
            module.visibilities.insert(name.clone(), ast::Visibility::Public);
        }
        modules[index].ast = module;
    }
    Ok(())
}

/// Resolve the source edition for a file from its containing package manifest.
///
/// Standalone files use the stable current edition. Package-backed tooling
/// should call this instead of bypassing the edition-routed frontend.
pub fn source_edition(path: &Utf8Path) -> Result<CellScriptEdition> {
    find_package_root(path)?
        .map(|root| load_manifest(&root).map(|manifest| manifest.package.edition))
        .transpose()
        .map(|edition| edition.unwrap_or(CURRENT_EDITION))
}

fn build_module_resolver_from_loaded_modules(modules: &[LoadedModule]) -> Result<ModuleResolver> {
    build_module_resolver_with_package_ids(modules, |module| loaded_module_package_id(&module.path))
}

pub(super) fn build_module_resolver_with_package_ids(
    modules: &[LoadedModule],
    mut package_id: impl FnMut(&LoadedModule) -> Result<String>,
) -> Result<ModuleResolver> {
    let mut resolver = ModuleResolver::new();
    for module in modules {
        let package_id = package_id(module)?;
        resolver
            .register_module_in_package(module.ast.clone(), package_id)
            .map_err(|error| attach_file_if_missing(error, &module.path))?;
    }
    validate_loaded_project_imports(&resolver, modules)?;
    Ok(resolver)
}

fn build_module_resolver_from_loaded_modules_in_single_package(modules: &[LoadedModule]) -> Result<ModuleResolver> {
    let mut resolver = ModuleResolver::new();
    for module in modules {
        resolver.register_module(module.ast.clone()).map_err(|error| attach_file_if_missing(error, &module.path))?;
    }
    validate_loaded_project_imports(&resolver, modules)?;
    Ok(resolver)
}

pub(super) fn loaded_module_package_id(path: &Utf8Path) -> Result<String> {
    if let Some(root) = find_package_root(path)? {
        let root = canonical_utf8_path(&root)?;
        return Ok(loaded_module_package_id_from_location(Some(&root), &root));
    }
    let parent = path.parent().unwrap_or_else(|| Utf8Path::new("."));
    let parent = if parent.exists() { canonical_utf8_path(parent)? } else { parent.to_path_buf() };
    Ok(loaded_module_package_id_from_location(None, &parent))
}

// Both directories are already canonical at native/captured input boundaries.
pub(super) fn loaded_module_package_id_from_location(package_root: Option<&Utf8Path>, source_parent: &Utf8Path) -> String {
    match package_root {
        Some(root) => format!("package:{root}"),
        None => format!("unpackaged:{source_parent}"),
    }
}

fn validate_loaded_project_imports(resolver: &ModuleResolver, modules: &[LoadedModule]) -> Result<()> {
    for module in modules {
        for import in resolver.imports_for_module(&module.ast.name) {
            resolver.validate_import(&module.ast.name, &import).map_err(|error| attach_file_if_missing(error, &module.path))?;
        }
    }
    Ok(())
}

/// The single authoritative source-file set for a package: the exact files
/// that participate in compilation, source hashing, and publish snapshots.
/// Publish `source_hash` must never cover a different set than the bytes a
/// consumer actually compiles, or one signed hash could front two different
/// compiled packages.
pub fn package_source_file_set(
    package_root: &Utf8Path,
    source_roots: &[String],
    entry: Option<&str>,
) -> Result<std::collections::BTreeSet<Utf8PathBuf>> {
    let mut roots = Vec::new();
    let mut seen_roots = HashSet::new();

    for source_root in source_roots {
        let root = package_root.join(source_root);
        if !root.exists() {
            return Err(CompileError::new(format!("configured source root '{}' does not exist", root), error::Span::default()));
        }
        if !root.is_dir() {
            return Err(CompileError::new(format!("configured source root '{}' is not a directory", root), error::Span::default()));
        }

        let root = canonical_utf8_path(&root)?;
        if seen_roots.insert(root.clone()) {
            roots.push(root);
        }
    }

    if roots.is_empty() {
        let src_root = package_root.join("src");
        if src_root.exists() && src_root.is_dir() {
            let src_root = canonical_utf8_path(&src_root)?;
            if seen_roots.insert(src_root.clone()) {
                roots.push(src_root);
            }
        }
    }

    let mut explicit_entry = None;
    if let Some(entry) = entry {
        let entry_path = package_root.join(entry);
        if !entry_path.exists() {
            return Err(CompileError::new(format!("package entry '{}' does not exist", entry_path), error::Span::default()));
        }
        let entry_path = canonical_utf8_path(&entry_path)?;
        if let Some(entry_parent) = entry_path.parent() {
            let entry_parent = canonical_utf8_path(entry_parent)?;
            if seen_roots.insert(entry_parent.clone()) {
                roots.push(entry_parent);
            }
        }
        explicit_entry = Some(entry_path);
    }

    let mut files = std::collections::BTreeSet::new();
    for root in roots {
        for file in collect_cell_files(&root)? {
            files.insert(file);
        }
    }

    if let Some(entry_path) = explicit_entry {
        files.insert(entry_path);
    }

    Ok(files)
}

pub(super) fn collect_package_cell_files(package_root: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    let manifest = load_manifest(package_root)?;
    let entry = explicit_manifest_entry(package_root)?;
    let files = package_source_file_set(package_root, &manifest.package.source_roots, entry.as_deref())?;
    Ok(files.into_iter().collect())
}

pub(super) fn collect_cell_files(root: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    let mut files = Vec::new();
    collect_cell_files_recursive(root, &mut files, 0)?;
    files.sort();
    Ok(files)
}

const MAX_CELL_SOURCE_DISCOVERY_DEPTH: usize = 64;

const MAX_CELL_SOURCE_FILES: usize = 4096;

fn collect_cell_files_recursive(root: &Utf8Path, files: &mut Vec<Utf8PathBuf>, depth: usize) -> Result<()> {
    if depth > MAX_CELL_SOURCE_DISCOVERY_DEPTH {
        return Err(CompileError::new(
            format!("module directory; nesting exceeds the discovery limit of {MAX_CELL_SOURCE_DISCOVERY_DEPTH}"),
            error::Span::default(),
        ));
    }
    let entries = std::fs::read_dir(root).map_err(|e| {
        CompileError::new(format!("failed to read module directory '{}': {}", root, e), error::Span::default())
            .with_category(error::CompileErrorCategory::Io)
            .with_file(root.to_owned())
            .with_source(e)
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| {
            CompileError::new(format!("failed to read directory entry in '{}': {}", root, e), error::Span::default())
                .with_category(error::CompileErrorCategory::Io)
                .with_file(root.to_owned())
                .with_source(e)
        })?;
        // Decide on the entry's own file type: directory symlinks are never
        // followed (a cycle would otherwise recurse without a visited set)
        // and `.cell` symlinks are not admitted either, so a source file is
        // always a regular file the workspace actually owns.
        let file_type = entry.file_type().map_err(|e| {
            CompileError::new(format!("failed to classify directory entry in '{}': {}", root, e), error::Span::default())
                .with_category(error::CompileErrorCategory::Io)
                .with_file(root.to_owned())
                .with_source(e)
        })?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        let Ok(candidate) = Utf8PathBuf::from_path_buf(path) else {
            continue;
        };

        if file_type.is_dir() {
            if should_skip_cell_dir(&candidate) {
                continue;
            }
            collect_cell_files_recursive(&candidate, files, depth + 1)?;
            continue;
        }

        if file_type.is_file() && candidate.extension() == Some("cell") {
            if files.len() >= MAX_CELL_SOURCE_FILES {
                return Err(CompileError::new(
                    format!("workspace source discovery exceeds the admission budget of {MAX_CELL_SOURCE_FILES} files"),
                    error::Span::default(),
                ));
            }
            files.push(canonical_utf8_path(&candidate)?);
        }
    }

    Ok(())
}

fn should_skip_cell_dir(path: &Utf8Path) -> bool {
    matches!(path.file_name(), Some(".git" | ".cell" | "target")) || path.join("Cell.toml").exists()
}

pub(super) fn canonical_utf8_path(path: &Utf8Path) -> Result<Utf8PathBuf> {
    let canonical = std::fs::canonicalize(path).map_err(|e| {
        CompileError::new(format!("failed to canonicalize '{}': {}", path, e), error::Span::default())
            .with_category(error::CompileErrorCategory::Io)
            .with_file(path.to_owned())
            .with_source(e)
    })?;
    Utf8PathBuf::from_path_buf(canonical)
        .map_err(|non_utf8| CompileError::new(format!("path is not valid UTF-8: {}", non_utf8.display()), error::Span::default()))
}

pub(super) fn load_manifest(package_root: &Utf8Path) -> Result<PackageManifest> {
    let manifest_path = package_root.join("Cell.toml");
    if !manifest_path.exists() {
        return Err(CompileError::new(format!("Cell.toml not found in '{}'", package_root), error::Span::default())
            .with_code(error::DiagnosticCode::CLI0006)
            .with_category(error::CompileErrorCategory::Io)
            .with_file(manifest_path));
    }

    let manifest_source = std::fs::read_to_string(&manifest_path).map_err(|e| {
        CompileError::new(format!("failed to read manifest '{}': {}", manifest_path, e), error::Span::default())
            .with_category(error::CompileErrorCategory::Io)
            .with_file(manifest_path.clone())
            .with_source(e)
    })?;
    parse_package_manifest(&manifest_path, &manifest_source)
}

pub(super) fn parse_package_manifest(manifest_path: &Utf8Path, manifest_source: &str) -> Result<PackageManifest> {
    let manifest: PackageManifest = toml::from_str(manifest_source)
        .map_err(|e| CompileError::new(format!("failed to parse manifest '{}': {}", manifest_path, e), error::Span::default()))?;
    artifact::validate_declarations(&manifest.artifacts)?;
    crate::package::validate_package_compiler_requirement(&manifest.package)
        .map_err(|error| error.with_file(manifest_path.to_owned()))?;
    crate::package::validate_package_path_selection(&manifest).map_err(|error| error.with_file(manifest_path.to_owned()))?;
    Ok(manifest)
}

pub(super) fn manifest_has_table(manifest_path: &Utf8Path, table: &str) -> Result<bool> {
    let manifest_source = std::fs::read_to_string(manifest_path)
        .map_err(|e| CompileError::new(format!("failed to read manifest '{}': {}", manifest_path, e), error::Span::default()))?;
    manifest_source_has_table(manifest_path, &manifest_source, table)
}

pub(super) fn manifest_source_has_table(manifest_path: &Utf8Path, manifest_source: &str, table: &str) -> Result<bool> {
    let manifest: toml::Value = toml::from_str(manifest_source)
        .map_err(|e| CompileError::new(format!("failed to parse manifest '{}': {}", manifest_path, e), error::Span::default()))?;
    Ok(manifest.get(table).is_some())
}

/// Whether Cell.toml declares an explicit `[package].entry`, and its value.
/// Hashing, compilation, and publish snapshots must all apply the same
/// explicit-entry semantics.
pub fn explicit_manifest_entry(package_root: &Utf8Path) -> Result<Option<String>> {
    let manifest_path = package_root.join("Cell.toml");
    let manifest_source = std::fs::read_to_string(&manifest_path)
        .map_err(|e| CompileError::new(format!("failed to read manifest '{}': {}", manifest_path, e), error::Span::default()))?;
    let manifest: toml::Value = toml::from_str(&manifest_source)
        .map_err(|e| CompileError::new(format!("failed to parse manifest '{}': {}", manifest_path, e), error::Span::default()))?;
    let Some(package) = manifest.get("package").and_then(toml::Value::as_table) else {
        return Ok(None);
    };
    let Some(entry) = package.get("entry").and_then(toml::Value::as_str) else {
        return Ok(None);
    };
    Ok(Some(entry.to_string()))
}

#[derive(Debug, Clone)]
pub struct LoadedModule {
    pub path: Utf8PathBuf,
    pub source: String,
    pub ast: ast::Module,
    pub edition: CellScriptEdition,
}

#[derive(Debug)]
pub(super) struct LoadedProject {
    pub(super) entry_index: usize,
    pub(super) modules: Vec<LoadedModule>,
    pub(super) resolver: ModuleResolver,
}

impl LoadedProject {
    pub(super) fn entry(&self) -> &LoadedModule {
        &self.modules[self.entry_index]
    }
}
