use super::Parser;
use crate::ast::{
    Expr, ReplaceCapacityTreatment, ReplaceDataTreatment, ReplaceFieldTreatment, ReplaceLockTreatment, ReplaceRelation,
    ReplaceTypeScriptTreatment,
};
use crate::error::{CompileError, Result};
use crate::lexer::token::TokenKind;
use crate::syntax::SyntaxKind;

impl Parser<'_> {
    /// The successor relation is authoring-route syntax: `replace x -> y {`.
    /// It is recognised contextually so `replace` stays an ordinary identifier
    /// everywhere else, including capability lists.
    pub(super) fn is_replace_relation_ahead(&self) -> bool {
        let mut following = self.tokens[self.position + 1..].iter().filter(|token| !matches!(token.kind, TokenKind::Newline));
        matches!(&self.current().kind, TokenKind::Identifier(name) if name == "replace")
            && following.next().is_some_and(|token| matches!(token.kind, TokenKind::Identifier(_)))
            && following.next().is_some_and(|token| matches!(token.kind, TokenKind::Arrow))
    }

    fn expect_identifier(&mut self, what: &str) -> Result<String> {
        self.skip_newlines();
        if let TokenKind::Identifier(name) = &self.current().kind {
            let name = name.clone();
            self.advance();
            Ok(name)
        } else {
            Err(CompileError::new(format!("expected {what}, found `{}`", self.current().kind), self.current().span))
        }
    }

    fn expect_identifier_keyword(&mut self, keyword: &str, spelling: &str) -> Result<()> {
        self.skip_newlines();
        if matches!(&self.current().kind, TokenKind::Identifier(name) if name == keyword) {
            self.advance();
            Ok(())
        } else {
            Err(CompileError::new(format!("expected {spelling}"), self.current().span))
        }
    }

    pub(super) fn parse_replace_relation(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.advance(); // `replace`
        let before = self.expect_identifier("relation predecessor")?;
        self.expect(TokenKind::Arrow)?;
        let after = self.expect_identifier("relation successor")?;

        let mut data: Option<ReplaceDataTreatment> = None;
        let mut lock: Option<ReplaceLockTreatment> = None;
        let mut capacity: Option<ReplaceCapacityTreatment> = None;
        let mut type_script: Option<ReplaceTypeScriptTreatment> = None;

        self.skip_newlines();
        self.expect(TokenKind::LBrace)?;
        loop {
            self.skip_newlines();
            if self.check(&TokenKind::RBrace) || self.check(&TokenKind::Eof) {
                break;
            }
            let keyword = match &self.current().kind {
                TokenKind::Identifier(name) => name.clone(),
                // `lock` is a lexer keyword, not an identifier.
                TokenKind::Lock => "lock".to_string(),
                _ => {
                    return Err(CompileError::new(
                        "expected a `data`, `lock`, `capacity` or `type_script` treatment",
                        self.current().span,
                    ));
                }
            };
            self.advance();
            match keyword.as_str() {
                "data" => {
                    if data.is_some() {
                        return Err(CompileError::new("duplicate `data` treatment", self.current().span));
                    }
                    data = Some(self.parse_replace_data_treatment()?);
                }
                "lock" => {
                    if lock.is_some() {
                        return Err(CompileError::new("duplicate `lock` treatment", self.current().span));
                    }
                    lock = Some(self.parse_replace_lock_treatment()?);
                }
                "capacity" => {
                    if capacity.is_some() {
                        return Err(CompileError::new("duplicate `capacity` treatment", self.current().span));
                    }
                    self.expect(TokenKind::Eq)?;
                    self.expect_identifier_keyword("same", "`capacity = same`")?;
                    capacity = Some(ReplaceCapacityTreatment::Same);
                }
                "type_script" => {
                    if type_script.is_some() {
                        return Err(CompileError::new("duplicate `type_script` treatment", self.current().span));
                    }
                    self.expect(TokenKind::Eq)?;
                    self.expect_identifier_keyword("same", "`type_script = same`")?;
                    type_script = Some(ReplaceTypeScriptTreatment::Same);
                }
                _ => {
                    return Err(CompileError::new(
                        format!("unknown relation treatment `{keyword}`; expected `data`, `lock`, `capacity` or `type_script`"),
                        self.current().span,
                    ));
                }
            }
            self.record_end(SyntaxKind::ClauseEnd);
            self.expect_statement_end()?;
        }
        self.skip_newlines();
        self.expect(TokenKind::RBrace)?;

        match (data, lock, capacity, type_script) {
            (Some(data), Some(lock), Some(capacity), Some(type_script)) => {
                Ok(Expr::ReplaceRelation(ReplaceRelation { before, after, data, lock, capacity, type_script, span: start_span }))
            }
            (data, lock, capacity, type_script) => {
                let missing = [
                    ("data", data.is_none()),
                    ("lock", lock.is_none()),
                    ("capacity", capacity.is_none()),
                    ("type_script", type_script.is_none()),
                ]
                .into_iter()
                .filter_map(|(name, missing)| missing.then_some(name))
                .collect::<Vec<_>>();
                Err(CompileError::new(
                    format!("replace relation must state every treatment explicitly; missing {}", missing.join(", ")),
                    start_span,
                ))
            }
        }
    }

    fn parse_replace_data_treatment(&mut self) -> Result<ReplaceDataTreatment> {
        self.skip_newlines();
        if self.check(&TokenKind::Eq) {
            // `data = same except { f = expr, ... }`
            self.advance();
            self.expect_identifier_keyword("same", "`data = same` or `data = same except { ... }`")?;
            self.skip_newlines();
            if self.current_identifier().as_deref() != Some("except") {
                return Ok(ReplaceDataTreatment::SameExcept(Vec::new()));
            }
            self.advance();
            self.expect(TokenKind::LBrace)?;
            let mut assigned = Vec::new();
            loop {
                self.skip_newlines();
                if self.check(&TokenKind::RBrace) || self.check(&TokenKind::Eof) {
                    break;
                }
                let field = self.expect_identifier("`same except` field")?;
                if assigned.iter().any(|(name, _)| *name == field) {
                    return Err(CompileError::new(format!("duplicate `same except` field `{field}`"), self.current().span));
                }
                self.expect(TokenKind::Eq)?;
                let value = self.parse_expr()?;
                assigned.push((field, value));
                self.record_end(SyntaxKind::ListElement);
                self.expect_list_separator(TokenKind::RBrace)?;
            }
            self.skip_newlines();
            self.expect(TokenKind::RBrace)?;
            if assigned.is_empty() {
                return Err(CompileError::new("use `data = same` when preserving every field", self.previous_non_newline().span));
            }
            return Ok(ReplaceDataTreatment::SameExcept(assigned));
        }

        // `data { first = same, second = expr, }`
        self.skip_newlines();
        self.expect(TokenKind::LBrace)?;
        let mut treatments = Vec::new();
        loop {
            self.skip_newlines();
            if self.check(&TokenKind::RBrace) || self.check(&TokenKind::Eof) {
                break;
            }
            let field = self.expect_identifier("relation data field")?;
            self.expect(TokenKind::Eq)?;
            self.skip_newlines();
            if matches!(&self.current().kind, TokenKind::Identifier(name) if name == "same") {
                self.advance();
                treatments.push(ReplaceFieldTreatment::Same(field));
            } else {
                let value = self.parse_expr()?;
                treatments.push(ReplaceFieldTreatment::Assign(field, value));
            }
            self.record_end(SyntaxKind::ListElement);
            self.expect_list_separator(TokenKind::RBrace)?;
        }
        self.skip_newlines();
        self.expect(TokenKind::RBrace)?;
        Ok(ReplaceDataTreatment::Fields(treatments))
    }

    fn parse_replace_lock_treatment(&mut self) -> Result<ReplaceLockTreatment> {
        self.expect(TokenKind::Eq)?;
        self.skip_newlines();
        if matches!(&self.current().kind, TokenKind::Identifier(name) if name == "same") {
            self.advance();
            return Ok(ReplaceLockTreatment::Same);
        }
        if matches!(&self.current().kind, TokenKind::Identifier(name) if name == "exact") {
            self.advance();
            self.expect(TokenKind::LParen)?;
            let lock = self.parse_expr()?;
            self.expect(TokenKind::RParen)?;
            return Ok(ReplaceLockTreatment::Exact(Box::new(lock)));
        }
        if matches!(&self.current().kind, TokenKind::Identifier(name) if name == "exact_hash") {
            self.advance();
            self.expect(TokenKind::LParen)?;
            let lock = self.parse_expr()?;
            self.expect(TokenKind::RParen)?;
            return Ok(ReplaceLockTreatment::ExactHash(Box::new(lock)));
        }
        Err(CompileError::new(
            "expected `lock = same`, `lock = exact(address)` or `lock = exact_hash(script_hash)`",
            self.current().span,
        ))
    }
}
