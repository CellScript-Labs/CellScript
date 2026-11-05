//! Immutable inputs for one workspace observation. Native discovery happens
//! here; parsing and resolution subsequently consume only captured values.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

type ReadResult<T> = std::result::Result<T, CompileError>;
type Scope = (Utf8PathBuf, Option<bool>);

#[cfg(test)]
#[derive(Default)]
pub(super) struct ReadStats {
    pub(super) observations: std::sync::atomic::AtomicUsize,
    pub(super) source_reads: std::sync::atomic::AtomicUsize,
    pub(super) source_hashes: std::sync::atomic::AtomicUsize,
    pub(super) source_parses: std::sync::atomic::AtomicUsize,
    #[cfg(all(feature = "lsp", not(feature = "wasm")))]
    pub(super) blocker: std::sync::Mutex<Option<ReadBlocker>>,
}

#[cfg(all(test, feature = "lsp", not(feature = "wasm")))]
pub(super) struct ReadBlocker {
    pub(super) path: Utf8PathBuf,
    pub(super) entered: tokio::sync::oneshot::Sender<()>,
    pub(super) release: std::sync::mpsc::Receiver<()>,
}

#[cfg(all(test, feature = "lsp", not(feature = "wasm")))]
impl ReadStats {
    fn before_read(&self, path: &camino::Utf8Path) {
        let blocked = {
            let mut blocker = self.blocker.lock().unwrap();
            if blocker.as_ref().is_some_and(|blocker| blocker.path == path) {
                blocker.take()
            } else {
                None
            }
        };
        if let Some(blocker) = blocked {
            let _ = blocker.entered.send(());
            let _ = blocker.release.recv();
        }
    }
}

// Open buffers may name a file that has not been saved yet. Resolve its real
// ancestor once so overlays and captured entries share the same package identity.
fn normalized_open_path(path: &camino::Utf8Path) -> ReadResult<Utf8PathBuf> {
    let failure = |error: std::io::Error| {
        CompileError::new(format!("failed to resolve open document '{}': {error}", path), Span::default())
            .with_category(crate::error::CompileErrorCategory::Io)
            .with_file(path.to_owned())
            .with_source(error)
    };
    let mut ancestor = path;
    let mut suffix = Vec::new();
    loop {
        match std::fs::canonicalize(ancestor) {
            Ok(canonical) => {
                let mut canonical = Utf8PathBuf::from_path_buf(canonical).map_err(|path| {
                    CompileError::without_span(format!("open document path is not valid UTF-8: {}", path.display()))
                })?;
                for part in suffix.iter().rev() {
                    canonical.push(part);
                }
                return Ok(canonical);
            }
            Err(error) => {
                if error.kind() != std::io::ErrorKind::NotFound {
                    return Err(failure(error));
                }
                match std::fs::symlink_metadata(ancestor) {
                    Ok(metadata) if metadata.file_type().is_symlink() => return Err(failure(error)),
                    Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(failure(error)),
                    _ => {}
                }
                let Some(name) = ancestor.file_name() else { return Err(failure(error)) };
                let Some(parent) = ancestor.parent() else { return Err(failure(error)) };
                suffix.push(name.to_owned());
                ancestor = parent;
            }
        }
    }
}

struct ManifestInput {
    source: ReadResult<Arc<String>>,
    digest: [u8; 32],
    parsed: OnceLock<ReadResult<Arc<crate::PackageManifest>>>,
}

impl ManifestInput {
    fn package(&self, path: &camino::Utf8Path) -> ReadResult<Arc<crate::PackageManifest>> {
        self.parsed
            .get_or_init(|| {
                self.source.as_ref().map_err(Clone::clone).and_then(|source| crate::parse_package_manifest(path, source).map(Arc::new))
            })
            .clone()
    }
}

struct SourceRoot {
    package: ReadResult<Option<Utf8PathBuf>>,
    manifests: Vec<(Utf8PathBuf, Arc<ManifestInput>)>,
}

