use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use crate::lexer::token::{Token, TokenKind};

const MAX_PARSE_RECURSION_DEPTH: u16 = 128;

pub struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
    eof: Token,
    recursion_depth: u16,
}

#[derive(Debug, Default, Clone)]
struct PendingAttrs {
    type_id: Option<TypeIdentity>,
    capabilities: Option<Vec<Capability>>,
    effect: Option<EffectClass>,
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
        Self { tokens, position: 0, eof: Token::new(TokenKind::Eof, Span::default(), ""), recursion_depth: 0 }
    }

    fn current(&self) -> &Token {
        self.tokens.get(self.position.min(self.tokens.len().saturating_sub(1))).unwrap_or(&self.eof)
    }

    fn current_identifier(&self) -> Option<String> {
        match &self.current().kind {
            TokenKind::Identifier(name) => Some(name.clone()),
            _ => None,
        }
    }

    fn peek(&self, offset: usize) -> &Token {
        self.tokens.get(self.position.saturating_add(offset).min(self.tokens.len().saturating_sub(1))).unwrap_or(&self.eof)
    }

    fn with_recursion_guard<T>(&mut self, span: Span, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if self.recursion_depth >= MAX_PARSE_RECURSION_DEPTH {
            return Err(CompileError::new(
                format!("parser recursion limit exceeded while parsing nested syntax (limit {})", MAX_PARSE_RECURSION_DEPTH),
                span,
            ));
        }

        self.recursion_depth += 1;
        let result = f(self);
        self.recursion_depth -= 1;
        result
    }

    fn advance(&mut self) -> &Token {
        if self.tokens.is_empty() {
            return &self.eof;
        }
        let index = self.position.min(self.tokens.len().saturating_sub(1));
        if self.position < self.tokens.len().saturating_sub(1) {
            self.position += 1;
        }
        &self.tokens[index]
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(kind)
    }

    fn expect(&mut self, kind: TokenKind) -> Result<&Token> {
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

    fn consume_optional_semi(&mut self) {
        if self.check(&TokenKind::Semi) {
            self.advance();
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
            TokenKind::Where => Some("where".to_string()),
            TokenKind::Transition => Some("transition".to_string()),
            TokenKind::Has => Some("has".to_string()),
            TokenKind::Store => Some("store".to_string()),
            TokenKind::Transfer => Some("transfer".to_string()),
            TokenKind::DestroyKw => Some("destroy".to_string()),
            TokenKind::Launch => Some("launch".to_string()),
            TokenKind::Assert => Some("assert".to_string()),
            TokenKind::Preserve => Some("preserve".to_string()),
            TokenKind::Claim => Some("claim".to_string()),
            TokenKind::Settle => Some("settle".to_string()),
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
        let name = self.ident_like_name().ok_or_else(|| CompileError::new("expected identifier", self.current().span))?;
        self.advance();
        Ok(name)
    }

    fn parse_name_with_span(&mut self) -> Result<(String, Span)> {
        let span = self.current().span;
        self.parse_name().map(|name| (name, span))
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

    fn looks_like_type_name(name: &str) -> bool {
        name.split("::").last().and_then(|segment| segment.chars().next()).is_some_and(|ch| ch.is_ascii_uppercase())
    }

    fn parse_binding_pattern(&mut self) -> Result<BindingPattern> {
        let span = self.current().span;
        self.with_recursion_guard(span, |this| this.parse_binding_pattern_inner())
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
                    let mut caps = Vec::new();
                    while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                        match &self.current().kind {
                            TokenKind::Store => {
                                caps.push(Capability::Store);
                                self.advance();
                            }
                            TokenKind::Transfer => {
                                caps.push(Capability::Transfer);
                                self.advance();
                            }
                            TokenKind::DestroyKw => {
                                caps.push(Capability::Destroy);
                                self.advance();
                            }
                            TokenKind::Create => {
                                caps.push(Capability::Create);
                                self.advance();
                            }
                            TokenKind::Consume => {
                                caps.push(Capability::Consume);
                                self.advance();
                            }
                            TokenKind::ReadRef => {
                                caps.push(Capability::ReadRef);
                                self.advance();
                            }
                            // v0.15 context-sensitive capability keywords (identifiers)
                            TokenKind::Identifier(s) if s == "replace" => {
                                caps.push(Capability::Replace);
                                self.advance();
                            }
                            TokenKind::Identifier(s) if s == "burn" => {
                                caps.push(Capability::Burn);
                                self.advance();
                            }
                            TokenKind::Identifier(s) if s == "relock" => {
                                caps.push(Capability::Relock);
                                self.advance();
                            }
                            TokenKind::Identifier(s) if s == "retarget_type" => {
                                caps.push(Capability::RetargetType);
                                self.advance();
                            }
                            _ => {
                                return Err(CompileError::new("expected capability name", self.current().span));
                            }
                        }

                        if self.check(&TokenKind::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    attrs.capabilities = Some(caps);
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
                    let mut has_mutating = false;
                    let mut has_creating = false;
                    let mut has_destroying = false;
                    let mut has_read_only = false;
                    while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                        let effect_name = self.parse_name()?;
                        match effect_name.as_str() {
                            "Pure" | "pure" => {}
                            "ReadOnly" | "readonly" | "read_only" => has_read_only = true,
                            "Mutating" | "mutating" => has_mutating = true,
                            "Creating" | "creating" => has_creating = true,
                            "Destroying" | "destroying" => has_destroying = true,
                            _ => return Err(CompileError::new("expected effect class", self.current().span)),
                        }

                        if self.check(&TokenKind::Comma) {
                            self.advance();
                            self.skip_newlines();
                        } else {
                            break;
                        }
                    }

                    attrs.effect = Some(if has_mutating || (has_creating && has_destroying) {
                        EffectClass::Mutating
                    } else if has_creating {
                        EffectClass::Creating
                    } else if has_destroying {
                        EffectClass::Destroying
                    } else if has_read_only {
                        EffectClass::ReadOnly
                    } else {
                        EffectClass::Pure
                    });
                }
                "scheduler_hint" => {
                    let mut parallelizable = true;
                    let mut estimated_cycles = 1000;
                    while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                        let hint_name = self.parse_name()?;
                        match hint_name.as_str() {
                            "parallel" => parallelizable = true,
                            "sequential" => parallelizable = false,
                            "estimated_cycles" => {
                                self.expect(TokenKind::Eq)?;
                                estimated_cycles = match &self.current().kind {
                                    TokenKind::Integer(n) => {
                                        let value = *n;
                                        self.advance();
                                        value
                                    }
                                    _ => return Err(CompileError::new("expected integer estimated_cycles", self.current().span)),
                                };
                            }
                            _ => {}
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
        let start_span = self.current().span;

        self.expect(TokenKind::Module)?;

        let full_name = self.parse_name_path()?;
        self.consume_optional_semi();

        self.skip_newlines();

        let mut items = Vec::new();
        while !self.check(&TokenKind::Eof) {
            self.skip_newlines();
            if self.check(&TokenKind::Eof) {
                break;
            }
            items.push(self.parse_item()?);
            self.skip_newlines();
        }

        let end_span = self.current().span;
        Ok(Module { name: full_name, items, span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column) })
    }

    fn parse_item(&mut self) -> Result<Item> {
        let attrs = self.parse_attrs()?;
        match &self.current().kind {
            TokenKind::Use => {
                self.reject_type_id_attr(&attrs)?;
                Ok(Item::Use(self.parse_use()?))
            }
            TokenKind::Resource => Ok(Item::Resource(self.parse_resource(attrs.type_id, attrs.capabilities)?)),
            TokenKind::Shared => Ok(Item::Shared(self.parse_shared(attrs.type_id, attrs.capabilities)?)),
            TokenKind::Receipt => Ok(Item::Receipt(self.parse_receipt(attrs.type_id, attrs.capabilities)?)),
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
                Ok(Item::Action(self.parse_action(attrs.effect, attrs.scheduler_hint)?))
            }
            TokenKind::Fn => {
                self.reject_type_id_attr(&attrs)?;
                Ok(Item::Function(self.parse_fn()?))
            }
            TokenKind::Lock => {
                self.reject_type_id_attr(&attrs)?;
                Ok(Item::Lock(self.parse_lock()?))
            }
            _ => Err(CompileError::new(format!("unexpected token: {}", self.current().kind), self.current().span)),
        }
    }

    fn reject_type_id_attr(&self, attrs: &PendingAttrs) -> Result<()> {
        if let Some(type_id) = &attrs.type_id {
            Err(CompileError::new("#[type_id] can only be applied to resource, shared, receipt, or struct definitions", type_id.span))
        } else {
            Ok(())
        }
    }

    fn reject_all_attrs(&self, item_kind: &str, attrs: &PendingAttrs) -> Result<()> {
        if attrs.type_id.is_some() || attrs.capabilities.is_some() || attrs.effect.is_some() || attrs.scheduler_hint.is_some() {
            Err(CompileError::new(format!("attributes cannot be applied to {} definitions", item_kind), self.current().span))
        } else {
            Ok(())
        }
    }

    fn reject_generic_type_params(&self, type_name: &str) -> Result<()> {
        if self.check(&TokenKind::Lt) {
            Err(CompileError::new(
                format!(
                    "generic type parameters on '{}' are post-v1 template/codegen syntax, not CellScript v1 executable core; define a concrete type or generate a specialized .cell module",
                    type_name
                ),
                self.current().span,
            ))
        } else {
            Ok(())
        }
    }

    fn parse_use(&mut self) -> Result<UseStmt> {
        let start_span = self.current().span;
        self.expect(TokenKind::Use)?;

        let path = self.parse_name_path()?.split("::").map(ToString::to_string).collect::<Vec<_>>();

        let (module_path, imports) = if self.check(&TokenKind::ColonColon) {
            self.advance();
            if self.check(&TokenKind::LBrace) {
                self.advance();
                let mut imports = Vec::new();
                while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
                    imports.push(UseImport { name: self.parse_name_path()?, alias: None });
                    if self.check(&TokenKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect(TokenKind::RBrace)?;
                (path, imports)
            } else {
                let import_name = self.parse_name_path()?;
                let alias = match &self.current().kind {
                    TokenKind::Identifier(s) if s == "as" => {
                        self.advance();
                        match &self.current().kind {
                            TokenKind::Identifier(n) => {
                                let a = n.clone();
                                self.advance();
                                Some(a)
                            }
                            _ => return Err(CompileError::new("expected import alias after 'as'", self.current().span)),
                        }
                    }
                    _ => None,
                };
                (path, vec![UseImport { name: import_name, alias }])
            }
        } else {
            let mut module_path = path;
            let import_name = module_path.pop().ok_or_else(|| CompileError::new("expected import path", self.current().span))?;
            let alias = match &self.current().kind {
                TokenKind::Identifier(s) if s == "as" => {
                    self.advance();
                    match &self.current().kind {
                        TokenKind::Identifier(n) => {
                            let a = n.clone();
                            self.advance();
                            Some(a)
                        }
                        _ => return Err(CompileError::new("expected import alias after 'as'", self.current().span)),
                    }
                }
                _ => None,
            };
            (module_path, vec![UseImport { name: import_name, alias }])
        };

        self.consume_optional_semi();

        let end_span = self.current().span;
        Ok(UseStmt { module_path, imports, span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column) })
    }

    fn parse_resource(&mut self, type_id: Option<TypeIdentity>, attr_capabilities: Option<Vec<Capability>>) -> Result<ResourceDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Resource)?;

        let name = self.parse_name()?;
        self.reject_generic_type_params(&name)?;

        let capabilities = merge_capabilities(attr_capabilities, self.parse_capabilities()?);
        self.skip_newlines();
        let type_policy = self.parse_type_policy_decls()?;
        self.skip_newlines();

        let fields = self.parse_fields()?;

        let end_span = self.current().span;
        Ok(ResourceDef {
            name,
            type_id,
            identity: type_policy.identity.unwrap_or_default(),
            default_hash_type: type_policy.default_hash_type,
            capacity_floor: type_policy.capacity_floor,
            capabilities,
            fields,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_shared(&mut self, type_id: Option<TypeIdentity>, attr_capabilities: Option<Vec<Capability>>) -> Result<SharedDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Shared)?;

        let name = self.parse_name()?;
        self.reject_generic_type_params(&name)?;

        let capabilities = merge_capabilities(attr_capabilities, self.parse_capabilities()?);
        self.skip_newlines();
        let type_policy = self.parse_type_policy_decls()?;
        self.skip_newlines();
        let fields = self.parse_fields()?;

        let end_span = self.current().span;
        Ok(SharedDef {
            name,
            type_id,
            identity: type_policy.identity.unwrap_or_default(),
            default_hash_type: type_policy.default_hash_type,
            capacity_floor: type_policy.capacity_floor,
            capabilities,
            fields,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_receipt(&mut self, type_id: Option<TypeIdentity>, attr_capabilities: Option<Vec<Capability>>) -> Result<ReceiptDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Receipt)?;

        let name = self.parse_name()?;
        self.reject_generic_type_params(&name)?;
        let claim_output = if self.check(&TokenKind::Arrow) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };

        let capabilities = merge_capabilities(attr_capabilities, self.parse_capabilities()?);
        self.skip_newlines();
        let type_policy = self.parse_type_policy_decls()?;
        self.skip_newlines();
        let fields = self.parse_fields()?;

        let end_span = self.current().span;
        Ok(ReceiptDef {
            name,
            type_id,
            identity: type_policy.identity.unwrap_or_default(),
            default_hash_type: type_policy.default_hash_type,
            capacity_floor: type_policy.capacity_floor,
            claim_output,
            capabilities,
            fields,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_struct(&mut self, type_id: Option<TypeIdentity>) -> Result<StructDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Struct)?;

        let name = self.parse_name()?;
        self.reject_generic_type_params(&name)?;

        self.skip_newlines();
        let type_policy = self.parse_type_policy_decls()?;
        self.skip_newlines();
        let fields = self.parse_fields()?;

        let end_span = self.current().span;
        Ok(StructDef {
            name,
            type_id,
            default_hash_type: type_policy.default_hash_type,
            capacity_floor: type_policy.capacity_floor,
            fields,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_flow(&mut self) -> Result<FlowDef> {
        let start_span = self.current().span;
        let keyword = self.parse_name()?;
        debug_assert_eq!(keyword, "flow");

        let first = self.parse_name_path()?;
        let (name, target) =
            if self.check(&TokenKind::For) || matches!(&self.current().kind, TokenKind::Identifier(name) if name == "for") {
                self.advance();
                (Some(first), self.parse_state_field_path()?)
            } else if self.check(&TokenKind::Dot) {
                self.advance();
                let field_start = self.current().span;
                let field = self.parse_name()?;
                let field_end = self.current().span;
                (
                    None,
                    StateFieldPath {
                        base: first,
                        field,
                        span: Span::new(start_span.start, field_end.end, field_start.line, field_start.column),
                    },
                )
            } else {
                return Err(CompileError::new("expected 'for' or '.field' in flow declaration", self.current().span));
            };
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();

        let mut transitions = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let transition_start = self.current().span;
            let from = self.parse_name_path()?;
            self.expect(TokenKind::Arrow)?;
            let to = self.parse_name_path()?;
            let action = if matches!(&self.current().kind, TokenKind::Identifier(name) if name == "by") {
                self.advance();
                Some(self.parse_name_path()?)
            } else {
                None
            };
            let transition_end = self.current().span;
            transitions.push(StateTransition {
                from,
                to,
                action,
                span: Span::new(transition_start.start, transition_end.end, transition_start.line, transition_start.column),
            });

            if self.check(&TokenKind::Comma) || self.check(&TokenKind::Semi) {
                self.advance();
            }
            self.skip_newlines();
        }

        self.expect(TokenKind::RBrace)?;
        self.consume_optional_semi();

        let end_span = self.current().span;
        Ok(FlowDef { name, target, transitions, span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column) })
    }

    fn parse_state_field_path(&mut self) -> Result<StateFieldPath> {
        let start_span = self.current().span;
        let base = self.parse_name_path()?;
        self.expect(TokenKind::Dot)?;
        let field = self.parse_name()?;
        let end_span = self.current().span;
        Ok(StateFieldPath { base, field, span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column) })
    }

    fn parse_type_policy_decls(&mut self) -> Result<TypePolicyDecls> {
        let mut policy = TypePolicyDecls::default();
        loop {
            self.skip_newlines();
            let Some(name) = self.current_identifier() else {
                break;
            };
            match name.as_str() {
                "with_default_hash_type" => {
                    if policy.default_hash_type.is_some() {
                        return Err(crate::error::CompileError::new(
                            "duplicate with_default_hash_type declaration",
                            self.current().span,
                        ));
                    }
                    policy.default_hash_type = Some(self.parse_default_hash_type_decl()?);
                }
                "with_capacity_floor" => {
                    if policy.capacity_floor.is_some() {
                        return Err(crate::error::CompileError::new("duplicate with_capacity_floor declaration", self.current().span));
                    }
                    policy.capacity_floor = Some(self.parse_capacity_floor_decl()?);
                }
                "identity" => {
                    if policy.identity.is_some() {
                        return Err(crate::error::CompileError::new("duplicate identity declaration", self.current().span));
                    }
                    policy.identity = Some(self.parse_identity_decl()?);
                }
                _ => break,
            }
        }
        Ok(policy)
    }

    fn parse_default_hash_type_decl(&mut self) -> Result<HashTypeDecl> {
        let start_span = self.current().span;
        self.advance();
        self.expect(TokenKind::LParen)?;
        let value = self.parse_name()?;
        self.expect(TokenKind::RParen)?;
        let normalized = normalize_hash_type_decl(&value).ok_or_else(|| {
            crate::error::CompileError::new(
                format!("unsupported CKB hash_type '{}'; expected Data, Data1, Data2, or Type", value),
                start_span,
            )
        })?;
        Ok(HashTypeDecl { value: normalized, span: start_span })
    }

    fn parse_capacity_floor_decl(&mut self) -> Result<CapacityFloorDecl> {
        let start_span = self.current().span;
        self.advance();
        self.expect(TokenKind::LParen)?;
        let shannons = match &self.current().kind {
            TokenKind::Integer(value) => {
                let value = *value;
                self.advance();
                value
            }
            _ => {
                return Err(crate::error::CompileError::new(
                    "expected integer shannon value in with_capacity_floor(...)",
                    self.current().span,
                ));
            }
        };
        self.expect(TokenKind::RParen)?;
        if shannons == 0 {
            return Err(crate::error::CompileError::new("capacity floor must be greater than zero shannons", start_span));
        }
        Ok(CapacityFloorDecl { shannons, span: start_span })
    }

    fn parse_identity_decl(&mut self) -> Result<IdentityPolicy> {
        self.advance(); // consume "identity"
        if self.check(&TokenKind::LParen) {
            self.advance();
            let (kind, kind_span) = self.parse_name_with_span()?;
            let policy = match kind.as_str() {
                "ckb_type_id" => IdentityPolicy::CkbTypeId,
                "field" => {
                    self.expect(TokenKind::LParen)?;
                    let path = self.parse_name()?;
                    self.expect(TokenKind::RParen)?;
                    IdentityPolicy::Field(path)
                }
                "script_args" => IdentityPolicy::ScriptArgs,
                "singleton_type" => IdentityPolicy::SingletonType,
                "none" => IdentityPolicy::None,
                other => {
                    return Err(crate::error::CompileError::new(
                        format!(
                            "unsupported identity policy '{}'; expected none, ckb_type_id, field(...), script_args, or singleton_type",
                            other
                        ),
                        kind_span,
                    ));
                }
            };
            self.expect(TokenKind::RParen)?;
            Ok(policy)
        } else {
            // `identity ckb_type_id` without parens
            let (kind, kind_span) = self.parse_name_with_span()?;
            match kind.as_str() {
                "ckb_type_id" => Ok(IdentityPolicy::CkbTypeId),
                "script_args" => Ok(IdentityPolicy::ScriptArgs),
                "singleton_type" => Ok(IdentityPolicy::SingletonType),
                "none" => Ok(IdentityPolicy::None),
                other => Err(crate::error::CompileError::new(
                    format!("unsupported identity policy '{}'; expected none, ckb_type_id, script_args, or singleton_type", other),
                    kind_span,
                )),
            }
        }
    }

    fn parse_const(&mut self) -> Result<ConstDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Const)?;
        let name = self.parse_name_path()?;
        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type()?;
        self.expect(TokenKind::Eq)?;
        let value = self.parse_expr()?;
        self.consume_optional_semi();

        let end_span = self.current().span;
        Ok(ConstDef { name, ty, value, span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column) })
    }

    fn parse_enum(&mut self) -> Result<EnumDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Enum)?;
        let name = self.parse_name_path()?;
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();

        let mut variants = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let variant_start = self.current().span;
            let variant_name = self.parse_name_path()?;
            let fields = if self.check(&TokenKind::LParen) {
                self.advance();
                let items = self.parse_type_list(TokenKind::RParen)?;
                self.expect(TokenKind::RParen)?;
                items
            } else {
                Vec::new()
            };
            let variant_end = self.current().span;
            variants.push(EnumVariant {
                name: variant_name,
                fields,
                span: Span::new(variant_start.start, variant_end.end, variant_start.line, variant_start.column),
            });

            if self.check(&TokenKind::Comma) {
                self.advance();
            }
            self.skip_newlines();
        }

        self.expect(TokenKind::RBrace)?;
        self.consume_optional_semi();

        let end_span = self.current().span;
        Ok(EnumDef { name, variants, span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column) })
    }

    fn parse_capabilities(&mut self) -> Result<Vec<Capability>> {
        let mut caps = Vec::new();

        if self.check(&TokenKind::Has) {
            self.advance();

            loop {
                match &self.current().kind {
                    TokenKind::Store => {
                        caps.push(Capability::Store);
                        self.advance();
                    }
                    TokenKind::Transfer => {
                        caps.push(Capability::Transfer);
                        self.advance();
                    }
                    TokenKind::DestroyKw => {
                        caps.push(Capability::Destroy);
                        self.advance();
                    }
                    TokenKind::Create => {
                        caps.push(Capability::Create);
                        self.advance();
                    }
                    TokenKind::Consume => {
                        caps.push(Capability::Consume);
                        self.advance();
                    }
                    TokenKind::ReadRef => {
                        caps.push(Capability::ReadRef);
                        self.advance();
                    }
                    // v0.15 context-sensitive capability keywords (identifiers)
                    TokenKind::Identifier(s) if s == "replace" => {
                        caps.push(Capability::Replace);
                        self.advance();
                    }
                    TokenKind::Identifier(s) if s == "burn" => {
                        caps.push(Capability::Burn);
                        self.advance();
                    }
                    TokenKind::Identifier(s) if s == "relock" => {
                        caps.push(Capability::Relock);
                        self.advance();
                    }
                    TokenKind::Identifier(s) if s == "retarget_type" => {
                        caps.push(Capability::RetargetType);
                        self.advance();
                    }
                    _ => break,
                }

                if self.check(&TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        Ok(caps)
    }

    fn parse_fields(&mut self) -> Result<Vec<Field>> {
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();

        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let field = self.parse_field()?;
            fields.push(field);
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
            self.skip_newlines();
        }

        self.expect(TokenKind::RBrace)?;
        Ok(fields)
    }

    fn parse_field(&mut self) -> Result<Field> {
        let start_span = self.current().span;

        let name = self.parse_name()?;

        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type()?;

        let end_span = self.current().span;
        Ok(Field { name, ty, span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column) })
    }

    fn parse_invariant(&mut self) -> Result<InvariantDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Invariant)?;
        let name = self.parse_name()?;
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();

        let mut trigger = None;
        let mut scope = None;
        let mut reads = Vec::new();
        let mut aggregates = Vec::new();
        let mut asserts = Vec::new();

        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            self.skip_newlines();
            if self.check(&TokenKind::RBrace) {
                break;
            }

            if self.check(&TokenKind::Assert) {
                if self.check(&TokenKind::Assert) && !self.check_invariant_call_assert() {
                    asserts.push(self.parse_invariant_assert()?);
                } else {
                    asserts.push(self.parse_expr()?);
                }
                self.consume_optional_semi();
                self.skip_newlines();
                continue;
            }

            if let Some(name) = self.ident_like_name() {
                if Self::is_invariant_aggregate_start(&name) {
                    aggregates.push(self.parse_aggregate_invariant()?);
                    self.consume_optional_semi();
                    self.skip_newlines();
                    continue;
                }
            }

            let key_span = self.current().span;
            let key = self.ident_like_name().ok_or_else(|| {
                CompileError::new("expected invariant field, aggregate assertion, or assert_invariant", self.current().span)
            })?;
            self.advance();
            self.expect(TokenKind::Colon)?;

            match key.as_str() {
                "trigger" => {
                    if trigger.is_some() {
                        return Err(CompileError::new("duplicate invariant trigger declaration", key_span));
                    }
                    trigger = Some(self.parse_name_path()?);
                }
                "scope" => {
                    if scope.is_some() {
                        return Err(CompileError::new("duplicate invariant scope declaration", key_span));
                    }
                    scope = Some(self.parse_name_path()?);
                }
                "reads" => {
                    reads.extend(self.parse_invariant_reads()?);
                }
                _ => {
                    return Err(CompileError::new(
                        format!("unknown invariant field '{}'; expected trigger, scope, reads, or assert_invariant", key),
                        key_span,
                    ));
                }
            }

            self.consume_optional_semi();
            self.skip_newlines();
        }

        let end_span = self.current().span;
        self.expect(TokenKind::RBrace)?;
        Ok(InvariantDef {
            name,
            trigger,
            scope,
            reads,
            aggregates,
            asserts,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn is_invariant_aggregate_start(name: &str) -> bool {
        matches!(name, "assert_sum" | "assert_conserved" | "assert_delta" | "assert_distinct" | "assert_singleton")
    }

    fn parse_aggregate_invariant(&mut self) -> Result<AggregateInvariant> {
        let start_span = self.current().span;
        let name = self.parse_name()?;
        if name == "assert_sum" {
            return self.parse_aggregate_sum_comparison(start_span);
        }

        self.expect(TokenKind::LParen)?;
        self.skip_newlines();
        let target = self.parse_invariant_read()?;
        self.skip_newlines();
        self.expect(TokenKind::Comma)?;
        self.skip_newlines();

        let (kind, argument, scope) = match name.as_str() {
            "assert_conserved" => (AggregateInvariantKind::Conserved, None, self.parse_aggregate_scope_arg()?),
            "assert_distinct" => (AggregateInvariantKind::Distinct, None, self.parse_aggregate_scope_arg()?),
            "assert_singleton" => (AggregateInvariantKind::Singleton, None, self.parse_aggregate_scope_arg()?),
            "assert_delta" => {
                let delta = self.parse_aggregate_argument()?;
                self.skip_newlines();
                self.expect(TokenKind::Comma)?;
                self.skip_newlines();
                (AggregateInvariantKind::Delta, Some(delta), self.parse_aggregate_scope_arg()?)
            }
            _ => {
                return Err(CompileError::new(format!("unknown aggregate invariant primitive '{}'", name), start_span));
            }
        };

        self.skip_newlines();
        let end_span = self.current().span;
        self.expect(TokenKind::RParen)?;
        Ok(AggregateInvariant {
            kind,
            target,
            scope,
            argument,
            relation: None,
            rhs: None,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_aggregate_sum_comparison(&mut self, start_span: Span) -> Result<AggregateInvariant> {
        let left = self.parse_aggregate_sum_operand()?;
        self.skip_newlines();
        let relation = match self.current().kind {
            TokenKind::Lt => AggregateRelation::Lt,
            TokenKind::Le => AggregateRelation::Le,
            TokenKind::EqEq => AggregateRelation::Eq,
            TokenKind::Ge => AggregateRelation::Ge,
            TokenKind::Gt => AggregateRelation::Gt,
            _ => {
                return Err(CompileError::new("assert_sum aggregate assertion requires a comparison operator", self.current().span));
            }
        };
        self.advance();
        self.skip_newlines();

        let rhs_start = self.current().span;
        let rhs_name = self.parse_name()?;
        if rhs_name != "assert_sum" {
            return Err(CompileError::new("assert_sum comparison right-hand side must be assert_sum(...)", rhs_start));
        }
        let right = self.parse_aggregate_sum_operand()?;
        let scope = aggregate_scope_from_targets(&left, Some(&right)).ok_or_else(|| {
            CompileError::new("assert_sum aggregate assertion requires explicit input/output source views", start_span)
        })?;
        let end_span = self.current().span;

        Ok(AggregateInvariant {
            kind: AggregateInvariantKind::Sum,
            target: left,
            scope,
            argument: None,
            relation: Some(relation),
            rhs: Some(right),
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_aggregate_sum_operand(&mut self) -> Result<String> {
        self.expect(TokenKind::LParen)?;
        self.skip_newlines();
        let target = self.parse_invariant_read()?;
        self.skip_newlines();
        self.expect(TokenKind::RParen)?;
        Ok(target)
    }

    fn parse_aggregate_scope_arg(&mut self) -> Result<String> {
        let key_span = self.current().span;
        let key = self.parse_name()?;
        if key != "scope" {
            return Err(CompileError::new("aggregate invariant primitive requires `scope = ...`", key_span));
        }
        self.skip_newlines();
        self.expect(TokenKind::Eq)?;
        self.skip_newlines();
        self.parse_name_path()
    }

    fn parse_aggregate_argument(&mut self) -> Result<String> {
        match &self.current().kind {
            TokenKind::Integer(value) => {
                let rendered = value.to_string();
                self.advance();
                Ok(rendered)
            }
            TokenKind::String(value) => {
                let rendered = format!("{:?}", value);
                self.advance();
                Ok(rendered)
            }
            _ => self.parse_invariant_read(),
        }
    }

    fn check_invariant_call_assert(&self) -> bool {
        self.peek(1).kind == TokenKind::LParen || self.peek(1).kind == TokenKind::Not
    }

    fn parse_invariant_assert(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::Assert)?;
        let condition = self.parse_expr()?;
        let end_span = self.current().span;
        Ok(Expr::Assert(AssertExpr {
            condition: Box::new(condition),
            message: Box::new(Expr::String("invariant assertion failed".to_string(), start_span)),
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn parse_invariant_reads(&mut self) -> Result<Vec<String>> {
        let mut reads = Vec::new();
        self.skip_newlines();
        if self.check(&TokenKind::RBrace) || self.check(&TokenKind::Semi) {
            return Ok(reads);
        }

        loop {
            reads.push(self.parse_invariant_read()?);
            if self.check(&TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
                if self.check(&TokenKind::RBrace) || self.check(&TokenKind::Semi) {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(reads)
    }

    fn parse_invariant_read(&mut self) -> Result<String> {
        let mut read = self.parse_name_path()?;
        if self.check(&TokenKind::Lt) {
            self.advance();
            let ty = self.parse_type()?;
            self.expect(TokenKind::Gt)?;
            read.push('<');
            read.push_str(&Self::render_type(&ty));
            read.push('>');
        }

        while self.check(&TokenKind::Dot) {
            self.advance();
            read.push('.');
            read.push_str(&self.parse_name()?);
        }

        Ok(read)
    }

    fn parse_type(&mut self) -> Result<Type> {
        let span = self.current().span;
        self.with_recursion_guard(span, |this| this.parse_type_inner())
    }

    fn parse_type_inner(&mut self) -> Result<Type> {
        let ty = match &self.current().kind {
            TokenKind::U8 => {
                self.advance();
                Type::U8
            }
            TokenKind::U16 => {
                self.advance();
                Type::U16
            }
            TokenKind::U32 => {
                self.advance();
                Type::U32
            }
            TokenKind::U64 => {
                self.advance();
                Type::U64
            }
            TokenKind::U128 => {
                self.advance();
                Type::U128
            }
            TokenKind::Bool => {
                self.advance();
                Type::Bool
            }
            TokenKind::Address => {
                self.advance();
                Type::Address
            }
            TokenKind::Hash => {
                self.advance();
                Type::Hash
            }
            TokenKind::Identifier(_) | TokenKind::Launch => {
                let mut name = self.parse_name_path()?;
                if self.check(&TokenKind::Lt) {
                    self.advance();
                    let args = self.parse_type_list(TokenKind::Gt)?;
                    self.expect(TokenKind::Gt)?;
                    let rendered_args = args.iter().map(Self::render_type).collect::<Vec<_>>().join(", ");
                    name.push('<');
                    name.push_str(&rendered_args);
                    name.push('>');
                }
                Type::Named(name)
            }
            TokenKind::LBracket => {
                self.advance();
                let elem_ty = self.parse_type()?;
                self.expect(TokenKind::Semi)?;
                let size = match &self.current().kind {
                    TokenKind::Integer(n) => {
                        let size = usize::try_from(*n)
                            .map_err(|_| CompileError::new("array size does not fit this target", self.current().span))?;
                        self.advance();
                        size
                    }
                    _ => {
                        return Err(CompileError::new("expected array size", self.current().span));
                    }
                };
                self.expect(TokenKind::RBracket)?;
                Type::Array(Box::new(elem_ty), size)
            }
            TokenKind::LParen => {
                self.advance();
                let elems = self.parse_type_list(TokenKind::RParen)?;
                self.expect(TokenKind::RParen)?;
                Type::Tuple(elems)
            }
            TokenKind::Ampersand => {
                self.advance();
                if self.check(&TokenKind::Mut) {
                    self.advance();
                    Type::MutRef(Box::new(self.parse_type()?))
                } else {
                    Type::Ref(Box::new(self.parse_type()?))
                }
            }
            TokenKind::ReadRef => {
                return Err(CompileError::new(
                    "`read_ref` is not a type qualifier; use `read name: T` for action/lock parameters or `read_ref<T>()` as an expression",
                    self.current().span,
                ));
            }
            _ => {
                return Err(CompileError::new(format!("expected type, found {}", self.current().kind), self.current().span));
            }
        };

        Ok(ty)
    }

    fn render_type(ty: &Type) -> String {
        match ty {
            Type::U8 => "u8".to_string(),
            Type::U16 => "u16".to_string(),
            Type::U32 => "u32".to_string(),
            Type::U64 => "u64".to_string(),
            Type::U128 => "u128".to_string(),
            Type::Bool => "bool".to_string(),
            Type::Unit => "()".to_string(),
            Type::Address => "Address".to_string(),
            Type::Hash => "Hash".to_string(),
            Type::Array(elem, size) => format!("[{}; {}]", Self::render_type(elem), size),
            Type::Tuple(types) => format!("({})", types.iter().map(Self::render_type).collect::<Vec<_>>().join(", ")),
            Type::Named(name) => name.clone(),
            Type::Ref(inner) => format!("&{}", Self::render_type(inner)),
            Type::MutRef(inner) => format!("&mut {}", Self::render_type(inner)),
        }
    }

    fn parse_action(&mut self, effect: Option<EffectClass>, scheduler_hint: Option<SchedulerHint>) -> Result<ActionDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Action)?;

        let name = self.parse_name()?;

        let params = self.parse_params()?;

        let (return_type, outputs) = if self.check(&TokenKind::Arrow) {
            self.advance();
            self.parse_action_output_signature()?
        } else {
            (None, Vec::new())
        };

        let state_edges = self.parse_action_clauses()?;
        let body = self.parse_action_where_block()?;

        let end_span = self.current().span;
        let effect_declared = effect.is_some();

        Ok(ActionDef {
            name,
            params,
            return_type,
            outputs,
            state_edges,
            body,
            effect: effect.unwrap_or(EffectClass::Pure),
            effect_declared,
            scheduler_hint,
            doc_comment: None,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_action_output_signature(&mut self) -> Result<(Option<Type>, Vec<ActionOutput>)> {
        self.skip_newlines();

        if self.check(&TokenKind::LParen) && self.action_output_tuple_looks_named() {
            self.advance();
            self.skip_newlines();
            let mut outputs = Vec::new();
            while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                outputs.push(self.parse_action_output_binding()?);
                if self.check(&TokenKind::Comma) {
                    self.advance();
                    self.skip_newlines();
                } else {
                    break;
                }
            }
            self.expect(TokenKind::RParen)?;
            return Ok((None, outputs));
        }

        if self.action_output_looks_named() {
            return Ok((None, vec![self.parse_action_output_binding()?]));
        }

        Ok((Some(self.parse_type()?), Vec::new()))
    }

    fn action_output_tuple_looks_named(&self) -> bool {
        matches!((&self.peek(1).kind, &self.peek(2).kind), (TokenKind::Identifier(_), TokenKind::Colon))
    }

    fn action_output_looks_named(&self) -> bool {
        matches!((&self.current().kind, &self.peek(1).kind), (TokenKind::Identifier(_), TokenKind::Colon))
    }

    fn parse_action_output_binding(&mut self) -> Result<ActionOutput> {
        let start_span = self.current().span;
        let name = self.parse_name()?;
        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type()?;
        let end_span = self.current().span;
        Ok(ActionOutput { name, ty, span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column) })
    }

    fn parse_action_clauses(&mut self) -> Result<Vec<ActionStateEdge>> {
        let mut state_edges = Vec::new();

        loop {
            self.skip_newlines();
            match &self.current().kind {
                TokenKind::Transition => {
                    state_edges.extend(self.parse_action_state_edges()?);
                }
                TokenKind::Identifier(name) if name == "move" => {
                    return Err(CompileError::new("state edge clauses use 'transition', not legacy 'move'", self.current().span));
                }
                _ => break,
            }
        }

        Ok(state_edges)
    }

    fn parse_action_where_block(&mut self) -> Result<Vec<Stmt>> {
        if !self.check(&TokenKind::Where) {
            if self.check(&TokenKind::LBrace) {
                return Err(CompileError::new(
                    "action proof blocks use `where`; `{ ... }` action bodies are not part of the current syntax",
                    self.current().span,
                ));
            }
            return Ok(Vec::new());
        }

        self.advance();
        self.skip_newlines();

        let mut stmts = Vec::new();
        while !self.check(&TokenKind::Eof) && !self.action_where_block_is_done() {
            stmts.push(self.parse_stmt()?);
            self.skip_newlines();
        }

        Ok(stmts)
    }

    fn action_where_block_is_done(&self) -> bool {
        if self.current().span.column != 1 {
            return false;
        }

        matches!(
            &self.current().kind,
            TokenKind::Pound
                | TokenKind::Module
                | TokenKind::Use
                | TokenKind::Resource
                | TokenKind::Shared
                | TokenKind::Receipt
                | TokenKind::Struct
                | TokenKind::Const
                | TokenKind::Enum
                | TokenKind::Action
                | TokenKind::Fn
                | TokenKind::Lock
        ) || self.looks_like_top_level_flow_decl()
            || self.looks_like_top_level_type_policy_decl()
    }

    fn looks_like_top_level_flow_decl(&self) -> bool {
        let TokenKind::Identifier(name) = &self.current().kind else {
            return false;
        };
        if name != "flow" {
            return false;
        }

        let mut offset = 1;
        let mut saw_target_separator = false;
        loop {
            match &self.peek(offset).kind {
                TokenKind::Identifier(_) | TokenKind::Resource | TokenKind::Shared | TokenKind::Receipt | TokenKind::Struct => {
                    offset += 1;
                }
                TokenKind::ColonColon => offset += 1,
                TokenKind::For => {
                    saw_target_separator = true;
                    offset += 1;
                }
                TokenKind::Dot => {
                    saw_target_separator = true;
                    offset += 1;
                }
                TokenKind::LBrace => return saw_target_separator,
                _ => return false,
            }
        }
    }

    fn looks_like_top_level_type_policy_decl(&self) -> bool {
        matches!(
            (&self.current().kind, &self.peek(1).kind),
            (TokenKind::Identifier(name), TokenKind::LParen)
                if matches!(name.as_str(), "with_default_hash_type" | "with_capacity_floor")
        )
    }

    fn parse_action_state_edges(&mut self) -> Result<Vec<ActionStateEdge>> {
        if !self.check(&TokenKind::Transition) {
            return Ok(Vec::new());
        }

        let mut edges = Vec::new();
        while self.check(&TokenKind::Transition) {
            self.advance();
            self.skip_newlines();
            if self.check(&TokenKind::LBrace) {
                let block_span = self.current().span;
                self.advance();
                let mut block_edge_count = 0usize;
                loop {
                    self.skip_newlines();
                    if self.check(&TokenKind::RBrace) {
                        if block_edge_count == 0 {
                            return Err(CompileError::new("transition block must contain at least one state edge", block_span));
                        }
                        self.advance();
                        break;
                    }
                    if self.check(&TokenKind::Eof) {
                        return Err(CompileError::new("unterminated transition block", self.current().span));
                    }
                    edges.push(self.parse_action_state_edge()?);
                    block_edge_count += 1;
                    self.consume_optional_semi();
                    if self.check(&TokenKind::Comma) {
                        self.advance();
                    }
                }
                self.skip_newlines();
                continue;
            }
            loop {
                edges.push(self.parse_action_state_edge()?);

                if self.check(&TokenKind::Comma) {
                    self.advance();
                    continue;
                }
                break;
            }
            self.skip_newlines();
        }

        Ok(edges)
    }

    fn parse_action_state_edge(&mut self) -> Result<ActionStateEdge> {
        let start_span = self.current().span;
        let first = self.parse_name_path()?;
        if !self.check(&TokenKind::Dot) {
            return Err(CompileError::new(
                "state transition must use an explicit input field path: transition input.state: From -> output.state: To",
                start_span,
            ));
        }
        self.advance();
        let field = self.parse_name()?;
        let path_span = Span::new(start_span.start, self.current().span.end, start_span.line, start_span.column);
        let path = StateFieldPath { base: first, field, span: path_span };
        if !self.check(&TokenKind::Colon) {
            return Err(CompileError::new(
                "state transition must put ':' before the source state: transition input.state: From -> output.state: To",
                self.current().span,
            ));
        }
        self.advance();
        let from = self.parse_name_path()?;
        self.expect(TokenKind::Arrow)?;
        let to_start_span = self.current().span;
        let to_first = self.parse_name_path()?;
        if !self.check(&TokenKind::Dot) {
            return Err(CompileError::new(
                "state transition must use an explicit output field path: transition input.state: From -> output.state: To",
                to_start_span,
            ));
        }
        self.advance();
        let to_field = self.parse_name()?;
        let to_path_span = Span::new(to_start_span.start, self.current().span.end, to_start_span.line, to_start_span.column);
        let to_path = StateFieldPath { base: to_first, field: to_field, span: to_path_span };
        if !self.check(&TokenKind::Colon) {
            return Err(CompileError::new(
                "state transition must put ':' before the target state: transition input.state: From -> output.state: To",
                self.current().span,
            ));
        }
        self.advance();
        let to = self.parse_name_path()?;
        let end_span = self.current().span;
        Ok(ActionStateEdge {
            path,
            to_path,
            from,
            to,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_fn(&mut self) -> Result<FnDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Fn)?;

        let name = self.parse_name()?;
        let params = self.parse_params()?;
        let return_type = if self.check(&TokenKind::Arrow) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = self.parse_block()?;
        let end_span = self.current().span;

        Ok(FnDef {
            name,
            params,
            return_type,
            body,
            doc_comment: None,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_lock(&mut self) -> Result<LockDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Lock)?;

        let name = self.parse_name()?;

        let params = self.parse_params()?;
        let return_type = if self.check(&TokenKind::Arrow) {
            self.advance();
            self.parse_type()?
        } else {
            Type::Bool
        };
        let body = self.parse_block()?;

        let end_span = self.current().span;
        Ok(LockDef {
            name,
            params,
            return_type,
            body,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_params(&mut self) -> Result<Vec<Param>> {
        self.expect(TokenKind::LParen)?;

        let mut params = Vec::new();
        loop {
            self.skip_newlines();
            if self.check(&TokenKind::RParen) || self.check(&TokenKind::Eof) {
                break;
            }
            let param = self.parse_param()?;
            params.push(param);

            if self.check(&TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        self.skip_newlines();
        self.expect(TokenKind::RParen)?;
        Ok(params)
    }

    fn parse_param(&mut self) -> Result<Param> {
        let start_span = self.current().span;

        if self.check(&TokenKind::Ref) {
            return Err(CompileError::new(
                "parameter modifier 'ref' is reserved but unsupported; use '&T' for read-only helper views, or `action(before: T) -> after: T` plus `transition` and `require` constraints for Cell state updates",
                self.current().span,
            ));
        }
        let is_ref = false;

        let is_mut = if self.check(&TokenKind::Mut) {
            self.advance();
            true
        } else {
            false
        };

        let (prefix_source, prefix_read_ref) = self.parse_param_source_prefix()?;
        let name = self.parse_name()?;

        self.expect(TokenKind::Colon)?;
        let postfix_source = if prefix_source == ParamSource::Default && !prefix_read_ref {
            self.parse_param_source_marker()
        } else {
            ParamSource::Default
        };
        let source = if postfix_source == ParamSource::Default { prefix_source } else { postfix_source };
        let is_read_ref = prefix_read_ref;
        let mut ty = if source == ParamSource::Protected {
            if is_read_ref {
                return Err(CompileError::new(
                    "protected parameters already denote the lock's protected read-only Cell view; remove 'read_ref'",
                    self.current().span,
                ));
            }
            let protected_ty = self.parse_type()?;
            match protected_ty {
                Type::Ref(_) | Type::MutRef(_) => {
                    return Err(CompileError::new(
                        "protected parameters use 'protected name: T', not 'protected name: &T'",
                        self.current().span,
                    ));
                }
                ty => Type::Ref(Box::new(ty)),
            }
        } else {
            self.parse_type()?
        };
        if prefix_read_ref && !matches!(ty, Type::Ref(_)) {
            ty = Type::Ref(Box::new(ty));
        }

        let end_span = self.current().span;
        Ok(Param {
            name,
            ty,
            is_mut,
            is_ref,
            is_read_ref,
            source,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_param_source_prefix(&mut self) -> Result<(ParamSource, bool)> {
        let current_name = Self::token_ident_like_name(&self.current().kind);
        let next_is_name = Self::token_ident_like_name(&self.peek(1).kind).is_some();
        let next_next_is_colon = matches!(&self.peek(2).kind, TokenKind::Colon);
        if !next_is_name || !next_next_is_colon {
            return Ok((ParamSource::Default, false));
        }
        match current_name.as_deref() {
            Some("read") => {
                self.advance();
                Ok((ParamSource::Default, true))
            }
            Some("input") => {
                self.advance();
                Ok((ParamSource::Input, false))
            }
            Some("output") => Err(CompileError::new(
                "`output name: T` is only valid on the action return side; write `action f(input: T) -> output: T`",
                self.current().span,
            )),
            Some("protected") => {
                self.advance();
                Ok((ParamSource::Protected, false))
            }
            Some("witness") => {
                self.advance();
                Ok((ParamSource::Witness, false))
            }
            Some("lock_args") => {
                self.advance();
                Ok((ParamSource::LockArgs, false))
            }
            _ => Ok((ParamSource::Default, false)),
        }
    }

    fn parse_param_source_marker(&mut self) -> ParamSource {
        let source = match &self.current().kind {
            TokenKind::Identifier(name) if name == "protected" => ParamSource::Protected,
            TokenKind::Identifier(name) if name == "witness" => ParamSource::Witness,
            TokenKind::Identifier(name) if name == "lock_args" => ParamSource::LockArgs,
            _ => ParamSource::Default,
        };
        if source != ParamSource::Default {
            self.advance();
        }
        source
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>> {
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();

        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            stmts.push(self.parse_stmt()?);
            self.skip_newlines();
        }

        self.expect(TokenKind::RBrace)?;
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> Result<Stmt> {
        let stmt = match &self.current().kind {
            TokenKind::Let => Stmt::Let(self.parse_let()?),
            TokenKind::Return => Stmt::Return(self.parse_return()?),
            TokenKind::If => Stmt::If(self.parse_if()?),
            TokenKind::For => Stmt::For(self.parse_for()?),
            TokenKind::While => Stmt::While(self.parse_while()?),
            _ => {
                let expr = self.parse_expr()?;
                Stmt::Expr(expr)
            }
        };
        self.consume_optional_semi();
        Ok(stmt)
    }

    fn parse_let(&mut self) -> Result<LetStmt> {
        let start_span = self.current().span;
        self.expect(TokenKind::Let)?;

        let is_mut = if self.check(&TokenKind::Mut) {
            self.advance();
            true
        } else {
            false
        };

        let pattern = self.parse_binding_pattern()?;

        let ty = if self.check(&TokenKind::Colon) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(TokenKind::Eq)?;
        let value = self.parse_expr()?;

        let end_span = self.current().span;
        Ok(LetStmt { pattern, ty, value, is_mut, span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column) })
    }

    fn parse_return(&mut self) -> Result<Option<Expr>> {
        self.expect(TokenKind::Return)?;

        if self.check(&TokenKind::Newline) || self.check(&TokenKind::RBrace) || self.check(&TokenKind::Eof) {
            Ok(None)
        } else {
            Ok(Some(self.parse_expr()?))
        }
    }

    fn parse_if(&mut self) -> Result<IfStmt> {
        let span = self.current().span;
        self.with_recursion_guard(span, |this| this.parse_if_inner())
    }

    fn parse_if_inner(&mut self) -> Result<IfStmt> {
        let start_span = self.current().span;
        self.expect(TokenKind::If)?;

        let condition = self.parse_expr()?;
        let then_branch = self.parse_block()?;

        let else_branch = if self.check(&TokenKind::Else) {
            self.advance();
            if self.check(&TokenKind::If) {
                // else if
                let inner_if = self.parse_if()?;
                Some(vec![Stmt::If(inner_if)])
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };

        let end_span = self.current().span;
        Ok(IfStmt {
            condition,
            then_branch,
            else_branch,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_for(&mut self) -> Result<ForStmt> {
        let start_span = self.current().span;
        self.expect(TokenKind::For)?;

        let pattern = self.parse_binding_pattern()?;

        self.expect(TokenKind::In)?;
        let iterable = self.parse_expr()?;
        let body = self.parse_block()?;

        let end_span = self.current().span;
        Ok(ForStmt { pattern, iterable, body, span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column) })
    }

    fn parse_while(&mut self) -> Result<WhileStmt> {
        let start_span = self.current().span;
        self.expect(TokenKind::While)?;

        let condition = self.parse_expr()?;
        let body = self.parse_block()?;

        let end_span = self.current().span;
        Ok(WhileStmt { condition, body, span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column) })
    }

    fn parse_expr(&mut self) -> Result<Expr> {
        let span = self.current().span;
        self.with_recursion_guard(span, |this| this.parse_expr_inner())
    }

    fn parse_expr_inner(&mut self) -> Result<Expr> {
        self.skip_newlines();
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr> {
        let left = self.parse_range()?;
        let start_span = self.current().span;

        if self.check(&TokenKind::Eq) {
            self.advance();
            let right = self.parse_expr()?;
            return Ok(Expr::Assign(AssignExpr {
                target: Box::new(left),
                op: AssignOp::Assign,
                value: Box::new(right),
                span: start_span,
            }));
        }

        if self.check(&TokenKind::Plus) && self.peek(1).kind == TokenKind::Eq {
            self.advance();
            self.advance();
            let right = self.parse_expr()?;
            return Ok(Expr::Assign(AssignExpr {
                target: Box::new(left),
                op: AssignOp::AddAssign,
                value: Box::new(right),
                span: start_span,
            }));
        }

        Ok(left)
    }

    fn parse_range(&mut self) -> Result<Expr> {
        let left = self.parse_or()?;
        if self.check(&TokenKind::Dot) && self.peek(1).kind == TokenKind::Dot {
            let start_span = self.current().span;
            self.advance();
            self.advance();
            let right = self.parse_or()?;
            Ok(Expr::Range(RangeExpr { start: Box::new(left), end: Box::new(right), span: start_span }))
        } else {
            Ok(left)
        }
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut left = self.parse_and()?;

        loop {
            self.skip_newlines();
            if !self.check(&TokenKind::Or) {
                break;
            }
            let op = BinaryOp::Or;
            let start_span = self.current().span;
            self.advance();
            self.skip_newlines();
            let right = self.parse_and()?;
            left = Expr::Binary(BinaryExpr { op, left: Box::new(left), right: Box::new(right), span: start_span });
        }

        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_equality()?;

        loop {
            self.skip_newlines();
            if !self.check(&TokenKind::And) {
                break;
            }
            let op = BinaryOp::And;
            let start_span = self.current().span;
            self.advance();
            self.skip_newlines();
            let right = self.parse_equality()?;
            left = Expr::Binary(BinaryExpr { op, left: Box::new(left), right: Box::new(right), span: start_span });
        }

        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr> {
        let mut left = self.parse_comparison()?;

        loop {
            self.skip_newlines();
            let op = if self.check(&TokenKind::EqEq) {
                let start_span = self.current().span;
                self.advance();
                Some((BinaryOp::Eq, start_span))
            } else if self.check(&TokenKind::NotEq) {
                let start_span = self.current().span;
                self.advance();
                Some((BinaryOp::Ne, start_span))
            } else {
                None
            };

            if let Some((op, start_span)) = op {
                self.skip_newlines();
                let right = self.parse_comparison()?;
                left = Expr::Binary(BinaryExpr { op, left: Box::new(left), right: Box::new(right), span: start_span });
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr> {
        let mut left = self.parse_term()?;

        loop {
            self.skip_newlines();
            let op = if self.check(&TokenKind::Lt) {
                let start_span = self.current().span;
                self.advance();
                Some((BinaryOp::Lt, start_span))
            } else if self.check(&TokenKind::Le) {
                let start_span = self.current().span;
                self.advance();
                Some((BinaryOp::Le, start_span))
            } else if self.check(&TokenKind::Gt) {
                let start_span = self.current().span;
                self.advance();
                Some((BinaryOp::Gt, start_span))
            } else if self.check(&TokenKind::Ge) {
                let start_span = self.current().span;
                self.advance();
                Some((BinaryOp::Ge, start_span))
            } else {
                None
            };

            if let Some((op, start_span)) = op {
                self.skip_newlines();
                let right = self.parse_term()?;
                left = Expr::Binary(BinaryExpr { op, left: Box::new(left), right: Box::new(right), span: start_span });
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn parse_term(&mut self) -> Result<Expr> {
        let mut left = self.parse_factor()?;

        loop {
            self.skip_newlines();
            let op = if self.check(&TokenKind::Plus) && !matches!(&self.peek(1).kind, TokenKind::Eq) {
                let start_span = self.current().span;
                self.advance();
                Some((BinaryOp::Add, start_span))
            } else if self.check(&TokenKind::Minus) {
                let start_span = self.current().span;
                self.advance();
                Some((BinaryOp::Sub, start_span))
            } else {
                None
            };

            if let Some((op, start_span)) = op {
                self.skip_newlines();
                let right = self.parse_factor()?;
                left = Expr::Binary(BinaryExpr { op, left: Box::new(left), right: Box::new(right), span: start_span });
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn parse_factor(&mut self) -> Result<Expr> {
        let mut left = self.parse_cast()?;

        loop {
            self.skip_newlines();
            let op = if self.check(&TokenKind::Star) {
                let start_span = self.current().span;
                self.advance();
                Some((BinaryOp::Mul, start_span))
            } else if self.check(&TokenKind::Slash) {
                let start_span = self.current().span;
                self.advance();
                Some((BinaryOp::Div, start_span))
            } else if self.check(&TokenKind::Percent) {
                let start_span = self.current().span;
                self.advance();
                Some((BinaryOp::Mod, start_span))
            } else {
                None
            };

            if let Some((op, start_span)) = op {
                self.skip_newlines();
                let right = self.parse_cast()?;
                left = Expr::Binary(BinaryExpr { op, left: Box::new(left), right: Box::new(right), span: start_span });
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn parse_cast(&mut self) -> Result<Expr> {
        let mut expr = self.parse_unary()?;
        loop {
            self.skip_newlines();
            let is_as = matches!(&self.current().kind, TokenKind::Identifier(name) if name == "as");
            if !is_as {
                break;
            }
            let start_span = self.current().span;
            self.advance();
            self.skip_newlines();
            let ty = self.parse_type()?;
            expr = Expr::Cast(CastExpr { expr: Box::new(expr), ty, span: start_span });
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        let span = self.current().span;
        self.with_recursion_guard(span, |this| this.parse_unary_inner())
    }

    fn parse_unary_inner(&mut self) -> Result<Expr> {
        if self.check(&TokenKind::Minus) {
            let start_span = self.current().span;
            self.advance();
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary(UnaryExpr { op: UnaryOp::Neg, expr: Box::new(expr), span: start_span }));
        }

        if self.check(&TokenKind::Not) {
            let start_span = self.current().span;
            self.advance();
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary(UnaryExpr { op: UnaryOp::Not, expr: Box::new(expr), span: start_span }));
        }

        if self.check(&TokenKind::Ampersand) {
            let start_span = self.current().span;
            self.advance();
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary(UnaryExpr { op: UnaryOp::Ref, expr: Box::new(expr), span: start_span }));
        }

        if self.check(&TokenKind::Star) {
            let start_span = self.current().span;
            self.advance();
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary(UnaryExpr { op: UnaryOp::Deref, expr: Box::new(expr), span: start_span }));
        }

        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut expr = self.parse_primary()?;

        loop {
            // Allow multiline method chains by skipping newlines before a dot,
            // but still break if the next non-newline token is not a postfix operator.
            while self.check(&TokenKind::Newline) {
                if self.peek(1).kind == TokenKind::Dot {
                    self.advance(); // skip the newline
                } else {
                    break;
                }
            }
            if self.check(&TokenKind::Newline) {
                break;
            }
            if self.check(&TokenKind::Dot) && !matches!(&self.peek(1).kind, TokenKind::Dot) {
                let start_span = expr.span();
                self.advance();
                let (field, end_span) = match &self.current().kind {
                    _ if self.ident_like_name().is_some() => {
                        let end_span = self.current().span;
                        (self.parse_name()?, end_span)
                    }
                    TokenKind::Integer(n) => {
                        let end_span = self.current().span;
                        let index = n.to_string();
                        self.advance();
                        (index, end_span)
                    }
                    _ => {
                        return Err(CompileError::new("expected field name", self.current().span));
                    }
                };
                expr = Expr::FieldAccess(FieldAccessExpr { expr: Box::new(expr), field, span: start_span.combine(&end_span) });
            } else if self.check(&TokenKind::LParen) {
                let start_span = expr.span();
                let (args, end_span) = self.parse_args_with_end_span()?;
                expr = Expr::Call(CallExpr { func: Box::new(expr), args, span: start_span.combine(&end_span) });
            } else if self.check(&TokenKind::LBracket) {
                let start_span = expr.span();
                self.advance();
                let index = self.parse_expr()?;
                let end_span = self.expect(TokenKind::RBracket)?.span;
                expr = Expr::Index(IndexExpr { expr: Box::new(expr), index: Box::new(index), span: start_span.combine(&end_span) });
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        match &self.current().kind {
            TokenKind::Integer(n) => {
                let span = self.current().span;
                let val = *n;
                self.advance();
                Ok(Expr::Integer(val, span))
            }
            TokenKind::HexLiteral(text) => {
                let span = self.current().span;
                let val = u64::from_str_radix(text, 16)
                    .map_err(|_| CompileError::new(format!("invalid hex integer literal: 0x{}", text), span))?;
                self.advance();
                Ok(Expr::Integer(val, span))
            }
            TokenKind::True => {
                let span = self.current().span;
                self.advance();
                Ok(Expr::Bool(true, span))
            }
            TokenKind::False => {
                let span = self.current().span;
                self.advance();
                Ok(Expr::Bool(false, span))
            }
            TokenKind::String(s) => {
                let span = self.current().span;
                let val = s.clone();
                self.advance();
                Ok(Expr::String(val, span))
            }
            TokenKind::ByteString(b) => {
                let span = self.current().span;
                let val = b.clone();
                self.advance();
                Ok(Expr::ByteString(val, span))
            }
            TokenKind::Create => self.parse_create(),
            TokenKind::Consume => self.parse_consume(),
            TokenKind::Transfer => self.parse_transfer_expr(),
            TokenKind::Preserve => self.parse_preserve(),
            TokenKind::DestroyKw => self.parse_destroy(),
            TokenKind::Launch => Err(CompileError::new(
                "launch is reserved for a post-v1 transaction builder and is not part of the executable language core; use explicit create/consume/destroy operations",
                self.current().span,
            )),
            TokenKind::ReadRef => self.parse_read_ref_expr(),
            TokenKind::If => self.parse_if_expr(),
            TokenKind::Match => self.parse_match_expr(),
            TokenKind::Assert => self.parse_assert(),
            TokenKind::Require => self.parse_require(),
            TokenKind::Std => self.parse_stdlib_call(),
            TokenKind::Claim => self.parse_claim_expr(),
            TokenKind::Settle => self.parse_settle_expr(),
            _ if self.ident_like_name().is_some() => {
                let start_span = self.current().span;
                let name = self.parse_name_path()?;
                // v0.15 destruction policy forms (context-sensitive identifiers)
                match name.as_str() {
                    "destroy_singleton_type" => return self.parse_destroy_singleton_type(),
                    "destroy_unique" => return self.parse_destroy_unique(),
                    "destroy_instance" => return self.parse_destroy_instance(),
                    "burn_amount" => return self.parse_burn_amount(),
                    "create_unique" => return self.parse_create_unique_expr(),
                    "replace_unique" => return self.parse_replace_unique_expr(),
                    _ => {}
                }
                if self.check(&TokenKind::LBrace) && Self::looks_like_type_name(&name) {
                    self.parse_struct_init(name, start_span)
                } else {
                    let end = self.current().span.start.max(start_span.end);
                    Ok(Expr::Identifier(name, Span::new(start_span.start, end, start_span.line, start_span.column)))
                }
            }
            TokenKind::LParen => {
                let start_span = self.current().span;
                self.advance();
                if self.check(&TokenKind::RParen) {
                    let end_span = self.current().span;
                    self.advance();
                    return Ok(Expr::Tuple(vec![], Span::new(start_span.start, end_span.end, start_span.line, start_span.column)));
                }
                let expr = self.parse_expr()?;
                if self.check(&TokenKind::Comma) {
                    let mut elems = vec![expr];
                    while self.check(&TokenKind::Comma) {
                        self.advance();
                        if self.check(&TokenKind::RParen) {
                            break;
                        }
                        elems.push(self.parse_expr()?);
                    }
                    let end_span = self.current().span;
                    self.expect(TokenKind::RParen)?;
                    Ok(Expr::Tuple(elems, Span::new(start_span.start, end_span.end, start_span.line, start_span.column)))
                } else {
                    self.expect(TokenKind::RParen)?;
                    Ok(expr)
                }
            }
            TokenKind::LBracket => self.parse_array_expr(),
            TokenKind::LBrace => {
                let start_span = self.current().span;
                let stmts = self.parse_block()?;
                let end = self.current().span.start.max(start_span.end);
                Ok(Expr::Block(stmts, Span::new(start_span.start, end, start_span.line, start_span.column)))
            }
            _ => Err(CompileError::new(format!("unexpected token in expression: {}", self.current().kind), self.current().span)),
        }
    }

    fn parse_array_expr(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::LBracket)?;
        let mut elems = Vec::new();
        loop {
            self.skip_newlines();
            if self.check(&TokenKind::RBracket) || self.check(&TokenKind::Eof) {
                break;
            }
            elems.push(self.parse_expr()?);
            self.skip_newlines();
            if self.check(&TokenKind::Comma) {
                self.advance();
                continue;
            }
            break;
        }
        let end_span = self.current().span;
        self.expect(TokenKind::RBracket)?;
        Ok(Expr::Array(elems, Span::new(start_span.start, end_span.end, start_span.line, start_span.column)))
    }

    fn parse_args_with_end_span(&mut self) -> Result<(Vec<Expr>, Span)> {
        self.expect(TokenKind::LParen)?;

        let mut args = Vec::new();
        loop {
            self.skip_newlines();
            if self.check(&TokenKind::RParen) || self.check(&TokenKind::Eof) {
                break;
            }
            args.push(self.parse_expr()?);
            if self.check(&TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        self.skip_newlines();
        let end_span = self.expect(TokenKind::RParen)?.span;
        Ok((args, end_span))
    }

    fn parse_create(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::Create)?;
        let first = self.parse_name_path()?;
        let (target, ty) = if self.check(&TokenKind::Eq) {
            self.advance();
            (Some(first), self.parse_name_path()?)
        } else {
            (None, first)
        };

        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();

        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let field_span = self.current().span;
            let field_name = self.parse_name()?;
            let value = if self.check(&TokenKind::Colon) {
                self.advance();
                self.parse_expr()?
            } else {
                Expr::Identifier(field_name.clone(), field_span)
            };
            fields.push((field_name, value));

            if self.check(&TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
            } else {
                self.skip_newlines();
            }
        }

        self.expect(TokenKind::RBrace)?;

        let lock = match &self.current().kind {
            TokenKind::Identifier(s) if s == "with_lock" => {
                self.advance();
                self.expect(TokenKind::LParen)?;
                let lock_expr = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Some(Box::new(lock_expr))
            }
            _ => None,
        };

        let end_span = self.current().span;
        Ok(Expr::Create(CreateExpr {
            target,
            ty,
            fields,
            lock,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn parse_consume(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::Consume)?;

        let expr = self.parse_expr()?;

        let end_span = self.current().span;
        Ok(Expr::Consume(ConsumeExpr {
            expr: Box::new(expr),
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn parse_transfer_expr(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::Transfer)?;

        let expr = self.parse_postfix()?;
        let to_span = self.current().span;
        let keyword = self.parse_name()?;
        if keyword != "to" {
            return Err(CompileError::new("expected 'to' in transfer expression", to_span));
        }
        let to = self.parse_expr()?;

        let end_span = self.current().span;
        Ok(Expr::Transfer(TransferExpr {
            expr: Box::new(expr),
            to: Box::new(to),
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn parse_claim_expr(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        let keyword = self.parse_name()?;
        if keyword != "claim" {
            return Err(CompileError::new("expected 'claim'", start_span));
        }
        let receipt = self.parse_expr()?;

        let end_span = self.current().span;
        Ok(Expr::Claim(ClaimExpr {
            receipt: Box::new(receipt),
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn parse_settle_expr(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        let keyword = self.parse_name()?;
        if keyword != "settle" {
            return Err(CompileError::new("expected 'settle'", start_span));
        }
        let expr = self.parse_expr()?;

        let end_span = self.current().span;
        Ok(Expr::Settle(SettleExpr {
            expr: Box::new(expr),
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn parse_destroy(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::DestroyKw)?;

        // Check for v0.15 destruction policy forms:
        //   destroy_singleton_type(cell)
        //   destroy_unique(cell, identity = type_id)
        //   destroy_instance(cell, identity_field = id)
        //   burn_amount(cell, field = amount)
        //   bare: destroy cell  (legacy compat → Default policy)
        let policy = if self.check(&TokenKind::LParen) {
            return Err(CompileError::new(
                "expected cell expression after destroy; for policy-specific forms use destroy_singleton_type, destroy_unique, destroy_instance, or burn_amount",
                self.current().span,
            ));
        } else {
            DestructionPolicy::Default
        };

        let expr = self.parse_expr()?;

        let end_span = self.current().span;
        Ok(Expr::Destroy(DestroyExpr {
            expr: Box::new(expr),
            policy,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn parse_destroy_singleton_type(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        // Already consumed the identifier "destroy_singleton_type"
        self.expect(TokenKind::LParen)?;
        let expr = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;

        let end_span = self.current().span;
        Ok(Expr::Destroy(DestroyExpr {
            expr: Box::new(expr),
            policy: DestructionPolicy::SingletonType,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn parse_destroy_unique(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        // Already consumed the identifier "destroy_unique"
        self.expect(TokenKind::LParen)?;
        let expr = self.parse_expr()?;
        self.expect(TokenKind::Comma)?;
        let identity = self.parse_named_arg("identity")?;
        self.expect(TokenKind::RParen)?;

        let end_span = self.current().span;
        Ok(Expr::Destroy(DestroyExpr {
            expr: Box::new(expr),
            policy: DestructionPolicy::Unique { identity },
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn parse_destroy_instance(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        // Already consumed the identifier "destroy_instance"
        self.expect(TokenKind::LParen)?;
        let expr = self.parse_expr()?;
        self.expect(TokenKind::Comma)?;
        let identity_field = self.parse_named_arg("identity_field")?;
        self.expect(TokenKind::RParen)?;

        let end_span = self.current().span;
        Ok(Expr::Destroy(DestroyExpr {
            expr: Box::new(expr),
            policy: DestructionPolicy::Instance { identity_field },
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn parse_burn_amount(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        // Already consumed the identifier "burn_amount"
        self.expect(TokenKind::LParen)?;
        let expr = self.parse_expr()?;
        self.expect(TokenKind::Comma)?;
        let field = self.parse_named_arg("field")?;
        self.expect(TokenKind::RParen)?;

        let end_span = self.current().span;
        Ok(Expr::Destroy(DestroyExpr {
            expr: Box::new(expr),
            policy: DestructionPolicy::BurnAmount { field },
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    /// Parse a named argument like `identity = type_id` or `field = amount`.
    fn parse_named_arg(&mut self, expected_name: &str) -> Result<String> {
        let (name, name_span) = self.parse_name_with_span()?;
        if name != expected_name {
            return Err(CompileError::new(format!("expected named argument '{}', got '{}'", expected_name, name), name_span));
        }
        self.expect(TokenKind::Eq)?;
        let value = self.parse_name()?;
        Ok(value)
    }

    fn parse_create_unique_expr(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        // Already consumed the identifier "create_unique"
        self.expect(TokenKind::Lt)?;
        let ty = Self::render_type(&self.parse_type()?);
        self.expect(TokenKind::Gt)?;
        self.expect(TokenKind::LParen)?;
        let identity = self.parse_identity_policy_from_args()?;
        self.expect(TokenKind::RParen)?;

        let fields = self.parse_field_init_list()?;

        let lock = if self.ident_like_name().as_deref() == Some("with_lock") {
            self.advance();
            self.expect(TokenKind::LParen)?;
            let lock_expr = self.parse_expr()?;
            self.expect(TokenKind::RParen)?;
            Some(Box::new(lock_expr))
        } else {
            None
        };

        let end_span = self.current().span;
        Ok(Expr::CreateUnique(CreateUniqueExpr {
            ty,
            fields,
            lock,
            identity,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn parse_replace_unique_expr(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        // Already consumed the identifier "replace_unique"
        self.expect(TokenKind::Lt)?;
        let ty = Self::render_type(&self.parse_type()?);
        self.expect(TokenKind::Gt)?;
        self.expect(TokenKind::LParen)?;
        let identity = self.parse_identity_policy_from_args()?;
        self.expect(TokenKind::RParen)?;

        let expr = self.parse_expr()?;

        let fields = self.parse_field_init_list()?;

        let end_span = self.current().span;
        Ok(Expr::ReplaceUnique(ReplaceUniqueExpr {
            expr: Box::new(expr),
            ty,
            fields,
            identity,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn parse_identity_policy_from_args(&mut self) -> Result<IdentityPolicy> {
        let (name, name_span) = self.parse_name_with_span()?;
        if name != "identity" {
            return Err(CompileError::new(format!("expected 'identity' named argument, got '{}'", name), name_span));
        }
        self.expect(TokenKind::Eq)?;
        let (kind, kind_span) = self.parse_name_with_span()?;
        match kind.as_str() {
            "ckb_type_id" => Ok(IdentityPolicy::CkbTypeId),
            "field" => {
                self.expect(TokenKind::LParen)?;
                let path = self.parse_name()?;
                self.expect(TokenKind::RParen)?;
                Ok(IdentityPolicy::Field(path))
            }
            "script_args" => Ok(IdentityPolicy::ScriptArgs),
            "singleton_type" => Ok(IdentityPolicy::SingletonType),
            "none" => Ok(IdentityPolicy::None),
            other => Err(CompileError::new(
                format!(
                    "unsupported identity policy '{}'; expected none, ckb_type_id, field(...), script_args, or singleton_type",
                    other
                ),
                kind_span,
            )),
        }
    }

    fn parse_field_init_list(&mut self) -> Result<Vec<(String, Expr)>> {
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let name_span = self.current().span;
            let name = self.parse_name_path()?;
            if self.check(&TokenKind::Colon) {
                self.advance();
                let value = self.parse_expr()?;
                fields.push((name, value));
            } else {
                // Field shorthand: `amount` means `amount: amount`
                fields.push((name.clone(), Expr::Identifier(name, name_span)));
            }
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::RBrace)?;
        Ok(fields)
    }

    fn parse_read_ref_expr(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::ReadRef)?;

        let ty = if self.check(&TokenKind::Lt) {
            self.advance();
            let ty = self.parse_type()?;
            self.expect(TokenKind::Gt)?;
            Self::render_type(&ty)
        } else {
            Self::render_type(&self.parse_type()?)
        };

        if self.check(&TokenKind::LParen) {
            self.advance();
            self.expect(TokenKind::RParen)?;
        }

        let end_span = self.current().span;
        Ok(Expr::ReadRef(ReadRefExpr { ty, span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column) }))
    }

    fn parse_assert(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::Assert)?;
        if self.check(&TokenKind::Not) {
            return Err(CompileError::new(
                "expected '(' after assert; CellScript assertions use assert(condition, message), not assert!(...)",
                self.current().span,
            ));
        }
        self.expect(TokenKind::LParen)?;
        self.skip_newlines();
        let condition = self.parse_expr()?;
        self.expect(TokenKind::Comma)?;
        self.skip_newlines();
        let message = self.parse_expr()?;
        self.skip_newlines();
        let end_span = self.current().span;
        self.expect(TokenKind::RParen)?;
        Ok(Expr::Assert(AssertExpr {
            condition: Box::new(condition),
            message: Box::new(message),
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn parse_require(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::Require)?;

        // Anonymous require block: require { expr\n expr\n }
        if self.check(&TokenKind::LBrace) {
            self.advance();
            self.skip_newlines();
            let mut expressions = Vec::new();
            while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
                let expr = self.parse_expr()?;
                // Validate: only pure boolean expressions allowed
                match &expr {
                    Expr::Consume(_) | Expr::Create(_) | Expr::Destroy(_) => {
                        return Err(CompileError::new(
                            "require block contains statement — only pure boolean expressions are allowed",
                            expr.span(),
                        )
                        .with_code("E1005"));
                    }
                    Expr::If(_) | Expr::Match(_) => {
                        return Err(CompileError::new(
                            "require block contains control flow — only pure boolean expressions are allowed",
                            expr.span(),
                        )
                        .with_code("E1006"));
                    }
                    _ => {}
                }
                expressions.push(expr);
                self.consume_optional_semi();
                self.skip_newlines();
            }
            if expressions.is_empty() {
                return Err(CompileError::new("require block is empty — at least one boolean expression is required", start_span)
                    .with_code("E1001"));
            }
            let end_span = self.current().span;
            self.expect(TokenKind::RBrace)?;
            return Ok(Expr::RequireBlock(RequireBlockExpr {
                expressions,
                span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
            }));
        }

        // Standard require: require condition[, message]
        let condition = self.parse_expr()?;
        let message = if self.check(&TokenKind::Comma) {
            self.advance();
            self.skip_newlines();
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };
        let end_span = self.current().span;
        Ok(Expr::Require(RequireExpr {
            condition: Box::new(condition),
            message,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    /// Parse `preserve output_name from input_name { field1, field2, ... }`
    fn parse_preserve(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::Preserve)?;

        let output_name = self.parse_name()?;

        // Expect "from"
        let from_marker =
            self.ident_like_name().ok_or_else(|| CompileError::new("expected 'from' in preserve expression", self.current().span))?;
        if from_marker != "from" {
            return Err(CompileError::new("expected 'from' in preserve expression", self.current().span));
        }
        self.advance(); // consume "from"

        let input_name = self.parse_name()?;

        // Require a field block — bare `preserve output from input` is not allowed
        if !self.check(&TokenKind::LBrace) {
            return Err(CompileError::new(
                "bare 'preserve' requires a field block — use 'preserve output from input { field1, field2 }'",
                self.current().span,
            )
            .with_code("E1009"));
        }
        self.advance(); // consume {
        self.skip_newlines();

        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            // Reject wildcard
            if self.check(&TokenKind::Star) {
                return Err(CompileError::new("preserve wildcard '*' is not allowed — use explicit whitelist", self.current().span)
                    .with_code("E1007"));
            }
            // Reject 'except' keyword used as blacklist
            if self.ident_like_name().as_deref() == Some("except") {
                return Err(CompileError::new("preserve except is not allowed — use explicit whitelist", self.current().span)
                    .with_code("E1008"));
            }
            let field_name = self.parse_name()?;
            fields.push(field_name);
            self.consume_optional_semi();
            self.skip_newlines();
        }

        if fields.is_empty() {
            return Err(
                CompileError::new("preserve block is empty — at least one field name is required", start_span).with_code("E1001")
            );
        }

        let end_span = self.current().span;
        self.expect(TokenKind::RBrace)?;

        Ok(Expr::Preserve(PreserveExpr {
            output_name,
            input_name,
            fields,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    /// Parse `std::namespace::name(args)` or `std::namespace::name(args) { field1, field2 }`
    fn parse_stdlib_call(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::Std)?;

        // Expect ::
        self.expect(TokenKind::ColonColon)?;

        // Parse namespace name
        let namespace = self.parse_name()?;

        // Expect ::
        self.expect(TokenKind::ColonColon)?;

        // Parse function name
        let name = self.parse_name()?;

        // Parse argument list
        let (args, mut end_span) = self.parse_args_with_end_span()?;

        // Optional preserve-style field block for lifecycle patterns
        let preserve_fields = if self.check(&TokenKind::LBrace) {
            self.advance();
            self.skip_newlines();
            let mut fields = Vec::new();
            while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
                fields.push(self.parse_name()?);
                self.consume_optional_semi();
                self.skip_newlines();
            }
            end_span = self.expect(TokenKind::RBrace)?.span;
            fields
        } else {
            vec![]
        };

        Ok(Expr::StdlibCall(StdlibCallExpr {
            namespace,
            name,
            args,
            preserve_fields,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn parse_if_expr(&mut self) -> Result<Expr> {
        let span = self.current().span;
        self.with_recursion_guard(span, |this| this.parse_if_expr_inner())
    }

    fn parse_if_expr_inner(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::If)?;
        let condition = self.parse_expr()?;
        let then_branch = Box::new(self.parse_branch_expr()?);
        self.skip_newlines();
        self.expect(TokenKind::Else)?;
        let else_branch = Box::new(self.parse_branch_expr()?);
        Ok(Expr::If(IfExpr { condition: Box::new(condition), then_branch, else_branch, span: start_span }))
    }

    fn parse_branch_expr(&mut self) -> Result<Expr> {
        self.skip_newlines();
        if self.check(&TokenKind::LBrace) {
            let start_span = self.current().span;
            let stmts = self.parse_block()?;
            let end = self.current().span.start.max(start_span.end);
            Ok(Expr::Block(stmts, Span::new(start_span.start, end, start_span.line, start_span.column)))
        } else if self.check(&TokenKind::If) {
            self.parse_if_expr()
        } else {
            self.parse_expr()
        }
    }

    fn parse_struct_init(&mut self, ty: String, start_span: Span) -> Result<Expr> {
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();

        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let field_span = self.current().span;
            let field_name = self.parse_name()?;
            let value = if self.check(&TokenKind::Colon) {
                self.advance();
                self.parse_expr()?
            } else {
                Expr::Identifier(field_name.clone(), field_span)
            };
            fields.push((field_name, value));

            if self.check(&TokenKind::Comma) {
                self.advance();
            }
            self.skip_newlines();
        }

        let end_span = self.expect(TokenKind::RBrace)?.span;
        Ok(Expr::StructInit(StructInitExpr {
            ty,
            fields,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn parse_match_expr(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::Match)?;
        let expr = self.parse_expr()?;
        self.skip_newlines();
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();

        let mut arms = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let pattern =
                if self.check(&TokenKind::Underscore) || matches!(&self.current().kind, TokenKind::Identifier(name) if name == "_") {
                    self.advance();
                    "_".to_string()
                } else {
                    self.parse_name_path()?
                };
            self.skip_newlines();
            self.expect(TokenKind::FatArrow)?;
            self.skip_newlines();
            if !self.check(&TokenKind::LBrace) {
                return Err(CompileError::new("match arms must use '{}' proof blocks", self.current().span));
            }
            let block_span = self.current().span;
            let value = {
                let stmts = self.parse_block()?;
                let end = self.current().span.start.max(block_span.end);
                Expr::Block(stmts, Span::new(block_span.start, end, block_span.line, block_span.column))
            };
            let arm_span = self.current().span;
            arms.push(MatchArm { pattern, value, span: arm_span });
            self.skip_newlines();
            if self.check(&TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
            } else if self.check(&TokenKind::RBrace) {
                break;
            }
        }

        self.expect(TokenKind::RBrace)?;
        Ok(Expr::Match(MatchExpr { expr: Box::new(expr), arms, span: start_span }))
    }
}

pub fn parse(tokens: &[Token]) -> Result<Module> {
    let mut parser = Parser::new(tokens);
    parser.skip_newlines();
    parser.parse_module()
}

fn merge_capabilities(attr_capabilities: Option<Vec<Capability>>, inline_capabilities: Vec<Capability>) -> Vec<Capability> {
    let mut merged = attr_capabilities.unwrap_or_default();
    for capability in inline_capabilities {
        if !merged.contains(&capability) {
            merged.push(capability);
        }
    }
    merged
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

fn aggregate_scope_from_targets(left: &str, right: Option<&str>) -> Option<String> {
    let left_scope = aggregate_scope_from_target(left)?;
    if let Some(right) = right {
        let right_scope = aggregate_scope_from_target(right)?;
        if left_scope != right_scope {
            return None;
        }
    }
    Some(left_scope.to_string())
}

fn aggregate_scope_from_target(target: &str) -> Option<&'static str> {
    let base = target.split(['<', '.']).next().unwrap_or(target);
    match base {
        "group_input" | "group_inputs" | "group_output" | "group_outputs" => Some("group"),
        "input" | "inputs" | "output" | "outputs" => Some("transaction"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    #[test]
    fn parser_empty_token_slice_returns_controlled_error() {
        let mut parser = Parser::new(&[]);
        let err = parser.parse_module().unwrap_err();

        assert!(err.message.contains("expected 'module', found end of file"), "{}", err.message);
    }

    #[test]
    fn array_size_uses_checked_target_width_conversion() {
        let input = "module test\n\nstruct Huge { bytes: [u8; 18446744073709551615] }\n";
        let tokens = lex(input).unwrap();
        let result = parse(&tokens);

        #[cfg(target_pointer_width = "64")]
        assert!(result.is_ok(), "64-bit targets can represent u64::MAX as usize");

        #[cfg(target_pointer_width = "32")]
        {
            let err = result.unwrap_err();
            assert!(err.message.contains("array size does not fit this target"), "{}", err.message);
        }
    }

    #[test]
    fn parser_rejects_deep_unary_expression_before_stack_overflow() {
        let input = format!(
            "module test\n\naction test() -> u64\nwhere\n    return {}1\n",
            "-".repeat(MAX_PARSE_RECURSION_DEPTH as usize + 8)
        );
        let tokens = lex(&input).unwrap();
        let err = parse(&tokens).unwrap_err();

        assert!(err.message.contains("parser recursion limit exceeded"), "{}", err.message);
    }

    #[test]
    fn parser_rejects_deep_if_expression_before_stack_overflow() {
        let depth = MAX_PARSE_RECURSION_DEPTH as usize + 8;
        let input = format!(
            "module test\n\naction test() -> u64\nwhere\n    return {}1{}\n",
            "if true ".repeat(depth),
            " else 1".repeat(depth)
        );
        let tokens = lex(&input).unwrap();
        let err = parse(&tokens).unwrap_err();

        assert!(err.message.contains("parser recursion limit exceeded"), "{}", err.message);
    }

    #[test]
    fn binary_expr_spans_keep_operator_token() {
        let input = r#"
module test

action test() -> u64
where
    let eq = 1 == 2
    let ne = 1 != 2
    let lt = 1 < 2
    let le = 1 <= 2
    let gt = 2 > 1
    let ge = 2 >= 1
    let add = 1 + 2
    let sub = 2 - 1
    let mul = 2 * 3
    let div = 4 / 2
    let modulo = 5 % 2
    return 0
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let Item::Action(action) = &module.items[0] else {
            panic!("expected action");
        };

        let cases = [
            (0, BinaryOp::Eq, "1 == 2", "=="),
            (1, BinaryOp::Ne, "1 != 2", "!="),
            (2, BinaryOp::Lt, "1 < 2", "<"),
            (3, BinaryOp::Le, "1 <= 2", "<="),
            (4, BinaryOp::Gt, "2 > 1", ">"),
            (5, BinaryOp::Ge, "2 >= 1", ">="),
            (6, BinaryOp::Add, "1 + 2", "+"),
            (7, BinaryOp::Sub, "2 - 1", "-"),
            (8, BinaryOp::Mul, "2 * 3", "*"),
            (9, BinaryOp::Div, "4 / 2", "/"),
            (10, BinaryOp::Mod, "5 % 2", "%"),
        ];

        for (stmt_index, expected_op, expression_text, operator_text) in cases {
            let Stmt::Let(let_stmt) = &action.body[stmt_index] else {
                panic!("expected let statement");
            };
            let Expr::Binary(binary) = &let_stmt.value else {
                panic!("expected binary expression");
            };
            let expression_start = input.find(expression_text).unwrap();
            let operator_start = expression_start + expression_text.find(operator_text).unwrap();

            assert_eq!(binary.op, expected_op);
            assert_eq!(binary.span.start, operator_start);
            assert_eq!(binary.span.end, operator_start + operator_text.len());
        }
    }

    #[test]
    fn postfix_expr_spans_cover_the_full_postfix_chain() {
        let input = r#"
module test

action test() -> u64
where
    let field = foo.bar.baz
    let call = foo(1, 2)
    let index = values[0]
    return 0
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let Item::Action(action) = &module.items[0] else {
            panic!("expected action");
        };

        let cases = [(0, "foo.bar.baz"), (1, "foo(1, 2)"), (2, "values[0]")];

        for (stmt_index, expression_text) in cases {
            let Stmt::Let(let_stmt) = &action.body[stmt_index] else {
                panic!("expected let statement");
            };
            let expected_start = input.find(expression_text).unwrap();
            let expected_end = expected_start + expression_text.len();

            assert_eq!(let_stmt.value.span().start, expected_start);
            assert_eq!(let_stmt.value.span().end, expected_end);
        }
    }

    #[test]
    fn primitive_and_container_exprs_keep_source_spans() {
        let input = r#"
module test

action test() -> u64
where
    let values = [1, 2]
    return values
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let Item::Action(action) = &module.items[0] else {
            panic!("expected action");
        };

        let Stmt::Let(let_stmt) = &action.body[0] else {
            panic!("expected let statement");
        };
        let Expr::Array(items, array_span) = &let_stmt.value else {
            panic!("expected array expression");
        };
        assert_ne!(*array_span, Span::default());

        let Expr::Integer(_, integer_span) = &items[0] else {
            panic!("expected integer expression");
        };
        assert_ne!(*integer_span, Span::default());

        let Stmt::Return(Some(Expr::Identifier(_, identifier_span))) = &action.body[1] else {
            panic!("expected identifier return expression");
        };
        assert_ne!(*identifier_span, Span::default());
    }

    #[test]
    fn hex_literal_exprs_parse_as_integers() {
        let input = r#"
module test

const MASK: u64 = 0xFF
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let Item::Const(const_def) = &module.items[0] else {
            panic!("expected const");
        };
        let Expr::Integer(value, span) = &const_def.value else {
            panic!("expected integer literal");
        };
        let expected_start = input.find("0xFF").unwrap();

        assert_eq!(*value, 255);
        assert_eq!(span.start, expected_start);
        assert_eq!(span.end, expected_start + "0xFF".len());
    }

    #[test]
    fn struct_init_span_covers_type_name_and_body() {
        let input = r#"
module test

const SNAPSHOT: Token = Token { amount: 1 }
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let Item::Const(const_def) = &module.items[0] else {
            panic!("expected const");
        };
        let Expr::StructInit(init) = &const_def.value else {
            panic!("expected struct init");
        };
        let expected_start = input.find("Token {").unwrap();
        let expected_end = expected_start + "Token { amount: 1 }".len();

        assert_eq!(init.span.start, expected_start);
        assert_eq!(init.span.end, expected_end);
    }

    #[test]
    fn parser_rejects_bang_assert_syntax() {
        let input = r#"
module test

action test()
where
    assert!(true, "bang")
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();

        assert!(err.message.contains("not assert!(...)"), "{}", err.message);
        assert_eq!(err.span.start, input.find('!').unwrap());
    }

    #[test]
    fn named_arg_diagnostic_uses_bad_name_span() {
        let input = r#"
module test

const BAD: u64 = destroy_unique(token, wrong = token_id)
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();

        assert!(err.message.contains("expected named argument 'identity', got 'wrong'"), "{}", err.message);
        assert_eq!(err.span.start, input.find("wrong").unwrap());
    }

    #[test]
    fn identity_policy_diagnostic_uses_bad_policy_span() {
        let input = r#"
module test

resource Token
identity(mystery)
{
    amount: u64
}
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();

        assert!(err.message.contains("unsupported identity policy 'mystery'"), "{}", err.message);
        assert_eq!(err.span.start, input.find("mystery").unwrap());
    }

    #[test]
    fn postfix_exprs_cover_the_consumed_source_range() {
        let input = r#"
module test

action test()
where
    let value = foo.bar(1)[0]
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let Item::Action(action) = &module.items[0] else {
            panic!("expected action");
        };
        let Stmt::Let(let_stmt) = &action.body[0] else {
            panic!("expected let statement");
        };
        let Expr::Index(index) = &let_stmt.value else {
            panic!("expected index expression");
        };

        let index_start = input.find("foo.bar(1)[0]").unwrap();
        assert_eq!(index.span.start, index_start);
        assert_eq!(index.span.end, index_start + "foo.bar(1)[0]".len());

        let Expr::Call(call) = index.expr.as_ref() else {
            panic!("expected call expression");
        };
        let call_start = input.find("foo.bar(1)").unwrap();
        assert_eq!(call.span.start, call_start);
        assert_eq!(call.span.end, call_start + "foo.bar(1)".len());

        let Expr::FieldAccess(field) = call.func.as_ref() else {
            panic!("expected field access expression");
        };
        let field_start = input.find("foo.bar").unwrap();
        assert_eq!(field.span.start, field_start);
        assert_eq!(field.span.end, field_start + "foo.bar".len());
    }

    #[test]
    fn generic_type_arguments_allow_newlines() {
        let input = r#"
module test

action test(input: Vec<
    (
        u64,
        u32,
    ),
    Hash,
>) -> u64
where
    return 0
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let Item::Action(action) = &module.items[0] else {
            panic!("expected action");
        };

        assert_eq!(action.params[0].ty, Type::Named("Vec<(u64, u32), Hash>".to_string()));
    }

    #[test]
    fn test_parse_resource() {
        let input = r#"
module test

resource Token has store, transfer, destroy {
    amount: u64
    symbol: [u8; 8]
}
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        assert_eq!(module.name, "test");
        assert_eq!(module.items.len(), 1);
    }

    #[test]
    fn test_parse_merges_attribute_and_inline_capabilities() {
        let input = r#"
module test

#[capability(store)]
resource Token has transfer, destroy {
    amount: u64
}
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let resource = match &module.items[0] {
            Item::Resource(resource) => resource,
            other => panic!("expected resource item, found {:?}", other),
        };

        assert!(resource.capabilities.contains(&Capability::Store));
        assert!(resource.capabilities.contains(&Capability::Transfer));
        assert!(resource.capabilities.contains(&Capability::Destroy));
    }

    #[test]
    fn test_parse_invariant() {
        let input = r#"
module test

invariant token_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_invariant(true, "token amount is conserved")
}
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let invariant = match &module.items[0] {
            Item::Invariant(invariant) => invariant,
            other => panic!("expected invariant item, found {:?}", other),
        };

        assert_eq!(invariant.name, "token_conservation");
        assert_eq!(invariant.trigger.as_deref(), Some("type_group"));
        assert_eq!(invariant.scope.as_deref(), Some("group"));
        assert_eq!(invariant.reads, vec!["group_inputs<Token>.amount", "group_outputs<Token>.amount"]);
        assert_eq!(invariant.asserts.len(), 1);
    }

    #[test]
    fn test_parse_aggregate_invariant_primitives() {
        let input = r#"
module test

invariant token_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_conserved(Token.amount, scope = group)
    assert_sum(group_outputs<Token>.amount) <= assert_sum(group_inputs<Token>.amount)
}
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let invariant = match &module.items[0] {
            Item::Invariant(invariant) => invariant,
            other => panic!("expected invariant item, found {:?}", other),
        };

        assert_eq!(invariant.aggregates.len(), 2);
        assert_eq!(invariant.aggregates[0].kind, AggregateInvariantKind::Conserved);
        assert_eq!(invariant.aggregates[0].target, "Token.amount");
        assert_eq!(invariant.aggregates[0].scope, "group");
        assert_eq!(invariant.aggregates[1].kind, AggregateInvariantKind::Sum);
        assert_eq!(invariant.aggregates[1].relation, Some(AggregateRelation::Le));
        assert_eq!(invariant.aggregates[1].target, "group_outputs<Token>.amount");
        assert_eq!(invariant.aggregates[1].rhs.as_deref(), Some("group_inputs<Token>.amount"));
    }

    #[test]
    fn test_parse_invariant_assert_statement() {
        let input = r#"
module test

invariant token_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert true
}
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let invariant = match &module.items[0] {
            Item::Invariant(invariant) => invariant,
            other => panic!("expected invariant item, found {:?}", other),
        };

        assert_eq!(invariant.asserts.len(), 1);
        assert!(matches!(invariant.asserts[0], Expr::Assert(_)));
    }

    #[test]
    fn test_parse_type_id_attribute() {
        let input = r#"
module test

#[type_id("cellscript::token::Token:v1")]
resource Token has store {
    amount: u64
}
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let resource = match &module.items[0] {
            Item::Resource(resource) => resource,
            other => panic!("expected resource item, found {:?}", other),
        };

        assert_eq!(resource.type_id.as_ref().map(|type_id| type_id.value.as_str()), Some("cellscript::token::Token:v1"));
    }

    #[test]
    fn test_rejects_type_id_on_action() {
        let input = r#"
module test

#[type_id("cellscript::action:v1")]
action run() -> u64
where
    return 0
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();

        assert!(err.message.contains("#[type_id] can only be applied"), "unexpected error: {}", err.message);
    }

    #[test]
    fn test_rejects_generic_resource_definition() {
        let input = r#"
module test

resource Vault<T> has store {
    content: T
}
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();

        assert!(err.message.contains("post-v1 template/codegen syntax"), "unexpected error: {}", err.message);
    }

    #[test]
    fn test_parse_action() {
        let input = r#"
module test

action mint(amount: u64) -> token: Token
where
    create token = Token { amount: amount, symbol: b"TEST" }
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        assert_eq!(module.items.len(), 1);
    }

    #[test]
    fn test_action_where_column_one_flow_identifier_stays_in_body() {
        let input = r#"
module test

action echo(flow: u64) -> u64
where
flow
    return flow
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let action = match &module.items[0] {
            Item::Action(action) => action,
            other => panic!("expected action, found {:?}", other),
        };

        assert_eq!(action.body.len(), 2);
        assert!(matches!(&action.body[0], Stmt::Expr(Expr::Identifier(name, _)) if name == "flow"));
    }

    #[test]
    fn test_parse_flow_and_action_transition_clause() {
        let input = r#"
module test

flow OfferFlow for Offer.state {
    Created -> Live by publish;
    Live -> Filled by accept;
}

	action accept(input: Offer) -> output: Offer
	    transition input.state: Live -> output.state: Filled
	where
    require output.state == OfferState::Filled
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        assert!(matches!(&module.items[0], Item::Flow(machine) if machine.name.as_deref() == Some("OfferFlow")));
        let Item::Action(action) = &module.items[1] else {
            panic!("expected action");
        };
        assert_eq!(action.params[0].source, ParamSource::Default);
        assert_eq!(action.outputs.len(), 1);
        assert_eq!(action.outputs[0].name, "output");
        assert_eq!(action.state_edges.len(), 1);
        assert_eq!(action.state_edges[0].path.base, "input");
        assert_eq!(action.state_edges[0].to_path.base, "output");
        assert_eq!(action.state_edges[0].from, "Live");
        assert_eq!(action.state_edges[0].to, "Filled");
    }

    #[test]
    fn test_parse_action_transition_block() {
        let input = r#"
module test

action settle(input: Offer, receipt: Receipt) -> (output: Offer, next_receipt: Receipt)
    transition {
        input.state: Live -> output.state: Filled
        receipt.state: Open -> next_receipt.state: Closed
    }
where
    require output.state == OfferState::Filled
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let Item::Action(action) = &module.items[0] else {
            panic!("expected action");
        };
        assert_eq!(action.state_edges.len(), 2);
        assert_eq!(action.state_edges[0].path.base, "input");
        assert_eq!(action.state_edges[1].path.base, "receipt");
        assert_eq!(action.state_edges[1].to_path.base, "next_receipt");
    }

    #[test]
    fn test_rejects_empty_transition_block() {
        let input = r#"
module test

action accept(input: Offer) -> output: Offer
    transition {
    }
where
    require output.state == OfferState::Filled
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();
        assert!(err.message.contains("transition block must contain at least one state edge"), "unexpected error: {}", err.message);
    }

    #[test]
    fn test_rejects_legacy_move_clause() {
        let input = r#"
module test

action accept(input: Offer) -> output: Offer
    move input.state: Live -> output.state: Filled
where
    require output.state == OfferState::Filled
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();
        assert!(err.message.contains("use 'transition', not legacy 'move'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn test_rejects_transition_clause_without_state_colons() {
        let input = r#"
module test

action accept(input: Offer) -> output: Offer
    transition input.state Live -> output.state Filled
where
    require output.state == OfferState::Filled
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();
        assert!(err.message.contains("state transition must put ':' before the source state"), "unexpected error: {}", err.message);
    }

    #[test]
    fn test_rejects_action_brace_body() {
        let input = r#"
module test

action accept(input: Offer) -> output: Offer {
    require input.state == output.state
}
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();
        assert!(err.message.contains("action proof blocks use `where`"), "unexpected error: {}", err.message);
    }

    #[test]
    fn test_rejects_typed_let_without_initializer() {
        let input = r#"
module test

fn bad() -> u64 {
    let x: u64
    return x
}
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();
        assert!(err.message.contains("expected '='"), "unexpected error: {}", err.message);
    }

    #[test]
    fn test_rejects_read_ref_as_type_qualifier() {
        let input = r#"
module test

action grant(config: read_ref Config) -> grant: Grant
where
    create grant = Grant { admin: config.admin }
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();
        assert!(err.message.contains("`read_ref` is not a type qualifier"), "unexpected error: {}", err.message);
    }

    #[test]
    fn test_rejects_output_parameter_source_prefix() {
        let input = r#"
module test

action accept(input: Offer, output output: Offer)
where
    require output.state == 1
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();
        assert!(err.message.contains("only valid on the action return side"), "unexpected error: {}", err.message);
    }

    #[test]
    fn test_parse_prefix_source_and_create_target() {
        let input = r#"
module test

action grant(read config: Config, token: Token) -> grant: Grant
where
    consume token
    create grant = Grant { admin: config.admin }
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let Item::Action(action) = &module.items[0] else {
            panic!("expected action");
        };
        assert!(action.params[0].is_read_ref);
        assert_eq!(action.params[0].name, "config");
        assert_eq!(action.outputs[0].name, "grant");
        let Stmt::Expr(Expr::Create(create)) = &action.body[1] else {
            panic!("expected targeted create");
        };
        assert_eq!(create.target.as_deref(), Some("grant"));
    }

    #[test]
    fn test_parse_prefix_source_before_keyword_like_name() {
        let input = r#"
module test

lock receipt_owner(protected receipt: ReceiptCell, witness claimed_owner: Address) -> bool {
    require receipt.owner == claimed_owner
}
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let Item::Lock(lock) = &module.items[0] else {
            panic!("expected lock");
        };
        assert_eq!(lock.params[0].name, "receipt");
        assert_eq!(lock.params[0].source, ParamSource::Protected);
        assert_eq!(lock.params[1].source, ParamSource::Witness);
    }

    #[test]
    fn test_parse_create_field_shorthand() {
        let input = r#"
module test

action mint(amount: u64, symbol: [u8; 8]) -> Token
where
    create Token { amount, symbol }
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        assert_eq!(module.items.len(), 1);
    }

    #[test]
    fn test_parse_expression() {
        let input = r#"
module test

action test(x: u64, y: u64) -> u64
where
    let z = x + y * 2
    return z
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        assert_eq!(module.items.len(), 1);
    }

    #[test]
    fn test_parse_grouped_use_imports() {
        let input = r#"
module test

use cellscript::fungible_token::{Token, MintAuthority}
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();

        let use_stmt = match &module.items[0] {
            Item::Use(use_stmt) => use_stmt,
            other => panic!("expected use item, found {:?}", other),
        };

        assert_eq!(use_stmt.module_path, vec!["cellscript".to_string(), "fungible_token".to_string()]);
        assert_eq!(use_stmt.imports.len(), 2);
        assert_eq!(use_stmt.imports[0].name, "Token");
        assert_eq!(use_stmt.imports[1].name, "MintAuthority");
    }

    #[test]
    fn test_rejects_use_as_without_alias() {
        let input = r#"
module test

use demo::types::Token as
"#;
        let tokens = lex(input).unwrap();
        let error = parse(&tokens).unwrap_err();

        assert!(error.message.contains("expected import alias after 'as'"), "{}", error.message);
    }

    #[test]
    fn test_launch_expression_is_reserved_until_lowering_exists() {
        let input = r#"
module test

action bad() -> u64
where
    return launch(Token)
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();
        assert!(err.message.contains("launch is reserved for a post-v1 transaction builder"), "unexpected error: {}", err.message);
    }

    #[test]
    fn test_postfix_does_not_cross_statement_newline() {
        let input = r#"
module test

action test() -> (u64, u64)
where
    let value = foo(
        1,
        2
    )

    (value, 3)
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let action = match &module.items[0] {
            Item::Action(action) => action,
            other => panic!("expected action, found {:?}", other),
        };

        match &action.body[0] {
            Stmt::Let(let_stmt) => {
                assert!(matches!(let_stmt.value, Expr::Call(_)));
            }
            other => panic!("expected let statement, found {:?}", other),
        }

        match &action.body[1] {
            Stmt::Expr(Expr::Tuple(items, _)) => {
                assert_eq!(items.len(), 2);
            }
            other => panic!("expected tuple return expr, found {:?}", other),
        }
    }

    #[test]
    fn test_rejects_unbraced_match_arms() {
        let input = r#"
module test

enum Flag {
    On,
    Off,
}

action test(flag: Flag) -> u64
where
    match flag {
        On => 1,
        Off => 0,
    }
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();
        assert!(err.message.contains("match arms must use"), "unexpected error: {}", err.message);
    }

    // === 0.13.1 preserve sugar and anonymous require block tests ===

    #[test]
    fn test_parse_preserve_block() {
        let input = r#"
module test

resource Offer has store {
    seller: [u8; 20]
    price: u64
    payment_symbol: [u8; 8]
}

action fill(input: Offer) -> (output: Offer)
    transition input.state: Live -> output.state: Filled
where
    preserve output from input {
        seller
        price
        payment_symbol
    }
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let action = match &module.items[1] {
            Item::Action(action) => action,
            other => panic!("expected action, found {:?}", other),
        };
        // Find the preserve statement in the action body
        let found_preserve = action.body.iter().any(|stmt| matches!(stmt, Stmt::Expr(Expr::Preserve(_))));
        assert!(found_preserve, "expected preserve expression in action body");
    }

    #[test]
    fn test_parse_require_block() {
        let input = r#"
module test

action test(x: u64, y: u64) -> u64
where
    require {
        x > 0
        y > 0
    }
    return x + y
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let action = match &module.items[0] {
            Item::Action(action) => action,
            other => panic!("expected action, found {:?}", other),
        };
        let found_require_block = action.body.iter().any(|stmt| matches!(stmt, Stmt::Expr(Expr::RequireBlock(_))));
        assert!(found_require_block, "expected require block expression in action body");
    }

    #[test]
    fn test_parse_preserve_single_field() {
        let input = r#"
module test

resource Token has store {
    amount: u64
}

action test(input: Token) -> (output: Token)
where
    preserve output from input {
        amount
    }
"#;
        let tokens = lex(input).unwrap();
        let module = parse(&tokens).unwrap();
        let action = match &module.items[1] {
            Item::Action(action) => action,
            other => panic!("expected action, found {:?}", other),
        };
        let preserve = action
            .body
            .iter()
            .find_map(|stmt| match stmt {
                Stmt::Expr(Expr::Preserve(p)) => Some(p.clone()),
                _ => None,
            })
            .expect("expected preserve expression");
        assert_eq!(preserve.output_name, "output");
        assert_eq!(preserve.input_name, "input");
        assert_eq!(preserve.fields, vec!["amount".to_string()]);
    }

    #[test]
    fn test_reject_empty_preserve_block() {
        let input = r#"
module test

action test(input: u64) -> (output: u64)
where
    preserve output from input {
    }
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();
        assert!(err.message.contains("preserve block is empty"), "unexpected error: {}", err.message);
        assert_eq!(err.code.as_deref(), Some("E1001"));
    }

    #[test]
    fn test_reject_empty_require_block() {
        let input = r#"
module test

action test() -> u64
where
    require {
    }
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();
        assert!(err.message.contains("require block is empty"), "unexpected error: {}", err.message);
        assert_eq!(err.code.as_deref(), Some("E1001"));
    }

    #[test]
    fn test_reject_preserve_wildcard() {
        let input = r#"
module test

action test(input: u64) -> (output: u64)
where
    preserve output from input {
        *
    }
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();
        assert!(err.message.contains("wildcard"), "unexpected error: {}", err.message);
        assert_eq!(err.code.as_deref(), Some("E1007"));
    }

    #[test]
    fn test_reject_preserve_except() {
        let input = r#"
module test

action test(input: u64) -> (output: u64)
where
    preserve output from input {
        except
    }
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();
        assert!(err.message.contains("except"), "unexpected error: {}", err.message);
        assert_eq!(err.code.as_deref(), Some("E1008"));
    }

    #[test]
    fn test_reject_bare_preserve() {
        let input = r#"
module test

action test(input: u64) -> (output: u64)
where
    preserve output from input
    return 0
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();
        assert!(err.message.contains("bare"), "unexpected error: {}", err.message);
        assert_eq!(err.code.as_deref(), Some("E1009"));
    }

    #[test]
    fn test_reject_require_block_with_consume() {
        let input = r#"
module test

resource Token has store {
    amount: u64
}

action test(input: Token) -> u64
where
    require {
        consume input
    }
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();
        assert!(err.message.contains("statement"), "unexpected error: {}", err.message);
        assert_eq!(err.code.as_deref(), Some("E1005"));
    }

    #[test]
    fn test_reject_require_block_with_control_flow() {
        let input = r#"
module test

action test(x: u64) -> u64
where
    require {
        if x > 0 {
            true
        } else {
            false
        }
    }
"#;
        let tokens = lex(input).unwrap();
        let err = parse(&tokens).unwrap_err();
        assert!(err.message.contains("control flow"), "unexpected error: {}", err.message);
        assert_eq!(err.code.as_deref(), Some("E1006"));
    }
}
