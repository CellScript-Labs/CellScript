use super::{Parser, MAX_PARSE_RECURSION_DEPTH};
use crate::ast::{Expr, IfExpr, MatchArm, MatchExpr, MatchPattern, RequireExpr, StdlibCallExpr, StructInitExpr, Type, TypeArgument};
use crate::error::{CompileError, Result, Span};
use crate::lexer::token::TokenKind;
use crate::syntax::SyntaxKind;

impl Parser<'_> {
    /// Parse `std::namespace::name(args)` or `std::namespace::name(args) { field1, field2 }`
    pub(super) fn parse_stdlib_call(&mut self) -> Result<Expr> {
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
        let args = self.parse_args()?;

        // Optional preserve-style field block for lifecycle patterns
        let preserve_fields = if self.check(&TokenKind::LBrace) {
            self.advance();
            self.skip_newlines();
            let mut fields = Vec::new();
            while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
                fields.push(self.parse_name()?);
                self.record_end(SyntaxKind::ListElement);
                self.expect_list_separator(TokenKind::RBrace)?;
                self.skip_newlines();
            }
            self.expect(TokenKind::RBrace)?;
            fields
        } else {
            vec![]
        };

        let end_span = self.current().span;
        Ok(Expr::StdlibCall(StdlibCallExpr {
            namespace,
            name,
            args,
            preserve_fields,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    pub(super) fn parse_if_expr(&mut self) -> Result<Expr> {
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
            Ok(Expr::Block(self.parse_block()?))
        } else if self.check(&TokenKind::If) {
            self.parse_if_expr()
        } else {
            self.parse_expr()
        }
    }

    pub(super) fn parse_struct_init(&mut self, ty: String, type_args: Vec<Type>) -> Result<Expr> {
        let start_span = self.current().span;
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
        Ok(Expr::StructInit(StructInitExpr { ty, type_args, fields, span: start_span }))
    }

    pub(super) fn nominal_designation(ty: Type, span: Span) -> Result<(String, Vec<Type>)> {
        match ty {
            Type::Named(name) => Ok((name, Vec::new())),
            Type::Applied(application) => {
                let arguments = application
                    .arguments
                    .into_iter()
                    .map(|argument| match argument {
                        TypeArgument::Type(ty) => Ok(ty),
                        TypeArgument::UnsignedConstant(_) => {
                            Err(CompileError::new("nominal declarations accept type arguments, not constant arguments", span)
                                .with_code(crate::error::DiagnosticCode::E2111))
                        }
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok((application.constructor, arguments))
            }
            _ => Err(CompileError::new("this operation requires a nominal declaration", span)
                .with_code(crate::error::DiagnosticCode::E2111)),
        }
    }

    pub(super) fn parse_match_expr(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::Match)?;
        let expr = self.parse_expr()?;
        self.skip_newlines();
        self.expect(TokenKind::LBrace)?;
        self.skip_newlines();

        let mut arms = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let arm_start = self.current().span;
            let pattern = self.parse_match_pattern()?;
            self.skip_newlines();
            self.expect(TokenKind::FatArrow)?;
            self.skip_newlines();
            let value = if self.check(&TokenKind::LBrace) {
                Expr::Block(self.parse_block()?)
            } else if self.check(&TokenKind::Require) {
                self.parse_match_arm_require()?
            } else {
                self.parse_expr()?
            };
            let arm_end = self.current().span;
            let arm_span = Span::new(arm_start.start, arm_end.end, arm_start.line, arm_start.column);
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

    fn parse_match_pattern(&mut self) -> Result<MatchPattern> {
        let mut patterns = vec![self.parse_match_pattern_primary()?];
        while self.check(&TokenKind::Pipe) {
            self.advance();
            self.skip_newlines();
            patterns.push(self.parse_match_pattern_primary()?);
        }
        if patterns.len() == 1 {
            Ok(patterns.pop().expect("one match pattern"))
        } else {
            Ok(MatchPattern::Or(patterns))
        }
    }

    fn parse_match_pattern_primary(&mut self) -> Result<MatchPattern> {
        if self.pattern_depth >= MAX_PARSE_RECURSION_DEPTH {
            return Err(CompileError::new(
                format!("pattern nesting exceeds the parser limit of {}", MAX_PARSE_RECURSION_DEPTH),
                self.current().span,
            ));
        }
        self.pattern_depth += 1;
        let result = self.parse_match_pattern_primary_inner();
        self.pattern_depth -= 1;
        result
    }

    fn parse_match_pattern_primary_inner(&mut self) -> Result<MatchPattern> {
        if self.check(&TokenKind::Underscore) || matches!(&self.current().kind, TokenKind::Identifier(name) if name == "_") {
            self.advance();
            return Ok(MatchPattern::Wildcard);
        }
        if self.check(&TokenKind::LParen) {
            self.advance();
            self.skip_newlines();
            let mut items = Vec::new();
            let mut saw_comma = false;
            while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                items.push(self.parse_match_pattern()?);
                self.skip_newlines();
                if self.check(&TokenKind::Comma) {
                    saw_comma = true;
                    self.advance();
                    self.skip_newlines();
                } else {
                    break;
                }
            }
            self.expect(TokenKind::RParen)?;
            if items.len() == 1 && !saw_comma {
                return Ok(items.pop().expect("one grouped match pattern"));
            }
            return Ok(MatchPattern::Tuple(items));
        }
        let path = self.parse_name_path()?;
        if self.check(&TokenKind::LBrace) {
            self.advance();
            self.skip_newlines();
            let mut fields = Vec::new();
            while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
                let name = self.parse_name()?;
                let pattern = if self.check(&TokenKind::Colon) {
                    self.advance();
                    self.skip_newlines();
                    self.parse_match_pattern()?
                } else {
                    MatchPattern::Binding(name.clone())
                };
                fields.push((name, pattern));
                self.skip_newlines();
                if self.check(&TokenKind::Comma) {
                    self.advance();
                    self.skip_newlines();
                } else {
                    break;
                }
            }
            self.expect(TokenKind::RBrace)?;
            return Ok(MatchPattern::Struct { path, fields });
        }
        let mut fields = Vec::new();
        if self.check(&TokenKind::LParen) {
            self.advance();
            self.skip_newlines();
            while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                fields.push(self.parse_match_pattern()?);
                self.skip_newlines();
                if self.check(&TokenKind::Comma) {
                    self.advance();
                    self.skip_newlines();
                } else {
                    break;
                }
            }
            self.expect(TokenKind::RParen)?;
            return Ok(MatchPattern::Variant { path, fields });
        }
        if path.rsplit_once("::").is_some() || path.chars().next().is_some_and(char::is_uppercase) {
            Ok(MatchPattern::Variant { path, fields })
        } else {
            Ok(MatchPattern::Binding(path))
        }
    }

    fn parse_match_arm_require(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        self.expect(TokenKind::Require)?;
        let condition = self.parse_expr()?;
        let message = if self.check(&TokenKind::Comma) && matches!(self.peek(1).kind, TokenKind::String(_)) {
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
        let end_span = self.current().span;
        Ok(Expr::Require(RequireExpr {
            condition: Box::new(condition),
            message,
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }
}
