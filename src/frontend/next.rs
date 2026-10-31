use super::*;
use crate::ast::{
    ActionDef, ActionOutput, BinaryExpr, BinaryOp, CastExpr, CreateExpr, CreateUniqueExpr, DestroyExpr, DestructionPolicy, Expr,
    FieldAccessExpr, IdentityPolicy, Item, LockDef, NextAudit, NextAuditEvidence, NextDisposition, NextEntrySurface, NextFreshOutput,
    NextLockSurface, NextPool, NextPoolField, NextPoolFieldTreatment, NextReplacement, NextRetirement, Param, ParamSource,
    RequireExpr, Stmt, Type, Visibility,
};
use crate::error::{SourceDiagnosticCode, Span};
use crate::lexer::token::{Token, TokenKind};
use std::collections::BTreeSet;

pub(super) fn parse(source: &str) -> Result<ast::Module> {
    let tokens = lexer::lex(source)?;
    if !has_native_surface(&tokens) {
        return super::authoring::parse(&tokens);
    }
    let module = parse_native_surface(&tokens)?;
    let diagnostics = constitution_diagnostics(&module, &tokens);
    diagnostics.into_iter().next().map_or(Ok(module), Err)
}

pub(super) fn parse_diagnostics(source: &str) -> std::result::Result<ast::Module, Vec<CompileError>> {
    let tokens = lexer::lex(source).map_err(|error| vec![error])?;
    if !has_native_surface(&tokens) {
        return super::authoring::parse_diagnostics(&tokens);
    }
    let module = parse_native_surface(&tokens).map_err(|error| vec![error])?;
    let diagnostics = constitution_diagnostics(&module, &tokens);
    if diagnostics.is_empty() {
        Ok(module)
    } else {
        Err(diagnostics)
    }
}

pub(super) fn has_native_surface(tokens: &[Token]) -> bool {
    !top_level_word_positions(tokens, "type_script").is_empty() || !top_level_word_positions(tokens, "lock_script").is_empty()
}

fn parse_native_surface(tokens: &[Token]) -> Result<ast::Module> {
    let type_positions = top_level_word_positions(tokens, "type_script");
    let lock_positions = top_level_word_positions(tokens, "lock_script");
    if type_positions.len() + lock_positions.len() != 1 {
        let span =
            type_positions.first().or_else(|| lock_positions.first()).map_or_else(Span::default, |position| tokens[*position].span);
        return Err(CompileError::new(
            "Edition 2027 currently requires exactly one native type_script or lock_script container per source module",
            span,
        ));
    }
    if type_positions.len() == 1 {
        parse_type_script_surface(tokens)
    } else {
        parse_lock_script_surface(tokens)
    }
}

fn parse_type_script_surface(tokens: &[Token]) -> Result<ast::Module> {
    let positions = top_level_word_positions(tokens, "type_script");
    if positions.len() != 1 {
        return Err(CompileError::new(
            "Edition 2027 currently requires exactly one type_script container per source module",
            positions.first().map_or_else(Span::default, |position| tokens[*position].span),
        ));
    }
    let start = positions[0];
    let end = matching_container_end(tokens, start)?;
    if tokens[end + 1..].iter().any(|token| !matches!(token.kind, TokenKind::Newline | TokenKind::Eof)) {
        return Err(CompileError::new(
            "Edition 2027 type_script must be the final top-level declaration in this preview",
            tokens[end + 1].span,
        ));
    }

    let mut base_tokens = tokens[..start].to_vec();
    let eof = tokens.last().cloned().unwrap_or_else(|| Token::new(TokenKind::Eof, Span::default(), ""));
    base_tokens.push(eof);
    let mut module = parser::parse(&base_tokens)?;
    if module.items.iter().any(|item| matches!(item, Item::Action(_) | Item::Lock(_))) {
        return Err(CompileError::new(
            "Edition 2027 type_script cannot be combined with legacy action or lock declarations",
            tokens[start].span,
        ));
    }

    let mut cursor = Cursor::new(&tokens[start..=end]);
    let action = cursor.parse_type_script(&module)?;
    cursor.skip_newlines();
    if !cursor.is_done() {
        return Err(CompileError::new("unexpected tokens after Edition 2027 type_script", cursor.span()));
    }
    module.visibilities.insert(action.name.clone(), Visibility::LegacyPublic);
    module.items.push(Item::Action(action));
    module.span = module.span.combine(&tokens[end].span);
    Ok(module)
}

