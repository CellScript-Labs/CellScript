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
    declaration: Span,
}

#[derive(Clone, Copy)]
struct BindingSite {
    start: usize,
    end: usize,
    declaration: Span,
    head_start: usize,
    head_end: usize,
}

pub(super) struct ViewIndex {
    bindings: HashMap<String, Vec<Binding>>,
    definitions: HashMap<String, Vec<(Span, Span)>>,
    blocks: Vec<Span>,
    #[cfg(feature = "collection-explanation")]
    pub(super) complete_syntax: bool,
    catalogs: HashMap<String, CellRolesDef>,
    local_types: HashMap<String, crate::resolve::TypeDef>,
    resolver: Option<Arc<crate::ModuleResolver>>,
    owner: String,
    pub(super) modules: Arc<Vec<crate::LoadedModule>>,
    pub(super) group: Arc<workspace::SourceGroup>,
    pub(super) inputs: Arc<WorkspaceInputs>,
    #[cfg(feature = "collection-explanation")]
    pub(super) collection_storage: std::sync::OnceLock<crate::error::Result<Arc<crate::collection_explanation::Report>>>,
    pub(super) tokens: Vec<crate::lexer::token::Token>,
}

impl ViewIndex {
    fn new(source: &str, module: &Module, inputs: &Arc<WorkspaceInputs>) -> Self {
        use crate::resolve::TypeDef;
        let group = &inputs.group;
        let parsed = crate::frontend::parse_document(source, inputs.edition);
        #[cfg(feature = "collection-explanation")]
        let complete_syntax = parsed.is_ok();
        let (tokens, blocks) = match parsed {
            Ok(document) => (
                document.syntax.tokens,
                document
                    .syntax
                    .nodes
                    .into_iter()
                    .filter(|node| node.kind == crate::syntax::SyntaxKind::Block)
                    .map(|node| node.span)
                    .collect(),
            ),
            Err(_) => (crate::lexer::lex(source).unwrap_or_default(), Vec::new()),
        };
        let resolver = group.resolver();
        let normalized = resolver.as_deref().and_then(|resolver| crate::source_bounds::resolve_policies(module, resolver).ok());
        let module = normalized.as_ref().unwrap_or(module);
        let mut index = Self {
            bindings: HashMap::new(),
            definitions: HashMap::new(),
            blocks,
            #[cfg(feature = "collection-explanation")]
            complete_syntax,
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
            resolver,
            owner: module.name.clone(),
            modules: group.modules(),
            group: group.clone(),
            inputs: inputs.clone(),
            #[cfg(feature = "collection-explanation")]
            collection_storage: std::sync::OnceLock::new(),
            tokens,
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
                let body_start = index
                    .blocks
                    .iter()
                    .filter(|block| span.start <= block.start && block.end <= span.end)
                    .min_by_key(|block| block.start)
                    .map_or(span.start, |block| block.start);
                index.bind(
                    &param.name,
                    BindingSite {
                        start: body_start,
                        end: span.end,
                        declaration: param.span,
                        head_start: param.span.start,
                        head_end: param.span.end,
                    },
                    ty.clone(),
                );
                scope.insert(param.name.clone(), ty);
            }
            index.block(body, span.end, &mut scope, module);
        }
        index
    }

    pub(super) fn resolve_type(&self, name: &str) -> Option<crate::resolve::TypeDef> {
        self.local_types.get(name).cloned().or_else(|| self.resolver.as_ref()?.resolve_type(&self.owner, name))
    }

    fn bind(&mut self, name: &str, site: BindingSite, ty: Option<String>) {
        self.bindings.entry(name.into()).or_default().push(Binding {
            start: site.start,
            end: site.end,
            ty,
            declaration: site.declaration,
        });
        if let Some(token) = self.tokens.iter().find(|token| {
            token.span.start >= site.head_start
                && token.span.end <= site.head_end
                && matches!(&token.kind, TokenKind::Identifier(value) if value == name)
        }) {
            self.definitions.entry(name.into()).or_default().push((token.span, site.declaration));
        }
    }

    pub(super) fn binding_declaration(&self, name: &str, offset: usize) -> Option<Span> {
        if let Some(declarations) = self.definitions.get(name)
            && let Some((_, declaration)) = declarations.iter().find(|(span, _)| span.start <= offset && offset < span.end)
        {
            return Some(*declaration);
        }
        self.bindings
            .get(name)?
            .iter()
            .rev()
            .find(|binding| binding.start <= offset && offset <= binding.end)
            .map(|binding| binding.declaration)
    }

    fn head_boundary(&self, span: Span, kind: TokenKind, after: bool) -> usize {
        self.tokens
            .iter()
            .find(|token| span.start <= token.span.start && token.span.end <= span.end && token.kind == kind)
            .map_or(span.start, |token| if after { token.span.end } else { token.span.start })
    }

    fn enclosing_end(&self, span: Span, fallback: usize) -> usize {
        self.blocks
            .iter()
            .filter(|block| block.start <= span.start && span.end <= block.end)
            .min_by_key(|block| block.end - block.start)
            .map_or(fallback, |block| block.end)
    }

    fn body_start(&self, span: Span, fallback: usize) -> usize {
        self.blocks
            .iter()
            .filter(|block| span.start < block.start && block.end == span.end)
            .min_by_key(|block| block.start)
            .map_or(fallback, |block| block.start)
    }

    fn binding_tail_start(&self, span: Span, name: &str, before: usize) -> usize {
        self.tokens
            .iter()
            .rev()
            .find(|token| {
                span.start <= token.span.start
                    && token.span.end <= before
                    && matches!(&token.kind,TokenKind::Identifier(value) if value==name)
            })
            .map_or(span.start, |token| token.span.start)
    }

    fn pattern(
        &mut self,
        pattern: &BindingPattern,
        site: BindingSite,
        ty: Option<String>,
        scope: &mut HashMap<String, Option<String>>,
    ) {
        match pattern {
            BindingPattern::Name(name) => {
                self.bind(name, site, ty.clone());
                scope.insert(name.clone(), ty);
            }
            BindingPattern::Tuple(parts) => {
                for part in parts {
                    self.pattern(part, site, None, scope);
                }
            }
            BindingPattern::Wildcard => {}
        }
    }

    fn block(&mut self, body: &[Stmt], end: usize, scope: &mut HashMap<String, Option<String>>, module: &Module) {
        for stmt in body {
            match stmt {
                Stmt::Lifecycle(value) => {
                    for expression in value.value_exprs() {
                        self.expression_bindings(expression, scope, module);
                    }
                }
                Stmt::Let(binding) => {
                    let ty = binding
                        .ty
                        .as_ref()
                        .map(|ty| super::cell_views::shape_type(&type_to_string(ty), module))
                        .or_else(|| self.expression(&binding.value, &|name| scope.get(name).cloned().flatten()));
                    self.expression_bindings(&binding.value, scope, module);
                    self.pattern(
                        &binding.pattern,
                        BindingSite {
                            start: binding.span.end,
                            end: self.enclosing_end(binding.span, end),
                            declaration: binding.span,
                            head_start: self.head_boundary(binding.span, TokenKind::Let, true),
                            head_end: self.head_boundary(binding.span, TokenKind::Eq, false),
                        },
                        ty,
                        scope,
                    );
                }
                Stmt::If(branch) => {
                    self.expression_bindings(&branch.condition, scope, module);
                    for body in std::iter::once(&branch.then_branch).chain(branch.else_branch.iter()) {
                        let mut child = scope.clone();
                        let end = body.last().map_or(branch.span.end, |stmt| stmt_span(stmt).end);
                        self.block(body, end, &mut child, module);
                    }
                }
                Stmt::While(loop_) => {
                    self.expression_bindings(&loop_.condition, scope, module);
                    self.block(&loop_.body, loop_.span.end, &mut scope.clone(), module);
                }
                Stmt::For(loop_) => {
                    let ty = self
                        .expression(&loop_.iterable, &|name| scope.get(name).cloned().flatten())
                        .and_then(|ty| super::cell_views::nested_element(&ty));
                    let mut child = scope.clone();
                    let start = loop_.body.first().map_or(loop_.span.end, |stmt| stmt_span(stmt).start);
                    self.expression_bindings(&loop_.iterable, scope, module);
                    self.pattern(
                        &loop_.pattern,
                        BindingSite {
                            start: self.body_start(loop_.span, start),
                            end: loop_.span.end,
                            declaration: loop_.span,
                            head_start: self.head_boundary(loop_.span, TokenKind::For, true),
                            head_end: self.head_boundary(loop_.span, TokenKind::In, false),
                        },
                        ty,
                        &mut child,
                    );
                    self.block(&loop_.body, loop_.span.end, &mut child, module);
                }
                Stmt::Borrow(borrow) => {
                    let mut child = scope.clone();
                    let start = borrow.body.first().map_or(borrow.span.end, |stmt| stmt_span(stmt).start);
                    self.pattern(
                        &BindingPattern::Name(borrow.binding.clone()),
                        BindingSite {
                            start: self.body_start(borrow.span, start),
                            end: borrow.span.end,
                            declaration: borrow.span,
                            head_start: self.binding_tail_start(borrow.span, &borrow.binding, self.body_start(borrow.span, start)),
                            head_end: self.body_start(borrow.span, start),
                        },
                        None,
                        &mut child,
                    );
                    self.block(&borrow.body, borrow.span.end, &mut child, module);
                }
                Stmt::Expr(value) | Stmt::Discard(value) => self.expression_bindings(value, scope, module),
                Stmt::Return(value) => {
                    if let Some(value) = &value.value {
                        self.expression_bindings(value, scope, module);
                    }
                }
                Stmt::Break(_) | Stmt::Continue(_) => {}
            }
        }
    }

    fn match_bindings(&mut self, pattern: &crate::ast::MatchPattern, site: BindingSite, scope: &mut HashMap<String, Option<String>>) {
        use crate::ast::MatchPattern;
        match pattern {
            MatchPattern::Binding(name) => {
                self.bind(name, site, None);
                scope.insert(name.clone(), None);
            }
            MatchPattern::Tuple(items) | MatchPattern::Variant { fields: items, .. } | MatchPattern::Or(items) => {
                for item in items {
                    self.match_bindings(item, site, scope);
                }
            }
            MatchPattern::Struct { fields, .. } => {
                for (_, item) in fields {
                    self.match_bindings(item, site, scope);
                }
            }
            MatchPattern::Wildcard => {}
        }
    }

    /// Collect lexical definitions inside value expressions through the shared
    /// AST. This walk grants no type or storage facts; those still come from IR.
    fn expression_bindings(&mut self, expr: &Expr, scope: &HashMap<String, Option<String>>, module: &Module) {
        match expr {
            Expr::Block(body) => {
                let end = body.iter().map(|stmt| stmt.span().end).max().unwrap_or(0);
                self.block(body, end, &mut scope.clone(), module);
            }
            Expr::If(value) => {
                self.expression_bindings(&value.condition, scope, module);
                self.expression_bindings(&value.then_branch, scope, module);
                self.expression_bindings(&value.else_branch, scope, module);
            }
            Expr::Match(value) => {
                self.expression_bindings(&value.expr, scope, module);
                for arm in &value.arms {
                    let mut child = scope.clone();
                    let site = BindingSite {
                        start: self.head_boundary(arm.span, TokenKind::FatArrow, true),
                        end: arm.span.end,
                        declaration: arm.span,
                        head_start: arm.span.start,
                        head_end: self.head_boundary(arm.span, TokenKind::FatArrow, false),
                    };
                    self.match_bindings(&arm.pattern, site, &mut child);
                    self.expression_bindings(&arm.value, &child, module);
                }
            }
            Expr::Assign(value) => self.expression_bindings(&value.value, scope, module),
            Expr::Binary(value) => {
                self.expression_bindings(&value.left, scope, module);
                self.expression_bindings(&value.right, scope, module);
            }
            Expr::Unary(value) => self.expression_bindings(&value.expr, scope, module),
            Expr::Call(value) => {
                self.expression_bindings(&value.func, scope, module);
                for value in &value.args {
                    self.expression_bindings(value, scope, module);
                }
            }
            Expr::FieldAccess(value) => self.expression_bindings(&value.expr, scope, module),
            Expr::Index(value) => {
                self.expression_bindings(&value.expr, scope, module);
                self.expression_bindings(&value.index, scope, module);
            }
            Expr::Create(value) => {
                for (_, value) in &value.fields {
                    self.expression_bindings(value, scope, module);
                }
                if let Some(value) = &value.lock {
                    self.expression_bindings(value, scope, module);
                }
            }
            Expr::Consume(value) => self.expression_bindings(&value.expr, scope, module),
            Expr::Destroy(value) => self.expression_bindings(&value.expr, scope, module),
            Expr::Claim(value) => self.expression_bindings(&value.receipt, scope, module),
            Expr::Settle(value) => self.expression_bindings(&value.expr, scope, module),
            Expr::CreateUnique(value) => {
                for (_, value) in &value.fields {
                    self.expression_bindings(value, scope, module);
                }
                if let Some(value) = &value.lock {
                    self.expression_bindings(value, scope, module);
                }
            }
            Expr::ReplaceUnique(value) => {
                self.expression_bindings(&value.expr, scope, module);
                for (_, value) in &value.fields {
                    self.expression_bindings(value, scope, module);
                }
            }
            Expr::Assert(value) => {
                self.expression_bindings(&value.condition, scope, module);
                self.expression_bindings(&value.message, scope, module);
            }
            Expr::Require(value) => {
                self.expression_bindings(&value.condition, scope, module);
                if let Some(value) = &value.message {
                    self.expression_bindings(value, scope, module);
                }
            }
            Expr::RequireBlock(value) => {
                for value in &value.expressions {
                    self.expression_bindings(value, scope, module);
                }
            }
            Expr::ReplaceRelation(value) => {
                for value in value.value_exprs() {
                    self.expression_bindings(value, scope, module);
                }
            }
            Expr::Tuple(values) | Expr::Array(values) => {
                for value in values {
                    self.expression_bindings(value, scope, module);
                }
            }
            Expr::Cast(value) => self.expression_bindings(&value.expr, scope, module),
            Expr::Range(value) => {
                self.expression_bindings(&value.start, scope, module);
                self.expression_bindings(&value.end, scope, module);
            }
            Expr::StructInit(value) => {
                for (_, value) in &value.fields {
                    self.expression_bindings(value, scope, module);
                }
            }
            Expr::StdlibCall(value) => {
                for value in &value.args {
                    self.expression_bindings(value, scope, module);
                }
            }
            Expr::Integer(_)
            | Expr::Bool(_)
            | Expr::String(_)
            | Expr::ByteString(_)
            | Expr::Identifier(_)
            | Expr::ReadRef(_)
            | Expr::Preserve(_) => {}
        }
    }

    fn expression(&self, expr: &Expr, binding: &dyn Fn(&str) -> Option<String>) -> Option<String> {
        super::cell_views::expression_type(expr, binding, &self.catalogs, &|name| self.resolve_type(name))
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
        if let Ok(cache) = self.view_cache.lock()
            && let Some((key, index)) = cache.get(uri)
            && key == &inputs.fingerprint
        {
            return Some(index.clone());
        }
        let index = Arc::new(ViewIndex::new(source, module, &inputs));
        if let Ok(mut cache) = self.view_cache.lock() {
            cache.insert(uri.into(), (inputs.fingerprint, index.clone()));
        }
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
