use super::Parser;
use crate::ast::{Expr, PreserveExpr, ReadRefExpr, RequireBlockExpr, RequireExpr};
use crate::error::{CompileError, Result, Span};
use crate::lexer::token::TokenKind;
use crate::syntax::SyntaxKind;

impl Parser<'_> {
    pub(super) fn parse_read_ref_expr(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::ReadRef)?;

        self.expect_explicit_type_arguments()?;
        let parsed_type = self.parse_type()?;
        self.expect(TokenKind::Gt)?;
        let (ty, type_args) = Self::nominal_designation(parsed_type, start_span)?;
        self.expect(TokenKind::LParen)?;
        self.expect(TokenKind::RParen)?;

        let end_span = self.current().span;
        Ok(Expr::ReadRef(ReadRefExpr {
            ty,
            type_args,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    pub(super) fn parse_require(&mut self) -> Result<Expr> {
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
                        .with_code(crate::error::DiagnosticCode::E1005));
                    }
                    Expr::If(_) | Expr::Match(_) => {
                        return Err(CompileError::new(
                            "require block contains control flow — only pure boolean expressions are allowed",
                            expr.span(),
                        )
                        .with_code(crate::error::DiagnosticCode::E1006));
                    }
                    _ => {}
                }
                expressions.push(expr);
                self.record_end(SyntaxKind::ListElement);
                self.expect_list_separator(TokenKind::RBrace)?;
                self.skip_newlines();
            }
            if expressions.is_empty() {
                return Err(CompileError::new("require block is empty — at least one boolean expression is required", start_span)
                    .with_code(crate::error::DiagnosticCode::E1001));
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
        } else if self.check(&TokenKind::Else) {
            self.advance();
            self.skip_newlines();
            let message_span = self.current().span;
            if let Some(name) = self.ident_like_name() {
                self.advance();
                Some(Box::new(Expr::String(name)))
            } else {
                return Err(CompileError::new("require else must name an error identifier", message_span));
            }
        } else {
            None
        };
        let end_span = self.previous_non_newline().span;
        Ok(Expr::Require(RequireExpr {
            condition: Box::new(condition),
            message,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    /// Parse `preserve output_name from input_name { field1, field2, ... }`
    pub(super) fn parse_preserve(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::Preserve)?;

        let output_name = self.parse_name()?;

        // Expect "from"
        let from_marker = self.ident_like_name().unwrap_or_default();
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
            .with_code(crate::error::DiagnosticCode::E1009));
        }
        self.advance(); // consume {
        self.skip_newlines();

        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            // Reject wildcard
            if self.check(&TokenKind::Star) {
                return Err(CompileError::new("preserve wildcard '*' is not allowed — use explicit whitelist", self.current().span)
                    .with_code(crate::error::DiagnosticCode::E1007));
            }
            // Reject 'except' keyword used as blacklist
            if self.ident_like_name().as_deref() == Some("except") {
                return Err(CompileError::new("preserve except is not allowed — use explicit whitelist", self.current().span)
                    .with_code(crate::error::DiagnosticCode::E1008));
            }
            let field_name = self.parse_name()?;
            fields.push(field_name);
            self.record_end(SyntaxKind::ListElement);
            self.expect_list_separator(TokenKind::RBrace)?;
            self.skip_newlines();
        }

        if fields.is_empty() {
            return Err(CompileError::new("preserve block is empty — at least one field name is required", start_span)
                .with_code(crate::error::DiagnosticCode::E1001));
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
}
