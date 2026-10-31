//! Lexical view types for one document and its resolved workspace snapshot.
//! Each statement is visited once. Unknown shadowing bindings mask outer types.
use super::*;
use std::sync::Arc;

pub(super) type ViewCache = Arc<std::sync::Mutex<HashMap<String, ([u8; 32], Arc<ViewIndex>)>>>;

struct Binding {
    start: usize,
    end: usize,
    ty: Option<String>,
}

pub(super) struct ViewIndex {
    bindings: HashMap<String, Vec<Binding>>,
    catalogs: HashMap<String, CellRolesDef>,
    local_types: HashMap<String, crate::resolve::TypeDef>,
    resolver: Option<crate::ModuleResolver>,
    owner: String,
    pub(super) modules: Vec<crate::LoadedModule>,
    pub(super) tokens: Vec<crate::lexer::token::Token>,
}

impl ViewIndex {
    fn new(source: &str, module: &Module, modules: &[crate::LoadedModule]) -> Self {
        use crate::resolve::TypeDef;
        let mut index = Self {
            bindings: HashMap::new(),
            catalogs: module
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::CellRoles(def) => Some((def.name.clone(), def.clone())),
                    _ => None,
                })
                .collect(),
            local_types: module
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Struct(def) => Some((def.name.clone(), TypeDef::Struct(def.clone()))),
                    Item::Resource(def) => Some((def.name.clone(), TypeDef::Resource(def.clone()))),
                    Item::Shared(def) => Some((def.name.clone(), TypeDef::Shared(def.clone()))),
                    Item::Receipt(def) => Some((def.name.clone(), TypeDef::Receipt(def.clone()))),
                    _ => None,
                })
                .collect(),
            resolver: crate::build_module_resolver_from_loaded_modules(modules).ok(),
            owner: module.name.clone(),
            modules: modules.to_vec(),
            tokens: crate::lexer::lex(source).unwrap_or_default(),
        };
        for item in &module.items {
            let (params, body, span) = match item {
                Item::Action(def) => (&def.params, &def.body, def.span),
                Item::Function(def) => (&def.params, &def.body, def.span),
                Item::Lock(def) => (&def.params, &def.body, def.span),
                _ => continue,
            };
            let mut scope = HashMap::new();
            for param in params {
                let ty = Some(super::cell_views::shape_type(&type_to_string(&param.ty), module));
                index.bind(&param.name, span.start, span.end, ty.clone());
                scope.insert(param.name.clone(), ty);
            }
            index.block(body, span.end, &mut scope, module);
        }
        index
    }

    pub(super) fn resolve_type(&self, name: &str) -> Option<crate::resolve::TypeDef> {
        self.local_types.get(name).cloned().or_else(|| self.resolver.as_ref()?.resolve_type(&self.owner, name))
    }

    fn record_fields(&self, name: &str) -> Option<Vec<Field>> {
        let crate::resolve::TypeDef::Struct(record) = self.resolve_type(name)? else { return None };
        Some(record.fields)
    }

    fn bind(&mut self, name: &str, start: usize, end: usize, ty: Option<String>) {
        self.bindings.entry(name.into()).or_default().push(Binding { start, end, ty });
    }

    fn pattern(
        &mut self,
        pattern: &BindingPattern,
        start: usize,
        end: usize,
        ty: Option<String>,
        scope: &mut HashMap<String, Option<String>>,
    ) {
        match pattern {
            BindingPattern::Name(name) => {
                self.bind(name, start, end, ty.clone());
                scope.insert(name.clone(), ty);
            }
            BindingPattern::Tuple(parts) => {
                for part in parts {
                    self.pattern(part, start, end, None, scope);
                }
            }
            BindingPattern::Wildcard => {}
        }
    }

    fn block(&mut self, body: &[Stmt], end: usize, scope: &mut HashMap<String, Option<String>>, module: &Module) {
        for stmt in body {
            match stmt {
                Stmt::Let(binding) => {
                    let ty = binding
                        .ty
                        .as_ref()
                        .map(|ty| super::cell_views::shape_type(&type_to_string(ty), module))
                        .or_else(|| self.expression(&binding.value, &|name| scope.get(name).cloned().flatten()));
                    self.pattern(&binding.pattern, binding.span.end, end, ty, scope);
                }
                Stmt::If(branch) => {
                    for body in std::iter::once(&branch.then_branch).chain(branch.else_branch.iter()) {
                        let mut child = scope.clone();
                        let end = body.last().map_or(branch.span.end, |stmt| stmt_span(stmt).end);
                        self.block(body, end, &mut child, module);
                    }
                }
                Stmt::While(loop_) => self.block(&loop_.body, loop_.span.end, &mut scope.clone(), module),
                Stmt::For(loop_) => {
                    let ty = self
                        .expression(&loop_.iterable, &|name| scope.get(name).cloned().flatten())
                        .and_then(|ty| super::cell_views::nested_element(&ty));
                    let mut child = scope.clone();
                    let start = loop_.body.first().map_or(loop_.span.end, |stmt| stmt_span(stmt).start);
                    self.pattern(&loop_.pattern, start, loop_.span.end, ty, &mut child);
                    self.block(&loop_.body, loop_.span.end, &mut child, module);
                }
                Stmt::Borrow(borrow) => {
                    let mut child = scope.clone();
                    let start = borrow.body.first().map_or(borrow.span.end, |stmt| stmt_span(stmt).start);
                    self.pattern(&BindingPattern::Name(borrow.binding.clone()), start, borrow.span.end, None, &mut child);
                    self.block(&borrow.body, borrow.span.end, &mut child, module);
                }
                Stmt::Expr(_) | Stmt::Return(_) | Stmt::Break(_) | Stmt::Continue(_) => {}
            }
        }
    }

    fn expression(&self, expr: &Expr, binding: &dyn Fn(&str) -> Option<String>) -> Option<String> {
        super::cell_views::expression_type(expr, binding, &self.catalogs, &|name| self.record_fields(name))
    }

    pub(super) fn expression_at(&self, offset: usize, expr: &Expr) -> Option<String> {
        self.expression(expr, &|name| {
            self.bindings.get(name)?.iter().rev().find(|binding| binding.start <= offset && offset <= binding.end)?.ty.clone()
        })
    }
}

