//! Native frozen source provenance for checked module projection.
//! This private construction path binds actual selected package sources and
//! module owners. It is not a stable interface family, deployment receipt,
//! independent source-equivalence proof or open-handle admission.
use super::{DependencyScope, Lockfile, PackageManager, PackageSource, ResolutionOptions};
use crate::error::{CompileError, Result};
use crate::{CompileEntryScope, CompileOptions, ExecutableSurfacePolicy};
use camino::{Utf8Path, Utf8PathBuf};
use cellscript_artifact_checker::{canonical_bytes, canonical_hash, interface::CheckedModuleProjection, CheckerBudgets};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

const MAX_FILE: usize = 4 * 1024 * 1024;
const MAX_INPUTS: usize = 16 * 1024 * 1024;
const MAX_PACKAGES: usize = 32;
const MAX_MODULES: usize = 256;

mod catalog;
pub use catalog::{freeze_module_catalog, FrozenModuleCatalog};
mod code_catalog;
pub use code_catalog::{freeze_code_catalog, CodeCandidateInput, FrozenCodeCandidate, FrozenCodeCatalog};
mod resolved_catalog;
pub use resolved_catalog::{resolve_code_catalog_source, ResolvedCodeCatalog};

#[derive(Debug, Clone)]
pub enum EntrySelection {
    Default,
    Action(String),
    Lock(String),
    Artifact(String),
}

