use super::*;
use crate::ast::{
    ActionDef, ActionOutput, BinaryExpr, BinaryOp, CastExpr, CreateExpr, CreateUniqueExpr, DestroyExpr, DestructionPolicy, Expr,
    FieldAccessExpr, IdentityPolicy, Item, LockDef, NextAudit, NextAuditEvidence, NextBodyItem, NextDisposition, NextEntrySurface,
    NextFreshOutput, NextLockSurface, NextPool, NextPoolField, NextPoolFieldTreatment, NextReplacement, NextRetirement, Param,
    ParamSource, ReplaceDataTreatment, ReplaceFieldTreatment, ReplaceLockTreatment, RequireExpr, Stmt, Type, Visibility,
};
use crate::error::{SourceDiagnosticCode, Span};
use crate::lexer::token::{Token, TokenKind};
use crate::parser;
use crate::syntax::{SyntaxKind, SyntaxNode};
use std::collections::BTreeSet;

pub(super) fn parse(source: &str) -> Result<ast::Module> {
    let tokens = lexer::lex(source)?;
    parse_tokens(&tokens)
}

pub(super) fn parse_tokens(tokens: &[Token]) -> Result<ast::Module> {
    parse_tokens_recording(tokens, false).map(|(module, _)| module)
}

pub(super) fn parse_tokens_with_syntax(tokens: &[Token]) -> Result<(ast::Module, Vec<SyntaxNode>)> {
    parse_tokens_recording(tokens, true)
}

fn parse_tokens_recording(tokens: &[Token], record: bool) -> Result<(ast::Module, Vec<SyntaxNode>)> {
    let mut nodes = Vec::new();
    let native = has_native_surface(tokens);
    let module = if native {
        parse_native_surface(tokens, record, &mut nodes)?
    } else if record {
        let (module, trace) = parser::parse_with_syntax(tokens)?;
        nodes = trace;
        module
    } else {
        parser::parse(tokens)?
    };
    let mut diagnostics = if native { constitution_diagnostics(&module, tokens) } else { Vec::new() };
    diagnostics.extend(super::authoring::successor_completeness_diagnostics(&module));
    if let Some(error) = diagnostics.into_iter().next() {
        return Err(error);
    }
    Ok((module, nodes))
}

pub(super) fn parse_diagnostics(source: &str) -> std::result::Result<ast::Module, Vec<CompileError>> {
    let tokens = lexer::lex(source).map_err(|error| vec![error])?;
    if !has_native_surface(&tokens) {
        return super::authoring::parse_diagnostics(&tokens);
    }
    let module = parse_native_surface(&tokens, false, &mut Vec::new()).map_err(|error| vec![error])?;
    let mut diagnostics = constitution_diagnostics(&module, &tokens);
    diagnostics.extend(super::authoring::successor_completeness_diagnostics(&module));
    if diagnostics.is_empty() {
        Ok(module)
    } else {
        Err(diagnostics)
    }
}

pub(super) fn has_native_surface(tokens: &[Token]) -> bool {
    !top_level_word_positions(tokens, "type_script").is_empty() || !top_level_word_positions(tokens, "lock_script").is_empty()
}

fn parse_native_surface(tokens: &[Token], record: bool, nodes: &mut Vec<SyntaxNode>) -> Result<ast::Module> {
    // The formatter retains trivia from the original token stream and uses
    // these same spans. Native grammar consumes only significant tokens.
    let significant_tokens = tokens.iter().filter(|token| !matches!(token.kind, TokenKind::Newline)).cloned().collect::<Vec<_>>();
    let tokens = significant_tokens.as_slice();
    let type_positions = top_level_word_positions(tokens, "type_script");
    let lock_positions = top_level_word_positions(tokens, "lock_script");
    if type_positions.len() + lock_positions.len() != 1 {
        let span =
            type_positions.first().or_else(|| lock_positions.first()).map_or_else(Span::default, |position| tokens[*position].span);
        return Err(CompileError::new("a group-bound module requires exactly one type_script or lock_script entry", span));
    }
    if type_positions.len() == 1 {
        parse_type_script_surface(tokens, record, nodes)
    } else {
        parse_lock_script_surface(tokens, record, nodes)
    }
}

fn parse_type_script_surface(tokens: &[Token], record: bool, nodes: &mut Vec<SyntaxNode>) -> Result<ast::Module> {
    let positions = top_level_word_positions(tokens, "type_script");
    if positions.len() != 1 {
        return Err(CompileError::new(
            "a Type Script module requires exactly one type_script entry",
            positions.first().map_or_else(Span::default, |position| tokens[*position].span),
        ));
    }
    let start = positions[0];
    let end = matching_entry_end(tokens, start)?;
    if tokens[end + 1..].iter().any(|token| !matches!(token.kind, TokenKind::Newline | TokenKind::Eof)) {
        return Err(CompileError::new("type_script must be the final top-level declaration", tokens[end + 1].span));
    }

    let mut base_tokens = tokens[..start].to_vec();
    let eof = tokens.last().cloned().unwrap_or_else(|| Token::new(TokenKind::Eof, Span::default(), ""));
    base_tokens.push(eof);
    let mut module = if record {
        let (module, trace) = parser::parse_with_syntax(&base_tokens)?;
        nodes.extend(trace);
        module
    } else {
        parser::parse(&base_tokens)?
    };
    if module.items.iter().any(|item| matches!(item, Item::Action(_) | Item::Lock(_))) {
        return Err(CompileError::new("type_script must be the module's only entry declaration", tokens[start].span));
    }

    let mut cursor = Cursor::new(&tokens[start..=end]);
    cursor.record_syntax = record;
    let action = cursor.parse_type_script(&module)?;

    if !cursor.is_done() {
        return Err(CompileError::new("unexpected tokens after Edition 2027 type_script", cursor.span()));
    }
    module.visibilities.insert(action.name.clone(), Visibility::ImplicitPublic);
    module.items.push(Item::Action(action));
    module.span = module.span.combine(&tokens[end].span);
    nodes.extend(cursor.syntax_nodes);
    if record {
        nodes.push(SyntaxNode { kind: SyntaxKind::Item, span: tokens[start].span.combine(&tokens[end].span) });
    }
    Ok(module)
}