/// Immutable source inputs read outside the transport lock. Parsing and
/// resolution consume this overlay; invalid buffers cannot fall back to disk.
struct WorkspaceInputs {
    entry: Option<Utf8PathBuf>,
    sources: std::collections::BTreeMap<Utf8PathBuf, String>,
    fingerprint: [u8; 32],
}

impl WorkspaceInputs {
    fn capture(server: &LspServer, uri: &str) -> Self {
        let entry = file_uri_to_utf8_path(uri);
        let overlays = server.source_overlays();
        let paths = entry.as_ref().and_then(|path| crate::collect_source_paths_for_compile_file(path).ok()).unwrap_or_default();
        let mut sources = std::collections::BTreeMap::new();
        let mut roots = std::collections::BTreeSet::new();
        let mut state = blake2b_simd::Params::new().hash_length(32).to_state();
        let mut hash = |bytes: &[u8]| {
            state.update(&(bytes.len() as u64).to_le_bytes());
            state.update(bytes);
        };
        hash(server.documents.get(uri).map_or("", |source| source.as_str()).as_bytes());
        hash(server.document_edition(uri).as_str().as_bytes());
        for path in paths {
            hash(path.as_str().as_bytes());
            let source = overlays.get(&path).cloned().map(Ok).unwrap_or_else(|| std::fs::read_to_string(&path));
            match source {
                Ok(source) => {
                    hash(source.as_bytes());
                    sources.insert(path.clone(), source);
                }
                Err(error) => hash(error.to_string().as_bytes()),
            }
            if let Ok(Some(root)) = crate::find_package_root(&path) {
                roots.insert(root);
            }
        }
        for root in roots {
            for name in ["Cell.toml", "Cell.lock"] {
                hash(root.join(name).as_str().as_bytes());
                match std::fs::read(root.join(name)) {
                    Ok(bytes) => hash(&bytes),
                    Err(_) => hash(b"unavailable"),
                }
            }
        }
        Self { entry, sources, fingerprint: state.finalize().as_bytes().try_into().expect("32-byte BLAKE2b output") }
    }

    fn modules(&self) -> Vec<crate::LoadedModule> {
        self.entry
            .as_ref()
            .and_then(|path| crate::load_project_modules_for_entry_diagnostics(path, &self.sources).ok())
            .unwrap_or_default()
    }
}

impl LspServer {
    #[cfg(all(feature = "lsp", not(feature = "wasm")))]
    pub(super) fn document_revision(&self, uri: &str) -> [u8; 32] {
        WorkspaceInputs::capture(self, uri).fingerprint
    }

    #[cfg(all(feature = "lsp", not(feature = "wasm")))]
    pub(super) fn dependency_revision(&self) -> Vec<(String, [u8; 32])> {
        let mut revisions =
            self.documents.keys().map(|uri| (uri.clone(), WorkspaceInputs::capture(self, uri).fingerprint)).collect::<Vec<_>>();
        revisions.sort_by(|left, right| left.0.cmp(&right.0));
        revisions
    }