fn parse_lock_script_surface(tokens: &[Token]) -> Result<ast::Module> {
    let positions = top_level_word_positions(tokens, "lock_script");
    debug_assert_eq!(positions.len(), 1);
    let start = positions[0];
    let end = matching_container_end(tokens, start)?;
    if tokens[end + 1..].iter().any(|token| !matches!(token.kind, TokenKind::Newline | TokenKind::Eof)) {
        return Err(CompileError::new(
            "Edition 2027 lock_script must be the final top-level declaration in this preview",
            tokens[end + 1].span,
        ));
    }

    let mut base_tokens = tokens[..start].to_vec();
    let eof = tokens.last().cloned().unwrap_or_else(|| Token::new(TokenKind::Eof, Span::default(), ""));
    base_tokens.push(eof);
    let mut module = parser::parse(&base_tokens)?;
    if module.items.iter().any(|item| matches!(item, Item::Action(_) | Item::Lock(_))) {
        return Err(CompileError::new(
            "Edition 2027 lock_script cannot be combined with legacy action or lock declarations",
            tokens[start].span,
        ));
    }

    let mut cursor = Cursor::new(&tokens[start..=end]);
    let lock = cursor.parse_lock_script(&module)?;
    cursor.skip_newlines();
    if !cursor.is_done() {
        return Err(CompileError::new("unexpected tokens after Edition 2027 lock_script", cursor.span()));
    }
    module.visibilities.insert(lock.name.clone(), Visibility::LegacyPublic);
    module.items.push(Item::Lock(lock));
    module.span = module.span.combine(&tokens[end].span);
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
                    && following.next().is_some_and(|token| token_is_word(token, "on"))
                {
                    positions.push(index);
                }
            }
            _ => {}
        }
    }
    positions
}

fn matching_container_end(tokens: &[Token], start: usize) -> Result<usize> {
    let open = tokens[start..]
        .iter()
        .position(|token| matches!(token.kind, TokenKind::LBrace))
        .map(|offset| start + offset)
        .ok_or_else(|| CompileError::new("native Script container requires a body", tokens[start].span))?;
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
    Err(CompileError::new("unterminated native Script container body", tokens[open].span))
}

struct Cursor<'a> {
    tokens: &'a [Token],
    position: usize,
}