fn parse_lock_script_surface(tokens: &[Token], record: bool, nodes: &mut Vec<SyntaxNode>) -> Result<ast::Module> {
    let positions = top_level_word_positions(tokens, "lock_script");
    debug_assert_eq!(positions.len(), 1);
    let start = positions[0];
    let end = matching_entry_end(tokens, start)?;
    if tokens[end + 1..].iter().any(|token| !matches!(token.kind, TokenKind::Newline | TokenKind::Eof)) {
        return Err(CompileError::new("lock_script must be the final top-level declaration", tokens[end + 1].span));
    }

    let mut base_tokens = tokens[..start].to_vec();
    let eof = tokens.last().cloned().unwrap_or_else(|| Token::new(TokenKind::Eof, Span::default(), ""));
    base_tokens.push(eof);
    let mut module = if record {
        let (module, trace) = parser::parse_with_syntax(&base_tokens)?;
        nodes.extend(trace);
        module
    } else {
        parser::parse(&base_tokens)?
    };
    if module.items.iter().any(|item| matches!(item, Item::Action(_) | Item::Lock(_))) {
        return Err(CompileError::new("lock_script must be the module's only entry declaration", tokens[start].span));
    }

    let mut cursor = Cursor::new(&tokens[start..=end]);
    cursor.record_syntax = record;
    let lock = cursor.parse_lock_script(&module)?;

    if !cursor.is_done() {
        return Err(CompileError::new("unexpected tokens after Edition 2027 lock_script", cursor.span()));
    }
    module.visibilities.insert(lock.name.clone(), Visibility::ImplicitPublic);
    module.items.push(Item::Lock(lock));
    module.span = module.span.combine(&tokens[end].span);
    nodes.extend(cursor.syntax_nodes);
    if record {
        nodes.push(SyntaxNode { kind: SyntaxKind::Item, span: tokens[start].span.combine(&tokens[end].span) });
    }
    Ok(module)
}

fn top_level_word_positions(tokens: &[Token], word: &str) -> Vec<usize> {
    let mut depth = 0usize;
    let mut positions = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::LBrace => depth = depth.saturating_add(1),
            TokenKind::RBrace => depth = depth.saturating_sub(1),
            _ if depth == 0 && token_is_word(token, word) => {
                // These are contextual declaration words, not reserved names.
                // A helper/constant/module called `type_script` remains legal
                // authoring source and must not select the preview parser.
                let mut following = tokens[index + 1..].iter().filter(|token| !matches!(token.kind, TokenKind::Newline));
                if following.next().is_some_and(|token| matches!(token.kind, TokenKind::Identifier(_)))
                    && following
                        .next()
                        .is_some_and(|token| matches!(token.kind, TokenKind::LParen | TokenKind::LBrace) || token_is_word(token, "on"))
                {
                    positions.push(index);
                }
            }
            _ => {}
        }
    }
    positions
}

fn matching_entry_end(tokens: &[Token], start: usize) -> Result<usize> {
    let open = tokens[start..]
        .iter()
        .position(|token| matches!(token.kind, TokenKind::LBrace))
        .map(|offset| start + offset)
        .ok_or_else(|| CompileError::new("group entry requires a body", tokens[start].span))?;
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        match token.kind {
            TokenKind::LBrace => depth = depth.saturating_add(1),
            TokenKind::RBrace => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Ok(index);
                }
            }
            _ => {}
        }
    }
    Err(CompileError::new("unterminated group entry body", tokens[open].span))
}

struct Cursor<'a> {
    tokens: &'a [Token],
    position: usize,
    record_syntax: bool,
    syntax_nodes: Vec<SyntaxNode>,
}