struct SourceInput {
    path: Utf8PathBuf,
    source: ReadResult<Arc<String>>,
    digest: [u8; 32],
    root: Arc<SourceRoot>,
    edition: ReadResult<crate::CellScriptEdition>,
    package_id: ReadResult<String>,
    ast: OnceLock<std::result::Result<Arc<Module>, Vec<CompileError>>>,
    #[cfg(test)]
    stats: Arc<ReadStats>,
}

impl SourceInput {
    fn ast(&self) -> std::result::Result<Arc<Module>, Vec<CompileError>> {
        self.ast
            .get_or_init(|| {
                let source = self.source.as_ref().map_err(|error| vec![error.clone()])?;
                let edition = self.edition.as_ref().map_err(|error| vec![error.clone()])?;
                #[cfg(test)]
                self.stats.source_parses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                crate::frontend::parse_diagnostics(source, *edition)
                    .map(Arc::new)
                    .map_err(|errors| errors.into_iter().map(|error| error.with_file(self.path.clone())).collect())
            })
            .clone()
    }
}

pub(super) struct SourceGroup {
    sources: BTreeMap<Utf8PathBuf, Arc<SourceInput>>,
    errors: Vec<CompileError>,
    digest: [u8; 32],
    modules: OnceLock<std::result::Result<Arc<Vec<crate::LoadedModule>>, Vec<CompileError>>>,
    resolver: OnceLock<Option<Arc<crate::ModuleResolver>>>,
}

impl SourceGroup {
    fn empty() -> Self {
        Self {
            sources: BTreeMap::new(),
            errors: vec![],
            digest: crate::ckb_blake2b256(&[]),
            modules: OnceLock::new(),
            resolver: OnceLock::new(),
        }
    }

    fn parsed_modules(&self) -> &std::result::Result<Arc<Vec<crate::LoadedModule>>, Vec<CompileError>> {
        self.modules.get_or_init(|| {
            let mut errors = self.errors.clone();
            let mut modules = Vec::with_capacity(self.sources.len());
            for input in self.sources.values() {
                match input.ast() {
                    Ok(ast) => modules.push(crate::LoadedModule {
                        path: input.path.clone(),
                        source: input.source.as_ref().expect("a parsed input has source bytes").as_ref().clone(),
                        ast: ast.as_ref().clone(),
                        edition: *input.edition.as_ref().expect("a parsed input has an edition"),
                    }),
                    Err(diagnostics) => errors.extend(diagnostics),
                }
            }
            if errors.is_empty() {
                Ok(Arc::new(modules))
            } else {
                Err(errors)
            }
        })
    }

    pub(super) fn modules(&self) -> Arc<Vec<crate::LoadedModule>> {
        self.parsed_modules().as_ref().cloned().unwrap_or_default()
    }

    pub(super) fn resolver(&self) -> Option<Arc<crate::ModuleResolver>> {
        self.resolver
            .get_or_init(|| {
                let modules = self.parsed_modules().as_ref().ok()?;
                crate::build_module_resolver_with_package_ids(modules, |module| {
                    self.sources
                        .get(&module.path)
                        .ok_or_else(|| CompileError::without_span("captured module has no package identity"))?
                        .package_id
                        .clone()
                })
                .ok()
                .map(Arc::new)
            })
            .clone()
    }

    pub(super) fn prepare_project(&self, entry: &camino::Utf8Path) -> std::result::Result<crate::LoadedProject, Vec<CompileError>> {
        let modules = self.parsed_modules().as_ref().map_err(Clone::clone)?;
        crate::prepare_loaded_project_for_entry_diagnostics(entry, modules.as_ref().clone(), |module| {
            self.sources
                .get(&module.path)
                .ok_or_else(|| CompileError::without_span("captured module has no package identity"))?
                .package_id
                .clone()
        })
    }
}

