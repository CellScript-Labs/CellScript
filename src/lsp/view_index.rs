//! Lexical view types for one document and its resolved workspace snapshot.
//! Each statement is visited once. Unknown shadowing bindings mask outer types.
use super::workspace::WorkspaceInputs;
#[cfg(any(test, all(feature = "lsp", not(feature = "wasm"))))]
use super::workspace::WorkspaceReadBatch;
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
    resolver: Option<Arc<crate::ModuleResolver>>,
    owner: String,
    pub(super) modules: Arc<Vec<crate::LoadedModule>>,
    pub(super) group: Arc<workspace::SourceGroup>,
    pub(super) inputs: Arc<WorkspaceInputs>,
    pub(super) tokens: Vec<crate::lexer::token::Token>,
}

impl ViewIndex {
    fn new(source: &str, module: &Module, inputs: &Arc<WorkspaceInputs>) -> Self {
        use crate::resolve::TypeDef;
        let group = &inputs.group;
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
            resolver: group.resolver(),
            owner: module.name.clone(),
            modules: group.modules(),
            group: group.clone(),
            inputs: inputs.clone(),
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
                Stmt::Expr(_) | Stmt::Discard(_) | Stmt::Return(_) | Stmt::Break(_) | Stmt::Continue(_) => {}
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

impl LspServer {
    #[cfg(all(feature = "lsp", not(feature = "wasm")))]
    pub(super) fn prepare_navigation_snapshot(&mut self, uri: &str) -> [u8; 32] {
        let inputs = WorkspaceInputs::capture(self, uri);
        let revision = inputs.fingerprint;
        self.observed_inputs = Some(Arc::new(std::collections::BTreeMap::from([(uri.into(), inputs)])));
        self.prepare_navigation_for(uri);
        revision
    }

    #[cfg(all(feature = "lsp", not(feature = "wasm")))]
    pub(super) fn document_revision(&self, uri: &str) -> [u8; 32] {
        WorkspaceInputs::capture(self, uri).fingerprint
    }

    #[cfg(all(feature = "lsp", not(feature = "wasm")))]
    pub(super) fn dependency_revision(&self) -> Vec<(String, [u8; 32])> {
        let mut batch = WorkspaceReadBatch::new(self);
        let mut revisions = self.documents.keys().map(|uri| (uri.clone(), batch.capture(uri).fingerprint)).collect::<Vec<_>>();
        revisions.sort_by(|left, right| left.0.cmp(&right.0));
        revisions
    }

    #[cfg(all(feature = "lsp", not(feature = "wasm")))]
    pub(super) fn prepare_diagnostic_snapshot(&mut self) -> Vec<(String, [u8; 32])> {
        let inputs = {
            let mut batch = WorkspaceReadBatch::new(self);
            self.documents.keys().map(|uri| (uri.clone(), batch.capture(uri))).collect::<std::collections::BTreeMap<_, _>>()
        };
        let revision = inputs.iter().map(|(uri, inputs)| (uri.clone(), inputs.fingerprint)).collect();
        self.observed_inputs = Some(Arc::new(inputs));
        revision
    }

    pub(super) fn view_index(&self, uri: &str) -> Option<Arc<ViewIndex>> {
        let source = self.documents.get(uri)?;
        let module = self.ast_cache.get(uri)?;
        let inputs = self
            .observed_inputs
            .as_ref()
            .and_then(|inputs| inputs.get(uri).cloned())
            .unwrap_or_else(|| WorkspaceInputs::capture(self, uri));
        if let Some((key, index)) = self.view_cache.lock().unwrap().get(uri)
            && key == &inputs.fingerprint
        {
            return Some(index.clone());
        }
        let index = Arc::new(ViewIndex::new(source, module, &inputs));
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
        server.open_document(uri.into(), "module original;\naction check() {  require true; }".into());
        let snapshot = server.clone();
        assert!(Arc::ptr_eq(&server.documents[uri], &snapshot.documents[uri]));
        assert!(Arc::ptr_eq(&server.ast_cache[uri], &snapshot.ast_cache[uri]));
        assert!(Arc::ptr_eq(&server.diagnostics[uri], &snapshot.diagnostics[uri]));
        server.replace_buffer(uri.into(), "module changed;".into());
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
        std::fs::write(&dependency, "module schema;\npublic struct Entry { revision: u64, payload: Vec<u8>, }").unwrap();
        for i in 1..16 {
            std::fs::write(src.join(format!("extra{i}.cell")), format!("module extra{i};\npublic struct Extra{i} {{ value: u64, }}"))
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
        std::fs::write(&dependency, "module schema;\npublic struct Entry { revision: u32, payload: Vec<u8>, }").unwrap();
        let disk = server.view_index(&uri).unwrap();
        assert!(!Arc::ptr_eq(&first, &disk));
        server
            .replace_buffer(dependency_uri.clone(), "module schema;\npublic struct Entry { unsaved: u64, payload: Vec<u8>, }".into());
        server.prepare_navigation();
        let unsaved = server.view_index(&uri).unwrap();
        let crate::resolve::TypeDef::Struct(record) = unsaved.resolve_type("Entry").unwrap() else { panic!("record") };
        assert_eq!(record.fields[0].name, "unsaved");
        server.replace_buffer(dependency_uri.clone(), "module schema;\npublic struct Entry {".into());
        server.prepare_navigation();
        assert!(server.view_index(&uri).unwrap().resolve_type("Entry").is_none());
        server.close_document(&dependency_uri);
        let crate::resolve::TypeDef::Struct(record) = server.view_index(&uri).unwrap().resolve_type("Entry").unwrap() else {
            panic!("record")
        };
        assert_eq!(record.fields[0].name, "revision");
        assert_eq!(record.fields[0].ty, Type::U32);
    }

    #[test]
    fn revision_batches_read_shared_dependency_sources_once_and_do_not_outlive_the_pass() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(
            root.path().join("Cell.toml"),
            "[package]\nname = \"batch_reads\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\n",
        )
        .unwrap();
        let mut server = LspServer::new();
        for index in 0..18 {
            let path = src.join(format!("unit{index}.cell"));
            let source = format!("module unit{index};\npublic struct Value{index} {{ amount: u64, }}");
            std::fs::write(&path, &source).unwrap();
            if index < 4 {
                server.replace_buffer(utf8_path_to_file_uri(camino::Utf8Path::from_path(&path).unwrap()), source);
            }
        }
        let mut batch = WorkspaceReadBatch::new(&server);
        let mut before = Vec::new();
        for uri in server.documents.keys() {
            before.push((uri.clone(), batch.capture(uri).fingerprint));
        }
        assert_eq!(batch.source_reads, 14, "open overlays are not read from disk; dependencies are read once per pass");
        assert_eq!(batch.source_hashes, 18, "shared source bytes must be hashed once per observation");
        assert_eq!(batch.manifest_reads, 2, "one manifest and one missing-lockfile observation");
        assert_eq!(batch.source_sets.len(), 1, "one package/scope discovery for four documents");
        let dependency = src.join("unit17.cell");
        let source = std::fs::read_to_string(&dependency).unwrap();
        std::fs::write(&dependency, source.replace("u64", "u32")).unwrap();
        for (uri, old) in before {
            assert_eq!(batch.capture(&uri).fingerprint, old, "the same observation is immutable");
            assert_ne!(WorkspaceInputs::capture(&server, &uri).fingerprint, old, "next observation must see a same-length edit");
        }
    }

    #[test]
    fn fresh_byte_observations_share_the_same_immutable_module_graph() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(
            root.path().join("Cell.toml"),
            "[package]\nname = \"shared_graph\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\n",
        )
        .unwrap();
        let mut server = LspServer::new();
        let mut uris = Vec::new();
        for index in 0..4 {
            let path = src.join(format!("unit{index}.cell"));
            let source = format!("module unit{index};\npublic struct Value{index} {{ amount: u64, }}");
            std::fs::write(&path, &source).unwrap();
            let uri = utf8_path_to_file_uri(camino::Utf8Path::from_path(&path).unwrap());
            if index < 2 {
                server.replace_buffer(uri.clone(), source);
            }
            uris.push(uri);
        }
        let mut batch = WorkspaceReadBatch::new(&server);
        let first = batch.capture(&uris[0]);
        let second = batch.capture(&uris[1]);
        assert!(Arc::ptr_eq(&first.group, &second.group));
        assert!(Arc::ptr_eq(&first.group.modules(), &second.group.modules()));
        assert!(Arc::ptr_eq(&first.group.resolver().unwrap(), &second.group.resolver().unwrap()));
        let mut fresh = WorkspaceReadBatch::new(&server);
        let current = fresh.capture(&uris[1]);
        assert_eq!(fresh.source_reads, 2, "graph reuse must not skip fresh disk reads");
        assert_eq!(fresh.source_hashes, 4);
        assert!(Arc::ptr_eq(&first.group, &current.group));
        std::fs::write(src.join("unit3.cell"), "module unit3;\npublic struct Value3 { amount: u32, }").unwrap();
        let changed = WorkspaceInputs::capture(&server, &uris[1]);
        assert!(!Arc::ptr_eq(&first.group, &changed.group));
        assert_ne!(current.fingerprint, changed.fingerprint);
    }
}