impl<'a> Cursor<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, position: 0, record_syntax: false, syntax_nodes: Vec::new() }
    }

    fn current(&self) -> Option<&'a Token> {
        self.tokens.get(self.position)
    }

    fn span(&self) -> Span {
        self.current().map_or_else(|| self.tokens.last().map_or_else(Span::default, |token| token.span), |token| token.span)
    }

    fn is_done(&self) -> bool {
        self.position >= self.tokens.len()
    }

    fn advance(&mut self) -> Option<&'a Token> {
        let token = self.current();
        self.position = self.position.saturating_add(1);
        token
    }

    fn expect_statement_end(&mut self) -> Result<()> {
        let previous = self.tokens[..self.position]
            .iter()
            .rfind(|token| !matches!(token.kind, TokenKind::Newline))
            .expect("a native policy clause consumed a token");
        let end = previous.span;
        let end_column = end.column + previous.text.chars().count();

        if !self.at_kind(&TokenKind::Semi) {
            return Err(CompileError::new(
                "expected `;` to end this native policy clause; newlines do not terminate statements",
                Span::new(end.end, end.end, end.line, end_column),
            ));
        }
        self.advance();

        Ok(())
    }

    fn expect_list_separator(&mut self) -> Result<()> {
        if self.at_kind(&TokenKind::Comma) {
            self.advance();
        } else if !self.at_kind(&TokenKind::RBrace) {
            return Err(CompileError::new("expected `,` between list entries; a newline is whitespace", self.span()));
        }
        Ok(())
    }

    fn starts_named_block(&self, word: &str) -> bool {
        if !self.current().is_some_and(|token| token_is_word(token, word)) {
            return false;
        }
        let mut next = self.tokens[self.position + 1..].iter().filter(|token| !matches!(token.kind, TokenKind::Newline));
        next.next().is_some_and(|token| matches!(token.kind, TokenKind::Identifier(_)))
            && next.next().is_some_and(|token| matches!(token.kind, TokenKind::LBrace))
    }

    fn starts_block(&self, word: &str) -> bool {
        self.current().is_some_and(|token| token_is_word(token, word))
            && self.tokens[self.position + 1..]
                .iter()
                .find(|token| !matches!(token.kind, TokenKind::Newline))
                .is_some_and(|token| matches!(token.kind, TokenKind::LBrace))
    }

    fn at_kind(&self, expected: &TokenKind) -> bool {
        self.current().is_some_and(|token| std::mem::discriminant(&token.kind) == std::mem::discriminant(expected))
    }

    fn expect_kind(&mut self, expected: TokenKind) -> Result<Token> {
        let Some(token) = self.current() else {
            return Err(CompileError::new(format!("expected {expected}, found end of group entry"), self.span()));
        };
        if std::mem::discriminant(&token.kind) != std::mem::discriminant(&expected) {
            return Err(CompileError::new(format!("expected {expected}, found {}", token.kind), token.span));
        }
        let token = token.clone();
        self.advance();
        Ok(token)
    }

    fn expect_word(&mut self, word: &str) -> Result<Token> {
        let Some(token) = self.current() else {
            return Err(CompileError::new(format!("expected '{word}', found end of group entry"), self.span()));
        };
        if !token_is_word(token, word) {
            return Err(CompileError::new(format!("expected '{word}', found {}", token.kind), token.span));
        }
        let token = token.clone();
        self.advance();
        Ok(token)
    }

    fn parse_name(&mut self, context: &str) -> Result<String> {
        let Some(token) = self.current() else {
            return Err(CompileError::new(format!("expected {context}, found end of group entry"), self.span()));
        };
        let TokenKind::Identifier(name) = &token.kind else {
            return Err(CompileError::new(format!("expected {context}, found {}", token.kind), token.span));
        };
        let name = name.clone();
        self.advance();
        Ok(name)
    }

    fn parse_type_script(&mut self, module: &ast::Module) -> Result<ActionDef> {
        let start = self.expect_word("type_script")?.span;
        let entry_name = self.parse_name("type_script entry name")?;
        let (params, outputs) = self.parse_ports(module, true)?;
        let trigger_type = infer_type_script_schema(&params, &outputs, start)?;

        self.expect_kind(TokenKind::LBrace)?;

        let ParsedEntryBody { body, audits, dispositions, body_items } = self.parse_entry_body(&outputs, true)?;

        let end = self.expect_kind(TokenKind::RBrace)?.span;

        validate_surface(module, &trigger_type, &params, &outputs, &body, &dispositions, start)?;
        Ok(ActionDef {
            name: entry_name,
            params,
            return_type: None,
            outputs,
            state_edges: Vec::new(),
            body,
            scheduler_hint: None,
            next_surface: Some(NextEntrySurface { trigger_type, audits, dispositions, body_items }),
            doc_comment: None,
            span: start.combine(&end),
        })
    }

    fn parse_lock_script(&mut self, module: &ast::Module) -> Result<LockDef> {
        let start = self.expect_word("lock_script")?.span;
        let entry_name = self.parse_name("lock_script entry name")?;
        let (params, _) = self.parse_ports(module, false)?;

        self.expect_kind(TokenKind::LBrace)?;

        let ParsedEntryBody { body, audits, body_items, .. } = self.parse_entry_body(&[], false)?;
        let end = self.expect_kind(TokenKind::RBrace)?.span;
        Ok(LockDef {
            name: entry_name,
            params,
            return_type: Type::Bool,
            body,
            next_surface: Some(NextLockSurface { audits, body_items }),
            span: start.combine(&end),
        })
    }

    fn parse_ports(&mut self, module: &ast::Module, type_script: bool) -> Result<(Vec<Param>, Vec<ActionOutput>)> {
        self.expect_kind(TokenKind::LParen)?;
        let mut params = Vec::new();
        let mut outputs = Vec::new();
        let mut names = BTreeSet::new();
        let mut protected_count = 0;
        while !self.at_kind(&TokenKind::RParen) {
            let start = self.span();
            let role = self.parse_name("entry port role")?;
            let source = match (type_script, role.as_str()) {
                (true, "input") => ParamSource::Input,
                (true, "output") => ParamSource::Output,
                (false, "protected") => ParamSource::Protected,
                (false, "lock_args") => ParamSource::LockArgs,
                (_, "witness") => ParamSource::Witness,
                _ => {
                    let expected = if type_script { "input, output, or witness" } else { "protected, lock_args, or witness" };
                    return Err(CompileError::new(format!("expected {expected} port, found '{role}'"), start));
                }
            };
            let name = self.parse_name("entry port binding")?;
            if !names.insert(name.clone()) {
                return Err(CompileError::new(format!("duplicate entry port '{name}'"), start));
            }
            self.expect_kind(TokenKind::Colon)?;
            let (mut ty, consumed, trace) = parser::parse_type_prefix_with_syntax(&self.tokens[self.position..], self.record_syntax)?;
            self.position += consumed;
            self.syntax_nodes.extend(trace);
            let span = start.combine(&self.tokens[self.position - 1].span);
            if source == ParamSource::Protected {
                validate_protected_type(module, &ty, start)?;
                ty = Type::Ref(Box::new(ty));
                protected_count += 1;
            }
            if source == ParamSource::Output {
                outputs.push(ActionOutput { name, ty, span });
            } else {
                params.push(Param { name, ty, is_mut: false, is_ref: false, is_read_ref: false, source, span });
            }
            if self.at_kind(&TokenKind::Comma) {
                self.advance();
            } else if !self.at_kind(&TokenKind::RParen) {
                return Err(CompileError::new("expected ',' or ')' after entry port", self.span()));
            }
        }
        self.expect_kind(TokenKind::RParen)?;
        if !type_script && protected_count != 1 {
            return Err(CompileError::new("lock_script requires exactly one protected input role", self.span()));
        }
        Ok((params, outputs))
    }

    fn parse_audits(&mut self) -> Result<Vec<NextAudit>> {
        let mut audits = Vec::new();
        let mut names = BTreeSet::new();
        while self.starts_named_block("audit") {
            let start = self.expect_word("audit")?.span;
            let name = self.parse_name("audit declaration name")?;
            if !names.insert(name.clone()) {
                return Err(CompileError::new(format!("duplicate audit declaration '{name}'"), start));
            }

            self.expect_kind(TokenKind::LBrace)?;

            self.expect_word("expected_evidence")?;
            self.expect_kind(TokenKind::Eq)?;
            self.expect_word("external_policy")?;
            let subject = self.parse_parenthesized_expression()?;
            self.expect_statement_end()?;
            let end = self.expect_kind(TokenKind::RBrace)?.span;
            audits.push(NextAudit {
                name,
                evidence: NextAuditEvidence::ExternalPolicy,
                subject,
                span: Span::new(start.start, end.end, start.line, start.column),
            });
        }
        Ok(audits)
    }

    fn parse_entry_body(&mut self, outputs: &[ActionOutput], type_group: bool) -> Result<ParsedEntryBody> {
        let mut parsed = ParsedEntryBody::default();
        let body_start =
            self.tokens[..self.position].iter().rfind(|token| matches!(token.kind, TokenKind::LBrace)).map(|token| token.span);
        if self.at_kind(&TokenKind::Verification) {
            return Err(CompileError::new("`verification` was removed; write ordinary checked statements directly", self.span()));
        }
        while !self.at_kind(&TokenKind::RBrace) {
            if self.is_done() {
                return Err(CompileError::new("unterminated group-bound entry body", self.span()));
            }
            if self.starts_block("effects") {
                return Err(SourceDiagnosticCode::RemovedNativeEffectsBlock
                    .error("write replace, pool, retire, or fresh directly in the action body", self.span()));
            }
            if self.starts_named_block("audit") {
                let first = parsed.audits.len();
                parsed.audits.extend(self.parse_audits()?);
                for index in first..parsed.audits.len() {
                    parsed.body_items.push(NextBodyItem::Audit(index));
                }
                continue;
            }
            let start_statement = parsed.body.len();
            let disposition = if type_group && self.starts_named_block("pool") {
                let pool = self.parse_pool(outputs)?;
                parsed.body.extend(lower_pool_body(&pool)?);
                Some(NextDisposition::Pool(pool))
            } else if type_group && self.starts_named_block("retire") {
                let retirement = self.parse_retirement()?;
                parsed.body.push(Stmt::Discard(Expr::Destroy(DestroyExpr {
                    expr: Box::new(Expr::Identifier(retirement.input.clone())),
                    policy: retirement.absence_policy.clone(),
                    span: retirement.span,
                })));
                Some(NextDisposition::Retire(retirement))
            } else if type_group && self.starts_named_block("fresh") {
                let fresh = self.parse_fresh_output(outputs)?;
                let lock = exact_lock_expression(&fresh.lock).cloned().map(Box::new);
                let expression = if fresh.identity == IdentityPolicy::None {
                    Expr::Create(CreateExpr {
                        target: Some(fresh.output.clone()),
                        ty: fresh.ty.clone(),
                        fields: fresh.data_fields.clone(),
                        lock,
                        span: fresh.span,
                    })
                } else {
                    Expr::CreateUnique(CreateUniqueExpr {
                        target: Some(fresh.output.clone()),
                        ty: fresh.ty.clone(),
                        type_args: Vec::new(),
                        fields: fresh.data_fields.clone(),
                        lock,
                        identity: fresh.identity.clone(),
                        span: fresh.span,
                    })
                };
                parsed.body.push(Stmt::Discard(expression));
                Some(NextDisposition::Fresh(fresh))
            } else {
                let (statement, consumed, trace) =
                    parser::parse_statement_prefix_with_syntax(&self.tokens[self.position..], self.record_syntax)?;
                self.syntax_nodes.extend(trace);
                self.position += consumed;
                let index = parsed.body.len();
                if type_group {
                    for relation_index in 0..crate::ast::statement_replace_relations(&statement).len() {
                        parsed.dispositions.push(NextDisposition::Replace(NextReplacement { statement_index: index, relation_index }));
                    }
                }
                parsed.body.push(statement);
                parsed.body_items.push(NextBodyItem::Statement(index));
                None
            };
            if let Some(disposition) = disposition {
                // A special constructor's values are ordinary shared
                // expressions and can contain their own successor relations.
                // Keep references to the executable owned range without
                // introducing duplicate source statements for formatting.
                for (statement_index, statement) in parsed.body.iter().enumerate().skip(start_statement) {
                    for relation_index in 0..crate::ast::statement_replace_relations(statement).len() {
                        parsed.dispositions.push(NextDisposition::Replace(NextReplacement { statement_index, relation_index }));
                    }
                }
                parsed.body_items.push(NextBodyItem::Disposition {
                    index: parsed.dispositions.len(),
                    start_statement,
                    statement_count: parsed.body.len() - start_statement,
                });
                parsed.dispositions.push(disposition);
            }
        }
        if type_group && parsed.dispositions.is_empty() {
            return Err(CompileError::new("type-group action must contain an exhaustive disposition", self.span()));
        }
        let mut audit_names = BTreeSet::new();
        for audit in &parsed.audits {
            if !audit_names.insert(&audit.name) {
                return Err(CompileError::new(format!("duplicate audit declaration '{}'", audit.name), audit.span));
            }
        }
        if self.record_syntax
            && let Some(start) = body_start
        {
            self.syntax_nodes.push(SyntaxNode { kind: SyntaxKind::Block, span: start.combine(&self.span()) });
        }
        Ok(parsed)
    }

    fn parse_lock_target(&mut self) -> Result<ReplaceLockTreatment> {
        let spelling = self.parse_name("lock target: exact(address) or exact_hash(script_hash)")?;
        let value = match spelling.as_str() {
            "exact" | "exact_hash" => self.parse_parenthesized_expression()?,
            _ => return Err(CompileError::new("lock target must be exact(address) or exact_hash(script_hash)", self.span())),
        };
        Ok(if spelling == "exact" {
            ReplaceLockTreatment::Exact(Box::new(value))
        } else {
            ReplaceLockTreatment::ExactHash(Box::new(value))
        })
    }

    fn parse_pool(&mut self, outputs: &[ActionOutput]) -> Result<NextPool> {
        let start = self.expect_word("pool")?.span;
        let name = self.parse_name("pool name")?;
        self.expect_kind(TokenKind::LBrace)?;
        let mut inputs = None;
        let mut output_names = None;
        let mut data_fields = None;
        let mut output_locks = None;
        while !self.at_kind(&TokenKind::RBrace) {
            let clause = self.take_policy_name("pool")?;
            match clause.text.as_str() {
                "inputs" if inputs.is_none() => inputs = Some(self.parse_name_block("pool input role")?),
                "outputs" if output_names.is_none() => output_names = Some(self.parse_name_block("pool output role")?),
                "data" if data_fields.is_none() => data_fields = Some(self.parse_braced_list(Self::parse_pool_field)?),
                "lock" if output_locks.is_none() => {
                    output_locks = Some(self.parse_braced_list(|cursor| {
                        let output = cursor.parse_name("pool output lock assignment")?;
                        cursor.expect_kind(TokenKind::Eq)?;
                        Ok((output, cursor.parse_lock_target()?))
                    })?);
                }
                "inputs" | "outputs" | "data" | "lock" => {
                    return Err(CompileError::new(format!("duplicate pool clause '{}'", clause.text), clause.span));
                }
                _ => {
                    return Err(CompileError::new(
                        format!("unknown pool clause '{}'; expected inputs, outputs, data, or lock", clause.text),
                        clause.span,
                    ));
                }
            }
            self.expect_statement_end()?;
        }
        let end = self.expect_kind(TokenKind::RBrace)?.span;
        let (Some(inputs), Some(output_names), Some(data_fields), Some(output_locks)) =
            (inputs, output_names, data_fields, output_locks)
        else {
            return Err(CompileError::new("pool requires inputs, outputs, data, and lock clauses", start));
        };
        let ty = output_names
            .first()
            .and_then(|name| outputs.iter().find(|candidate| candidate.name == *name))
            .and_then(|candidate| match &candidate.ty {
                Type::Named(name) => Some(name.clone()),
                _ => None,
            })
            .ok_or_else(|| CompileError::new("pool outputs must name at least one declared Cell output role", start))?;
        Ok(NextPool { name, ty, inputs, outputs: output_names, data_fields, output_locks, span: start.combine(&end) })
    }

    fn parse_pool_field(&mut self) -> Result<NextPoolField> {
        let span = self.span();
        let field = self.parse_name("pool data field")?;
        let treatment = if self.at_kind(&TokenKind::Eq) {
            self.advance();
            self.expect_word("conserve")?;
            NextPoolFieldTreatment::Conserve
        } else {
            NextPoolFieldTreatment::Set(self.parse_braced_list(|cursor| cursor.parse_assignment("pool output field assignment"))?)
        };
        Ok(NextPoolField { span, field, treatment })
    }

    fn parse_name_block(&mut self, context: &str) -> Result<Vec<String>> {
        let names = self.parse_braced_list(|cursor| cursor.parse_name(context))?;
        if names.is_empty() {
            return Err(SourceDiagnosticCode::PoolRoles.error(format!("{context} block must not be empty"), self.span()));
        }
        Ok(names)
    }

    fn parse_braced_list<T>(&mut self, mut parse_item: impl FnMut(&mut Self) -> Result<T>) -> Result<Vec<T>> {
        self.expect_kind(TokenKind::LBrace)?;
        let mut items = Vec::new();
        while !self.at_kind(&TokenKind::RBrace) {
            items.push(parse_item(self)?);
            self.expect_list_separator()?;
        }
        self.expect_kind(TokenKind::RBrace)?;
        Ok(items)
    }

    fn parse_assignment(&mut self, context: &str) -> Result<(String, Expr)> {
        let field = self.parse_name(context)?;
        self.expect_kind(TokenKind::Eq)?;
        Ok((field, self.parse_expression()?))
    }

    fn take_policy_name(&mut self, construct: &str) -> Result<Token> {
        let token = self.current().ok_or_else(|| CompileError::new(format!("unterminated {construct} block"), self.span()))?;
        if !matches!(token.kind, TokenKind::Identifier(_) | TokenKind::Lock) {
            return Err(CompileError::new(format!("expected a {construct} clause, found {}", token.kind), token.span));
        }
        let token = token.clone();
        self.advance();
        Ok(token)
    }

    fn parse_retirement(&mut self) -> Result<NextRetirement> {
        let start = self.expect_word("retire")?.span;
        let input = self.parse_name("retirement input role")?;
        self.expect_kind(TokenKind::LBrace)?;
        self.expect_word("absence")?;
        self.expect_kind(TokenKind::Eq)?;
        let absence_policy = self.parse_absence_policy()?;
        self.expect_statement_end()?;
        let end = self.expect_kind(TokenKind::RBrace)?.span;
        Ok(NextRetirement { input, absence_policy, span: start.combine(&end) })
    }

    fn parse_fresh_output(&mut self, outputs: &[ActionOutput]) -> Result<NextFreshOutput> {
        let start = self.expect_word("fresh")?.span;
        let output = self.parse_name("fresh output role")?;
        let ty = outputs
            .iter()
            .find(|candidate| candidate.name == output)
            .and_then(|candidate| match &candidate.ty {
                Type::Named(name) => Some(name.clone()),
                _ => None,
            })
            .ok_or_else(|| CompileError::new(format!("fresh output '{output}' must name a declared Cell output role"), start))?;
        self.expect_kind(TokenKind::LBrace)?;
        let mut data = None;
        let mut identity = None;
        let mut lock = None;
        while !self.at_kind(&TokenKind::RBrace) {
            let clause = self.take_policy_name("fresh")?;
            match clause.text.as_str() {
                "data" if data.is_none() => {
                    data = Some(self.parse_braced_list(|cursor| cursor.parse_assignment("fresh output data field"))?);
                }
                "identity" if identity.is_none() => {
                    self.expect_kind(TokenKind::Eq)?;
                    identity = Some(self.parse_identity_policy()?);
                }
                "lock" if lock.is_none() => {
                    self.expect_kind(TokenKind::Eq)?;
                    lock = Some(self.parse_lock_target()?);
                }
                "data" | "identity" | "lock" => {
                    return Err(CompileError::new(format!("duplicate fresh clause '{}'", clause.text), clause.span));
                }
                _ => {
                    return Err(CompileError::new(
                        format!("unknown fresh clause '{}'; expected data, identity, or lock", clause.text),
                        clause.span,
                    ));
                }
            }
            self.expect_statement_end()?;
        }
        let end = self.expect_kind(TokenKind::RBrace)?.span;
        let (Some(data_fields), Some(identity), Some(lock)) = (data, identity, lock) else {
            return Err(CompileError::new("fresh requires data, identity, and lock clauses", start));
        };
        Ok(NextFreshOutput { output, ty, data_fields, identity, lock, span: start.combine(&end) })
    }

    fn parse_absence_policy(&mut self) -> Result<DestructionPolicy> {
        let policy = self.parse_name("retirement absence policy")?;
        match policy.as_str() {
            "singleton_type" => Ok(DestructionPolicy::SingletonType),
            "ckb_type_id" => Ok(DestructionPolicy::Unique { identity: "ckb_type_id".to_string() }),
            "field" => {
                self.expect_kind(TokenKind::LParen)?;
                let field = self.parse_name("retirement identity field")?;
                self.expect_kind(TokenKind::RParen)?;
                Ok(DestructionPolicy::Instance { identity_field: field })
            }
            _ => Err(CompileError::new("retire absence policy must be singleton_type, ckb_type_id, or field(name)", self.span())),
        }
    }

    fn parse_identity_policy(&mut self) -> Result<IdentityPolicy> {
        let policy = self.parse_name("fresh output identity policy")?;
        match policy.as_str() {
            "none" => Ok(IdentityPolicy::None),
            "ckb_type_id" => Ok(IdentityPolicy::CkbTypeId),
            "script_args" => Ok(IdentityPolicy::ScriptArgs),
            "singleton_type" => Ok(IdentityPolicy::SingletonType),
            "field" => {
                self.expect_kind(TokenKind::LParen)?;
                let field = self.parse_name("fresh output identity field")?;
                self.expect_kind(TokenKind::RParen)?;
                Ok(IdentityPolicy::Field(field))
            }
            _ => Err(CompileError::new(
                "fresh identity policy must be none, ckb_type_id, field(name), script_args, or singleton_type",
                self.span(),
            )),
        }
    }

    fn parse_expression(&mut self) -> Result<Expr> {
        let (value, consumed, trace) = parser::parse_expression_prefix_with_syntax(&self.tokens[self.position..], self.record_syntax)?;
        self.position += consumed;
        self.syntax_nodes.extend(trace);
        Ok(value)
    }

    fn parse_parenthesized_expression(&mut self) -> Result<Expr> {
        self.expect_kind(TokenKind::LParen)?;
        let expression = self.parse_expression()?;
        self.expect_kind(TokenKind::RParen)?;
        Ok(expression)
    }
}

