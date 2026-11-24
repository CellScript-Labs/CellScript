use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use crate::lexer::token::{Token, TokenKind};
use crate::syntax::{SyntaxKind, SyntaxNode};

mod action_policies;
mod callables;
mod cell_operations;
mod cell_roles;
mod checks;
mod collections;
mod control_flow;
mod data_declarations;
mod data_shape;
mod expressions;
mod invariants;
mod replace_relations;
mod statements;
mod type_syntax;
mod value_syntax;

pub(crate) use action_policies::validate_action_surface;

const MAX_PARSE_RECURSION_DEPTH: usize = 32;
const MAX_BINARY_CHAIN_LINKS: usize = 256;
const MAX_PARAM_COUNT: usize = 1024;

pub struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
    recover: bool,
    diagnostics: Vec<CompileError>,
    expression_depth: usize,
    type_depth: usize,
    unary_depth: usize,
    statement_depth: usize,
    if_depth: usize,
    pattern_depth: usize,
    binary_chain_links: usize,
    syntax_nodes: Vec<SyntaxNode>,
    record_syntax: bool,
}

#[derive(Debug, Default, Clone)]
struct PendingAttrs {
    type_id: Option<TypeIdentity>,
    scheduler_hint: Option<SchedulerHint>,
}

#[derive(Debug, Default, Clone)]
struct TypePolicyDecls {
    default_hash_type: Option<HashTypeDecl>,
    capacity_floor: Option<CapacityFloorDecl>,
    identity: Option<IdentityPolicy>,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            position: 0,
            recover: false,
            diagnostics: Vec::new(),
            expression_depth: 0,
            type_depth: 0,
            unary_depth: 0,
            statement_depth: 0,
            if_depth: 0,
            pattern_depth: 0,
            binary_chain_links: 0,
            syntax_nodes: Vec::new(),
            record_syntax: false,
        }
    }

    pub fn new_recovering(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            position: 0,
            recover: true,
            diagnostics: Vec::new(),
            expression_depth: 0,
            type_depth: 0,
            unary_depth: 0,
            statement_depth: 0,
            if_depth: 0,
            pattern_depth: 0,
            binary_chain_links: 0,
            syntax_nodes: Vec::new(),
            record_syntax: false,
        }
    }

    fn current(&self) -> &Token {
        &self.tokens[self.position.min(self.tokens.len() - 1)]
    }

    fn record_syntax_node(&mut self, kind: SyntaxKind, start: Span) {
        if self.record_syntax {
            self.syntax_nodes.push(SyntaxNode { kind, span: start.combine(&self.previous_non_newline().span) });
        }
    }

    fn parse_verification_marker(&mut self) -> Result<()> {
        self.skip_newlines();
        if self.check(&TokenKind::Verification) {
            return Err(CompileError::new(
                "`verification` was removed; write ordinary checked statements directly",
                self.current().span,
            ));
        }
        Ok(())
    }

    fn current_identifier(&self) -> Option<String> {
        match &self.current().kind {
            TokenKind::Identifier(name) => Some(name.clone()),
            _ => None,
        }
    }

    fn peek(&self, offset: usize) -> &Token {
        &self.tokens[(self.position + offset).min(self.tokens.len() - 1)]
    }

    fn advance(&mut self) -> &Token {
        let token = &self.tokens[self.position];
        if self.position < self.tokens.len() - 1 {
            self.position += 1;
        }
        token
    }

    fn previous_non_newline(&self) -> &Token {
        let mut index = self.position.saturating_sub(1).min(self.tokens.len() - 1);
        while index > 0 && matches!(self.tokens[index].kind, TokenKind::Newline) {
            index -= 1;
        }
        &self.tokens[index]
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(kind)
    }

    fn expect(&mut self, kind: TokenKind) -> Result<&Token> {
        self.skip_newlines();
        if self.check(&kind) {
            Ok(self.advance())
        } else {
            Err(CompileError::new(format!("expected {}, found {}", kind, self.current().kind), self.current().span))
        }
    }

    fn skip_newlines(&mut self) {
        while self.check(&TokenKind::Newline) {
            self.advance();
        }
    }

    fn record_end(&mut self, kind: SyntaxKind) {
        let end = self.previous_non_newline().span;
        self.record_syntax_node(kind, Span::new(end.end, end.end, end.line, end.column));
    }

    fn expect_statement_end(&mut self) -> Result<()> {
        let previous = self.previous_non_newline();
        let end = previous.span;
        let end_column = end.column + previous.text.chars().count();
        self.skip_newlines();
        if !self.check(&TokenKind::Semi) {
            return Err(CompileError::new(
                "expected `;` to end this statement or policy clause; newlines do not terminate statements; construct values with `new Type { ... }`",
                Span::new(end.end, end.end, end.line, end_column),
            ));
        }
        self.advance();
        Ok(())
    }

    fn expect_list_separator(&mut self, close: TokenKind) -> Result<()> {
        self.skip_newlines();
        if self.check(&TokenKind::Comma) {
            self.advance();
            self.skip_newlines();
            Ok(())
        } else if self.check(&close) {
            Ok(())
        } else {
            Err(CompileError::new("expected `,` between list entries; a newline is whitespace", self.current().span))
        }
    }

    fn record_diagnostic(&mut self, error: CompileError) {
        self.diagnostics.push(error);
    }

    fn is_item_start(&self) -> bool {
        matches!(
            &self.current().kind,
            TokenKind::Pound
                | TokenKind::Use
                | TokenKind::Resource
                | TokenKind::Shared
                | TokenKind::Receipt
                | TokenKind::Struct
                | TokenKind::Invariant
                | TokenKind::Const
                | TokenKind::Enum
                | TokenKind::Action
                | TokenKind::Fn
                | TokenKind::Lock
        ) || matches!(&self.current().kind, TokenKind::Identifier(name) if name == "flow" || name == "cell_roles" || name == "data_shape" || name == "collections")
    }

    fn synchronize_item(&mut self, start_position: usize) {
        if self.position == start_position && !self.check(&TokenKind::Eof) {
            self.advance();
        }
        let mut brace_depth = 0usize;
        while !self.check(&TokenKind::Eof) {
            match &self.current().kind {
                TokenKind::LBrace => {
                    brace_depth = brace_depth.saturating_add(1);
                    self.advance();
                }
                TokenKind::RBrace => {
                    brace_depth = brace_depth.saturating_sub(1);
                    self.advance();
                    if brace_depth == 0 {
                        self.skip_newlines();
                        if self.is_item_start() {
                            break;
                        }
                    }
                }
                TokenKind::Newline | TokenKind::Semi if brace_depth == 0 => {
                    self.advance();
                    self.skip_newlines();
                    if self.is_item_start() {
                        break;
                    }
                }
                _ if brace_depth == 0 && self.position != start_position && self.is_item_start() => break,
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn synchronize_stmt(&mut self, start_position: usize) {
        if self.position == start_position && !self.check(&TokenKind::Eof) {
            self.advance();
        }
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        while !self.check(&TokenKind::Eof) {
            match &self.current().kind {
                TokenKind::LParen => {
                    paren_depth = paren_depth.saturating_add(1);
                    self.advance();
                }
                TokenKind::RParen => {
                    paren_depth = paren_depth.saturating_sub(1);
                    self.advance();
                }
                TokenKind::LBracket => {
                    bracket_depth = bracket_depth.saturating_add(1);
                    self.advance();
                }
                TokenKind::RBracket => {
                    bracket_depth = bracket_depth.saturating_sub(1);
                    self.advance();
                }
                TokenKind::LBrace => {
                    brace_depth = brace_depth.saturating_add(1);
                    self.advance();
                }
                TokenKind::RBrace if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => break,
                TokenKind::RBrace => {
                    brace_depth = brace_depth.saturating_sub(1);
                    self.advance();
                }
                TokenKind::Newline | TokenKind::Semi if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                    self.advance();
                    self.skip_newlines();
                    break;
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn parse_stmt_recovering(&mut self) -> Result<Option<Stmt>> {
        let start_position = self.position;
        match self.parse_stmt() {
            Ok(stmt) => Ok(Some(stmt)),
            Err(error) if self.recover => {
                self.record_diagnostic(error);
                self.synchronize_stmt(start_position);
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    fn token_ident_like_name(kind: &TokenKind) -> Option<String> {
        match kind {
            TokenKind::Identifier(name) => Some(name.clone()),
            TokenKind::Module => Some("module".to_string()),
            TokenKind::Use => Some("use".to_string()),
            TokenKind::Resource => Some("resource".to_string()),
            TokenKind::Shared => Some("shared".to_string()),
            TokenKind::Receipt => Some("receipt".to_string()),
            TokenKind::Struct => Some("struct".to_string()),
            TokenKind::Const => Some("const".to_string()),
            TokenKind::Enum => Some("enum".to_string()),
            TokenKind::Invariant => Some("invariant".to_string()),
            TokenKind::Action => Some("action".to_string()),
            TokenKind::Lock => Some("lock".to_string()),
            TokenKind::Transition => Some("transition".to_string()),
            TokenKind::Verification => Some("verification".to_string()),
            TokenKind::Has => Some("has".to_string()),
            TokenKind::Store => Some("store".to_string()),
            TokenKind::Destroy | TokenKind::DestroyKw => Some("destroy".to_string()),
            TokenKind::Launch => Some("launch".to_string()),
            TokenKind::Assert => Some("assert".to_string()),
            TokenKind::Preserve => Some("preserve".to_string()),
            TokenKind::Address => Some("Address".to_string()),
            TokenKind::Hash => Some("Hash".to_string()),
            TokenKind::Env => Some("env".to_string()),
            TokenKind::Std => Some("std".to_string()),
            TokenKind::Self_ => Some("self".to_string()),
            _ => None,
        }
    }

    fn ident_like_name(&self) -> Option<String> {
        Self::token_ident_like_name(&self.current().kind)
    }

    fn parse_name(&mut self) -> Result<String> {
        self.skip_newlines();
        let name = self.ident_like_name().ok_or_else(|| CompileError::new("expected identifier", self.current().span))?;
        self.advance();
        self.skip_newlines();
        Ok(name)
    }

    fn parse_name_path(&mut self) -> Result<String> {
        let mut name = self.parse_name()?;

        while self.check(&TokenKind::ColonColon) && Self::token_ident_like_name(&self.peek(1).kind).is_some() {
            self.advance();
            let segment = self.ident_like_name().ok_or_else(|| CompileError::new("expected path segment", self.current().span))?;
            name.push_str("::");
            name.push_str(&segment);
            self.advance();
        }

        Ok(name)
    }

    fn parse_type_list(&mut self, end: TokenKind) -> Result<Vec<Type>> {
        let mut items = Vec::new();
        self.skip_newlines();
        while !self.check(&end) && !self.check(&TokenKind::Eof) {
            items.push(self.parse_type()?);
            self.skip_newlines();
            if self.check(&TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
            } else {
                break;
            }
        }
        Ok(items)
    }

    fn parse_binding_pattern(&mut self) -> Result<BindingPattern> {
        if self.pattern_depth >= MAX_PARSE_RECURSION_DEPTH {
            return Err(CompileError::new(
                format!("pattern nesting exceeds the parser limit of {}", MAX_PARSE_RECURSION_DEPTH),
                self.current().span,
            ));
        }
        self.pattern_depth += 1;
        let result = self.parse_binding_pattern_inner();
        self.pattern_depth -= 1;
        result
    }

    fn parse_binding_pattern_inner(&mut self) -> Result<BindingPattern> {
        self.skip_newlines();
        match &self.current().kind {
            TokenKind::Underscore => {
                self.advance();
                Ok(BindingPattern::Wildcard)
            }
            TokenKind::LParen => {
                self.advance();
                self.skip_newlines();
                let mut items = Vec::new();
                while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                    items.push(self.parse_binding_pattern()?);
                    self.skip_newlines();
                    if self.check(&TokenKind::Comma) {
                        self.advance();
                        self.skip_newlines();
                    } else {
                        break;
                    }
                }
                self.expect(TokenKind::RParen)?;
                Ok(BindingPattern::Tuple(items))
            }
            _ => Ok(BindingPattern::Name(self.parse_name()?)),
        }
    }

    fn parse_attrs(&mut self) -> Result<PendingAttrs> {
        let mut attrs = PendingAttrs::default();

        loop {
            self.skip_newlines();
            if !self.check(&TokenKind::Pound) {
                break;
            }

            self.advance();
            self.expect(TokenKind::LBracket)?;
            let attr_name = self.parse_name_path()?;
            self.expect(TokenKind::LParen)?;

            match attr_name.as_str() {
                "capability" => {
                    return Err(CompileError::new(
                        "#[capability(...)] was removed; declare lifecycle permissions with `has ...`",
                        self.current().span,
                    ));
                }
                "type_id" => {
                    if attrs.type_id.is_some() {
                        return Err(CompileError::new("duplicate type_id attribute", self.current().span));
                    }
                    let span = self.current().span;
                    let value = match &self.current().kind {
                        TokenKind::String(value) => {
                            let value = value.clone();
                            self.advance();
                            value
                        }
                        _ => return Err(CompileError::new("expected string literal type_id", self.current().span)),
                    };
                    if value.is_empty() {
                        return Err(CompileError::new("type_id must not be empty", span));
                    }
                    if value.chars().any(char::is_control) {
                        return Err(CompileError::new("type_id must not contain control characters", span));
                    }
                    attrs.type_id = Some(TypeIdentity { value, span });
                }
                "effect" => {
                    return Err(crate::error::SourceDiagnosticCode::RemovedEffectDeclaration.error(
                        "effect declarations have been removed; remove #[effect(...)] and review the inferred public contract",
                        self.current().span,
                    ));
                }
                "scheduler_hint" => {
                    if attrs.scheduler_hint.is_some() {
                        return Err(CompileError::new("duplicate scheduler_hint attribute", self.current().span));
                    }
                    let mut seen = std::collections::BTreeSet::new();
                    let mut parallelizable = true;
                    let mut estimated_cycles = 1000;
                    while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                        let hint_span = self.current().span;
                        let hint_name = self.parse_name()?;
                        let category = if matches!(hint_name.as_str(), "parallel" | "sequential") {
                            "execution_mode"
                        } else {
                            hint_name.as_str()
                        };
                        if !seen.insert(category.to_string()) {
                            return Err(CompileError::new("duplicate or conflicting scheduler hint", hint_span));
                        }
                        match hint_name.as_str() {
                            "parallel" => parallelizable = true,
                            "sequential" => parallelizable = false,
                            "estimated_cycles" => {
                                self.expect(TokenKind::Eq)?;
                                estimated_cycles = match &self.current().kind {
                                    TokenKind::Integer(n) => {
                                        let value = u64::try_from(*n)
                                            .map_err(|_| CompileError::new("estimated_cycles must fit in u64", self.current().span))?;
                                        self.advance();
                                        value
                                    }
                                    _ => return Err(CompileError::new("expected integer estimated_cycles", self.current().span)),
                                };
                            }
                            _ => {
                                return Err(crate::error::SourceDiagnosticCode::UnknownSchedulerHint.error(
                                    format!("unknown scheduler hint '{hint_name}'; use parallel, sequential, or estimated_cycles = <u64 integer literal>"),
                                    hint_span,
                                ));
                            }
                        }

                        if self.check(&TokenKind::Comma) {
                            self.advance();
                            self.skip_newlines();
                        } else {
                            break;
                        }
                    }
                    attrs.scheduler_hint = Some(SchedulerHint { parallelizable, estimated_cycles });
                }
                _ => {
                    return Err(CompileError::new(format!("unknown attribute '{}'", attr_name), self.current().span));
                }
            }

            self.expect(TokenKind::RParen)?;
            self.expect(TokenKind::RBracket)?;
            self.skip_newlines();
        }

        Ok(attrs)
    }

    pub fn parse_module(&mut self) -> Result<Module> {
        if !self.tokens.last().is_some_and(|token| token.kind == TokenKind::Eof) {
            let span = self.tokens.last().map_or_else(Span::default, |token| token.span);
            return Err(CompileError::new("token stream must end with EOF", span));
        }
        self.skip_newlines();
        let start_span = self.current().span;

        self.expect(TokenKind::Module)?;

        let full_name = self.parse_name_path()?;
        self.record_end(SyntaxKind::ClauseEnd);
        self.expect_statement_end()?;
        self.record_syntax_node(SyntaxKind::ModuleHeader, start_span);

        self.skip_newlines();

        let mut items = Vec::new();
        let mut visibilities = std::collections::BTreeMap::new();
        while !self.check(&TokenKind::Eof) {
            self.skip_newlines();
            if self.check(&TokenKind::Eof) {
                break;
            }
            let start_position = self.position;
            match self.parse_item() {
                Ok((item, visibility)) => {
                    if let Some(name) = item.name() {
                        visibilities.insert(name.to_string(), visibility);
                    }
                    items.push(item);
                }
                Err(error) if self.recover => {
                    self.record_diagnostic(error);
                    self.synchronize_item(start_position);
                }
                Err(error) => return Err(error),
            }
            self.skip_newlines();
        }

        let end_span = self.current().span;
        Ok(Module {
            name: full_name,
            items,
            interface_templates: Vec::new(),
            specializations: Default::default(),
            visibilities,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_item(&mut self) -> Result<(Item, Visibility)> {
        let start = self.current().span;
        let item = self.parse_item_inner()?;
        self.record_syntax_node(SyntaxKind::Item, start);
        Ok(item)
    }

    fn parse_item_inner(&mut self) -> Result<(Item, Visibility)> {
        let attrs = self.parse_attrs()?;
        let visibility = self.parse_visibility()?;
        if self.check(&TokenKind::Pound) {
            return Err(CompileError::new("attributes must precede the visibility modifier", self.current().span));
        }
        if attrs.scheduler_hint.is_some() && !self.check(&TokenKind::Action) {
            return Err(CompileError::new("scheduler_hint is only valid on action declarations", self.current().span));
        }
        let item = match &self.current().kind {
            TokenKind::Identifier(name) if name == "collections" => {
                self.reject_all_attrs("collections", &attrs)?;
                Ok(Item::Collections(self.parse_collections()?))
            }
            TokenKind::Identifier(name) if name == "data_shape" => {
                self.reject_all_attrs("data_shape", &attrs)?;
                Ok(Item::DataShape(self.parse_data_shape()?))
            }
            TokenKind::Identifier(name) if name == "cell_roles" => {
                self.reject_all_attrs("cell_roles", &attrs)?;
                Ok(Item::CellRoles(self.parse_cell_roles()?))
            }
            TokenKind::Use => {
                self.reject_type_id_attr(&attrs)?;
                Ok(Item::Use(self.parse_use()?))
            }
            TokenKind::Resource => Ok(Item::Resource(self.parse_resource(attrs.type_id)?)),
            TokenKind::Shared => Ok(Item::Shared(self.parse_shared(attrs.type_id)?)),
            TokenKind::Receipt => Ok(Item::Receipt(self.parse_receipt(attrs.type_id)?)),
            TokenKind::Struct => Ok(Item::Struct(self.parse_struct(attrs.type_id)?)),
            TokenKind::Identifier(name) if name == "flow" => {
                self.reject_type_id_attr(&attrs)?;
                Ok(Item::Flow(self.parse_flow()?))
            }
            TokenKind::Invariant => {
                self.reject_all_attrs("invariant", &attrs)?;
                Ok(Item::Invariant(self.parse_invariant()?))
            }
            TokenKind::Const => {
                self.reject_type_id_attr(&attrs)?;
                Ok(Item::Const(self.parse_const()?))
            }
            TokenKind::Enum => {
                self.reject_type_id_attr(&attrs)?;
                Ok(Item::Enum(self.parse_enum()?))
            }
            TokenKind::Action => {
                self.reject_type_id_attr(&attrs)?;
                Ok(Item::Action(self.parse_action(attrs.scheduler_hint)?))
            }
            TokenKind::Fn => {
                self.reject_type_id_attr(&attrs)?;
                if attrs.scheduler_hint.is_some() {
                    return Err(CompileError::new("fn definitions do not accept attributes", self.current().span));
                }
                Ok(Item::Function(self.parse_fn()?))
            }
            TokenKind::Lock => {
                self.reject_type_id_attr(&attrs)?;
                Ok(Item::Lock(self.parse_lock()?))
            }
            _ => Err(CompileError::new(format!("unexpected token: {}", self.current().kind), self.current().span)),
        }?;
        if item.name().is_none() && visibility != Visibility::ImplicitPublic {
            return Err(CompileError::new("visibility modifiers are only valid on named top-level declarations", self.current().span));
        }
        let visibility = if let Item::Collections(def) = &item {
            if !matches!(visibility, Visibility::ImplicitPublic | Visibility::Private) {
                return Err(CompileError::new(
                    "collections declarations are module-local; exported owner contracts are not yet implemented",
                    def.span,
                ));
            }
            Visibility::Private
        } else if let Item::CellRoles(def) = &item {
            if !matches!(visibility, Visibility::ImplicitPublic | Visibility::Private) {
                return Err(CompileError::new(
                    "cell_roles catalogs are module-local; exported catalog interfaces are not yet implemented",
                    def.span,
                ));
            }
            Visibility::Private
        } else if let Item::DataShape(def) = &item {
            if !matches!(visibility, Visibility::ImplicitPublic | Visibility::Private) {
                return Err(CompileError::new(
                    "data_shape names are module-local; exported shape aliases are not yet implemented",
                    def.span,
                ));
            }
            Visibility::Private
        } else {
            visibility
        };
        Ok((item, visibility))
    }

    fn parse_visibility(&mut self) -> Result<Visibility> {
        if self.check(&TokenKind::Private) {
            self.advance();
            return Ok(Visibility::Private);
        }
        if !self.check(&TokenKind::Public) {
            return Ok(Visibility::ImplicitPublic);
        }
        let span = self.current().span;
        self.advance();
        if !self.check(&TokenKind::LParen) {
            return Ok(Visibility::Public);
        }
        self.advance();
        if self.current().text != "package" {
            return Err(CompileError::new("expected 'package' in public(package)", self.current().span));
        }
        self.advance();
        self.expect(TokenKind::RParen).map_err(|_| CompileError::new("expected ')' after public(package)", span))?;
        Ok(Visibility::Package)
    }

    fn reject_type_id_attr(&self, attrs: &PendingAttrs) -> Result<()> {
        if let Some(type_id) = &attrs.type_id {
            Err(CompileError::new("#[type_id] can only be applied to resource, shared, receipt, or struct definitions", type_id.span))
        } else {
            Ok(())
        }
    }

    fn reject_all_attrs(&self, item_kind: &str, attrs: &PendingAttrs) -> Result<()> {
        if attrs.type_id.is_some() || attrs.scheduler_hint.is_some() {
            Err(CompileError::new(format!("attributes cannot be applied to {} definitions", item_kind), self.current().span))
        } else {
            Ok(())
        }
    }
}

pub fn parse(tokens: &[Token]) -> Result<Module> {
    let mut parser = Parser::new(tokens);
    parser.parse_module()
}

pub(crate) fn parse_with_syntax(tokens: &[Token]) -> Result<(Module, Vec<SyntaxNode>)> {
    let mut parser = Parser::new(tokens);
    parser.record_syntax = true;
    let module = parser.parse_module()?;
    Ok((module, parser.syntax_nodes))
}

pub fn parse_diagnostics(tokens: &[Token]) -> std::result::Result<Module, Vec<CompileError>> {
    let mut parser = Parser::new_recovering(tokens);
    let parsed = parser.parse_module();
    match (parsed, parser.diagnostics.is_empty()) {
        (Ok(module), true) => Ok(module),
        (Ok(_), false) => Err(parser.diagnostics),
        (Err(error), true) => Err(vec![error]),
        (Err(error), false) => {
            parser.diagnostics.push(error);
            Err(parser.diagnostics)
        }
    }
}

/// Parse exactly one statement through the same kernel used by ordinary
/// actions and locks, returning the number of original tokens consumed.
pub(crate) fn parse_statement_prefix_with_syntax(tokens: &[Token], record: bool) -> Result<(Stmt, usize, Vec<SyntaxNode>)> {
    let mut fragment = tokens.to_vec();
    let end = fragment.last().map_or(Span::default(), |token| token.span);
    fragment.push(Token::new(TokenKind::Eof, Span::new(end.end, end.end, end.line, end.column), ""));
    let mut parser = Parser::new(&fragment);
    parser.record_syntax = record;
    parser.skip_newlines();
    let statement = parser.parse_stmt()?;
    Ok((statement, parser.position, parser.syntax_nodes))
}

pub(crate) fn parse_type_prefix_with_syntax(tokens: &[Token], record: bool) -> Result<(Type, usize, Vec<SyntaxNode>)> {
    let mut parser = Parser::new(tokens);
    parser.record_syntax = record;
    parser.skip_newlines();
    let ty = parser.parse_type()?;
    Ok((ty, parser.position, parser.syntax_nodes))
}

pub(crate) fn parse_expression_fragment(tokens: &[Token]) -> Result<Expr> {
    parse_expression_fragment_with_syntax(tokens, false).map(|(value, _)| value)
}

pub(crate) fn parse_expression_prefix_with_syntax(tokens: &[Token], record: bool) -> Result<(Expr, usize, Vec<SyntaxNode>)> {
    let mut parser = Parser::new(tokens);
    parser.record_syntax = record;
    let expression = parser.parse_expr()?;
    Ok((expression, parser.position, parser.syntax_nodes))
}

pub(crate) fn parse_expression_fragment_with_syntax(tokens: &[Token], record: bool) -> Result<(Expr, Vec<SyntaxNode>)> {
    let mut fragment = tokens.to_vec();
    let end = fragment.last().map_or(Span::default(), |token| token.span);
    fragment.push(Token::new(TokenKind::Eof, Span::new(end.end, end.end, end.line, end.column), ""));
    let mut parser = Parser::new(&fragment);
    parser.record_syntax = record;
    parser.skip_newlines();
    let expression = parser.parse_expr()?;
    parser.skip_newlines();
    parser.expect(TokenKind::Eof)?;
    Ok((expression, parser.syntax_nodes))
}

fn normalize_hash_type_decl(value: &str) -> Option<String> {
    match value {
        "Data" | "data" => Some("data".to_string()),
        "Data1" | "data1" => Some("data1".to_string()),
        "Data2" | "data2" => Some("data2".to_string()),
        "Type" | "type" => Some("type".to_string()),
        _ => None,
    }
}

fn aggregate_scope_from_targets(left: &AggregateTarget, right: Option<&AggregateTarget>) -> Option<String> {
    let left_scope = aggregate_scope_from_target(left)?;
    if let Some(right) = right {
        let right_scope = aggregate_scope_from_target(right)?;
        if left_scope != right_scope {
            return None;
        }
    }
    Some(left_scope.to_string())
}

fn aggregate_scope_from_target(target: &AggregateTarget) -> Option<&'static str> {
    target.source.aggregate_scope()
}

#[cfg(test)]
mod tests;