impl SourceGroup {
    fn diagnostic_report(
        &self,
        entry: &camino::Utf8Path,
        edition: crate::CellScriptEdition,
    ) -> crate::CompileMetadataDiagnosticReport {
        let manifest = (|| -> ReadResult<_> {
            let input = self.sources.get(entry).ok_or_else(|| CompileError::without_span("captured entry source is missing"))?;
            let root = input.root.package.clone()?;
            let manifest = match &root {
                Some(root) => {
                    let path = root.join("Cell.toml");
                    Some(
                        input
                            .root
                            .manifests
                            .iter()
                            .find(|(name, _)| name == &path)
                            .ok_or_else(|| CompileError::without_span("captured package manifest is missing"))?
                            .1
                            .package(&path)?,
                    )
                }
                None => None,
            };
            Ok((root, manifest))
        })();
        let (root, manifest) = match manifest {
            Ok(manifest) => manifest,
            Err(error) => return crate::CompileMetadataDiagnosticReport { metadata: None, diagnostics: vec![error] },
        };
        let project = match self.prepare_project(entry) {
            Ok(project) => project,
            Err(diagnostics) => return crate::CompileMetadataDiagnosticReport { metadata: None, diagnostics },
        };
        crate::compile_pipeline::diagnostics::compile_loaded_metadata_with_diagnostics(
            &project,
            &crate::CompileOptions { edition, ..Default::default() },
            manifest.as_deref(),
            |_| {
                self.sources
                    .values()
                    .map(|input| {
                        let source = input.source.as_ref().map_err(Clone::clone)?;
                        let role = if input.path == entry {
                            "entry"
                        } else if root.as_ref().is_some_and(|root| input.path.starts_with(root)) {
                            "package"
                        } else {
                            "dependency"
                        };
                        Ok(crate::SourceUnitMetadata {
                            path: input.path.to_string(),
                            role: role.into(),
                            hash: crate::hex_encode(&input.digest),
                            size_bytes: source.len(),
                        })
                    })
                    .collect()
            },
        )
    }
}

pub(super) type GroupCache = Arc<std::sync::Mutex<BTreeMap<[u8; 32], std::sync::Weak<SourceGroup>>>>;

pub(super) struct WorkspaceInputs {
    pub(super) entry: Option<Utf8PathBuf>,
    document: Arc<String>,
    pub(super) edition: crate::CellScriptEdition,
    pub(super) fingerprint: [u8; 32],
    pub(super) group: Arc<SourceGroup>,
}

impl WorkspaceInputs {
    pub(super) fn capture(server: &LspServer, uri: &str) -> Arc<Self> {
        WorkspaceReadBatch::new(server).capture(uri)
    }

    pub(super) fn entry_ast(&self) -> Option<std::result::Result<Arc<Module>, Vec<CompileError>>> {
        let input = self.group.sources.get(self.entry.as_deref()?)?;
        let source = input.source.as_ref().ok()?;
        if input.edition.is_err() || source != &self.document {
            return None;
        }
        Some(input.ast())
    }

    pub(super) fn diagnostics(&self) -> crate::CompileMetadataDiagnosticReport {
        match &self.entry {
            Some(entry) => self.group.diagnostic_report(entry, self.edition),
            None if !self.group.errors.is_empty() => {
                crate::CompileMetadataDiagnosticReport { metadata: None, diagnostics: self.group.errors.clone() }
            }
            None => crate::compile_metadata_with_diagnostics(&self.document, self.edition, None),
        }
    }
}

/// Byte, digest and discovery caches end with their observation. Reusing a
/// parsed group requires establishing its fingerprint from fresh inputs.
pub(super) struct WorkspaceReadBatch<'a> {
    server: &'a LspServer,
    overlays: BTreeMap<Utf8PathBuf, Arc<String>>,
    overrides: BTreeMap<Utf8PathBuf, Vec<Option<crate::CellScriptEdition>>>,
    conflicting_sources: BTreeSet<Utf8PathBuf>,
    sources: BTreeMap<Utf8PathBuf, Arc<SourceInput>>,
    manifests: BTreeMap<Utf8PathBuf, Arc<ManifestInput>>,
    roots: BTreeMap<Utf8PathBuf, Arc<SourceRoot>>,
    pub(super) source_sets: BTreeMap<Scope, Arc<BTreeSet<Utf8PathBuf>>>,
    discovery_errors: BTreeMap<Scope, CompileError>,
    groups: BTreeMap<(Scope, Option<Utf8PathBuf>), Arc<SourceGroup>>,
    documents: BTreeMap<String, Arc<WorkspaceInputs>>,
    #[cfg(test)]
    pub(super) source_reads: usize,
    #[cfg(test)]
    pub(super) manifest_reads: usize,
    #[cfg(test)]
    pub(super) source_hashes: usize,
}