#[derive(Default)]
struct ParsedEntryBody {
    body: Vec<Stmt>,
    audits: Vec<NextAudit>,
    dispositions: Vec<NextDisposition>,
    body_items: Vec<NextBodyItem>,
}

fn exact_lock_expression(treatment: &ReplaceLockTreatment) -> Option<&Expr> {
    match treatment {
        ReplaceLockTreatment::Exact(value) | ReplaceLockTreatment::ExactHash(value) => Some(value),
        ReplaceLockTreatment::Same => None,
    }
}

fn validate_pool_assignments(pool: &NextPool) -> Result<()> {
    if pool.inputs.is_empty() || pool.outputs.is_empty() {
        return Err(SourceDiagnosticCode::PoolRoles.error("pool requires nonempty input and output roles", pool.span));
    }
    if pool.inputs.iter().collect::<BTreeSet<_>>().len() != pool.inputs.len()
        || pool.outputs.iter().collect::<BTreeSet<_>>().len() != pool.outputs.len()
    {
        return Err(SourceDiagnosticCode::PoolRoles.error("pool input and output roles must each be distinct", pool.span));
    }
    for field in &pool.data_fields {
        if let NextPoolFieldTreatment::Set(assignments) = &field.treatment
            && !assignments.iter().map(|(output, _)| output).eq(pool.outputs.iter())
        {
            return Err(SourceDiagnosticCode::PoolAssignments.error(
                format!(
                    "pool field '{}' must assign every pool output exactly once in declared order: {}",
                    field.field,
                    pool.outputs.join(", ")
                ),
                field.span,
            ));
        }
    }
    Ok(())
}