impl<'a> Cursor<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, position: 0 }
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

    fn skip_newlines(&mut self) {
        while self.current().is_some_and(|token| matches!(token.kind, TokenKind::Newline)) {
            self.advance();
        }
    }

    fn consume_statement_separator(&mut self) {
        if self.current().is_some_and(|token| matches!(token.kind, TokenKind::Semi)) {
            self.advance();
        }
        self.skip_newlines();
    }

    fn at_kind(&self, expected: &TokenKind) -> bool {
        self.current().is_some_and(|token| std::mem::discriminant(&token.kind) == std::mem::discriminant(expected))
    }

    fn expect_kind(&mut self, expected: TokenKind) -> Result<Token> {
        let Some(token) = self.current() else {
            return Err(CompileError::new(format!("expected {expected}, found end of native Script container"), self.span()));
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
            return Err(CompileError::new(format!("expected '{word}', found end of native Script container"), self.span()));
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
            return Err(CompileError::new(format!("expected {context}, found end of native Script container"), self.span()));
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
        let container_name = self.parse_name("type_script name")?;
        self.expect_word("on")?;
        self.expect_word("type_group")?;
        self.expect_kind(TokenKind::Lt)?;
        let trigger_type = self.parse_name("type_group schema")?;
        self.expect_kind(TokenKind::Gt)?;
        self.skip_newlines();
        self.expect_kind(TokenKind::LBrace)?;
        self.skip_newlines();
        self.expect_word("entry")?;
        let entry_name = self.parse_name("entry name")?;
        let (params, outputs) = self.parse_ports()?;
        self.skip_newlines();
        self.expect_kind(TokenKind::LBrace)?;
        self.skip_newlines();
        let (verify, mut body) = self.parse_verify()?;
        self.skip_newlines();
        let audits = self.parse_audits()?;
        let (dispositions, transitions) = self.parse_dispositions(&outputs)?;
        body.extend(transitions);
        self.skip_newlines();
        let entry_end = self.expect_kind(TokenKind::RBrace)?.span;
        self.skip_newlines();
        let container_end = self.expect_kind(TokenKind::RBrace)?.span;

        validate_surface(module, &trigger_type, &params, &outputs, &dispositions, start)?;
        Ok(ActionDef {
            name: entry_name,
            params,
            return_type: None,
            outputs,
            state_edges: Vec::new(),
            body,
            scheduler_hint: None,
            next_surface: Some(NextEntrySurface { container_name, trigger_type, verify, audits, dispositions }),
            doc_comment: None,
            span: Span::new(start.start, container_end.end, start.line, start.column).combine(&entry_end),
        })
    }

    fn parse_lock_script(&mut self, module: &ast::Module) -> Result<LockDef> {
        let start = self.expect_word("lock_script")?.span;
        let container_name = self.parse_name("lock_script name")?;
        self.expect_word("on")?;
        self.expect_word("lock_group")?;
        self.skip_newlines();
        self.expect_kind(TokenKind::LBrace)?;
        self.skip_newlines();
        self.expect_word("entry")?;
        let entry_name = self.parse_name("entry name")?;
        let params = self.parse_lock_ports(module)?;
        self.skip_newlines();
        self.expect_kind(TokenKind::LBrace)?;
        self.skip_newlines();
        let (verify, body) = self.parse_verify()?;
        self.skip_newlines();
        let audits = self.parse_audits()?;
        let entry_end = self.expect_kind(TokenKind::RBrace)?.span;
        self.skip_newlines();
        let container_end = self.expect_kind(TokenKind::RBrace)?.span;
        Ok(LockDef {
            name: entry_name,
            params,
            return_type: Type::Bool,
            body,
            next_surface: Some(NextLockSurface { container_name, verify, audits }),
            span: Span::new(start.start, container_end.end, start.line, start.column).combine(&entry_end),
        })
    }

    fn parse_lock_ports(&mut self, module: &ast::Module) -> Result<Vec<Param>> {
        self.expect_kind(TokenKind::LParen)?;
        self.skip_newlines();
        let mut params = Vec::new();
        let mut names = BTreeSet::new();
        let mut protected_ordinal = 0usize;
        while !self.at_kind(&TokenKind::RParen) {
            let start = self.span();
            let source_word = self.parse_name("lock port source (protected, witness, or lock_args)")?;
            let source = match source_word.as_str() {
                "protected" => ParamSource::Protected,
                "witness" => ParamSource::Witness,
                "lock_args" => ParamSource::LockArgs,
                _ => {
                    return Err(CompileError::new(
                        "Edition 2027 lock_script ports currently support protected, witness, and lock_args",
                        start,
                    ));
                }
            };
            let name = self.parse_name("lock port binding")?;
            if !names.insert(name.clone()) {
                return Err(CompileError::new(format!("duplicate entry port '{name}'"), start));
            }
            self.expect_kind(TokenKind::Colon)?;
            let type_start = self.position;
            let mut depth = 0usize;
            while let Some(token) = self.current() {
                match token.kind {
                    TokenKind::Lt | TokenKind::LBracket | TokenKind::LParen => {
                        depth = depth.saturating_add(1);
                        self.advance();
                    }
                    TokenKind::Gt | TokenKind::RBracket | TokenKind::RParen if depth > 0 => {
                        depth = depth.saturating_sub(1);
                        self.advance();
                    }
                    _ if depth == 0 && token_is_word(token, "from") => break,
                    _ => {
                        self.advance();
                    }
                }
            }
            if type_start == self.position {
                return Err(CompileError::new("lock entry port requires a type", self.span()));
            }
            let mut ty = parser::parse_type_fragment(&self.tokens[type_start..self.position])?;
            self.expect_word("from")?;
            match source {
                ParamSource::Protected => {
                    self.parse_indexed_source("group_input", protected_ordinal)?;
                    protected_ordinal += 1;
                    validate_protected_type(module, &ty, start)?;
                    ty = Type::Ref(Box::new(ty));
                }
                ParamSource::Witness => {
                    self.expect_word("group_witness")?;
                    self.expect_kind(TokenKind::Dot)?;
                    self.expect_word("input_type")?;
                }
                ParamSource::LockArgs => {
                    self.expect_word("current_script")?;
                    self.expect_kind(TokenKind::Dot)?;
                    self.expect_word("args")?;
                }
                ParamSource::Default | ParamSource::Input | ParamSource::Output => unreachable!(),
            }
            params.push(Param {
                name,
                ty,
                is_mut: false,
                is_ref: false,
                is_read_ref: false,
                source,
                span: start.combine(&self.span()),
            });
            self.skip_newlines();
            if self.at_kind(&TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
            } else if !self.at_kind(&TokenKind::RParen) {
                return Err(CompileError::new("expected ',' or ')' after lock entry port", self.span()));
            }
        }
        self.expect_kind(TokenKind::RParen)?;
        if protected_ordinal != 1 {
            return Err(CompileError::new(
                "Edition 2027 lock_script preview requires exactly one protected group_input[0] role",
                self.span(),
            ));
        }
        Ok(params)
    }

    fn parse_ports(&mut self) -> Result<(Vec<Param>, Vec<ActionOutput>)> {
        self.expect_kind(TokenKind::LParen)?;
        self.skip_newlines();
        let mut params = Vec::new();
        let mut outputs = Vec::new();
        let mut names = BTreeSet::new();
        let mut input_ordinal = 0usize;
        let mut output_ordinal = 0usize;
        while !self.at_kind(&TokenKind::RParen) {
            let start = self.span();
            let source_word = self.parse_name("port source (input, output, or witness)")?;
            let source = match source_word.as_str() {
                "input" => ParamSource::Input,
                "output" => ParamSource::Output,
                "witness" => ParamSource::Witness,
                _ => {
                    return Err(CompileError::new(
                        "Edition 2027 type_script ports currently support input, output, and witness",
                        start,
                    ));
                }
            };
            let name = self.parse_name("port binding")?;
            if !names.insert(name.clone()) {
                return Err(CompileError::new(format!("duplicate entry port '{name}'"), start));
            }
            self.expect_kind(TokenKind::Colon)?;
            let type_start = self.position;
            let mut depth = 0usize;
            while let Some(token) = self.current() {
                match token.kind {
                    TokenKind::Lt | TokenKind::LBracket | TokenKind::LParen => {
                        depth = depth.saturating_add(1);
                        self.advance();
                    }
                    TokenKind::Gt | TokenKind::RBracket | TokenKind::RParen if depth > 0 => {
                        depth = depth.saturating_sub(1);
                        self.advance();
                    }
                    _ if depth == 0 && token_is_word(token, "from") => break,
                    _ => {
                        self.advance();
                    }
                }
            }
            if type_start == self.position {
                return Err(CompileError::new("entry port requires a type", self.span()));
            }
            let ty = parser::parse_type_fragment(&self.tokens[type_start..self.position])?;
            self.expect_word("from")?;
            match source {
                ParamSource::Input => {
                    self.parse_indexed_source("group_input", input_ordinal)?;
                    input_ordinal += 1;
                    params.push(Param {
                        name,
                        ty,
                        is_mut: false,
                        is_ref: false,
                        is_read_ref: false,
                        source,
                        span: start.combine(&self.span()),
                    });
                }
                ParamSource::Output => {
                    self.parse_indexed_source("group_output", output_ordinal)?;
                    output_ordinal += 1;
                    outputs.push(ActionOutput { name, ty, span: start.combine(&self.span()) });
                }
                ParamSource::Witness => {
                    self.expect_word("group_witness")?;
                    self.expect_kind(TokenKind::Dot)?;
                    self.expect_word("input_type")?;
                    params.push(Param {
                        name,
                        ty,
                        is_mut: false,
                        is_ref: false,
                        is_read_ref: false,
                        source,
                        span: start.combine(&self.span()),
                    });
                }
                ParamSource::Default | ParamSource::Protected | ParamSource::LockArgs => unreachable!(),
            }
            self.skip_newlines();
            if self.at_kind(&TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
            } else if !self.at_kind(&TokenKind::RParen) {
                return Err(CompileError::new("expected ',' or ')' after entry port", self.span()));
            }
        }
        self.expect_kind(TokenKind::RParen)?;
        Ok((params, outputs))
    }

    fn parse_indexed_source(&mut self, source: &str, expected: usize) -> Result<()> {
        self.expect_word(source)?;
        self.expect_kind(TokenKind::LBracket)?;
        let ordinal = match self.current().map(|token| &token.kind) {
            Some(TokenKind::Integer(value)) => usize::try_from(*value)
                .map_err(|_| CompileError::new(format!("{source} ordinal exceeds the supported index range"), self.span()))?,
            _ => return Err(CompileError::new(format!("{source} requires a numeric ordinal"), self.span())),
        };
        if ordinal != expected {
            return Err(CompileError::new(
                format!("{source}[{ordinal}] is non-canonical here; expected {source}[{expected}]"),
                self.span(),
            ));
        }
        self.advance();
        self.expect_kind(TokenKind::RBracket)?;
        Ok(())
    }

    fn parse_verify(&mut self) -> Result<(Vec<Expr>, Vec<Stmt>)> {
        self.expect_word("verify")?;
        self.skip_newlines();
        self.expect_kind(TokenKind::LBrace)?;
        self.skip_newlines();
        let mut verify = Vec::new();
        let mut body = Vec::new();
        while !self.at_kind(&TokenKind::RBrace) {
            let enforce = self.expect_word("enforce")?.span;
            let expression_tokens = self.take_statement_tokens()?;
            if expression_tokens.is_empty() {
                return Err(CompileError::new("enforce requires a boolean expression", enforce));
            }
            let expression = parser::parse_expression_fragment(expression_tokens)?;
            let span = enforce.combine(&expression.span());
            verify.push(expression.clone());
            body.push(Stmt::Expr(Expr::Require(RequireExpr { condition: Box::new(expression), message: None, span })));
            self.consume_statement_separator();
        }
        self.expect_kind(TokenKind::RBrace)?;
        Ok((verify, body))
    }

    fn parse_audits(&mut self) -> Result<Vec<NextAudit>> {
        let mut audits = Vec::new();
        let mut names = BTreeSet::new();
        while self.current().is_some_and(|token| token_is_word(token, "audit")) {
            let start = self.expect_word("audit")?.span;
            let name = self.parse_name("audit declaration name")?;
            if !names.insert(name.clone()) {
                return Err(CompileError::new(format!("duplicate audit declaration '{name}'"), start));
            }
            self.skip_newlines();
            self.expect_kind(TokenKind::LBrace)?;
            self.skip_newlines();
            self.expect_word("expected_evidence")?;
            self.expect_kind(TokenKind::Eq)?;
            self.expect_word("external_policy")?;
            let subject = self.parse_parenthesized_expression("external_policy")?;
            self.consume_statement_separator();
            let end = self.expect_kind(TokenKind::RBrace)?.span;
            audits.push(NextAudit {
                name,
                evidence: NextAuditEvidence::ExternalPolicy,
                subject,
                span: Span::new(start.start, end.end, start.line, start.column),
            });
            self.consume_statement_separator();
        }
        Ok(audits)
    }

    fn parse_dispositions(&mut self, outputs: &[ActionOutput]) -> Result<(Vec<NextDisposition>, Vec<Stmt>)> {
        let mut dispositions = Vec::new();
        let mut body = Vec::new();
        while !self.at_kind(&TokenKind::RBrace) {
            if self.current().is_some_and(|token| token_is_word(token, "effects")) {
                return Err(SourceDiagnosticCode::RemovedNativeEffectsBlock.error(
                    "the native effects block was removed; write replace, pool, retire, or fresh directly in the entry body",
                    self.span(),
                ));
            }
            if self.current().is_some_and(|token| token_is_word(token, "replace")) {
                let replacement = self.parse_replacement()?;
                let span = replacement.span;
                body.push(Stmt::Expr(Expr::StdlibCall(crate::ast::StdlibCallExpr {
                    namespace: "lifecycle".to_string(),
                    name: "transfer".to_string(),
                    args: vec![
                        Expr::Identifier(replacement.input.clone()),
                        Expr::Identifier(replacement.output.clone()),
                        replacement.lock_script.clone(),
                    ],
                    preserve_fields: replacement.data_fields.clone(),
                    span,
                })));
                body.push(Stmt::Expr(Expr::StdlibCall(crate::ast::StdlibCallExpr {
                    namespace: "cell".to_string(),
                    name: "preserve_capacity".to_string(),
                    args: vec![Expr::Identifier(replacement.output.clone()), Expr::Identifier(replacement.input.clone())],
                    preserve_fields: Vec::new(),
                    span,
                })));
                dispositions.push(NextDisposition::Replace(replacement));
            } else if self.current().is_some_and(|token| token_is_word(token, "pool")) {
                let pool = self.parse_pool(outputs)?;
                body.extend(lower_pool_body(&pool)?);
                dispositions.push(NextDisposition::Pool(pool));
            } else if self.current().is_some_and(|token| token_is_word(token, "retire")) {
                let retirement = self.parse_retirement()?;
                body.push(Stmt::Expr(Expr::Destroy(DestroyExpr {
                    expr: Box::new(Expr::Identifier(retirement.input.clone())),
                    policy: retirement.absence_policy.clone(),
                    span: retirement.span,
                })));
                dispositions.push(NextDisposition::Retire(retirement));
            } else if self.current().is_some_and(|token| token_is_word(token, "fresh")) {
                let fresh = self.parse_fresh_output(outputs)?;
                let expression = if fresh.identity == IdentityPolicy::None {
                    Expr::Create(CreateExpr {
                        target: Some(fresh.output.clone()),
                        ty: fresh.ty.clone(),
                        fields: fresh.data_fields.clone(),
                        lock: Some(Box::new(fresh.lock_script.clone())),
                        span: fresh.span,
                    })
                } else {
                    Expr::CreateUnique(CreateUniqueExpr {
                        target: Some(fresh.output.clone()),
                        ty: fresh.ty.clone(),
                        fields: fresh.data_fields.clone(),
                        lock: Some(Box::new(fresh.lock_script.clone())),
                        identity: fresh.identity.clone(),
                        span: fresh.span,
                    })
                };
                body.push(Stmt::Expr(expression));
                dispositions.push(NextDisposition::Fresh(fresh));
            } else {
                return Err(CompileError::new("entry expects an explicit replace, pool, retire, or fresh disposition", self.span()));
            }
            self.consume_statement_separator();
        }
        if dispositions.is_empty() {
            return Err(CompileError::new("entry must contain at least one exhaustive disposition", self.span()));
        }
        Ok((dispositions, body))
    }

    fn parse_replacement(&mut self) -> Result<NextReplacement> {
        let start = self.expect_word("replace")?.span;
        let input = self.parse_name("replacement input role")?;
        self.expect_kind(TokenKind::Arrow)?;
        let output = self.parse_name("replacement output role")?;
        self.skip_newlines();
        self.expect_kind(TokenKind::LBrace)?;
        self.skip_newlines();
        self.expect_word("data")?;
        self.skip_newlines();
        self.expect_kind(TokenKind::LBrace)?;
        self.skip_newlines();
        let mut data_fields = Vec::new();
        while !self.at_kind(&TokenKind::RBrace) {
            let field = self.parse_name("data field")?;
            self.expect_kind(TokenKind::Eq)?;
            self.expect_word("same")?;
            data_fields.push(field);
            self.consume_statement_separator();
            if self.at_kind(&TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
            }
        }
        self.expect_kind(TokenKind::RBrace)?;
        self.consume_statement_separator();
        self.parse_same_property("identity")?;
        self.parse_same_property("type_script")?;
        self.expect_word("lock_script")?;
        self.expect_kind(TokenKind::Eq)?;
        self.expect_word("exact_hash")?;
        let lock_script = self.parse_parenthesized_expression("exact_hash")?;
        self.consume_statement_separator();
        self.parse_same_property("capacity")?;
        self.expect_word("cardinality")?;
        self.expect_kind(TokenKind::Eq)?;
        self.expect_word("one_to_one")?;
        self.consume_statement_separator();
        let end = self.expect_kind(TokenKind::RBrace)?.span;
        Ok(NextReplacement {
            input,
            output,
            data_fields,
            lock_script,
            span: Span::new(start.start, end.end, start.line, start.column),
        })
    }

    fn parse_pool(&mut self, outputs: &[ActionOutput]) -> Result<NextPool> {
        let start = self.expect_word("pool")?.span;
        let name = self.parse_name("pool name")?;
        self.skip_newlines();
        self.expect_kind(TokenKind::LBrace)?;
        self.skip_newlines();
        self.expect_word("inputs")?;
        let inputs = self.parse_name_block("pool input role")?;
        self.expect_word("outputs")?;
        let output_names = self.parse_name_block("pool output role")?;
        let ty = output_names
            .first()
            .and_then(|name| outputs.iter().find(|candidate| candidate.name == *name))
            .and_then(|candidate| match &candidate.ty {
                Type::Named(name) => Some(name.clone()),
                _ => None,
            })
            .ok_or_else(|| CompileError::new("pool outputs must name at least one declared Cell output role", start))?;

        self.expect_word("data")?;
        self.skip_newlines();
        self.expect_kind(TokenKind::LBrace)?;
        self.skip_newlines();
        let mut data_fields = Vec::new();
        while !self.at_kind(&TokenKind::RBrace) {
            let span = self.span();
            let field = self.parse_name("pool data field")?;
            let treatment = if self.at_kind(&TokenKind::Eq) {
                self.advance();
                self.expect_word("conserve")?;
                self.consume_statement_separator();
                NextPoolFieldTreatment::Conserve
            } else {
                self.skip_newlines();
                self.expect_kind(TokenKind::LBrace)?;
                self.skip_newlines();
                let mut assignments = Vec::new();
                while !self.at_kind(&TokenKind::RBrace) {
                    let output = self.parse_name("pool output field assignment")?;
                    self.expect_kind(TokenKind::Eq)?;
                    let expression_tokens = self.take_statement_tokens()?;
                    if expression_tokens.is_empty() {
                        return Err(CompileError::new(
                            format!("pool field '{field}' output '{output}' requires an expression"),
                            self.span(),
                        ));
                    }
                    assignments.push((output, parser::parse_expression_fragment(expression_tokens)?));
                    self.consume_statement_separator();
                }
                self.expect_kind(TokenKind::RBrace)?;
                self.consume_statement_separator();
                NextPoolFieldTreatment::Set(assignments)
            };
            data_fields.push(NextPoolField { span, field, treatment });
        }
        self.expect_kind(TokenKind::RBrace)?;
        self.consume_statement_separator();
        self.parse_exact_property("identity", "pooled")?;
        self.parse_exact_property("type_script", "same")?;
        self.expect_word("lock_script")?;
        self.skip_newlines();
        self.expect_kind(TokenKind::LBrace)?;
        self.skip_newlines();
        let mut output_locks = Vec::new();
        while !self.at_kind(&TokenKind::RBrace) {
            let output = self.parse_name("pool output lock assignment")?;
            self.expect_kind(TokenKind::Eq)?;
            self.expect_word("exact_hash")?;
            let expression = self.parse_parenthesized_expression("exact_hash")?;
            output_locks.push((output, expression));
            self.consume_statement_separator();
        }
        self.expect_kind(TokenKind::RBrace)?;
        self.consume_statement_separator();
        self.parse_exact_property("capacity", "builder_computed")?;
        self.parse_exact_property("cardinality", "declared")?;
        let end = self.expect_kind(TokenKind::RBrace)?.span;
        Ok(NextPool {
            name,
            ty,
            inputs,
            outputs: output_names,
            data_fields,
            output_locks,
            span: Span::new(start.start, end.end, start.line, start.column),
        })
    }

    fn parse_name_block(&mut self, context: &str) -> Result<Vec<String>> {
        self.skip_newlines();
        self.expect_kind(TokenKind::LBrace)?;
        self.skip_newlines();
        let mut names = Vec::new();
        while !self.at_kind(&TokenKind::RBrace) {
            names.push(self.parse_name(context)?);
            if self.at_kind(&TokenKind::Comma) {
                self.advance();
            }
            self.consume_statement_separator();
        }
        self.expect_kind(TokenKind::RBrace)?;
        self.consume_statement_separator();
        if names.is_empty() {
            return Err(SourceDiagnosticCode::PoolRoles.error(format!("{context} block must not be empty"), self.span()));
        }
        Ok(names)
    }

    fn parse_retirement(&mut self) -> Result<NextRetirement> {
        let start = self.expect_word("retire")?.span;
        let input = self.parse_name("retirement input role")?;
        self.skip_newlines();
        self.expect_kind(TokenKind::LBrace)?;
        self.skip_newlines();
        self.expect_word("absence")?;
        self.expect_kind(TokenKind::Eq)?;
        let absence_policy = self.parse_absence_policy()?;
        self.consume_statement_separator();
        self.parse_exact_property("data", "discarded")?;
        self.parse_exact_property("lock_script", "none")?;
        self.parse_exact_property("type_script", "absent")?;
        self.parse_exact_property("capacity", "released")?;
        self.parse_exact_property("cardinality", "one")?;
        let end = self.expect_kind(TokenKind::RBrace)?.span;
        Ok(NextRetirement { input, absence_policy, span: Span::new(start.start, end.end, start.line, start.column) })
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
        self.skip_newlines();
        self.expect_kind(TokenKind::LBrace)?;
        self.skip_newlines();
        self.expect_word("data")?;
        self.skip_newlines();
        self.expect_kind(TokenKind::LBrace)?;
        self.skip_newlines();
        let mut data_fields = Vec::new();
        while !self.at_kind(&TokenKind::RBrace) {
            let field = self.parse_name("fresh output data field")?;
            self.expect_kind(TokenKind::Eq)?;
            let expression_tokens = self.take_statement_tokens()?;
            if expression_tokens.is_empty() {
                return Err(CompileError::new(format!("fresh output field '{field}' requires an expression"), self.span()));
            }
            let expression = parser::parse_expression_fragment(expression_tokens)?;
            data_fields.push((field, expression));
            self.consume_statement_separator();
        }
        self.expect_kind(TokenKind::RBrace)?;
        self.consume_statement_separator();
        self.expect_word("identity")?;
        self.expect_kind(TokenKind::Eq)?;
        let identity = self.parse_identity_policy()?;
        self.consume_statement_separator();
        self.parse_exact_property("type_script", "declared")?;
        self.expect_word("lock_script")?;
        self.expect_kind(TokenKind::Eq)?;
        self.expect_word("exact_hash")?;
        let lock_script = self.parse_parenthesized_expression("exact_hash")?;
        self.consume_statement_separator();
        self.parse_exact_property("capacity", "builder_computed")?;
        self.parse_exact_property("cardinality", "one")?;
        let end = self.expect_kind(TokenKind::RBrace)?.span;
        Ok(NextFreshOutput {
            output,
            ty,
            data_fields,
            identity,
            lock_script,
            span: Span::new(start.start, end.end, start.line, start.column),
        })
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

    fn parse_parenthesized_expression(&mut self, context: &str) -> Result<Expr> {
        self.expect_kind(TokenKind::LParen)?;
        let start = self.position;
        let mut depth = 0usize;
        while let Some(token) = self.current() {
            match token.kind {
                TokenKind::LParen => {
                    depth = depth.saturating_add(1);
                    self.advance();
                }
                TokenKind::RParen if depth == 0 => break,
                TokenKind::RParen => {
                    depth = depth.saturating_sub(1);
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
        }
        if start == self.position {
            return Err(CompileError::new(format!("{context} requires an expression"), self.span()));
        }
        let expression = parser::parse_expression_fragment(&self.tokens[start..self.position])?;
        self.expect_kind(TokenKind::RParen)?;
        Ok(expression)
    }

    fn parse_same_property(&mut self, property: &str) -> Result<()> {
        self.expect_word(property)?;
        self.expect_kind(TokenKind::Eq)?;
        self.expect_word("same")?;
        self.consume_statement_separator();
        Ok(())
    }

    fn parse_exact_property(&mut self, property: &str, value: &str) -> Result<()> {
        self.expect_word(property)?;
        self.expect_kind(TokenKind::Eq)?;
        self.expect_word(value)?;
        self.consume_statement_separator();
        Ok(())
    }

    fn take_statement_tokens(&mut self) -> Result<&'a [Token]> {
        let start = self.position;
        let mut paren = 0usize;
        let mut bracket = 0usize;
        let mut brace = 0usize;
        while let Some(token) = self.current() {
            match token.kind {
                TokenKind::LParen => paren = paren.saturating_add(1),
                TokenKind::RParen if paren > 0 => paren = paren.saturating_sub(1),
                TokenKind::LBracket => bracket = bracket.saturating_add(1),
                TokenKind::RBracket if bracket > 0 => bracket = bracket.saturating_sub(1),
                TokenKind::LBrace => brace = brace.saturating_add(1),
                TokenKind::RBrace if brace > 0 => brace = brace.saturating_sub(1),
                TokenKind::Newline | TokenKind::Semi if paren == 0 && bracket == 0 && brace == 0 => break,
                TokenKind::RBrace if paren == 0 && bracket == 0 && brace == 0 => break,
                _ => {}
            }
            self.advance();
        }
        if self.position == start {
            return Ok(&self.tokens[start..start]);
        }
        Ok(&self.tokens[start..self.position])
    }
}

fn validate_pool_assignments(pool: &NextPool) -> Result<()> {
    if pool.inputs.is_empty() || pool.outputs.is_empty() {
        return Err(SourceDiagnosticCode::PoolRoles.error("pool requires nonempty input and output roles", pool.span));
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
            body.push(Stmt::Expr(Expr::Require(RequireExpr {
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
        let lock =
            pool.output_locks.iter().find(|(candidate, _)| candidate == output).map(|(_, expression)| Box::new(expression.clone()));
        body.push(Stmt::Expr(Expr::Create(CreateExpr {
            target: Some(output.clone()),
            ty: pool.ty.clone(),
            fields,
            lock,
            span: pool.span,
        })));
    }
    body.extend(pool.inputs.iter().map(|input| {
        Stmt::Expr(Expr::Consume(crate::ast::ConsumeExpr { expr: Box::new(Expr::Identifier(input.clone())), span: pool.span }))
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

fn validate_surface(
    module: &ast::Module,
    trigger_type: &str,
    params: &[Param],
    outputs: &[ActionOutput],
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
            format!("type_group<{trigger_type}> must name a Cell-backed resource, shared type, or receipt in this module"),
            span,
        ));
    };
    let trigger_ty = Type::Named(trigger_type.to_string());
    if params.iter().any(|param| param.source == ParamSource::Input && param.ty != trigger_ty)
        || outputs.iter().any(|output| output.ty != trigger_ty)
    {
        return Err(CompileError::new(
            format!("Edition 2027 preview input/output ports must all use the declared type_group<{trigger_type}> schema"),
            span,
        ));
    }
    let declared_fields = fields.iter().map(|field| field.name.clone()).collect::<Vec<_>>();
    let inputs = params
        .iter()
        .filter(|param| param.source == ParamSource::Input && param.ty == Type::Named(trigger_type.to_string()))
        .map(|param| param.name.as_str())
        .collect::<BTreeSet<_>>();
    let output_names = outputs
        .iter()
        .filter(|output| output.ty == Type::Named(trigger_type.to_string()))
        .map(|output| output.name.as_str())
        .collect::<BTreeSet<_>>();
    if inputs.is_empty() && output_names.is_empty() {
        return Err(CompileError::new(
            format!("type_group<{trigger_type}> entry requires at least one explicit input or output role of type {trigger_type}"),
            span,
        ));
    }
    let mut disposed_inputs = BTreeSet::new();
    let mut produced_outputs = BTreeSet::new();
    let mut produced_output_order = Vec::new();
    let mut pool_names = BTreeSet::new();
    for disposition in dispositions {
        match disposition {
            NextDisposition::Replace(replacement) => {
                if !inputs.contains(replacement.input.as_str()) || !output_names.contains(replacement.output.as_str()) {
                    return Err(CompileError::new(
                        format!(
                            "replace {} -> {} must bind declared type_group<{trigger_type}> input/output roles",
                            replacement.input, replacement.output
                        ),
                        replacement.span,
                    ));
                }
                if !disposed_inputs.insert(replacement.input.as_str()) || !produced_outputs.insert(replacement.output.as_str()) {
                    return Err(CompileError::new(
                        "each input and output role may appear in exactly one disposition",
                        replacement.span,
                    ));
                }
                produced_output_order.push(replacement.output.as_str());
                if replacement.data_fields != declared_fields {
                    return Err(CompileError::new(
                        format!("replace data must exhaustively list fields in schema order: {}", declared_fields.join(", ")),
                        replacement.span,
                    ));
                }
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
                if pool.inputs.iter().any(|input| !disposed_inputs.insert(input.as_str()))
                    || pool.outputs.iter().any(|output| !produced_outputs.insert(output.as_str()))
                {
                    return Err(CompileError::new("each input and output role may appear in exactly one disposition", pool.span));
                }
                let pool_fields = pool.data_fields.iter().map(|field| field.field.clone()).collect::<Vec<_>>();
                if pool_fields != declared_fields {
                    return Err(CompileError::new(
                        format!("pool data must exhaustively list fields in schema order: {}", declared_fields.join(", ")),
                        pool.span,
                    ));
                }
                produced_output_order.extend(pool.outputs.iter().map(String::as_str));
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
                        format!(
                            "pool lock_script must assign every pool output exactly once in declared order: {}",
                            pool.outputs.join(", ")
                        ),
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
                if !disposed_inputs.insert(retirement.input.as_str()) {
                    return Err(CompileError::new("each input role may appear in exactly one disposition", retirement.span));
                }
            }
            NextDisposition::Fresh(fresh) => {
                if !output_names.contains(fresh.output.as_str()) || fresh.ty != trigger_type {
                    return Err(CompileError::new(
                        format!("fresh {} must bind a declared type_group<{trigger_type}> output role", fresh.output),
                        fresh.span,
                    ));
                }
                if !produced_outputs.insert(fresh.output.as_str()) {
                    return Err(CompileError::new("each output role may appear in exactly one disposition", fresh.span));
                }
                produced_output_order.push(fresh.output.as_str());
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
    let declared_output_order = outputs.iter().map(|output| output.name.as_str()).collect::<Vec<_>>();
    if produced_output_order != declared_output_order {
        return Err(CompileError::new(
            format!(
                "entry dispositions must account for outputs in declared group_output order: {}",
                declared_output_order.join(", ")
            ),
            span,
        ));
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