impl<'a> WorkspaceReadBatch<'a> {
    pub(super) fn new(server: &'a LspServer) -> Self {
        #[cfg(test)]
        server.read_stats.observations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut overlays = BTreeMap::new();
        let mut overrides = BTreeMap::<_, Vec<_>>::new();
        let mut conflicting_sources = BTreeSet::new();
        for (uri, source) in &server.documents {
            if let Some(path) = file_uri_to_utf8_path(uri).and_then(|path| normalized_open_path(&path).ok()) {
                overrides.entry(path.clone()).or_default().push(server.edition_overrides.get(uri).copied());
                if let Some(previous) = overlays.insert(path.clone(), source.clone())
                    && previous != *source
                {
                    conflicting_sources.insert(path);
                }
            }
        }
        Self {
            server,
            overlays,
            overrides,
            conflicting_sources,
            sources: Default::default(),
            manifests: Default::default(),
            roots: Default::default(),
            source_sets: Default::default(),
            discovery_errors: Default::default(),
            groups: Default::default(),
            documents: Default::default(),
            #[cfg(test)]
            source_reads: 0,
            #[cfg(test)]
            manifest_reads: 0,
            #[cfg(test)]
            source_hashes: 0,
        }
    }

    fn manifest(&mut self, path: &camino::Utf8Path) -> Arc<ManifestInput> {
        if let Some(input) = self.manifests.get(path) {
            return input.clone();
        }
        #[cfg(test)]
        {
            self.manifest_reads += 1;
        }
        let bytes = std::fs::read(path);
        let digest = match &bytes {
            Ok(bytes) => crate::ckb_blake2b256(bytes),
            Err(error) => crate::ckb_blake2b256(error.to_string().as_bytes()),
        };
        let source = bytes
            .and_then(|bytes| {
                String::from_utf8(bytes)
                    .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "stream did not contain valid UTF-8"))
            })
            .map(Arc::new)
            .map_err(|error| CompileError::new(format!("failed to read manifest '{}': {}", path, error), Span::default()));
        let input = Arc::new(ManifestInput { source, digest, parsed: OnceLock::new() });
        self.manifests.insert(path.to_owned(), input.clone());
        input
    }

    fn root(&mut self, path: &camino::Utf8Path) -> Arc<SourceRoot> {
        let parent = path.parent().unwrap_or(path).to_owned();
        if let Some(root) = self.roots.get(&parent) {
            return root.clone();
        }
        let mut manifests = Vec::new();
        let mut package = Ok(None);
        let mut directory = Some(parent.as_path());
        while let Some(current) = directory {
            let path = current.join("Cell.toml");
            if path.exists() {
                let manifest = self.manifest(&path);
                let has_package = manifest
                    .source
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|source| crate::manifest_source_has_table(&path, source, "package"));
                manifests.push((path, manifest));
                match has_package {
                    Ok(true) => {
                        package = Ok(Some(current.to_owned()));
                        break;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        package = Err(error);
                        break;
                    }
                }
            }
            directory = current.parent();
        }
        let root = Arc::new(SourceRoot { package, manifests });
        self.roots.insert(parent, root.clone());
        root
    }

    fn source(&mut self, path: &camino::Utf8Path) -> Arc<SourceInput> {
        if let Some(input) = self.sources.get(path) {
            return input.clone();
        }
        let source = if self.conflicting_sources.contains(path) {
            Err(CompileError::without_span(format!("multiple open document URIs provide different sources for '{}'", path))
                .with_code(crate::error::DiagnosticCode::E1031)
                .with_file(path.to_owned()))
        } else if let Some(source) = self.overlays.get(path) {
            Ok(source.clone())
        } else {
            #[cfg(test)]
            {
                self.source_reads += 1;
                self.server.read_stats.source_reads.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            #[cfg(all(test, feature = "lsp", not(feature = "wasm")))]
            self.server.read_stats.before_read(path);
            crate::read_module_source(path).map(Arc::new)
        };
        #[cfg(test)]
        {
            self.source_hashes += 1;
            self.server.read_stats.source_hashes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let digest = match &source {
            Ok(source) => crate::ckb_blake2b256(source.as_bytes()),
            Err(error) => crate::ckb_blake2b256(error.message.as_bytes()),
        };
        let root = self.root(path);
        let edition = root
            .package
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|package| match package {
                Some(root) => {
                    self.manifest(&root.join("Cell.toml")).package(&root.join("Cell.toml")).map(|manifest| manifest.package.edition)
                }
                None => Ok(crate::CURRENT_EDITION),
            })
            .and_then(|edition| {
                let selected = self.overrides.get(path).and_then(|choices| choices.first()).copied().flatten().unwrap_or(edition);
                if self.overrides.get(path).is_some_and(|choices| choices.iter().any(|choice| choice.unwrap_or(edition) != selected)) {
                    return Err(CompileError::without_span(format!(
                        "multiple open document URIs select different editions for '{}'",
                        path
                    ))
                    .with_code(crate::error::DiagnosticCode::E1031)
                    .with_file(path.to_owned()));
                }
                Ok(selected)
            });
        let package_id = root
            .package
            .as_ref()
            .map_err(Clone::clone)
            .map(|root| crate::loaded_module_package_id_from_location(root.as_deref(), path.parent().unwrap_or(path)));
        let input = Arc::new(SourceInput {
            path: path.to_owned(),
            source,
            digest,
            root,
            edition,
            package_id,
            ast: OnceLock::new(),
            #[cfg(test)]
            stats: self.server.read_stats.clone(),
        });
        self.sources.insert(path.to_owned(), input.clone());
        input
    }

    fn group(&mut self, entry: &camino::Utf8Path) -> Arc<SourceGroup> {
        let root = self.root(entry);
        let standalone_overlay = matches!(root.package, Ok(None)) && self.overlays.contains_key(entry) && !entry.exists();
        let scope = match &root.package {
            _ if standalone_overlay => (entry.to_owned(), None),
            Ok(Some(root)) => (root.clone(), Some(entry.starts_with(root.join("tests")))),
            _ => (entry.parent().unwrap_or(entry).to_owned(), None),
        };
        if !self.source_sets.contains_key(&scope) {
            let paths = if standalone_overlay {
                Ok(BTreeSet::new())
            } else if let Some(test) = scope.1 {
                let dependency_scope =
                    if test { crate::package::DependencyScope::Test } else { crate::package::DependencyScope::Runtime };
                let mut paths = BTreeSet::new();
                crate::collect_package_source_paths_recursive(&scope.0, dependency_scope, &mut Default::default(), &mut paths)
                    .map(|_| paths)
            } else {
                crate::collect_cell_files(&scope.0).map(|paths| paths.into_iter().collect())
            };
            let paths = match paths {
                Ok(paths) => paths,
                Err(error) => {
                    self.discovery_errors.insert(scope.clone(), error);
                    BTreeSet::new()
                }
            };
            self.source_sets.insert(scope.clone(), Arc::new(paths));
        }
        let paths = self.source_sets[&scope].clone();
        let extra = (!paths.contains(entry)).then(|| entry.to_owned());
        let key = (scope.clone(), extra.clone());
        if let Some(group) = self.groups.get(&key) {
            return group.clone();
        }
        let mut sources = paths.iter().chain(extra.iter()).map(|path| self.source(path)).collect::<Vec<_>>();
        sources.sort_by(|left, right| left.path.cmp(&right.path));
        let errors = self.discovery_errors.get(&scope).cloned().into_iter().collect::<Vec<_>>();
        let mut digest = blake2b_simd::Params::new().hash_length(32).to_state();
        let mut hash = |bytes: &[u8]| {
            digest.update(&(bytes.len() as u64).to_le_bytes());
            digest.update(bytes);
        };
        let mut manifests = BTreeSet::new();
        for input in &sources {
            hash(input.path.as_str().as_bytes());
            hash(&[u8::from(input.source.is_ok())]);
            hash(&input.digest);
            match &input.edition {
                Ok(edition) => hash(edition.as_str().as_bytes()),
                Err(error) => hash(error.message.as_bytes()),
            }
            match &input.package_id {
                Ok(id) => hash(id.as_bytes()),
                Err(error) => hash(error.message.as_bytes()),
            }
            for (path, manifest) in &input.root.manifests {
                if manifests.insert(path.clone()) {
                    hash(path.as_str().as_bytes());
                    hash(&[u8::from(manifest.source.is_ok())]);
                    hash(&manifest.digest);
                }
            }
            if let Ok(Some(root)) = &input.root.package {
                let path = root.join("Cell.lock");
                if manifests.insert(path.clone()) {
                    let manifest = self.manifest(&path);
                    hash(path.as_str().as_bytes());
                    hash(&[u8::from(manifest.source.is_ok())]);
                    hash(&manifest.digest);
                }
            }
        }
        for error in &errors {
            hash(error.message.as_bytes());
        }
        let digest = digest.finalize().as_bytes().try_into().expect("32-byte hash");
        // A fresh byte observation can reuse immutable parsed graphs only
        // after establishing the complete same source/manifest fingerprint.
        let group = {
            let mut cache = self.server.source_groups.lock().unwrap();
            cache.retain(|_, group| group.strong_count() != 0);
            if let Some(group) = cache.get(&digest).and_then(std::sync::Weak::upgrade) {
                group
            } else {
                let group = Arc::new(SourceGroup {
                    sources: sources.into_iter().map(|input| (input.path.clone(), input)).collect(),
                    errors,
                    digest,
                    modules: OnceLock::new(),
                    resolver: OnceLock::new(),
                });
                cache.insert(digest, Arc::downgrade(&group));
                group
            }
        };
        self.groups.insert(key, group.clone());
        group
    }

    pub(super) fn capture(&mut self, uri: &str) -> Arc<WorkspaceInputs> {
        if let Some(inputs) = self.documents.get(uri) {
            return inputs.clone();
        }
        // Browser language-service documents have no native filesystem authority.
        let raw_entry = if cfg!(target_arch = "wasm32") { None } else { file_uri_to_utf8_path(uri) };
        let resolved = raw_entry.as_ref().map(|path| normalized_open_path(path)).transpose();
        let (entry, group) = match resolved {
            Ok(Some(entry)) => {
                let group = self.group(&entry);
                (Some(entry), group)
            }
            Ok(None) => (None, Arc::new(SourceGroup::empty())),
            Err(error) => {
                let mut group = SourceGroup::empty();
                group.digest = crate::ckb_blake2b256(error.message.as_bytes());
                group.errors.push(error);
                (None, Arc::new(group))
            }
        };
        let edition = self
            .server
            .edition_overrides
            .get(uri)
            .copied()
            .or_else(|| entry.as_ref().map(|entry| self.source(entry)).and_then(|input| input.edition.clone().ok()))
            .or_else(|| {
                let root = self.root(raw_entry.as_deref()?);
                let root = root.package.as_ref().ok()?.as_ref()?;
                self.manifest(&root.join("Cell.toml")).package(&root.join("Cell.toml")).ok().map(|manifest| manifest.package.edition)
            })
            .unwrap_or(crate::CURRENT_EDITION);
        let source = self.server.documents.get(uri);
        let entry_digest = entry
            .as_ref()
            .and_then(|entry| self.sources.get(entry))
            .filter(|input| {
                input
                    .source
                    .as_ref()
                    .ok()
                    .zip(source)
                    .is_some_and(|(captured, document)| Arc::ptr_eq(captured, document) || captured == document)
            })
            .map(|input| input.digest)
            .unwrap_or_else(|| crate::ckb_blake2b256(source.map_or("", |source| source.as_str()).as_bytes()));
        let mut state = blake2b_simd::Params::new().hash_length(32).to_state();
        state.update(&entry_digest);
        state.update(edition.as_str().as_bytes());
        state.update(&group.digest);
        let document = source.cloned().unwrap_or_else(|| Arc::new(String::new()));
        let inputs = Arc::new(WorkspaceInputs {
            entry,
            document,
            edition,
            fingerprint: state.finalize().as_bytes().try_into().expect("32-byte hash"),
            group,
        });
        self.documents.insert(uri.into(), inputs.clone());
        inputs
    }
}