fn lower_pool_body(pool: &NextPool) -> Result<Vec<Stmt>> {
    validate_pool_assignments(pool)?;
    let mut body = Vec::new();
    for field in &pool.data_fields {
        if matches!(field.treatment, NextPoolFieldTreatment::Conserve) {
            let left = sum_pool_field(&pool.inputs, &field.field, pool.span)?;
            let right = sum_pool_field(&pool.outputs, &field.field, pool.span)?;
            body.push(Stmt::Discard(Expr::Require(RequireExpr {
                condition: Box::new(Expr::Binary(BinaryExpr {
                    op: BinaryOp::Eq,
                    left: Box::new(left),
                    right: Box::new(right),
                    span: pool.span,
                })),
                message: None,
                span: pool.span,
            })));
        }
    }
    for output in &pool.outputs {
        let fields = pool
            .data_fields
            .iter()
            .map(|field| {
                let value = match &field.treatment {
                    NextPoolFieldTreatment::Conserve => pool_field(output, &field.field, pool.span),
                    NextPoolFieldTreatment::Set(assignments) => assignments
                        .iter()
                        .find(|(candidate, _)| candidate == output)
                        .map(|(_, expression)| expression.clone())
                        .ok_or_else(|| {
                            SourceDiagnosticCode::PoolAssignments
                                .error(format!("pool field '{}' has no assignment for output '{output}'", field.field), field.span)
                        })?,
                };
                Ok((field.field.clone(), value))
            })
            .collect::<Result<Vec<_>>>()?;
        let lock = pool
            .output_locks
            .iter()
            .find(|(candidate, _)| candidate == output)
            .and_then(|(_, treatment)| exact_lock_expression(treatment))
            .cloned()
            .map(Box::new);
        body.push(Stmt::Discard(Expr::Create(CreateExpr {
            target: Some(output.clone()),
            ty: pool.ty.clone(),
            fields,
            lock,
            span: pool.span,
        })));
    }
    body.extend(pool.inputs.iter().map(|input| {
        Stmt::Discard(Expr::Consume(crate::ast::ConsumeExpr { expr: Box::new(Expr::Identifier(input.clone())), span: pool.span }))
    }));
    Ok(body)
}

