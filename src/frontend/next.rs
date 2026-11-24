use super::*;
use crate::ast::{
    Expr, Item, LockDef, NextAudit, NextAuditEvidence, NextBodyItem, NextLockSurface, Param, ParamSource, Stmt, Type, Visibility,
};
use crate::error::Span;
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
    !top_level_word_positions(tokens, "lock_script").is_empty()
}

fn parse_native_surface(tokens: &[Token], record: bool, nodes: &mut Vec<SyntaxNode>) -> Result<ast::Module> {
    // The formatter retains trivia from the original token stream and uses
    // these same spans. Native grammar consumes only significant tokens.
    let significant_tokens = tokens.iter().filter(|token| !matches!(token.kind, TokenKind::Newline)).cloned().collect::<Vec<_>>();
    let tokens = significant_tokens.as_slice();
    let positions = top_level_word_positions(tokens, "lock_script");
    if positions.len() != 1 {
        let span = positions.first().map_or_else(Span::default, |position| tokens[*position].span);
        return Err(CompileError::new("a Lock Script module requires exactly one lock_script entry", span));
    }
    parse_lock_script_surface(tokens, record, nodes)
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
                // A helper/constant/module called `lock_script` remains legal
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

    fn starts_named_block(&self, word: &str) -> bool {
        if !self.current().is_some_and(|token| token_is_word(token, word)) {
            return false;
        }
        let mut next = self.tokens[self.position + 1..].iter().filter(|token| !matches!(token.kind, TokenKind::Newline));
        next.next().is_some_and(|token| matches!(token.kind, TokenKind::Identifier(_)))
            && next.next().is_some_and(|token| matches!(token.kind, TokenKind::LBrace))
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

    fn parse_lock_script(&mut self, module: &ast::Module) -> Result<LockDef> {
        let start = self.expect_word("lock_script")?.span;
        let entry_name = self.parse_name("lock_script entry name")?;
        let params = self.parse_lock_ports(module)?;

        self.expect_kind(TokenKind::LBrace)?;

        let ParsedEntryBody { body, audits, body_items } = self.parse_entry_body()?;
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

    fn parse_lock_ports(&mut self, module: &ast::Module) -> Result<Vec<Param>> {
        self.expect_kind(TokenKind::LParen)?;
        let mut params = Vec::new();
        let mut names = BTreeSet::new();
        let mut protected_count = 0;
        while !self.at_kind(&TokenKind::RParen) {
            let start = self.span();
            let role = self.parse_name("entry port role")?;
            let source = match role.as_str() {
                "protected" => ParamSource::Protected,
                "lock_args" => ParamSource::LockArgs,
                "witness" => ParamSource::Witness,
                _ => return Err(CompileError::new(format!("expected protected, lock_args, or witness port, found '{role}'"), start)),
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
            params.push(Param { name, ty, is_mut: false, is_ref: false, is_read_ref: false, source, span });
            if self.at_kind(&TokenKind::Comma) {
                self.advance();
            } else if !self.at_kind(&TokenKind::RParen) {
                return Err(CompileError::new("expected ',' or ')' after entry port", self.span()));
            }
        }
        self.expect_kind(TokenKind::RParen)?;
        if protected_count != 1 {
            return Err(CompileError::new("lock_script requires exactly one protected input role", self.span()));
        }
        Ok(params)
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

    fn parse_entry_body(&mut self) -> Result<ParsedEntryBody> {
        let mut parsed = ParsedEntryBody::default();
        let body_start = self.tokens[self.position - 1].span;
        if self.at_kind(&TokenKind::Verification) {
            return Err(CompileError::new("`verification` was removed; write ordinary checked statements directly", self.span()));
        }
        let mut audit_names = BTreeSet::new();
        while !self.at_kind(&TokenKind::RBrace) {
            if self.is_done() {
                return Err(CompileError::new("unterminated group-bound entry body", self.span()));
            }
            if self.starts_named_block("audit") {
                for audit in self.parse_audits()? {
                    if !audit_names.insert(audit.name.clone()) {
                        return Err(CompileError::new(format!("duplicate audit declaration '{}'", audit.name), audit.span));
                    }
                    parsed.body_items.push(NextBodyItem::Audit(parsed.audits.len()));
                    parsed.audits.push(audit);
                }
                continue;
            }
            let (statement, consumed, trace) =
                parser::parse_statement_prefix_with_syntax(&self.tokens[self.position..], self.record_syntax)?;
            self.syntax_nodes.extend(trace);
            self.position += consumed;
            parsed.body_items.push(NextBodyItem::Statement(parsed.body.len()));
            parsed.body.push(statement);
        }
        if self.record_syntax {
            self.syntax_nodes.push(SyntaxNode { kind: SyntaxKind::Block, span: body_start.combine(&self.span()) });
        }
        Ok(parsed)
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
    body_items: Vec<NextBodyItem>,
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
