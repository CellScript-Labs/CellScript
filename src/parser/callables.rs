use super::action_policies::ParsedActionBody;
use super::{Parser, MAX_PARAM_COUNT};
use crate::ast::{
    ActionDef, ActionOutput, ActionStateEdge, DefinitionOrigin, FnDef, LockDef, Param, ParamSource, SchedulerHint, StateFieldPath,
    Type,
};
use crate::error::{CompileError, Result, Span};
use crate::lexer::token::TokenKind;
use crate::syntax::SyntaxKind;

impl Parser<'_> {
    pub(super) fn parse_action(&mut self, scheduler_hint: Option<SchedulerHint>) -> Result<ActionDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Action)?;

        let name = self.parse_name()?;

        let mut params = Vec::new();
        let mut outputs = Vec::new();
        for param in self.parse_params(true)? {
            if param.source == ParamSource::Output {
                if param.is_mut {
                    return Err(CompileError::new("an output role cannot be mutable", param.span));
                }
                outputs.push(ActionOutput { name: param.name, ty: param.ty, span: param.span });
            } else {
                params.push(param);
            }
        }
        let inline_outputs = !outputs.is_empty();
        self.skip_newlines();
        let return_type = if self.check(&TokenKind::Arrow) {
            self.advance();
            self.skip_newlines();
            self.reject_named_action_outputs()?;
            if inline_outputs {
                return Err(CompileError::new("an action with output ports uses the unit entry convention", self.current().span));
            }
            Some(self.parse_type()?)
        } else {
            None
        };

        let previous_outputs = self.entry_outputs.replace(outputs.clone());
        let parsed = self.parse_action_body();
        self.entry_outputs = previous_outputs;
        let (state_edges, parsed) = parsed?;
        let (body, surface) = parsed.finish();
        let unit_policy = !surface.audits.is_empty()
            || surface.dispositions.iter().any(|item| !matches!(item, crate::ast::NextDisposition::Replace(_)));
        if unit_policy && return_type.as_ref().is_some_and(|ty| *ty != Type::Unit) {
            return Err(CompileError::new("an action with structured dispositions uses the unit entry convention", start_span));
        }
        let end_span = self.previous_non_newline().span;

        Ok(ActionDef {
            name,
            params,
            return_type,
            outputs,
            state_edges,
            body,
            scheduler_hint,
            next_surface: Some(surface),
            doc_comment: None,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn reject_named_action_outputs(&self) -> Result<()> {
        let mut tokens = self.tokens[self.position..].iter().filter(|token| !matches!(token.kind, TokenKind::Newline));
        let mut first = tokens.next();
        if first.is_some_and(|token| token.kind == TokenKind::LParen) {
            first = tokens.next();
        }
        if matches!(first.map(|token| &token.kind), Some(TokenKind::Identifier(_)))
            && tokens.next().is_some_and(|token| token.kind == TokenKind::Colon)
        {
            return Err(crate::error::SourceDiagnosticCode::RemovedAuthoringSpelling.error(
                "named Cell outputs after '->' were removed; put 'output name: Type' in the action parameter list; '-> Type' denotes a returned value",
                self.current().span,
            ));
        }
        Ok(())
    }

    fn parse_action_clauses(&mut self) -> Result<Vec<ActionStateEdge>> {
        let mut state_edges = Vec::new();

        loop {
            self.skip_newlines();
            match &self.current().kind {
                TokenKind::Transition => {
                    state_edges.extend(self.parse_action_state_edges()?);
                }
                _ => break,
            }
        }

        Ok(state_edges)
    }

    fn parse_action_body(&mut self) -> Result<(Vec<ActionStateEdge>, ParsedActionBody)> {
        self.skip_newlines();
        let syntax_start = self.current().span;
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();

        let state_edges = self.parse_action_clauses()?;
        self.skip_newlines();
        if matches!(&self.current().kind, TokenKind::Identifier(name) if name == "where") {
            return Err(CompileError::new(
                "`where` action proof blocks are not supported; write `require ...;` directly",
                self.current().span,
            ));
        }
        self.parse_verification_marker()?;

        let mut body = ParsedActionBody::default();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            if !self.parse_action_policy(&mut body)?
                && let Some(stmt) = self.parse_stmt_recovering()?
            {
                body.push_statement(stmt);
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::RBrace)?;
        self.record_syntax_node(SyntaxKind::Block, syntax_start);
        Ok((state_edges, body))
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
                return Err(CompileError::new(
                    "transition block syntax is not part of the canonical action surface",
                    self.current().span,
                ));
            }
            loop {
                edges.push(self.parse_action_state_edge()?);

                if self.check(&TokenKind::Comma) {
                    self.advance();
                    self.skip_newlines();
                    continue;
                }
                break;
            }
            self.record_end(SyntaxKind::ClauseEnd);
            self.expect_statement_end()?;
            self.skip_newlines();
        }

        Ok(edges)
    }

    fn parse_action_state_edge(&mut self) -> Result<ActionStateEdge> {
        let start_span = self.current().span;
        let first = self.parse_name_path()?;
        if !self.check(&TokenKind::Dot) {
            self.expect(TokenKind::Arrow)?;
            let output_start = self.current().span;
            let output = self.parse_name_path()?;
            if self.check(&TokenKind::Dot) {
                return Err(CompileError::new(
                    "lineage transition output must be a binding name: transition input -> output",
                    output_start,
                ));
            }
            let end_span = self.current().span;
            return Ok(ActionStateEdge {
                path: StateFieldPath { base: first, field: String::new(), span: start_span },
                to_path: StateFieldPath { base: output, field: String::new(), span: output_start },
                from: String::new(),
                to: String::new(),
                span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
            });
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

    pub(super) fn parse_fn(&mut self) -> Result<FnDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Fn)?;

        let name = self.parse_name()?;
        let type_params = self.parse_type_params()?;
        let params = self.parse_params(false)?;
        let return_type = if self.check(&TokenKind::Arrow) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = self.parse_block()?;
        let end_span = self.current().span;

        Ok(FnDef {
            origin: DefinitionOrigin::Source,
            name,
            type_params,
            params,
            return_type,
            body,
            doc_comment: None,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    pub(super) fn parse_lock(&mut self) -> Result<LockDef> {
        let start_span = self.current().span;
        self.expect(TokenKind::Lock)?;

        let name = self.parse_name()?;

        let params = self.parse_params(false)?;
        if let Some(param) = params.iter().find(|param| matches!(param.source, ParamSource::Input | ParamSource::Output)) {
            return Err(CompileError::new(
                "Lock parameters require protected, witness, lock_args or read provenance; input and output roles belong to actions",
                param.span,
            ));
        }
        let return_type = if self.check(&TokenKind::Arrow) {
            self.advance();
            self.parse_type()?
        } else {
            Type::Bool
        };
        self.skip_newlines();
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();
        self.parse_verification_marker()?;
        let mut parsed = ParsedActionBody::default();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            if !self.parse_action_policy(&mut parsed)?
                && let Some(stmt) = self.parse_stmt_recovering()?
            {
                parsed.push_statement(stmt);
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::RBrace)?;
        let (body, surface) = parsed.finish_lock();

        let end_span = self.current().span;
        Ok(LockDef {
            name,
            params,
            return_type,
            body,
            next_surface: Some(surface),
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })
    }

    fn parse_params(&mut self, allow_output: bool) -> Result<Vec<Param>> {
        self.expect(TokenKind::LParen)?;

        let mut params = Vec::new();
        loop {
            self.skip_newlines();
            if self.check(&TokenKind::RParen) || self.check(&TokenKind::Eof) {
                break;
            }
            let param = self.parse_param(allow_output)?;
            params.push(param);
            if params.len() > MAX_PARAM_COUNT {
                return Err(CompileError::new(
                    format!("parameter lists exceed the parser limit of {} entries", MAX_PARAM_COUNT),
                    self.current().span,
                ));
            }

            self.skip_newlines();
            if self.check(&TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
            } else {
                break;
            }
        }

        self.skip_newlines();
        self.expect(TokenKind::RParen)?;
        Ok(params)
    }

    fn parse_param(&mut self, allow_output: bool) -> Result<Param> {
        let start_span = self.current().span;

        if self.check(&TokenKind::Ref) {
            return Err(CompileError::new(
                "parameter modifier 'ref' is reserved but unsupported; use '&T' for read-only helper views, or `action(input before: T, output after: T)` with explicit lifecycle policies for Cell state updates",
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

        let (prefix_source, prefix_read_ref) = self.parse_param_source_prefix(allow_output)?;
        let name = self.parse_name()?;

        self.expect(TokenKind::Colon)?;
        self.reject_postfix_param_source(&name)?;
        let source = prefix_source;
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

    fn parse_param_source_prefix(&mut self, allow_output: bool) -> Result<(ParamSource, bool)> {
        self.skip_newlines();
        let current_name = Self::token_ident_like_name(&self.current().kind);
        let mut following = self.tokens[self.position + 1..].iter().filter(|token| !matches!(token.kind, TokenKind::Newline));
        let next_is_name = following.next().is_some_and(|token| Self::token_ident_like_name(&token.kind).is_some());
        let next_next_is_colon = following.next().is_some_and(|token| matches!(token.kind, TokenKind::Colon));
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
            Some("output") if allow_output => {
                self.advance();
                Ok((ParamSource::Output, false))
            }
            Some("output") => Err(CompileError::new("output ports belong to action declarations", self.current().span)),
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

    fn reject_postfix_param_source(&mut self, binding: &str) -> Result<()> {
        self.skip_newlines();
        let TokenKind::Identifier(role) = &self.current().kind else { return Ok(()) };
        if !matches!(role.as_str(), "protected" | "witness" | "lock_args") {
            return Ok(());
        }
        let next = self.tokens[self.position + 1..].iter().find(|token| !matches!(token.kind, TokenKind::Newline));
        // A nominal type named `witness`, `protected` or `lock_args` remains
        // legal, including paths and applications. Only the removed role
        // followed by a separate type receives the migration diagnostic.
        if next.is_some_and(|token| {
            matches!(
                token.kind,
                TokenKind::Identifier(_)
                    | TokenKind::Launch
                    | TokenKind::U8
                    | TokenKind::U16
                    | TokenKind::U32
                    | TokenKind::I32
                    | TokenKind::U64
                    | TokenKind::U128
                    | TokenKind::Bool
                    | TokenKind::Address
                    | TokenKind::Hash
                    | TokenKind::LParen
                    | TokenKind::LBracket
                    | TokenKind::Ampersand
                    | TokenKind::ReadRef
            )
        }) {
            return Err(crate::error::SourceDiagnosticCode::RemovedAuthoringSpelling.error(
                format!("postfix parameter source '{role}' was removed; write '{role} {binding}: Type' with the source before the binding name"),
                self.current().span,
            ));
        }
        Ok(())
    }
}