fn sum_pool_field(bindings: &[String], field: &str, span: Span) -> Result<Expr> {
    bindings
        .iter()
        .map(|binding| Expr::Cast(CastExpr { expr: Box::new(pool_field(binding, field, span)), ty: Type::U128, span }))
        .reduce(|left, right| Expr::Binary(BinaryExpr { op: BinaryOp::Add, left: Box::new(left), right: Box::new(right), span }))
        .ok_or_else(|| SourceDiagnosticCode::PoolRoles.error("pool requires nonempty input and output roles", span))
}

fn pool_field(binding: &str, field: &str, span: Span) -> Expr {
    Expr::FieldAccess(FieldAccessExpr { expr: Box::new(Expr::Identifier(binding.to_string())), field: field.to_string(), span })
}

fn infer_type_script_schema(params: &[Param], outputs: &[ActionOutput], span: Span) -> Result<String> {
    let mut cell_roles = params
        .iter()
        .filter(|param| param.source == ParamSource::Input)
        .map(|param| (&param.ty, param.span))
        .chain(outputs.iter().map(|output| (&output.ty, output.span)));
    let (expected, role_span) = cell_roles
        .next()
        .ok_or_else(|| CompileError::new("type_script requires at least one explicitly typed input or output Cell role", span))?;
    let Type::Named(schema) = expected else {
        return Err(CompileError::new("type_script Cell roles must name a Cell-backed resource, shared type, or receipt", role_span));
    };
    if let Some((other, other_span)) = cell_roles.find(|(ty, _)| *ty != expected) {
        return Err(CompileError::new(
            format!(
                "type_script input/output ports must all use the same Cell schema: expected {schema}, found {}",
                other.source_syntax()
            ),
            other_span,
        ));
    }
    Ok(schema.clone())
}

