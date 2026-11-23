use super::{Parser, MAX_BINARY_CHAIN_LINKS, MAX_PARSE_RECURSION_DEPTH};
use crate::ast::{
    AssignExpr, AssignOp, BinaryExpr, BinaryOp, CallExpr, CastExpr, Expr, FieldAccessExpr, IndexExpr, RangeExpr, UnaryExpr, UnaryOp,
};
use crate::error::{CompileError, Result, Span};
use crate::lexer::token::TokenKind;
use crate::syntax::SyntaxKind;

impl Parser<'_> {
    pub(super) fn parse_expr(&mut self) -> Result<Expr> {
        let syntax_start = self.current().span;
        if self.expression_depth >= MAX_PARSE_RECURSION_DEPTH {
            return Err(CompileError::new(
                format!("expression nesting exceeds the parser limit of {}", MAX_PARSE_RECURSION_DEPTH),
                self.current().span,
            ));
        }
        // Nested expressions share the outer expression's budget. Resetting
        // on parentheses would let `(1) + (1) + ...` grow an unbounded AST.
        if self.expression_depth == 0 {
            self.binary_chain_links = 0;
        }
        self.expression_depth += 1;
        let result = self.parse_expr_inner();
        self.expression_depth -= 1;
        if result.is_ok() {
            self.record_syntax_node(SyntaxKind::Expression, syntax_start);
        }
        result
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

        if self.check(&TokenKind::Plus) && self.peek(1).kind == TokenKind::Eq && self.current().span.end == self.peek(1).span.start {
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
        if self.check(&TokenKind::Dot) && self.peek(1).kind == TokenKind::Dot && self.current().span.end == self.peek(1).span.start {
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
            self.binary_chain_links += 1;
            if self.binary_chain_links > MAX_BINARY_CHAIN_LINKS {
                return Err(CompileError::new(
                    format!("binary operator chain exceeds the parser limit of {}", MAX_BINARY_CHAIN_LINKS),
                    self.current().span,
                ));
            }
            left = Expr::Binary(BinaryExpr {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span: Span::new(start_span.start, self.previous_non_newline().span.end, start_span.line, start_span.column),
            });
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
            self.binary_chain_links += 1;
            if self.binary_chain_links > MAX_BINARY_CHAIN_LINKS {
                return Err(CompileError::new(
                    format!("binary operator chain exceeds the parser limit of {}", MAX_BINARY_CHAIN_LINKS),
                    self.current().span,
                ));
            }
            left = Expr::Binary(BinaryExpr {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span: Span::new(start_span.start, self.previous_non_newline().span.end, start_span.line, start_span.column),
            });
        }

        Ok(left)
    }

    fn parse_bitwise_or(&mut self) -> Result<Expr> {
        let mut left = self.parse_bitwise_xor()?;
        loop {
            self.skip_newlines();
            if !self.check(&TokenKind::Pipe) {
                break;
            }
            let start_span = self.current().span;
            self.advance();
            self.skip_newlines();
            let right = self.parse_bitwise_xor()?;
            self.binary_chain_links += 1;
            if self.binary_chain_links > MAX_BINARY_CHAIN_LINKS {
                return Err(CompileError::new(
                    format!("binary operator chain exceeds the parser limit of {}", MAX_BINARY_CHAIN_LINKS),
                    self.current().span,
                ));
            }
            left = Expr::Binary(BinaryExpr {
                op: BinaryOp::BitOr,
                left: Box::new(left),
                right: Box::new(right),
                span: Span::new(start_span.start, self.previous_non_newline().span.end, start_span.line, start_span.column),
            });
        }
        Ok(left)
    }

    fn parse_bitwise_xor(&mut self) -> Result<Expr> {
        let mut left = self.parse_bitwise_and()?;
        loop {
            self.skip_newlines();
            if !self.check(&TokenKind::Caret) {
                break;
            }
            let start_span = self.current().span;
            self.advance();
            self.skip_newlines();
            let right = self.parse_bitwise_and()?;
            self.binary_chain_links += 1;
            if self.binary_chain_links > MAX_BINARY_CHAIN_LINKS {
                return Err(CompileError::new(
                    format!("binary operator chain exceeds the parser limit of {}", MAX_BINARY_CHAIN_LINKS),
                    self.current().span,
                ));
            }
            left = Expr::Binary(BinaryExpr {
                op: BinaryOp::BitXor,
                left: Box::new(left),
                right: Box::new(right),
                span: Span::new(start_span.start, self.previous_non_newline().span.end, start_span.line, start_span.column),
            });
        }
        Ok(left)
    }

    fn parse_bitwise_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_shift()?;
        loop {
            self.skip_newlines();
            if !self.check(&TokenKind::Ampersand) {
                break;
            }
            let start_span = self.current().span;
            self.advance();
            self.skip_newlines();
            let right = self.parse_shift()?;
            self.binary_chain_links += 1;
            if self.binary_chain_links > MAX_BINARY_CHAIN_LINKS {
                return Err(CompileError::new(
                    format!("binary operator chain exceeds the parser limit of {}", MAX_BINARY_CHAIN_LINKS),
                    self.current().span,
                ));
            }
            left = Expr::Binary(BinaryExpr {
                op: BinaryOp::BitAnd,
                left: Box::new(left),
                right: Box::new(right),
                span: Span::new(start_span.start, self.previous_non_newline().span.end, start_span.line, start_span.column),
            });
        }
        Ok(left)
    }

    fn comparison_chain_error(&self) -> CompileError {
        CompileError::new("comparison chains are not allowed; use `a < b && b < c`, or explicitly group a boolean comparison; generic calls use `function::<Type>(...)`", self.current().span)
    }

    fn parse_equality(&mut self) -> Result<Expr> {
        let (mut left, ordered) = self.parse_comparison()?;
        self.skip_newlines();
        if self.check(&TokenKind::EqEq) || self.check(&TokenKind::NotEq) {
            if ordered {
                return Err(self.comparison_chain_error());
            }
            let span = self.current().span;
            let op = if self.check(&TokenKind::EqEq) { BinaryOp::Eq } else { BinaryOp::Ne };
            self.advance();
            self.skip_newlines();
            let (right, ordered) = self.parse_comparison()?;
            if ordered {
                return Err(self.comparison_chain_error());
            }
            self.binary_chain_links += 1;
            if self.binary_chain_links > MAX_BINARY_CHAIN_LINKS {
                return Err(CompileError::new(
                    format!("binary operator chain exceeds the parser limit of {}", MAX_BINARY_CHAIN_LINKS),
                    self.current().span,
                ));
            }
            left = Expr::Binary(BinaryExpr {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span: Span::new(span.start, self.previous_non_newline().span.end, span.line, span.column),
            });
            self.skip_newlines();
            if self.check(&TokenKind::EqEq) || self.check(&TokenKind::NotEq) {
                return Err(self.comparison_chain_error());
            }
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<(Expr, bool)> {
        let mut left = self.parse_bitwise_or()?;
        self.skip_newlines();
        let op = match self.current().kind {
            TokenKind::Lt => Some(BinaryOp::Lt),
            TokenKind::Le => Some(BinaryOp::Le),
            TokenKind::Gt => Some(BinaryOp::Gt),
            TokenKind::Ge => Some(BinaryOp::Ge),
            _ => None,
        };
        if let Some(op) = op {
            let span = self.current().span;
            self.advance();
            self.skip_newlines();
            let right = self.parse_bitwise_or()?;
            self.binary_chain_links += 1;
            if self.binary_chain_links > MAX_BINARY_CHAIN_LINKS {
                return Err(CompileError::new(
                    format!("binary operator chain exceeds the parser limit of {}", MAX_BINARY_CHAIN_LINKS),
                    self.current().span,
                ));
            }
            left = Expr::Binary(BinaryExpr {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span: Span::new(span.start, self.previous_non_newline().span.end, span.line, span.column),
            });
            self.skip_newlines();
            if matches!(self.current().kind, TokenKind::Lt | TokenKind::Le | TokenKind::Gt | TokenKind::Ge) {
                return Err(self.comparison_chain_error());
            }
            Ok((left, true))
        } else {
            Ok((left, false))
        }
    }

    fn parse_shift(&mut self) -> Result<Expr> {
        let mut left = self.parse_term()?;
        loop {
            self.skip_newlines();
            let op = if self.check(&TokenKind::Lt)
                && self.peek(1).kind == TokenKind::Lt
                && self.current().span.end == self.peek(1).span.start
            {
                let span = self.current().span;
                self.advance();
                self.advance();
                Some((BinaryOp::Shl, span))
            } else if self.check(&TokenKind::Gt)
                && self.peek(1).kind == TokenKind::Gt
                && self.current().span.end == self.peek(1).span.start
            {
                let span = self.current().span;
                self.advance();
                self.advance();
                Some((BinaryOp::Shr, span))
            } else {
                None
            };
            let Some((op, start_span)) = op else {
                break;
            };
            self.skip_newlines();
            let right = self.parse_term()?;
            self.binary_chain_links += 1;
            if self.binary_chain_links > MAX_BINARY_CHAIN_LINKS {
                return Err(CompileError::new(
                    format!("binary operator chain exceeds the parser limit of {}", MAX_BINARY_CHAIN_LINKS),
                    self.current().span,
                ));
            }
            left = Expr::Binary(BinaryExpr {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span: Span::new(start_span.start, self.previous_non_newline().span.end, start_span.line, start_span.column),
            });
        }
        Ok(left)
    }

    fn parse_term(&mut self) -> Result<Expr> {
        let mut left = self.parse_factor()?;

        loop {
            self.skip_newlines();
            let op = if self.check(&TokenKind::Plus) && !matches!(&self.peek(1).kind, TokenKind::Eq) {
                self.advance();
                Some(BinaryOp::Add)
            } else if self.check(&TokenKind::Minus) {
                self.advance();
                Some(BinaryOp::Sub)
            } else {
                None
            };

            if let Some(op) = op {
                let start_span = self.current().span;
                self.skip_newlines();
                let right = self.parse_factor()?;
                self.binary_chain_links += 1;
                if self.binary_chain_links > MAX_BINARY_CHAIN_LINKS {
                    return Err(CompileError::new(
                        format!("binary operator chain exceeds the parser limit of {}", MAX_BINARY_CHAIN_LINKS),
                        self.current().span,
                    ));
                }
                left = Expr::Binary(BinaryExpr {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                    span: Span::new(start_span.start, self.previous_non_newline().span.end, start_span.line, start_span.column),
                });
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
                self.advance();
                Some(BinaryOp::Mul)
            } else if self.check(&TokenKind::Slash) {
                self.advance();
                Some(BinaryOp::Div)
            } else if self.check(&TokenKind::Percent) {
                self.advance();
                Some(BinaryOp::Mod)
            } else {
                None
            };

            if let Some(op) = op {
                let start_span = self.current().span;
                self.skip_newlines();
                let right = self.parse_cast()?;
                self.binary_chain_links += 1;
                if self.binary_chain_links > MAX_BINARY_CHAIN_LINKS {
                    return Err(CompileError::new(
                        format!("binary operator chain exceeds the parser limit of {}", MAX_BINARY_CHAIN_LINKS),
                        self.current().span,
                    ));
                }
                left = Expr::Binary(BinaryExpr {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                    span: Span::new(start_span.start, self.previous_non_newline().span.end, start_span.line, start_span.column),
                });
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
            expr = Expr::Cast(CastExpr {
                expr: Box::new(expr),
                ty,
                span: Span::new(start_span.start, self.previous_non_newline().span.end, start_span.line, start_span.column),
            });
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        self.skip_newlines();
        if self.unary_depth >= MAX_PARSE_RECURSION_DEPTH {
            return Err(CompileError::new(
                format!("unary expression nesting exceeds the parser limit of {}", MAX_PARSE_RECURSION_DEPTH),
                self.current().span,
            ));
        }
        let syntax_start = self.current().span;
        self.unary_depth += 1;
        let result = self.parse_unary_inner();
        self.unary_depth -= 1;
        if matches!(&result, Ok(Expr::Unary(_))) {
            self.record_syntax_node(SyntaxKind::PrefixExpression, syntax_start);
        }
        result
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
        let start_span = self.current().span;
        let mut expr = self.parse_primary()?;

        loop {
            self.skip_newlines();
            if self.check(&TokenKind::Dot) && !matches!(&self.peek(1).kind, TokenKind::Dot) {
                self.advance();
                let field = match &self.current().kind {
                    _ if self.ident_like_name().is_some() => self.parse_name()?,
                    TokenKind::Integer(n) => {
                        let index = n.to_string();
                        self.advance();
                        index
                    }
                    _ => {
                        return Err(CompileError::new("expected field name", self.current().span));
                    }
                };
                expr = Expr::FieldAccess(FieldAccessExpr { expr: Box::new(expr), field, span: self.current().span });
            } else if self.check(&TokenKind::Lt)
                && matches!(
                    self.peek(1).kind,
                    TokenKind::U8
                        | TokenKind::U16
                        | TokenKind::U32
                        | TokenKind::U64
                        | TokenKind::U128
                        | TokenKind::I32
                        | TokenKind::Bool
                        | TokenKind::Address
                        | TokenKind::Hash
                )
            {
                return Err(CompileError::new(
                    "generic calls require `function::<Type>(...)`; comparisons take values",
                    self.current().span,
                ));
            } else if self.check(&TokenKind::ColonColon) && self.peek(1).kind == TokenKind::Lt {
                self.advance();
                let arguments_start = self.current().span;
                self.advance();
                self.skip_newlines();
                let mut type_args = Vec::new();
                while !self.check(&TokenKind::Gt) && !self.check(&TokenKind::Eof) {
                    type_args.push(self.parse_type()?);
                    if self.check(&TokenKind::Comma) {
                        self.advance();
                        self.skip_newlines();
                    } else {
                        break;
                    }
                }
                self.expect(TokenKind::Gt)?;
                self.skip_newlines();
                self.record_syntax_node(SyntaxKind::GenericArguments, arguments_start);
                if !self.check(&TokenKind::LParen) {
                    return Err(CompileError::new("generic callable type arguments must be followed by '(...)'", self.current().span));
                }
                let args = self.parse_args()?;
                expr = Expr::Call(CallExpr { func: Box::new(expr), type_args, args, span: self.current().span });
            } else if self.check(&TokenKind::LParen) {
                let exact_call_origin =
                    matches!(&expr, Expr::Identifier(name) if matches!(name.as_str(), "zk::require_valid" | "spawn"));
                let args = self.parse_args()?;
                // Exact ZK provenance and bare-spawn source rejection belong
                // to the call, not the following token. Preserve other
                // already-versioned callable span identities.
                let span = if exact_call_origin {
                    Span::new(start_span.start, self.previous_non_newline().span.end, start_span.line, start_span.column)
                } else {
                    self.current().span
                };
                expr = Expr::Call(CallExpr { func: Box::new(expr), type_args: Vec::new(), args, span });
            } else if self.check(&TokenKind::LBracket) {
                self.advance();
                let index = self.parse_expr()?;
                self.expect(TokenKind::RBracket)?;
                expr = Expr::Index(IndexExpr { expr: Box::new(expr), index: Box::new(index), span: self.current().span });
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        match &self.current().kind {
            TokenKind::Integer(n) => {
                let val = *n;
                self.advance();
                Ok(Expr::Integer(val))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            TokenKind::String(s) => {
                let val = s.clone();
                self.advance();
                Ok(Expr::String(val))
            }
            TokenKind::ByteString(b) => {
                let val = b.clone();
                self.advance();
                Ok(Expr::ByteString(val))
            }
            TokenKind::Create => self.parse_create(),
            TokenKind::Consume => self.parse_consume(),
            TokenKind::Preserve => self.parse_preserve(),
            TokenKind::DestroyKw => self.parse_destroy(),
            TokenKind::Launch => Err(CompileError::new(
                "launch is reserved for a post-v1 transaction builder and is not part of the executable language core; use explicit create/consume/destroy operations",
                self.current().span,
            )),
            TokenKind::ReadRef => self.parse_read_ref_expr(),
            TokenKind::If => self.parse_if_expr(),
            TokenKind::Match => self.parse_match_expr(),
            TokenKind::Require => self.parse_require(),
            TokenKind::Std => self.parse_stdlib_call(),
            _ if self.current().text == "new" && Self::token_ident_like_name(&self.peek(1).kind).is_some() => {
                let start = self.current().span;
                self.advance();
                let ty = self.parse_type()?;
                let (name, arguments) = Self::nominal_designation(ty, start)?;
                let value = self.parse_struct_init(name, arguments)?;
                self.record_syntax_node(SyntaxKind::Constructor, start);
                Ok(value)
            }
            _ if self.ident_like_name().is_some() => {
                if matches!(self.ident_like_name().as_deref(), Some("consume_each" | "create_each")) {
                    return self.parse_bounded_collection_each();
                }
                if matches!(self.ident_like_name().as_deref(), Some("claim")) && !self.check_next_lparen() {
                    return self.parse_claim_expr();
                }
                if matches!(self.ident_like_name().as_deref(), Some("settle")) && !self.check_next_lparen() {
                    return self.parse_settle_expr();
                }
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
                Ok(Expr::Identifier(name))
            }

            TokenKind::LParen => {
                self.advance();
                self.skip_newlines();
                if self.check(&TokenKind::RParen) {
                    self.advance();
                    return Ok(Expr::Tuple(vec![]));
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
                    self.expect(TokenKind::RParen)?;
                    Ok(Expr::Tuple(elems))
                } else {
                    self.expect(TokenKind::RParen)?;
                    Ok(expr)
                }
            }
            TokenKind::LBracket => self.parse_array_expr(),
            TokenKind::LBrace => {
                let stmts = self.parse_block()?;
                Ok(Expr::Block(stmts))
            }
            _ => Err(CompileError::new(format!("unexpected token in expression: {}", self.current().kind), self.current().span)),
        }
    }

    fn parse_bounded_collection_each(&mut self) -> Result<Expr> {
        let start_span = self.current().span;
        let operation = self.parse_name()?;
        let binding = self.parse_name()?;
        self.expect(TokenKind::In)?;
        let collection = self.parse_expr()?;
        if !self.check(&TokenKind::LBrace) {
            return Err(CompileError::new(format!("{} requires a braced per-element contract body", operation), self.current().span));
        }
        let body = self.parse_block()?;
        let end_span = self.current().span;
        Ok(Expr::Call(CallExpr {
            func: Box::new(Expr::Identifier(format!("__cellscript_{}", operation))),
            type_args: Vec::new(),
            args: vec![Expr::Identifier(binding), collection, Expr::Block(body)],
            span: Span::new(start_span.start, end_span.end, start_span.line, start_span.column),
        }))
    }

    fn check_next_lparen(&self) -> bool {
        self.peek(1).kind == TokenKind::LParen
    }

    fn parse_array_expr(&mut self) -> Result<Expr> {
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
                self.skip_newlines();
                continue;
            }
            break;
        }
        self.expect(TokenKind::RBracket)?;
        Ok(Expr::Array(elems))
    }

    pub(super) fn parse_args(&mut self) -> Result<Vec<Expr>> {
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
                self.skip_newlines();
            } else {
                break;
            }
        }

        self.skip_newlines();
        self.expect(TokenKind::RParen)?;
        Ok(args)
    }
}