#[derive(Debug)]
pub struct FrozenPackageModule {
    context: Context,
    context_id: String,
    bundle: [Vec<u8>; 4],
    projection: CheckedModuleProjection,
}
impl FrozenPackageModule {
    pub fn context_identity(&self) -> &str {
        &self.context_id
    }
    pub fn context_bytes(&self) -> Result<Vec<u8>> {
        canonical_bytes(&self.context).map_err(checker_error)
    }
    pub fn projection(&self) -> &CheckedModuleProjection {
        &self.projection
    }
    /// Exact stored bytes; callers cannot mutate a checked context's bundle.
    pub fn bundle(&self) -> [&[u8]; 4] {
        std::array::from_fn(|index| self.bundle[index].as_slice())
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct Context {
    schema: &'static str,
    lock_hash: String,
    compiler: &'static str,
    chain_id: String,
    network_genesis: String,
    entry_module: String,
    packages: BTreeMap<String, Package>,
    modules: BTreeMap<String, Module>,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
struct Package {
    name: String,
    namespace: Option<String>,
    version: String,
    edition: crate::CellScriptEdition,
    manifest_digest: String,
    source_hash: String,
    compiler_requirement: String,
    source: Source,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum Source {
    RootSnapshot,
    LocalSnapshot,
    Git { url: String, revision: String },
    Registry { registry: String, url: String, revision: String, namespace: String, version: String },
}
#[derive(Debug, Serialize, PartialEq, Eq)]
struct Module {
    package: String,
    relative_path: String,
    source_hash: String,
    source_bytes: usize,
}

fn invalid(message: impl Into<String>) -> CompileError {
    CompileError::without_span(message).with_code("E2610")
}
fn checker_error(error: cellscript_artifact_checker::CheckerError) -> CompileError {
    CompileError::without_span(error.to_string()).with_code(error.code.as_str())
}
fn read_bounded(path: &Utf8Path, total: &mut usize) -> Result<Vec<u8>> {
    if !std::fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(invalid("frozen interface inputs must be regular files without symbolic links"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take((MAX_FILE + 1) as u64).read_to_end(&mut bytes)?;
    *total = total.checked_add(bytes.len()).ok_or_else(|| invalid("frozen interface input length overflow"))?;
    if bytes.len() > MAX_FILE || *total > MAX_INPUTS {
        return Err(invalid("frozen interface inputs exceed 4 MiB/file or 16 MiB total"));
    }
    Ok(bytes)
}
fn hash(bytes: &[u8]) -> String {
    crate::hex_encode(&crate::ckb_blake2b256(bytes))
}

/// Compile from a real package/lock closure without repinning, network access or
/// lock writes. The named environment must be pinned even for zero dependencies.
/// Exported context bytes alone cannot reconstruct `FrozenPackageModule`.
pub fn compile_module(
    root: &Utf8Path,
    environment: &str,
    mut options: CompileOptions,
    selection: EntrySelection,
) -> Result<FrozenPackageModule> {
    if options.target.as_deref().is_some_and(|target| target != "riscv64-elf") {
        return Err(invalid("frozen interface projection requires a RISC-V ELF bundle"));
    }
    options.target = Some("riscv64-elf".into());
    options.source_contracts = true;
    crate::validate_compile_options(&options)?;
    let root = crate::canonical_utf8_path(root)?;
    if !root.is_dir() {
        return Err(invalid("frozen interface input must be a package directory"));
    }
    if super::active_lockfile_override(root.as_std_path())?.is_some() {
        return Err(invalid("frozen interface context cannot use a planned lockfile override"));
    }
    let resolution = ResolutionOptions {
        scope: DependencyScope::Runtime,
        environment: Some(environment.into()),
        offline: true,
        ..ResolutionOptions::default()
    };
    super::with_resolution_options(resolution.clone(), || {
        let (before, paths) = capture(&root, environment, &resolution)?;
        options.edition = PackageManager::new(root.as_std_path()).read_manifest()?.package.edition;
        let entry = crate::canonical_utf8_path(&crate::resolve_input_path(&root)?)?;
        let project = crate::load_project_for_entry(&entry, None)?;
        if project.modules.len() > MAX_MODULES {
            return Err(invalid("frozen interface module count exceeds 256"));
        }
        let scope = match selection {
            EntrySelection::Default => None,
            EntrySelection::Action(name) => Some(CompileEntryScope::Action(name)),
            EntrySelection::Lock(name) => Some(CompileEntryScope::Lock(name)),
            EntrySelection::Artifact(name) => Some(CompileEntryScope::Artifact(crate::resolve_named_artifact(&entry, &name)?)),
        };
        let compiled = crate::compile_path_with_executable_surface_policy(
            &root,
            options.clone(),
            scope.clone(),
            ExecutableSurfacePolicy::DenyFailClosed,
        )?;
        if crate::interface::build(&project.entry().ast, &compiled.metadata) != compiled.metadata.public_interface {
            return Err(invalid("checked public interface differs from the frozen package source"));
        }
        let (_, ir) = crate::prepare_compile_ir(
            &project.entry().ast,
            &options,
            Some((&project.resolver, &project.entry().ast.name)),
            scope.as_ref(),
        )?;
        let typed = &compiled.metadata.typed_semantics;
        // Wire records sort declaration identities, while source declarations
        // retain source order. Compare the same canonical representation;
        // field, parameter and binder order remain significant.
        let mut declarations = cellscript_artifact_checker::TypedSemanticRecord {
            generic_declarations: crate::typed_semantics::generic_catalog_for_context(&project.entry().ast, Some(&project.resolver)),
            nominal_declarations: crate::typed_semantics::nominal_catalog(&project.entry().ast, Some(&project.resolver), &ir, typed),
            ..Default::default()
        };
        declarations.canonicalize();
        if typed.generic_declarations != declarations.generic_declarations
            || typed.nominal_declarations != declarations.nominal_declarations
        {
            return Err(invalid("checked declaration owners differ from the frozen package sources"));
        }
        let (after, after_paths) = capture(&root, environment, &resolution)?;
        if before != after || paths != after_paths {
            return Err(invalid("frozen package sources or lock changed during compilation"));
        }
        if compiled.metadata.module != before.entry_module || compiled.metadata.source_units.len() != project.modules.len() {
            return Err(invalid("compiled source closure differs from the frozen module closure"));
        }
        for loaded in &project.modules {
            let module =
                before.modules.get(&loaded.ast.name).ok_or_else(|| invalid("compiled module lacks its frozen package owner"))?;
            if hash(loaded.source.as_bytes()) != module.source_hash || loaded.source.len() != module.source_bytes {
                return Err(invalid("parsed module source differs from its frozen bytes"));
            }
            let unit = compiled
                .metadata
                .source_units
                .iter()
                .find(|unit| Utf8Path::new(&unit.path) == loaded.path)
                .ok_or_else(|| invalid("compiled module lacks a source-digest binding"))?;
            if unit.hash != module.source_hash || unit.size_bytes != module.source_bytes {
                return Err(invalid("compiled source digest differs from its frozen module"));
            }
        }
        let bundle = [
            compiled.artifact_bytes,
            serde_json::to_vec(&compiled.metadata)?,
            canonical_bytes(
                compiled.verified_lowering_record.as_ref().ok_or_else(|| invalid("frozen module has no lowering record"))?,
            )
            .map_err(checker_error)?,
            canonical_bytes(compiled.source_artifact_map.as_ref().ok_or_else(|| invalid("frozen module has no source map"))?)
                .map_err(checker_error)?,
        ];
        let projection = cellscript_artifact_checker::interface::project_bundle(
            &bundle[0],
            &bundle[1],
            &bundle[2],
            &bundle[3],
            &CheckerBudgets::default(),
        )
        .map_err(checker_error)?;
        if canonical_bytes(&before).map_err(checker_error)?.len() > MAX_FILE {
            return Err(invalid("frozen module source context exceeds 4 MiB"));
        }
        let context_id = canonical_hash("cellscript-frozen-module-source-context-id-v1", &before).map_err(checker_error)?;
        Ok(FrozenPackageModule { context: before, context_id, bundle, projection })
    })
}

fn capture(root: &Utf8Path, environment: &str, resolution: &ResolutionOptions) -> Result<(Context, BTreeMap<Utf8PathBuf, String>)> {
    let mut total = 0usize;
    read_bounded(&root.join("Cell.toml"), &mut total)?;
    let lock_bytes = read_bounded(&root.join("Cell.lock"), &mut total)
        .map_err(|_| invalid("frozen interface requires an existing bounded Cell.lock; run cellc lock explicitly"))?;
    let lock: Lockfile = toml::from_str(std::str::from_utf8(&lock_bytes).map_err(|_| invalid("Cell.lock is not UTF-8"))?)?;
    lock.validate_schema()?;
    if lock.dependencies.len() >= MAX_PACKAGES {
        return Err(invalid("frozen interface package closure exceeds 32 packages"));
    }
    let mut manager = PackageManager::new(root.as_std_path());
    let manifest = manager.read_manifest()?;
    let pinned =
        lock.environments.get(environment).ok_or_else(|| invalid("frozen interface environment is not pinned in Cell.lock"))?;
    let declared =
        manifest.environments.get(environment).ok_or_else(|| invalid("frozen interface environment is missing from Cell.toml"))?;
    preflight(root, &manager, &manifest, &lock, pinned)?;
    if pinned.chain_id != declared.chain_id
        || super::normalized_genesis_hash(&pinned.genesis_hash) != super::normalized_genesis_hash(&declared.genesis_hash)
        || hex::decode(declared.genesis_hash.strip_prefix("0x").unwrap_or(&declared.genesis_hash))
            .ok()
            .is_none_or(|hash| hash.len() != 32 || hash.iter().all(|byte| *byte == 0))
    {
        return Err(invalid("frozen interface chain identity is missing or disagrees with its pinned environment"));
    }
    if lock.root.manifest_digest != super::compute_manifest_digest(root.as_std_path())?
        || lock.package.name != manifest.package.name
        || lock.package.namespace != manifest.package.namespace
        || lock.package.version != manifest.package.version
        || lock.package.edition != manifest.package.edition
        || lock.package.source_hash.as_deref() != Some(&super::registry::compute_source_hash(root.as_std_path())?)
    {
        return Err(invalid("frozen root package identity, manifest or source hash differs from Cell.lock; repin explicitly"));
    }
    manager.resolve_locked_dependencies(resolution)?;
    let mut packages = BTreeMap::new();
    let mut paths = BTreeMap::new();
    let mut sources = vec![(root.to_path_buf(), Source::RootSnapshot)];
    for package in manager.get_resolved().values() {
        let path = Utf8PathBuf::from_path_buf(std::fs::canonicalize(&package.path)?)
            .map_err(|_| invalid("frozen package path is not UTF-8"))?;
        let source = match &package.source {
            PackageSource::Local(_) => Source::LocalSnapshot,
            PackageSource::Git { url, revision } => Source::Git { url: url.clone(), revision: revision.clone() },
            PackageSource::Registry { registry, url, revision, namespace, version } => Source::Registry {
                registry: registry.clone(),
                url: url.clone(),
                revision: revision.clone(),
                namespace: namespace.clone(),
                version: version.clone(),
            },
        };
        sources.push((path, source));
    }
    for (path, source) in sources {
        read_bounded(&path.join("Cell.toml"), &mut total)?;
        let manifest = PackageManager::new(path.as_std_path()).read_manifest()?;
        let package = Package {
            name: manifest.package.name,
            namespace: manifest.package.namespace,
            version: manifest.package.version,
            edition: manifest.package.edition,
            manifest_digest: super::compute_manifest_digest(path.as_std_path())?,
            source_hash: super::registry::compute_source_hash(path.as_std_path())?,
            compiler_requirement: manifest.package.cellscript_version,
            source,
        };
        let id = canonical_hash("cellscript-frozen-source-package-id-v1", &package).map_err(checker_error)?;
        if packages.insert(id.clone(), package).is_some() || paths.insert(path, id).is_some() {
            return Err(invalid("frozen source package ownership is ambiguous"));
        }
    }
    let entry = crate::canonical_utf8_path(&crate::resolve_input_path(root)?)?;
    let source_paths = crate::collect_source_paths_for_compile_file(&entry)?;
    if source_paths.len() > MAX_MODULES {
        return Err(invalid("frozen interface module count exceeds 256"));
    }
    let mut modules = BTreeMap::new();
    let mut entry_module = None;
    for source_path in source_paths {
        let bytes = read_bounded(&source_path, &mut total)?;
        let source = String::from_utf8(bytes).map_err(|_| invalid("frozen CellScript module is not UTF-8"))?;
        let (owner, package) = paths
            .iter()
            .filter(|(path, _)| source_path.starts_with(path))
            .max_by_key(|(path, _)| path.as_str().len())
            .ok_or_else(|| invalid("frozen source has no selected package owner"))?;
        let ast = crate::frontend::parse(&source, packages[package].edition)?;
        if source_path == entry {
            entry_module = Some(ast.name.clone());
        }
        let relative = source_path.strip_prefix(owner).map_err(|_| invalid("frozen source is outside its package"))?;
        let module = Module {
            package: package.clone(),
            relative_path: relative.as_str().replace('\\', "/"),
            source_hash: hash(source.as_bytes()),
            source_bytes: source.len(),
        };
        if modules.insert(ast.name, module).is_some() {
            return Err(invalid("frozen source closure contains duplicate module owners"));
        }
    }
    Ok((
        Context {
            schema: "cellscript-frozen-module-source-context-v1",
            lock_hash: hash(&lock_bytes),
            compiler: crate::VERSION,
            chain_id: declared.chain_id.clone(),
            network_genesis: super::normalized_genesis_hash(&declared.genesis_hash),
            entry_module: entry_module.ok_or_else(|| invalid("frozen package has no entry module"))?,
            packages,
            modules,
        },
        paths,
    ))
}

// Bound static native input trees before the existing resolver hashes/loads
// them. This is not a sandbox against a concurrently hostile filesystem:
// before/after snapshots reject changes, but upstream re-reads remain native I/O.
fn preflight(
    root: &Utf8Path,
    manager: &PackageManager,
    manifest: &super::PackageManifest,
    lock: &Lockfile,
    environment: &super::LockedEnvironment,
) -> Result<()> {
    let mut inputs = InputTree::default();
    inputs.file(&root.join("Cell.lock"))?;
    inputs.package(root, manifest)?;
    let mut pending: Vec<_> = environment.dependencies.values().cloned().collect();
    let mut visited = BTreeSet::new();
    while let Some(node) = pending.pop() {
        if !visited.insert(node.clone()) {
            continue;
        }
        if visited.len() >= MAX_PACKAGES {
            return Err(invalid("frozen interface package closure exceeds 32 packages"));
        }
        let selected = lock.dependencies.get(&node).ok_or_else(|| invalid("frozen dependency edge has no locked node"))?;
        let path = Utf8PathBuf::from_path_buf(manager.locked_source_path(selected, true)?)
            .map_err(|_| invalid("frozen dependency path is not UTF-8"))?;
        let path = crate::canonical_utf8_path(&path)?;
        let bytes = inputs.file(&path.join("Cell.toml"))?;
        let manifest = toml::from_str(std::str::from_utf8(&bytes).map_err(|_| invalid("Cell.toml is not UTF-8"))?)?;
        inputs.package(&path, &manifest)?;
        pending.extend(selected.dependencies.values().cloned());
    }
    Ok(())
}

#[derive(Default)]
struct InputTree {
    total: usize,
    entries: usize,
    modules: usize,
    files: BTreeSet<Utf8PathBuf>,
    directories: BTreeSet<Utf8PathBuf>,
}
impl InputTree {
    fn file(&mut self, path: &Utf8Path) -> Result<Vec<u8>> {
        let bytes = read_bounded(path, &mut self.total)?;
        if self.files.insert(path.to_path_buf()) && path.extension() == Some("cell") {
            self.modules += 1;
            if self.modules > MAX_MODULES {
                return Err(invalid("frozen interface module count exceeds 256"));
            }
        }
        Ok(bytes)
    }

    fn package(&mut self, root: &Utf8Path, manifest: &super::PackageManifest) -> Result<()> {
        if !self.files.contains(&root.join("Cell.toml")) {
            self.file(&root.join("Cell.toml"))?;
        }
        let entry = self.inside(root, &root.join(&manifest.package.entry))?;
        let mut directories = manifest.package.source_roots.iter().map(|relative| root.join(relative)).collect::<Vec<_>>();
        if directories.is_empty() && root.join("src").exists() {
            directories.push(root.join("src"));
        }
        directories.push(entry.parent().ok_or_else(|| invalid("frozen entry has no parent"))?.to_path_buf());
        for directory in directories {
            let directory = self.inside(root, &directory)?;
            self.walk(root, &directory, 0)?;
        }
        if !self.files.contains(&entry) {
            self.file(&entry)?;
        }
        Ok(())
    }

    fn inside(&self, root: &Utf8Path, path: &Utf8Path) -> Result<Utf8PathBuf> {
        let relative = path.strip_prefix(root).map_err(|_| invalid("frozen source root escapes its package"))?;
        let mut current = root.to_path_buf();
        for component in relative.components() {
            if !matches!(component, camino::Utf8Component::Normal(_)) {
                return Err(invalid("frozen source paths require normal package-relative components"));
            }
            current.push(component.as_str());
            if std::fs::symlink_metadata(&current)?.file_type().is_symlink() {
                return Err(invalid("frozen source paths cannot contain symbolic links"));
            }
        }
        let canonical = crate::canonical_utf8_path(path)?;
        if !canonical.starts_with(root) {
            return Err(invalid("frozen source root escapes its package"));
        }
        Ok(canonical)
    }

    fn walk(&mut self, root: &Utf8Path, directory: &Utf8Path, depth: usize) -> Result<()> {
        if depth > 16 {
            return Err(invalid("frozen source directory depth exceeds 16"));
        }
        if !self.directories.insert(directory.to_path_buf()) {
            return Ok(());
        }
        for entry in std::fs::read_dir(directory)? {
            self.entries += 1;
            if self.entries > 4096 {
                return Err(invalid("frozen source tree exceeds 4096 directory entries"));
            }
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                return Err(invalid("frozen source trees cannot contain symbolic links"));
            }
            let path = Utf8PathBuf::from_path_buf(entry.path()).map_err(|_| invalid("frozen source path is not UTF-8"))?;
            if kind.is_dir() {
                self.walk(root, &path, depth + 1)?;
            } else if path.extension() == Some("cell") && !self.files.contains(&path) {
                self.inside(root, &path)?;
                self.file(&path)?;
            }
        }
        Ok(())
    }
}