fn validate_surface(
    module: &ast::Module,
    trigger_type: &str,
    params: &[Param],
    outputs: &[ActionOutput],
    body: &[Stmt],
    dispositions: &[NextDisposition],
    span: Span,
) -> Result<()> {
    let fields = module.items.iter().find_map(|item| match item {
        Item::Resource(definition) if definition.name == trigger_type => Some(&definition.fields),
        Item::Shared(definition) if definition.name == trigger_type => Some(&definition.fields),
        Item::Receipt(definition) if definition.name == trigger_type => Some(&definition.fields),
        _ => None,
    });
    let Some(fields) = fields else {
        return Err(CompileError::new(
            format!("type_script Cell role schema '{trigger_type}' must name a Cell-backed resource, shared type, or receipt in this module"),
            span,
        ));
    };
    let declared_fields = fields.iter().map(|field| field.name.clone()).collect::<Vec<_>>();
    let inputs =
        params.iter().filter(|param| param.source == ParamSource::Input).map(|param| param.name.as_str()).collect::<BTreeSet<_>>();
    let output_names = outputs.iter().map(|output| output.name.as_str()).collect::<BTreeSet<_>>();
    let mut disposed_inputs = BTreeSet::new();
    let mut produced_outputs = BTreeSet::new();
    let mut pool_names = BTreeSet::new();
    for disposition in dispositions {
        match disposition {
            NextDisposition::Replace(replacement) => {
                let Some(relation) = replacement.relation(body) else {
                    return Err(CompileError::new("replacement source must reference its shared body relation", span));
                };
                if !inputs.contains(relation.before.as_str()) || !output_names.contains(relation.after.as_str()) {
                    return Err(CompileError::new(
                        format!(
                            "replace {} -> {} must bind declared type_group<{trigger_type}> input/output roles",
                            relation.before, relation.after
                        ),
                        relation.span,
                    ));
                }
                disposed_inputs.insert(relation.before.as_str());
                produced_outputs.insert(relation.after.as_str());
                validate_replacement_fields(relation, &declared_fields)?;
            }
            NextDisposition::Pool(pool) => {
                if !pool_names.insert(pool.name.as_str()) {
                    return Err(CompileError::new(format!("duplicate pool disposition '{}'", pool.name), pool.span));
                }
                if pool.ty != trigger_type
                    || pool.inputs.iter().any(|input| !inputs.contains(input.as_str()))
                    || pool.outputs.iter().any(|output| !output_names.contains(output.as_str()))
                {
                    return Err(CompileError::new(
                        format!("pool {} must bind only declared type_group<{trigger_type}> input/output roles", pool.name),
                        pool.span,
                    ));
                }
                disposed_inputs.extend(pool.inputs.iter().map(String::as_str));
                produced_outputs.extend(pool.outputs.iter().map(String::as_str));
                let pool_fields = pool.data_fields.iter().map(|field| field.field.clone()).collect::<Vec<_>>();
                if pool_fields != declared_fields {
                    return Err(CompileError::new(
                        format!("pool data must exhaustively list fields in schema order: {}", declared_fields.join(", ")),
                        pool.span,
                    ));
                }
                if !pool.data_fields.iter().any(|field| matches!(field.treatment, NextPoolFieldTreatment::Conserve)) {
                    return Err(CompileError::new(
                        "pool must declare at least one numeric field as '= conserve' so pooled accounting is checked on chain",
                        pool.span,
                    ));
                }
                validate_pool_assignments(pool)?;
                let locked = pool.output_locks.iter().map(|(output, _)| output.as_str()).collect::<Vec<_>>();
                let expected = pool.outputs.iter().map(String::as_str).collect::<Vec<_>>();
                if locked != expected {
                    return Err(CompileError::new(
                        format!("pool lock must assign every pool output exactly once in declared order: {}", pool.outputs.join(", ")),
                        pool.span,
                    ));
                }
            }
            NextDisposition::Retire(retirement) => {
                if !inputs.contains(retirement.input.as_str()) {
                    return Err(CompileError::new(
                        format!("retire {} must bind a declared type_group<{trigger_type}> input role", retirement.input),
                        retirement.span,
                    ));
                }
                disposed_inputs.insert(retirement.input.as_str());
            }
            NextDisposition::Fresh(fresh) => {
                if !output_names.contains(fresh.output.as_str()) || fresh.ty != trigger_type {
                    return Err(CompileError::new(
                        format!("fresh {} must bind a declared type_group<{trigger_type}> output role", fresh.output),
                        fresh.span,
                    ));
                }
                produced_outputs.insert(fresh.output.as_str());
                let fresh_fields = fresh.data_fields.iter().map(|(field, _)| field.clone()).collect::<Vec<_>>();
                if fresh_fields != declared_fields {
                    return Err(CompileError::new(
                        format!("fresh data must exhaustively list fields in schema order: {}", declared_fields.join(", ")),
                        fresh.span,
                    ));
                }
            }
        }
    }
    if disposed_inputs != inputs || produced_outputs != output_names {
        return Err(CompileError::new(
            "entry dispositions must exhaustively dispose every type-group input and account for every type-group output",
            span,
        ));
    }
    Ok(())
}

