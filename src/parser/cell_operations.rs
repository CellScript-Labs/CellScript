use super::Parser;
use crate::ast::{
    ClaimExpr, ConsumeExpr, CreateExpr, CreateUniqueExpr, DestroyExpr, DestructionPolicy, Expr, IdentityPolicy, ReplaceUniqueExpr,
    SettleExpr, Stmt,
};
use crate::error::{CompileError, Result, Span};
use crate::lexer::token::TokenKind;
use crate::syntax::SyntaxKind;

impl Parser<'_> {
    pub(super) fn parse_create(&mut self) -> Result<Expr> {
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
            let field_name = self.parse_name()?;
            let value = if self.check(&TokenKind::Colon) {
                self.advance();
                self.parse_expr()?
            } else {
                Expr::Identifier(field_name.clone())
            };
            fields.push((field_name, value));
            self.record_end(SyntaxKind::ListElement);

            self.expect_list_separator(TokenKind::RBrace)?;
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

    pub(super) fn parse_consume_statement(&mut self) -> Result<Stmt> {
        let start_span = self.current().span;
        self.expect(TokenKind::Consume)?;
        self.skip_newlines();
        let binding = self
            .ident_like_name()
            .ok_or_else(|| CompileError::new("consume requires a named cell-backed value", self.current().span))?;
        self.advance();
        self.skip_newlines();
        let end_span = self.current().span;
        self.expect_statement_end()?;
        Ok(Stmt::Discard(Expr::Consume(ConsumeExpr {
            expr: Box::new(Expr::Identifier(binding)),
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        })))
    }

    pub(super) fn parse_claim_expr(&mut self) -> Result<Expr> {
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

    pub(super) fn parse_settle_expr(&mut self) -> Result<Expr> {
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

    pub(super) fn parse_destroy(&mut self) -> Result<Expr> {
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

    pub(super) fn parse_destroy_singleton_type(&mut self) -> Result<Expr> {
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

    pub(super) fn parse_destroy_unique(&mut self) -> Result<Expr> {
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

    pub(super) fn parse_destroy_instance(&mut self) -> Result<Expr> {
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

    pub(super) fn parse_burn_amount(&mut self) -> Result<Expr> {
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
        let name = self.parse_name()?;
        if name != expected_name {
            return Err(CompileError::new(
                format!("expected named argument '{}', got '{}'", expected_name, name),
                self.current().span,
            ));
        }
        self.expect(TokenKind::Eq)?;
        let value = self.parse_name()?;
        Ok(value)
    }

    pub(super) fn expect_explicit_type_arguments(&mut self) -> Result<Span> {
        if self.check(&TokenKind::Lt) {
            let span = self.current().span;
            return Err(CompileError::new(
                "generic operation type arguments use `::<T>`; insert `::` before `<`",
                Span::new(span.start, span.start, span.line, span.column),
            ));
        }
        self.expect(TokenKind::ColonColon)?;
        let start = self.expect(TokenKind::Lt)?.span;
        self.skip_newlines();
        Ok(start)
    }

    pub(super) fn parse_create_unique_expr(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        // Already consumed the identifier "create_unique"
        let arguments_start = self.expect_explicit_type_arguments()?;
        let parsed_type = self.parse_type()?;
        let (ty, type_args) = Self::nominal_designation(parsed_type, start_span)?;
        self.expect(TokenKind::Gt)?;
        self.record_syntax_node(SyntaxKind::GenericArguments, arguments_start);
        self.expect(TokenKind::LParen)?;
        self.skip_newlines();
        let identity = if self.check(&TokenKind::RParen) { None } else { Some(self.parse_identity_policy_from_args()?) };
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
            target: None,
            ty,
            type_args,
            fields,
            lock,
            identity,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    pub(super) fn parse_replace_unique_expr(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        // Already consumed the identifier "replace_unique"
        let arguments_start = self.expect_explicit_type_arguments()?;
        let parsed_type = self.parse_type()?;
        let (ty, type_args) = Self::nominal_designation(parsed_type, start_span)?;
        self.expect(TokenKind::Gt)?;
        self.record_syntax_node(SyntaxKind::GenericArguments, arguments_start);
        self.expect(TokenKind::LParen)?;
        self.skip_newlines();
        let identity = if self.check(&TokenKind::RParen) { None } else { Some(self.parse_identity_policy_from_args()?) };
        self.expect(TokenKind::RParen)?;

        let expr = self.parse_expr()?;

        let fields = self.parse_field_init_list()?;

        let end_span = self.current().span;
        Ok(Expr::ReplaceUnique(ReplaceUniqueExpr {
            expr: Box::new(expr),
            ty,
            type_args,
            fields,
            identity,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn parse_identity_policy_from_args(&mut self) -> Result<IdentityPolicy> {
        let name = self.parse_name()?;
        if name != "identity" {
            return Err(CompileError::new(format!("expected 'identity' named argument, got '{}'", name), self.current().span));
        }
        self.expect(TokenKind::Eq)?;
        let kind = self.parse_name()?;
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
                self.current().span,
            )),
        }
    }

    fn parse_field_init_list(&mut self) -> Result<Vec<(String, Expr)>> {
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let name = self.parse_name_path()?;
            if self.check(&TokenKind::Colon) {
                self.advance();
                let value = self.parse_expr()?;
                fields.push((name, value));
            } else {
                // Field shorthand: `amount` means `amount: amount`
                fields.push((name.clone(), Expr::Identifier(name)));
            }
            self.record_end(SyntaxKind::ListElement);
            self.expect_list_separator(TokenKind::RBrace)?;
        }
        self.expect(TokenKind::RBrace)?;
        Ok(fields)
    }
}