    pub(super) fn view_index(&self, uri: &str) -> Option<Arc<ViewIndex>> {
        let source = self.documents.get(uri)?;
        let module = self.ast_cache.get(uri)?;
        let inputs = WorkspaceInputs::capture(self, uri);
        if let Some((key, index)) = self.view_cache.lock().unwrap().get(uri)
            && key == &inputs.fingerprint
        {
            return Some(index.clone());
        }
        let index = Arc::new(ViewIndex::new(source, module, &inputs.modules()));
        self.view_cache.lock().unwrap().insert(uri.into(), (inputs.fingerprint, index.clone()));
        Some(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_share_immutable_buffers_trees_and_diagnostics() {
        let mut server = LspServer::new();
        let uri = "untitled:snapshot";
        server.open_document(uri.into(), "module original\naction check() { verification require true }".into());
        let snapshot = server.clone();
        assert!(Arc::ptr_eq(&server.documents[uri], &snapshot.documents[uri]));
        assert!(Arc::ptr_eq(&server.ast_cache[uri], &snapshot.ast_cache[uri]));
        assert!(Arc::ptr_eq(&server.diagnostics[uri], &snapshot.diagnostics[uri]));
        server.replace_buffer(uri.into(), "module changed".into());
        server.prepare_navigation_for(uri);
        assert_eq!(snapshot.ast_cache[uri].name, "original");
        assert_eq!(server.ast_cache[uri].name, "changed");
        assert!(!Arc::ptr_eq(&server.documents[uri], &snapshot.documents[uri]));
        assert!(!Arc::ptr_eq(&server.ast_cache[uri], &snapshot.ast_cache[uri]));
    }

    #[test]
    fn role_index_reuses_bindings_and_tracks_disk_and_unsaved_dependencies() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(
            root.path().join("Cell.toml"),
            "[package]\nname = \"role_latency\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\n",
        )
        .unwrap();
        let dependency = src.join("schema.cell");
        std::fs::write(&dependency, "module schema\npublic struct Entry { revision: u64, payload: Vec<u8> }").unwrap();
        for i in 1..16 {
            std::fs::write(src.join(format!("extra{i}.cell")), format!("module extra{i}\npublic struct Extra{i} {{ value: u64 }}"))
                .unwrap();
        }
        let source = include_str!("../../tests/lsp_responsiveness/fixtures/role_references.cell");
        let path = src.join("main.cell");
        std::fs::write(&path, source).unwrap();
        let uri = utf8_path_to_file_uri(camino::Utf8Path::from_path(&path).unwrap());
        let dependency_uri = utf8_path_to_file_uri(camino::Utf8Path::from_path(&dependency).unwrap());
        let mut server = LspServer::new();
        server.replace_buffer(uri.clone(), source.into());
        server.prepare_navigation();
        let first = server.view_index(&uri).unwrap();
        assert_eq!(first.modules.len(), 17);
        assert_eq!(first.bindings.values().map(Vec::len).sum::<usize>(), 259);
        let position = offset_to_position(source, source.find("orders: Order").unwrap());
        for _ in 0..2 {
            let references = server.find_references(&uri, position);
            assert_eq!(references.len(), 129);
            for reference in references {
                let start = position_to_offset(source, reference.range.start).unwrap();
                let end = position_to_offset(source, reference.range.end).unwrap();
                assert_eq!(&source[start..end], "orders");
            }
            assert!(Arc::ptr_eq(&first, &server.view_index(&uri).unwrap()));
        }
        // Same-length disk edits must invalidate without relying on mtimes.
        std::fs::write(&dependency, "module schema\npublic struct Entry { revision: u32, payload: Vec<u8> }").unwrap();
        let disk = server.view_index(&uri).unwrap();
        assert!(!Arc::ptr_eq(&first, &disk));
        server.replace_buffer(dependency_uri.clone(), "module schema\npublic struct Entry { unsaved: u64, payload: Vec<u8> }".into());
        server.prepare_navigation();
        let unsaved = server.view_index(&uri).unwrap();
        let crate::resolve::TypeDef::Struct(record) = unsaved.resolve_type("Entry").unwrap() else { panic!("record") };
        assert_eq!(record.fields[0].name, "unsaved");
        server.replace_buffer(dependency_uri.clone(), "module schema\npublic struct Entry {".into());
        server.prepare_navigation();
        assert!(server.view_index(&uri).unwrap().resolve_type("Entry").is_none());
        server.close_document(&dependency_uri);
        let crate::resolve::TypeDef::Struct(record) = server.view_index(&uri).unwrap().resolve_type("Entry").unwrap() else {
            panic!("record")
        };
        assert_eq!(record.fields[0].name, "revision");
        assert_eq!(record.fields[0].ty, Type::U32);
    }
}
