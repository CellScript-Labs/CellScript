use super::{Parser, MAX_PARAM_COUNT};
use crate::ast::{
    ActionDef, ActionOutput, ActionStateEdge, DefinitionOrigin, FnDef, LockDef, Param, ParamSource, SchedulerHint, StateFieldPath,
    Stmt, Type,
};
use crate::error::{CompileError, Result, Span};
use crate::lexer::token::TokenKind;
use crate::syntax::SyntaxKind;

impl Parser<'_> {
    pub(super) fn parse_action(&mut self, scheduler_hint: Option<SchedulerHint>) -> Result<ActionDef> {
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

        let (state_edges, body) = self.parse_action_body()?;

        let end_span = self.current().span;

        Ok(ActionDef {
            name,
            params,
            return_type,
            outputs,
            state_edges,
            body,
            scheduler_hint,
            next_surface: None,
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
        let mut tokens = self.tokens[self.position + 1..].iter().filter(|token| !matches!(token.kind, TokenKind::Newline));
        matches!(
            (tokens.next().map(|token| &token.kind), tokens.next().map(|token| &token.kind)),
            (Some(TokenKind::Identifier(_)), Some(TokenKind::Colon))
        )
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
                _ => break,
            }
        }

        Ok(state_edges)
    }

    fn parse_action_body(&mut self) -> Result<(Vec<ActionStateEdge>, Vec<Stmt>)> {
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

        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            if let Some(stmt) = self.parse_stmt_recovering()? {
                stmts.push(stmt);
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::RBrace)?;
        self.record_syntax_node(SyntaxKind::Block, syntax_start);
        Ok((state_edges, stmts))
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

        let params = self.parse_params()?;
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
        let mut body = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            if let Some(stmt) = self.parse_stmt_recovering()? {
                body.push(stmt);
            }
            self.skip_newlines();
        }
        self.expect(TokenKind::RBrace)?;

        let end_span = self.current().span;
        Ok(LockDef {
            name,
            params,
            return_type,
            body,
            next_surface: None,
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
            if params.len() > MAX_PARAM_COUNT {
                return Err(CompileError::new(
                    format!("parameter lists exceed the parser limit of {} entries", MAX_PARAM_COUNT),
                    self.current().span,
                ));
            }

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
}