fn validate_replacement_fields(relation: &crate::ast::ReplaceRelation, declared: &[String]) -> Result<()> {
    let fields = match &relation.data {
        ReplaceDataTreatment::Fields(fields) => fields
            .iter()
            .map(|field| match field {
                ReplaceFieldTreatment::Same(name) | ReplaceFieldTreatment::Assign(name, _) => name,
            })
            .collect::<Vec<_>>(),
        ReplaceDataTreatment::SameExcept(fields) => fields.iter().map(|(name, _)| name).collect(),
    };
    let mut seen = BTreeSet::new();
    for field in &fields {
        if !declared.contains(field) || !seen.insert(*field) {
            return Err(CompileError::new(format!("replace data contains an unknown or duplicate field '{field}'"), relation.span));
        }
    }
    if matches!(relation.data, ReplaceDataTreatment::Fields(_)) && seen.len() != declared.len() {
        return Err(CompileError::new(format!("replace data must exhaustively list fields: {}", declared.join(", ")), relation.span));
    }
    Ok(())
}

fn validate_protected_type(module: &ast::Module, ty: &Type, span: Span) -> Result<()> {
    let Type::Named(name) = ty else {
        return Err(CompileError::new("Edition 2027 protected ports must name a Cell-backed resource, shared type, or receipt", span));
    };
    let cell_backed = module.items.iter().any(|item| {
        matches!(item, Item::Resource(definition) if definition.name == *name)
            || matches!(item, Item::Shared(definition) if definition.name == *name)
            || matches!(item, Item::Receipt(definition) if definition.name == *name)
    });
    if !cell_backed {
        return Err(CompileError::new(format!("protected port type {name} is not a Cell-backed declaration in this module"), span));
    }
    Ok(())
}

fn constitution_diagnostics(module: &ast::Module, tokens: &[Token]) -> Vec<CompileError> {
    let mut diagnostics = Vec::new();
    let entries = module.items.iter().filter(|item| matches!(item, Item::Action(_) | Item::Lock(_))).collect::<Vec<_>>();
    if entries.len() > 1 {
        diagnostics.push(CompileError::new(
            "Edition 2027 preview emits only SingleEntry artifacts; select one source entry or split the artifact until explicit versioned dispatch is accepted",
            module.span,
        ));
    }
    for item in &module.items {
        let (entry_kind, entry_name, params) = match item {
            Item::Action(entry) => ("action", entry.name.as_str(), entry.params.as_slice()),
            Item::Lock(entry) => ("lock", entry.name.as_str(), entry.params.as_slice()),
            _ => continue,
        };
        for param in params {
            if param.source == ParamSource::Default {
                diagnostics.push(CompileError::new(
                    format!(
                        "Edition 2027 parameter '{}::{}' has no explicit source; use input, output, protected, witness, or lock_args provenance",
                        entry_kind, entry_name
                    ),
                    param.span,
                ));
            }
        }
    }
    for (index, token) in tokens.iter().enumerate() {
        let next_significant = tokens[index + 1..]
            .iter()
            .find(|candidate| !matches!(candidate.kind, TokenKind::Whitespace | TokenKind::Comment(_) | TokenKind::Newline));
        let consume_statement = matches!(token.kind, TokenKind::Consume)
            && next_significant.is_some_and(|candidate| {
                !matches!(candidate.kind, TokenKind::Comma | TokenKind::LBrace | TokenKind::RBrace | TokenKind::Semi | TokenKind::Eof)
            });
        let ambiguous_consume = consume_statement || matches!(&token.kind, TokenKind::Identifier(name) if name == "consume_each");
        if ambiguous_consume {
            diagnostics.push(CompileError::new(
                "Edition 2027 rejects ambiguous consume/consume_each disposition; choose an explicit successor, pooled accounting, or retirement policy",
                token.span,
            ));
        }
    }
    diagnostics
}

fn token_is_word(token: &Token, word: &str) -> bool {
    match &token.kind {
        TokenKind::Identifier(name) => name == word,
        _ => token.text == word,
    }
}
